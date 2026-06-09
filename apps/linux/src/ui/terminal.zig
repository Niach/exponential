//! Embedded ghostty terminal widget (M7). A `GtkGLArea` hosts a real
//! GPU-rendered ghostty surface via the embeddable libghostty (built from the
//! douglas/ghostty fork; see scripts/build-libghostty.sh). This replaces the
//! headless `std.process.run` child for agent runs with a visible, steerable
//! terminal — the agent's `claude`/`codex` session runs inside it.
//!
//! Architecture (mirrors Ghostty's own GTK apprt + cmux-gtk, the proven
//! reference for this exact libghostty commit):
//!   - A process-global ghostty app (`ghostty_init` + `ghostty_app_new`) ticked
//!     on the GTK main loop from a `wakeup` callback via `g_idle_add`.
//!   - Each terminal is a `GtkGLArea`; the ghostty surface is created lazily on
//!     the FIRST resize (when GTK has allocated real pixels and the GL context
//!     is current — ghostty loads GLAD from the current context at surface init).
//!   - "render" → glViewport + ghostty_surface_draw; "resize" → set_content_scale
//!     + set_size; input via GtkEventControllers forwarded to the surface.
//!
//! HARD RULE (same as the rest of the app): GTK + ghostty are touched only on
//! the main thread. The only off-thread entry is `wakeup`, which just schedules
//! a tick via the thread-safe `g_idle_add`.

const std = @import("std");
const gtk = @import("gtk.zig");
const g = @import("ghostty_ffi.zig");
const build_options = @import("build_options");

extern fn setenv(name: [*:0]const u8, value: [*:0]const u8, overwrite: c_int) c_int;
extern fn gdk_keyval_to_unicode(keyval: c_uint) u32;
extern fn gdk_keyval_to_lower(keyval: c_uint) c_uint;

// GDK4 ModifierType masks (gdk/gdkenums.h).
const GDK_SHIFT_MASK: c_uint = 1 << 0;
const GDK_LOCK_MASK: c_uint = 1 << 1;
const GDK_CONTROL_MASK: c_uint = 1 << 2;
const GDK_ALT_MASK: c_uint = 1 << 3;
const GDK_SUPER_MASK: c_uint = 1 << 26;

// -------------------------------------------------------------------------
// Process-global ghostty app (single-threaded; main-thread only).
// -------------------------------------------------------------------------

var g_app: g.App = null;
var g_runtime: g.runtime_config_s = undefined; // must outlive the app
var g_tick_queued: bool = false;

/// Lazily init the ghostty runtime + app. Returns null on failure.
fn ensureApp() g.App {
    if (g_app != null) return g_app;

    // Point ghostty at our bundled resources + terminfo (app-runtime=none does
    // not install them globally). Build-time absolute paths from build.zig.
    var buf: [4096]u8 = undefined;
    if (build_options.ghostty_resources_dir.len > 0) {
        if (std.fmt.bufPrintZ(&buf, "{s}", .{build_options.ghostty_resources_dir})) |z| {
            _ = setenv("GHOSTTY_RESOURCES_DIR", z.ptr, 0);
        } else |_| {}
    }
    if (build_options.ghostty_terminfo_dir.len > 0) {
        if (std.fmt.bufPrintZ(&buf, "{s}", .{build_options.ghostty_terminfo_dir})) |z| {
            _ = setenv("TERMINFO", z.ptr, 0);
        } else |_| {}
    }

    if (g.ghostty_init(0, null) != g.SUCCESS) {
        std.log.err("ghostty_init failed", .{});
        return null;
    }

    const config = g.ghostty_config_new();
    if (config == null) {
        std.log.err("ghostty_config_new returned null", .{});
        return null;
    }
    g.ghostty_config_load_default_files(config);
    g.ghostty_config_load_recursive_files(config);
    g.ghostty_config_finalize(config);

    g_runtime = .{
        .userdata = null,
        .supports_selection_clipboard = true,
        .wakeup_cb = onWakeup,
        .action_cb = onAction,
        .read_clipboard_cb = onReadClipboard,
        .confirm_read_clipboard_cb = onConfirmReadClipboard,
        .write_clipboard_cb = onWriteClipboard,
        .close_surface_cb = onCloseSurface,
    };

    const app = g.ghostty_app_new(&g_runtime, config);
    if (app == null) {
        std.log.err("ghostty_app_new returned null", .{});
        g.ghostty_config_free(config);
        return null;
    }
    g_app = app;
    return app;
}

fn onWakeup(_: ?*anyopaque) callconv(.c) void {
    // Called by ghostty (possibly off the main thread) to request a tick.
    // g_idle_add is thread-safe; coalesce so we schedule at most one pending.
    if (@atomicRmw(bool, &g_tick_queued, .Xchg, true, .seq_cst)) return;
    _ = gtk.g_idle_add(tickOnce, null);
}

fn tickOnce(_: gtk.gpointer) callconv(.c) c_int {
    g_tick_queued = false;
    if (g_app) |app| g.ghostty_app_tick(app);
    return 0; // G_SOURCE_REMOVE — fire once
}

// Action callback: ghostty asks the host to perform window/tab/etc. actions.
// We handle child/command exit (to notify agent runs) and otherwise decline.
fn onAction(_: g.App, target: g.target_s, action: g.action_s) callconv(.c) bool {
    switch (action.tag) {
        g.ACTION_RENDER => {
            // ghostty wants the surface redrawn — queue a render on its GLArea.
            if (termFromTarget(target)) |term| gtk.gtk_gl_area_queue_render(term.area);
            return true;
        },
        g.ACTION_SHOW_CHILD_EXITED => {
            notifyExit(target, @intCast(action.action.child_exited.exit_code));
            return true;
        },
        g.ACTION_COMMAND_FINISHED => {
            notifyExit(target, action.action.command_finished.exit_code);
            return true;
        },
        else => return false,
    }
}

/// Recover our Term from an action target (surface userdata == the Term ptr).
fn termFromTarget(target: g.target_s) ?*Term {
    if (target.tag != 1) return null; // GHOSTTY_TARGET_SURFACE == 1
    const surface = target.target.surface;
    if (surface == null) return null;
    const ud = g.ghostty_surface_userdata(surface) orelse return null;
    return @ptrCast(@alignCast(ud));
}

fn notifyExit(target: g.target_s, exit_code: i32) void {
    const term = termFromTarget(target) orelse return;
    if (term.notified_exit) return;
    term.notified_exit = true;
    if (term.on_exit) |cb| cb(term.on_exit_ctx, exit_code);
}

// Minimal clipboard handling for v1 (no OSC-52 read confirm UI yet).
fn onReadClipboard(_: ?*anyopaque, _: c_int, _: ?*anyopaque) callconv(.c) void {}
fn onConfirmReadClipboard(_: ?*anyopaque, _: [*c]const u8, _: ?*anyopaque, _: c_int) callconv(.c) void {}
fn onWriteClipboard(_: ?*anyopaque, _: c_int, _: ?*const anyopaque, _: usize, _: bool) callconv(.c) void {}
fn onCloseSurface(_: ?*anyopaque, _: bool) callconv(.c) void {}

// -------------------------------------------------------------------------
// Terminal widget
// -------------------------------------------------------------------------

pub const Options = struct {
    /// Working directory for the spawned process.
    cwd: ?[]const u8 = null,
    /// Command line to run (ghostty parses/splits it). Null → default shell.
    command: ?[]const u8 = null,
    /// Extra environment variables (key, value) layered on the child env.
    env: []const [2][]const u8 = &.{},
    /// Keep the surface open after the command exits so output stays visible.
    wait_after_command: bool = true,
    /// Fired (on the main thread) when the child process exits.
    on_exit: ?*const fn (ctx: ?*anyopaque, exit_code: i32) void = null,
    on_exit_ctx: ?*anyopaque = null,
};

/// Per-widget state, owned via g_object_set_data_full on the GtkGLArea.
const Term = struct {
    gpa: std.mem.Allocator,
    area: gtk.Object,
    surface: g.Surface = null,
    created: bool = false,
    grace: bool = true, // paint bg, not content, until the shell settles
    notified_exit: bool = false,
    on_exit: ?*const fn (ctx: ?*anyopaque, exit_code: i32) void = null,
    on_exit_ctx: ?*anyopaque = null,
    // GLib timeout source ids (0 = inactive). The widget can be destroyed while
    // the child is alive (cancel/sign-out teardown), so destroyTerm must remove
    // any still-armed source or it fires on the freed Term. Each callback zeroes
    // its id on the last fire so we never remove a dead source.
    grace_source: c_uint = 0,
    poll_source: c_uint = 0,

    // Owned, null-terminated config for deferred surface creation.
    cwd_z: ?[:0]u8 = null,
    command_z: ?[:0]u8 = null,
    env_keys: std.ArrayListUnmanaged([:0]u8) = .empty,
    env_vals: std.ArrayListUnmanaged([:0]u8) = .empty,
    env_c: std.ArrayListUnmanaged(g.env_var_s) = .empty,
};

/// Create an embedded ghostty terminal. Returns the `GtkGLArea` widget (add it
/// to your layout), or null if the app/widget couldn't be created.
pub fn create(gpa: std.mem.Allocator, opts: Options) ?gtk.Object {
    const app = ensureApp();
    if (app == null) return null;

    const area = gtk.gtk_gl_area_new();
    if (area == null) return null;

    // Match Ghostty's GTK surface config so resizes/invalidations produce
    // frames and the renderer gets a desktop GL 4.3 core context.
    gtk.gtk_gl_area_set_auto_render(area, 1);
    gtk.gtk_gl_area_set_has_depth_buffer(area, 0);
    gtk.gtk_gl_area_set_has_stencil_buffer(area, 0);
    // We build the GL context ourselves (see onCreateContext) — forcing desktop
    // GL 4.3 via the area's own setters gets rejected on some drivers (NVIDIA).
    _ = gtk.g_signal_connect_data(area, "create-context", @ptrCast(&onCreateContext), null, null, 0);
    gtk.gtk_widget_set_focusable(area, 1);
    gtk.gtk_widget_set_can_focus(area, 1);
    // Without expand, a GLArea in a vertical box gets 0 height → resize/render
    // never fire → permanent black screen.
    gtk.gtk_widget_set_hexpand(area, 1);
    gtk.gtk_widget_set_vexpand(area, 1);

    const term = gpa.create(Term) catch return null;
    term.* = .{ .gpa = gpa, .area = area, .on_exit = opts.on_exit, .on_exit_ctx = opts.on_exit_ctx };

    // Dupe config into owned null-terminated storage (read at first resize).
    if (opts.cwd) |c| term.cwd_z = gpa.dupeZ(u8, c) catch null;
    if (opts.command) |c| term.command_z = gpa.dupeZ(u8, c) catch null;
    for (opts.env) |kv| {
        const k = gpa.dupeZ(u8, kv[0]) catch continue;
        const v = gpa.dupeZ(u8, kv[1]) catch {
            gpa.free(k);
            continue;
        };
        term.env_keys.append(gpa, k) catch {
            gpa.free(k);
            gpa.free(v);
            continue;
        };
        term.env_vals.append(gpa, v) catch {};
    }

    // Own the state for the widget's lifetime; freed on destroy.
    gtk.g_object_set_data_full(area, "exp-term", term, destroyTerm);

    // Signals: realize (GL ctx), resize (create + size), render (draw).
    _ = gtk.g_signal_connect_data(area, "realize", @ptrCast(&onRealize), term, null, 0);
    _ = gtk.g_signal_connect_data(area, "resize", @ptrCast(&onResize), term, null, 0);
    _ = gtk.g_signal_connect_data(area, "render", @ptrCast(&onRender), term, null, 0);

    setupInput(term);
    return area;
}

fn destroyTerm(data: gtk.gpointer) callconv(.c) void {
    const term: *Term = @ptrCast(@alignCast(data orelse return));
    const gpa = term.gpa;
    if (term.grace_source != 0) {
        _ = gtk.g_source_remove(term.grace_source);
        term.grace_source = 0;
    }
    if (term.poll_source != 0) {
        _ = gtk.g_source_remove(term.poll_source);
        term.poll_source = 0;
    }
    // A forced widget teardown (e.g. the user closed the run window) must still
    // resolve the run, or its entry would dangle in the cancel registry and the
    // core's pipeline thread would stay parked. Teardown paths that free the
    // ctx themselves disarm the callback first (disarmExit), so this can't
    // double-fire. -130 is the core's EXIT_CANCELLED sentinel: a teardown is
    // an interruption, and must not take the failed path (error cards, or a
    // half-done code session pushing a PR).
    if (!term.notified_exit) {
        term.notified_exit = true;
        if (term.on_exit) |cb| cb(term.on_exit_ctx, -130);
    }
    if (term.surface != null) {
        g.ghostty_surface_free(term.surface);
        term.surface = null;
    }
    if (term.cwd_z) |c| gpa.free(c);
    if (term.command_z) |c| gpa.free(c);
    for (term.env_keys.items) |k| gpa.free(k);
    for (term.env_vals.items) |v| gpa.free(v);
    term.env_keys.deinit(gpa);
    term.env_vals.deinit(gpa);
    term.env_c.deinit(gpa);
    gpa.destroy(term);
}

// --- GL lifecycle ---

const GError = extern struct { domain: u32, code: c_int, message: ?[*:0]const u8 };

// Build the GL context for the area: desktop GL (not GLES), 4.3 core. Returning
// a context here makes GtkGLArea adopt it (it realizes it itself — we must not).
fn onCreateContext(area: gtk.Object, _: gtk.gpointer) callconv(.c) gtk.Object {
    const native = gtk.gtk_widget_get_native(area);
    if (native == null) return null;
    const surface = gtk.gtk_native_get_surface(native);
    if (surface == null) return null;
    var err: ?*anyopaque = null;
    const ctx = gtk.gdk_surface_create_gl_context(surface, &err);
    if (ctx == null) {
        if (err) |e| {
            const ge: *const GError = @ptrCast(@alignCast(e));
            std.log.err("ghostty terminal: create_gl_context failed: {s}", .{ge.message orelse "(no message)"});
        }
        return null;
    }
    // Force desktop GL (not GLES); let the driver pick the version — NVIDIA's
    // default core context is 4.6, which satisfies ghostty's ≥4.3 requirement.
    // Pinning required_version(4,3) makes realize fail ("fb setup not supported").
    gtk.gdk_gl_context_set_allowed_apis(ctx, gtk.GDK_GL_API_GL);
    return ctx;
}

fn onRealize(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const term: *Term = @ptrCast(@alignCast(data orelse return));
    gtk.gtk_gl_area_make_current(term.area);
    if (gtk.gtk_gl_area_get_error(term.area)) |err| {
        const ge: *const GError = @ptrCast(@alignCast(err));
        std.log.err("ghostty terminal: GL context error on realize: {s}", .{ge.message orelse "(no message)"});
        return;
    }
    if (term.surface != null) g.ghostty_surface_display_realized(term.surface);
}

fn onResize(_: gtk.Object, width: c_int, height: c_int, data: gtk.gpointer) callconv(.c) void {
    const term: *Term = @ptrCast(@alignCast(data orelse return));
    if (width <= 0 or height <= 0) return;

    if (!term.created) {
        term.created = true;
        createSurface(term);
        _ = gtk.gtk_widget_grab_focus(term.area);
    }
    if (term.surface != null) {
        const scale: f64 = @floatFromInt(gtk.gtk_widget_get_scale_factor(term.area));
        g.ghostty_surface_set_content_scale(term.surface, scale, scale);
        // GTK4's resize passes physical pixels already — don't rescale.
        g.ghostty_surface_set_size(term.surface, @intCast(width), @intCast(height));
    }
}

fn onRender(_: gtk.Object, _: gtk.Object, data: gtk.gpointer) callconv(.c) c_int {
    const term: *Term = @ptrCast(@alignCast(data orelse return 1));
    const scale = gtk.gtk_widget_get_scale_factor(term.area);
    const w = gtk.gtk_widget_get_width(term.area) * scale;
    const h = gtk.gtk_widget_get_height(term.area) * scale;
    g.glViewport(0, 0, w, h);
    if (term.grace or term.surface == null) {
        // Dark zinc background during the grace period (matches the app theme).
        g.glClearColor(0.07, 0.07, 0.09, 1.0);
        g.glClear(g.GL_COLOR_BUFFER_BIT);
    } else {
        g.ghostty_surface_draw(term.surface);
    }
    return 1; // GDK_EVENT_STOP
}

fn createSurface(term: *Term) void {
    if (g_app == null or term.surface != null) return;
    // ghostty's surfaceInit loads GLAD from the current GL context.
    gtk.gtk_gl_area_make_current(term.area);

    // Build the env_var_s array from owned strings.
    term.env_c.clearRetainingCapacity();
    const n = @min(term.env_keys.items.len, term.env_vals.items.len);
    for (0..n) |i| {
        term.env_c.append(term.gpa, .{
            .key = term.env_keys.items[i].ptr,
            .value = term.env_vals.items[i].ptr,
        }) catch {};
    }

    var config = std.mem.zeroes(g.surface_config_s);
    config.platform_tag = g.PLATFORM_LINUX;
    config.platform = .{ .linux = .{ .gl_area = term.area } };
    config.userdata = term;
    config.scale_factor = @floatFromInt(gtk.gtk_widget_get_scale_factor(term.area));
    config.working_directory = if (term.cwd_z) |c| c.ptr else null;
    config.command = if (term.command_z) |c| c.ptr else null;
    config.wait_after_command = true;
    config.context = g.SURFACE_CONTEXT_WINDOW;
    config.io_mode = g.SURFACE_IO_EXEC;
    if (term.env_c.items.len > 0) {
        config.env_vars = term.env_c.items.ptr;
        config.env_var_count = term.env_c.items.len;
    }

    const surface = g.ghostty_surface_new(g_app, &config);
    if (surface == null) {
        std.log.err("ghostty_surface_new returned null", .{});
        return;
    }
    term.surface = surface;

    // Grace period: paint the background (not the half-initialized prompt) for
    // a beat, then enable real content. Avoids a flash of mispositioned text.
    gtk.gtk_gl_area_set_auto_render(term.area, 0);
    term.grace_source = gtk.g_timeout_add(200, endGrace, term);

    // Poll for child-process exit. ghostty doesn't emit a child-exited action
    // when wait_after_command is set, so we poll ghostty_surface_process_exited
    // (true once the child is gone; by then the run wrapper has flushed its
    // capture files) and fire on_exit once. The window stays open for inspection.
    if (term.on_exit != null) term.poll_source = gtk.g_timeout_add(250, pollExit, term);
}

fn pollExit(data: gtk.gpointer) callconv(.c) c_int {
    const term: *Term = @ptrCast(@alignCast(data orelse return 0));
    if (term.surface == null or term.notified_exit) {
        term.poll_source = 0;
        return 0; // stop
    }
    if (g.ghostty_surface_process_exited(term.surface)) {
        term.poll_source = 0;
        term.notified_exit = true;
        if (term.on_exit) |cb| cb(term.on_exit_ctx, 0);
        return 0; // stop polling
    }
    return 1; // keep polling
}

fn endGrace(data: gtk.gpointer) callconv(.c) c_int {
    const term: *Term = @ptrCast(@alignCast(data orelse return 0));
    term.grace_source = 0;
    term.grace = false;
    gtk.gtk_gl_area_set_auto_render(term.area, 1);
    gtk.gtk_widget_queue_draw(term.area);
    return 0; // fire once
}

/// Disarm a terminal's exit callback before a forced teardown (cancel, core
/// stop). The teardown path frees the callback ctx itself, so a late exit
/// notification must never fire into it. Main thread only; safe to call on a
/// widget whose Term data was already finalized (g_object_get_data → null).
pub fn disarmExit(area: gtk.Object) void {
    const data = gtk.g_object_get_data(area, "exp-term") orelse return;
    const term: *Term = @ptrCast(@alignCast(data));
    term.notified_exit = true;
    term.on_exit = null;
    term.on_exit_ctx = null;
}

// --- input forwarding ---

fn setupInput(term: *Term) void {
    const key = gtk.gtk_event_controller_key_new();
    _ = gtk.g_signal_connect_data(key, "key-pressed", @ptrCast(&onKeyPressed), term, null, 0);
    _ = gtk.g_signal_connect_data(key, "key-released", @ptrCast(&onKeyReleased), term, null, 0);
    gtk.gtk_widget_add_controller(term.area, key);

    const click = gtk.gtk_gesture_click_new();
    gtk.gtk_gesture_single_set_button(click, 0); // all buttons
    _ = gtk.g_signal_connect_data(click, "pressed", @ptrCast(&onClickPressed), term, null, 0);
    _ = gtk.g_signal_connect_data(click, "released", @ptrCast(&onClickReleased), term, null, 0);
    gtk.gtk_widget_add_controller(term.area, click);

    const motion = gtk.gtk_event_controller_motion_new();
    _ = gtk.g_signal_connect_data(motion, "motion", @ptrCast(&onMotion), term, null, 0);
    gtk.gtk_widget_add_controller(term.area, motion);

    const scroll = gtk.gtk_event_controller_scroll_new(gtk.SCROLL_BOTH_AXES | gtk.SCROLL_DISCRETE);
    _ = gtk.g_signal_connect_data(scroll, "scroll", @ptrCast(&onScroll), term, null, 0);
    gtk.gtk_widget_add_controller(term.area, scroll);

    const focus = gtk.gtk_event_controller_focus_new();
    _ = gtk.g_signal_connect_data(focus, "enter", @ptrCast(&onFocusEnter), term, null, 0);
    _ = gtk.g_signal_connect_data(focus, "leave", @ptrCast(&onFocusLeave), term, null, 0);
    gtk.gtk_widget_add_controller(term.area, focus);
}

fn gdkModsToGhostty(state: c_uint) c_int {
    var mods: c_int = g.MODS_NONE;
    if (state & GDK_SHIFT_MASK != 0) mods |= g.MODS_SHIFT;
    if (state & GDK_CONTROL_MASK != 0) mods |= g.MODS_CTRL;
    if (state & GDK_ALT_MASK != 0) mods |= g.MODS_ALT;
    if (state & GDK_SUPER_MASK != 0) mods |= g.MODS_SUPER;
    if (state & GDK_LOCK_MASK != 0) mods |= g.MODS_CAPS;
    return mods;
}

fn sendKey(term: *Term, keyval: c_uint, keycode: c_uint, state: c_uint, action: c_int) bool {
    if (term.surface == null) return false;
    const mods = gdkModsToGhostty(state);

    // Printable text for this key (committed text). Control chars (< space)
    // are left to ghostty to derive from keycode+mods.
    var text_buf: [8]u8 = undefined;
    var text_ptr: ?[*:0]const u8 = null;
    if (action == g.ACTION_PRESS) {
        const cp = gdk_keyval_to_unicode(keyval);
        if (cp >= 0x20 and cp != 0x7f) {
            if (std.unicode.utf8Encode(@intCast(cp), text_buf[0..])) |len| {
                text_buf[len] = 0;
                text_ptr = @ptrCast(text_buf[0..len :0].ptr);
            } else |_| {}
        }
    }

    const unshifted = gdk_keyval_to_unicode(gdk_keyval_to_lower(keyval));

    const ev = g.input_key_s{
        .action = action,
        .mods = mods,
        .consumed_mods = mods,
        .keycode = keycode,
        .text = text_ptr,
        .unshifted_codepoint = unshifted,
        .composing = false,
    };
    return g.ghostty_surface_key(term.surface, ev);
}

fn onKeyPressed(_: gtk.Object, keyval: c_uint, keycode: c_uint, state: c_uint, data: gtk.gpointer) callconv(.c) c_int {
    const term: *Term = @ptrCast(@alignCast(data orelse return 0));
    return if (sendKey(term, keyval, keycode, state, g.ACTION_PRESS)) 1 else 0;
}

fn onKeyReleased(_: gtk.Object, keyval: c_uint, keycode: c_uint, state: c_uint, data: gtk.gpointer) callconv(.c) void {
    const term: *Term = @ptrCast(@alignCast(data orelse return));
    _ = sendKey(term, keyval, keycode, state, g.ACTION_RELEASE);
}

fn ghosttyButton(button: c_uint) c_int {
    return switch (button) {
        1 => g.MOUSE_LEFT,
        2 => g.MOUSE_MIDDLE,
        3 => g.MOUSE_RIGHT,
        else => g.MOUSE_UNKNOWN,
    };
}

fn onClickPressed(gesture: gtk.Object, _: c_int, _: f64, _: f64, data: gtk.gpointer) callconv(.c) void {
    const term: *Term = @ptrCast(@alignCast(data orelse return));
    _ = gtk.gtk_widget_grab_focus(term.area);
    if (term.surface == null) return;
    const btn = ghosttyButton(gtk.gtk_gesture_single_get_current_button(gesture));
    _ = g.ghostty_surface_mouse_button(term.surface, g.MOUSE_PRESS, btn, 0);
}

fn onClickReleased(gesture: gtk.Object, _: c_int, _: f64, _: f64, data: gtk.gpointer) callconv(.c) void {
    const term: *Term = @ptrCast(@alignCast(data orelse return));
    if (term.surface == null) return;
    const btn = ghosttyButton(gtk.gtk_gesture_single_get_current_button(gesture));
    _ = g.ghostty_surface_mouse_button(term.surface, g.MOUSE_RELEASE, btn, 0);
}

fn onMotion(_: gtk.Object, x: f64, y: f64, data: gtk.gpointer) callconv(.c) void {
    const term: *Term = @ptrCast(@alignCast(data orelse return));
    if (term.surface == null) return;
    g.ghostty_surface_mouse_pos(term.surface, x, y, 0);
}

fn onScroll(_: gtk.Object, dx: f64, dy: f64, data: gtk.gpointer) callconv(.c) c_int {
    const term: *Term = @ptrCast(@alignCast(data orelse return 1));
    if (term.surface == null) return 1;
    // Ghostty wants +up/+right; GTK delivers the inverse.
    g.ghostty_surface_mouse_scroll(term.surface, -dx, -dy, 0);
    return 1;
}

fn onFocusEnter(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const term: *Term = @ptrCast(@alignCast(data orelse return));
    if (term.surface != null) g.ghostty_surface_set_focus(term.surface, true);
}

fn onFocusLeave(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const term: *Term = @ptrCast(@alignCast(data orelse return));
    if (term.surface != null) g.ghostty_surface_set_focus(term.surface, false);
}

// -------------------------------------------------------------------------
// Smoke: a standalone window hosting one embedded terminal running a shell.
// Invoked via `exponential term-smoke` to visually verify M7 rendering/input.
// -------------------------------------------------------------------------

pub fn runSmoke(gpa: std.mem.Allocator) u8 {
    _ = gpa;
    const app = gtk.adw_application_new("at.exponential.desktop.termsmoke", gtk.APP_DEFAULT_FLAGS);
    _ = gtk.g_signal_connect_data(app, "activate", @ptrCast(&onSmokeActivate), null, null, 0);
    const status = gtk.g_application_run(app, 0, null);
    gtk.g_object_unref(app);
    return @intCast(status);
}

fn onSmokeActivate(app: gtk.Object, _: gtk.gpointer) callconv(.c) void {
    const win = gtk.adw_application_window_new(app);
    gtk.gtk_window_set_title(win, "ghostty terminal smoke");
    gtk.gtk_window_set_default_size(win, 900, 600);
    if (create(std.heap.page_allocator, .{})) |term| {
        gtk.adw_application_window_set_content(win, term);
    }
    gtk.gtk_window_present(win);
}
