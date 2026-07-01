//! Linux preview runtime — the twin of macOS `MacPreviewController`. Builds +
//! runs the selected named run target locally and embeds the live app in the
//! dedicated preview pane (preview_panel.zig). Everything runs on the developer's
//! machine; nothing executes on the cloud.
//!
//! HARD RULES (inherited from terminal.zig):
//!   - GTK is touched ONLY on the main thread. Builds/boots/health-polls run on
//!     worker threads and marshal results back via the thread-safe `g_idle_add`.
//!   - The controller OWNS every child it spawns (dev server, emulator) and runs
//!     an idempotent, ordered `stop()` on pane close / project switch / target
//!     switch / quit: detach the embed (un-reparent) BEFORE killing the emulator,
//!     then SIGTERM → hard kill, then free the well-known ports (5173/8554/3100).
//!   - Never mount the embed surface at 0×0 — the panel gives it a nonzero min
//!     size, matching the terminal dock's lazy-size discipline.
//!
//! Backends dispatch on `target.platform`:
//!   - web     → WebKitGTK 6 webview at the dev URL (libcurl health-poll first);
//!               falls back to "open in browser" when webkitgtk-6.0 is absent.
//!   - android → boot the local SDK emulator, install + launch the APK, and
//!               reparent the emulator's X11 window into the panel (XWayland ok;
//!               native Wayland falls back to a streamed surface — TODO stub).
//!   - ios     → "needs a Mac" empty state (local iOS emulation is impossible on
//!               Linux; the remote-Mac tunnel is deferred).

const std = @import("std");
const gtk = @import("../gtk.zig");
const build_options = @import("build_options");
const storage = @import("../../core/storage.zig");
const http = @import("../../core/api/http.zig");
const cfg = @import("preview_config.zig");
const panel = @import("preview_panel.zig");
const annotation_overlay = @import("annotation_overlay.zig");


// std.process.run/spawn must read the child's stdout AND stderr while it runs,
// which needs a CONCURRENT Io. The app-wide `global_single_threaded` Io has no
// worker threads and spuriously fails subprocess capture with error.OutOfMemory
// (confirmed). So lazily init ONE multi-threaded Threaded instance (cpu_count-1
// workers) and reuse it for every subprocess call here. Thread-safe — the
// android/health/stream/input workers all call this off the main thread.
// Swap-based spinlock (0.16's std.Io.Mutex needs an Io handle; std.Thread.Mutex
// is gone) — the same idiom as database.zig. Only contended once, at first init.
var preview_io_lock: std.atomic.Value(bool) = std.atomic.Value(bool).init(false);
var preview_io_instance: ?*std.Io.Threaded = null;
const io = struct {
    fn get() std.Io {
        while (preview_io_lock.swap(true, .acquire)) std.atomic.spinLoopHint();
        defer preview_io_lock.store(false, .release);
        if (preview_io_instance == null) {
            const t = std.heap.page_allocator.create(std.Io.Threaded) catch
                return std.Io.Threaded.global_single_threaded.io();
            t.* = std.Io.Threaded.init(std.heap.page_allocator, .{});
            preview_io_instance = t;
        }
        return preview_io_instance.?.io();
    }
};

// Zig 0.16 removed std.Thread.sleep (sleeps now need an Io); the worker threads
// here just need a plain blocking sleep, so call libc nanosleep directly — the
// same idiom the sync engine uses (core/electric/shape_client.zig).
const timespec = extern struct { sec: isize, nsec: isize };
extern fn nanosleep(req: *const timespec, rem: ?*timespec) c_int;
fn sleepMs(ms: u64) void {
    var ts = timespec{ .sec = @intCast(ms / 1000), .nsec = @intCast((ms % 1000) * 1_000_000) };
    _ = nanosleep(&ts, null);
}

/// The Android SDK ships `emulator` under `$ANDROID_HOME/emulator` and `adb`
/// under `platform-tools`; neither is reliably on PATH. Prepend the SDK dirs to
/// the env map we hand spawned children (`storage.environ()`), so the bare
/// `emulator`/`adb` spawns resolve. Idempotent + no-op when the SDK root isn't set.
var android_path_done: bool = false;
fn ensureAndroidPath() void {
    if (android_path_done) return;
    android_path_done = true;
    const home = std.c.getenv("ANDROID_HOME") orelse std.c.getenv("ANDROID_SDK_ROOT") orelse return;
    const home_s = std.mem.span(home);
    const map = storage.environ() orelse return;
    const cur = map.get("PATH") orelse "";
    const new_path = std.fmt.allocPrint(std.heap.page_allocator, "{s}/emulator:{s}/platform-tools:{s}/cmdline-tools/latest/bin:{s}", .{ home_s, home_s, home_s, cur }) catch return;
    defer std.heap.page_allocator.free(new_path);
    map.put("PATH", new_path) catch {};
}

const RunResult = struct { term: std.process.Child.Term, stdout: []u8, stderr: []u8 };

/// Run a command to completion, capturing stdout/stderr. Wraps the 0.16
/// `std.process.run(gpa, io, …)` signature so call sites read cleanly. Caller
/// frees stdout/stderr.
fn runCapture(gpa: std.mem.Allocator, argv: []const []const u8, cwd: ?[]const u8) ?RunResult {
    const res = std.process.run(gpa, io.get(), .{
        .argv = argv,
        .cwd = if (cwd) |c| .{ .path = c } else .inherit,
        .environ_map = storage.environ(),
        .stdout_limit = .limited(1 << 20),
        .stderr_limit = .limited(1 << 20),
    }) catch return null;
    return .{ .term = res.term, .stdout = res.stdout, .stderr = res.stderr };
}

fn termOk(term: std.process.Child.Term) bool {
    return term == .exited and term.exited == 0;
}

/// Well-known localhost ports the backends bind. Freed on teardown so a stale
/// owner from a crashed prior run is reclaimed before respawn.
const web_port_default: u16 = 5173;
const emulator_grpc_port: u16 = 8554;
const ios_serve_port: u16 = 3100; // reserved for the future remote-Mac tunnel

pub const Phase = enum {
    idle,
    doctor,
    setup,
    building,
    booting,
    installing,
    launching,
    running,
    err,
    needs_mac,

    pub fn label(self: Phase) [*:0]const u8 {
        return switch (self) {
            .idle => "Idle",
            .doctor => "Checking tools…",
            .setup => "Setting up…",
            .building => "Building…",
            .booting => "Booting emulator…",
            .installing => "Installing…",
            .launching => "Launching…",
            .running => "Running",
            .err => "Error",
            .needs_mac => "Needs a Mac",
        };
    }
};

/// Owns the lifecycle of a single active preview. There is at most one live
/// PreviewController per app (the pane hosts one preview), enforced by the
/// caller (app.zig) calling `stop` before `start`.
pub const PreviewController = struct {
    gpa: std.mem.Allocator,
    pane: *panel.PreviewPanel,

    // Server-derived context for the report path (filed against the feedback
    // project, or the previewed project). gpa-owned, refreshed on start.
    instance: []u8,
    token: ?[]u8 = null,

    phase: Phase = .idle,

    // The active target (deep-copied into `arena` so it outlives the parsed
    // config). null when idle.
    arena: ?std.heap.ArenaAllocator = null,
    platform: ?cfg.Platform = null,
    repo_slug: ?[]u8 = null, // gpa-owned, for messages
    root_dir: ?[]u8 = null, // gpa-owned absolute working dir of the target

    // Owned children (the controller kills them on stop). null when not running.
    run_child: ?std.process.Child = null, // web dev server
    emulator_child: ?std.process.Child = null, // android emulator

    // Web backend.
    web_url: ?[]u8 = null, // gpa-owned dev URL
    web_view: gtk.Object = null, // WebKitWebView (when embedded)

    // Android backend: the reparented top-level window + the display we
    // reparented on (so teardown can un-reparent before killing the emulator).
    reparented_xid: gtk.XID = 0,
    x_display: ?*anyopaque = null,
    embed_resize_connected: bool = false,

    // Android Wayland fallback: stream the emulator into a GtkPicture (no
    // reparent possible on native Wayland) and forward tap/swipe via `adb shell
    // input`. `stream_w/h` track the latest frame's pixel dims for coord mapping.
    android_streamed: bool = false,
    stream_picture: gtk.Object = null,
    stream_w: c_int = 0,
    stream_h: c_int = 0,
    drag_sx: f64 = 0,
    drag_sy: f64 = 0,

    // A worker is in flight; stop() flips this so a late g_idle_add result drops.
    cancel: std.atomic.Value(bool) = std.atomic.Value(bool).init(false),

    pub fn create(gpa: std.mem.Allocator, pane: *panel.PreviewPanel, instance: []const u8, token: ?[]const u8) ?*PreviewController {
        const self = gpa.create(PreviewController) catch return null;
        self.* = .{
            .gpa = gpa,
            .pane = pane,
            .instance = gpa.dupe(u8, instance) catch {
                gpa.destroy(self);
                return null;
            },
            .token = if (token) |t| (gpa.dupe(u8, t) catch null) else null,
        };
        return self;
    }

    pub fn destroy(self: *PreviewController) void {
        self.stop();
        self.gpa.free(self.instance);
        if (self.token) |t| self.gpa.free(t);
        self.gpa.destroy(self);
    }

    /// Capture the current preview frame and hand it to the annotation overlay as
    /// the base image, plus wire the report context (instance/token/project). The
    /// frame source is per-platform:
    ///   - android → `adb exec-out screencap -p` (pixel-exact PNG over stdout)
    ///   - web     → a webview snapshot is deferred (the web path keeps the JS
    ///               widget; the native overlay is only the fallback) — we show a
    ///               message and annotate with no base.
    /// `feedback_project_id` (from the DB mirror) routes the issue; null files
    /// against the previewed project. Main thread only.
    pub fn prepareAnnotation(self: *PreviewController, report_project_id: []const u8) void {
        const oc = self.pane.overlayCanvas() orelse return;
        oc.setReportContext(self.instance, self.token, report_project_id);
        switch (self.platform orelse return) {
            .android => self.captureAndroidFrame(oc),
            .web => self.pane.setMessage("Annotating the web preview: draw freely (frame snapshot is the JS widget's job; the native overlay flattens your strokes over a blank canvas as a fallback)."),
            .ios => {},
        }
    }

    /// `adb exec-out screencap -p` → temp PNG → GdkPixbuf → overlay base.
    fn captureAndroidFrame(self: *PreviewController, oc: *annotation_overlay.AnnotationOverlay) void {
        const dir = storage.configDir(self.gpa) catch return;
        defer self.gpa.free(dir);
        const path = std.fmt.allocPrint(self.gpa, "{s}/preview-frame.png", .{dir}) catch return;
        defer self.gpa.free(path);
        // screencap writes the PNG to stdout; capture it and write to the file
        // (CRLF translation isn't an issue with exec-out, unlike plain `shell`).
        const res = runCapture(self.gpa, &.{ "adb", "exec-out", "screencap", "-p" }, null) orelse {
            self.pane.setMessage("Couldn't capture an emulator frame (adb screencap failed).");
            return;
        };
        defer self.gpa.free(res.stdout);
        defer self.gpa.free(res.stderr);
        if (!termOk(res.term) or res.stdout.len == 0) {
            self.pane.setMessage("Couldn't capture an emulator frame.");
            return;
        }
        storage.writeSecret(path, res.stdout) catch return;
        const path_z = self.gpa.dupeZ(u8, path) catch return;
        defer self.gpa.free(path_z);
        const pixbuf = gtk.gdk_pixbuf_new_from_file(path_z.ptr, null);
        if (pixbuf == null) return;
        defer gtk.g_object_unref(pixbuf);
        oc.setBase(pixbuf);
    }

    /// Refresh the auth context when the active account changes.
    pub fn setAuth(self: *PreviewController, instance: []const u8, token: ?[]const u8) void {
        const new_inst = self.gpa.dupe(u8, instance) catch return;
        self.gpa.free(self.instance);
        self.instance = new_inst;
        if (self.token) |t| self.gpa.free(t);
        self.token = if (token) |t| (self.gpa.dupe(u8, t) catch null) else null;
    }

    // --- phase + UI helpers (main thread only) ---

    fn setPhase(self: *PreviewController, phase: Phase) void {
        self.phase = phase;
        self.pane.setStatus(phase.label());
    }

    fn fail(self: *PreviewController, comptime msg: []const u8) void {
        self.setPhase(.err);
        self.pane.setMessage(msg);
        std.log.warn("[preview] {s}", .{msg});
    }

    /// Start previewing `target` from the repo cloned at `repo_slug`. Runs the
    /// trust gate (commands come from the working-tree repo file) then dispatches
    /// by platform. Idempotent: stops any prior preview first.
    pub fn start(self: *PreviewController, repo_slug: []const u8, target: cfg.RunTarget) void {
        self.stop();
        self.cancel.store(false, .seq_cst);

        // Own a copy of the target + its strings for the run's lifetime.
        var arena = std.heap.ArenaAllocator.init(self.gpa);
        const a = arena.allocator();
        const owned = dupeTarget(a, target) catch {
            arena.deinit();
            self.fail("out of memory preparing the preview");
            return;
        };
        self.arena = arena;
        self.platform = owned.platform;
        self.repo_slug = self.gpa.dupe(u8, repo_slug) catch null;

        switch (owned.platform) {
            .ios => {
                self.setPhase(.needs_mac);
                self.pane.showNeedsMac();
                return;
            },
            .web => self.startWeb(repo_slug, owned),
            .android => self.startAndroid(repo_slug, owned),
        }
    }

    // =====================================================================
    // web backend — WebKitGTK 6 at the dev URL (libcurl health-poll first).
    // =====================================================================

    fn startWeb(self: *PreviewController, repo_slug: []const u8, target: cfg.RunTarget) void {
        const root = self.resolveRootDir(repo_slug, target.root_dir) orelse {
            self.fail("couldn't resolve the target's rootDir under the clone");
            return;
        };
        self.root_dir = self.gpa.dupe(u8, root) catch null;

        // Dev URL: explicit `url`, else http://localhost:<port|5173>.
        const port: u16 = if (target.port) |p| @intCast(p) else web_port_default;
        const url = if (target.url) |u|
            (self.gpa.dupe(u8, u) catch return)
        else
            (std.fmt.allocPrint(self.gpa, "http://localhost:{d}", .{port}) catch return);
        self.web_url = url;

        // setup (optional) + run command, both in the target's rootDir. We run
        // the dev server as an OWNED child so teardown can kill it + free the
        // port. Output is piped and logged (a full terminal-dock tee is the
        // agent-run path; the dev-server log is informational here).
        self.setPhase(.setup);
        if (!self.runSetup(root, target.setup)) return; // runSetup surfaced the error
        self.setPhase(.building);
        const run_cmd = target.run orelse {
            self.fail("web target has no `run` command");
            return;
        };
        self.run_child = self.spawnShell(root, run_cmd) catch {
            self.fail("couldn't start the dev server");
            return;
        };

        // Health-poll the URL off-thread, then load it in the webview.
        self.setPhase(.launching);
        self.spawnHealthPoll(url);
    }

    /// Worker: poll GET url+readyPath until 2xx (≈300ms cadence, 60s budget),
    /// then marshal back to mount the webview. Uses the existing libcurl client.
    fn spawnHealthPoll(self: *PreviewController, url: []const u8) void {
        const job = self.gpa.create(HealthJob) catch return;
        job.* = .{ .gpa = self.gpa, .ctrl = self, .url = self.gpa.dupe(u8, url) catch {
            self.gpa.destroy(job);
            return;
        } };
        const th = std.Thread.spawn(.{}, healthWorker, .{job}) catch {
            healthWorker(job);
            return;
        };
        th.detach();
    }

    fn mountWeb(self: *PreviewController, reachable: bool) void {
        if (self.cancel.load(.seq_cst)) return;
        const url = self.web_url orelse return;
        // Comptime-gate (not a runtime check) so an -Dwebkit=false build never
        // emits references to the unlinked webkit_web_view_* symbols below — the
        // same discipline embedEmulatorWindow uses for the X11 reparent symbols.
        if (comptime !build_options.enable_webkit) {
            // No embedded webview compiled in: open the dev URL in the browser.
            self.openInBrowser(url);
            self.setPhase(.running);
            self.pane.setMessage("WebKitGTK 6 not available — opened the preview in your browser.");
            return;
        }
        if (!reachable) {
            self.fail("dev server didn't become reachable in time");
            return;
        }
        const view = gtk.webkit_web_view_new();
        if (view == null) {
            self.openInBrowser(url);
            self.setPhase(.running);
            return;
        }
        self.web_view = view;
        const url_z = self.gpa.dupeZ(u8, url) catch return;
        defer self.gpa.free(url_z);
        gtk.webkit_web_view_load_uri(view, url_z.ptr);
        self.pane.mountSurface(view);
        self.setPhase(.running);
    }

    fn openInBrowser(self: *PreviewController, url: []const u8) void {
        const z = std.fmt.allocPrintSentinel(self.gpa, "{s}", .{url}, 0) catch return;
        defer self.gpa.free(z);
        _ = gtk.g_app_info_launch_default_for_uri(z.ptr, null, null);
    }

    // =====================================================================
    // android backend — boot emulator, install+launch APK, X11 reparent.
    // =====================================================================

    fn startAndroid(self: *PreviewController, repo_slug: []const u8, target: cfg.RunTarget) void {
        const root = self.resolveRootDir(repo_slug, target.root_dir) orelse {
            self.fail("couldn't resolve the target's rootDir under the clone");
            return;
        };
        self.root_dir = self.gpa.dupe(u8, root) catch null;

        const avd = target.avd orelse {
            self.fail("android target has no `avd`");
            return;
        };

        // Ensure the SDK's `emulator`/`adb` resolve (prepend the SDK dirs to PATH).
        ensureAndroidPath();

        // Embed strategy: GTK4 renders every widget into ONE window surface and
        // gives widgets no native X11 sub-window, so there's nothing to reparent
        // a foreign emulator window INTO — a reparent lands at the top-level
        // origin and mispositions off-screen (GTK dropped GtkSocket for this very
        // reason). So we stream the framebuffer into a GtkPicture (a normal
        // widget that embeds cleanly) and forward tap/swipe via `adb shell input`.
        // Works identically on X11 and Wayland.
        self.android_streamed = true;
        const pic = gtk.gtk_picture_new();
        gtk.gtk_picture_set_content_fit(pic, gtk.CONTENT_FIT_CONTAIN);
        self.stream_picture = pic;
        attachStreamInput(self, pic);
        self.pane.mountSurface(pic);

        // 1) Build the APK (owned, blocking on a worker — long).
        self.setPhase(.setup);
        if (!self.runSetup(root, target.setup)) return; // runSetup surfaced the error

        // The remaining boot/build/install/launch chain is long-running and must
        // not block the GTK main loop, so hand it to a worker that marshals each
        // phase transition back via g_idle_add.
        const job = self.gpa.create(AndroidJob) catch {
            self.fail("out of memory");
            return;
        };
        job.* = .{
            .gpa = self.gpa,
            .ctrl = self,
            .root = self.gpa.dupe(u8, root) catch "",
            .avd = self.gpa.dupe(u8, avd) catch "",
            .build_cmd = if (target.build) |b| (self.gpa.dupe(u8, b) catch null) else null,
            .apk = if (target.apk) |p| (self.gpa.dupe(u8, p) catch null) else null,
            .install_cmd = if (target.install_command) |c| (self.gpa.dupe(u8, c) catch null) else null,
            .application_id = if (target.application_id) |i| (self.gpa.dupe(u8, i) catch null) else null,
            .activity = if (target.activity) |c| (self.gpa.dupe(u8, c) catch null) else null,
        };
        const th = std.Thread.spawn(.{}, androidWorker, .{job}) catch {
            androidWorker(job);
            return;
        };
        th.detach();
    }

    // =====================================================================
    // shared helpers
    // =====================================================================

    /// Absolute working dir = <clone>/<rootDir>. Rejects `..` traversal in
    /// rootDir (the server also strips it, but defend locally too).
    fn resolveRootDir(self: *PreviewController, repo_slug: []const u8, root_dir: ?[]const u8) ?[]const u8 {
        const a = self.arena.?.allocator();
        const clone = cfg.repoCloneDir(a, repo_slug) catch return null;
        const rd = root_dir orelse return clone;
        if (std.mem.indexOf(u8, rd, "..") != null) return null;
        return std.fs.path.join(a, &.{ clone, rd }) catch null;
    }

    /// Run an optional setup command synchronously (blocking; called from the
    /// main thread for short setups, or the worker for android). Returns false
    /// only on a non-zero exit. A null command is a success no-op.
    fn runSetup(self: *PreviewController, root: []const u8, setup: ?[]const u8) bool {
        const command = setup orelse return true;
        if (command.len == 0) return true;
        const res = runCapture(self.gpa, &.{ "/usr/bin/env", "bash", "-lc", command }, root) orelse {
            self.fail("setup couldn't start (failed to spawn a shell)");
            return false;
        };
        defer self.gpa.free(res.stdout);
        defer self.gpa.free(res.stderr);
        if (termOk(res.term)) return true;
        // Surface the tail of the command's output so the failure is actionable
        // instead of a generic "setup failed".
        const detail = if (res.stderr.len > 0) res.stderr else res.stdout;
        const trimmed = std.mem.trim(u8, detail, " \t\r\n");
        const tail = trimmed[trimmed.len -| 280 ..];
        self.setPhase(.err);
        if (std.fmt.allocPrint(self.gpa, "Setup `{s}` failed: {s}", .{ command, tail })) |msg| {
            defer self.gpa.free(msg);
            self.pane.setMessage(msg);
        } else |_| self.pane.setMessage("Setup command failed.");
        return false;
    }

    /// Spawn a long-running shell command as an owned child in `root` (the dev
    /// server). Output is dropped here — visible build/run logs are the agent-run
    /// terminal-dock path; the dev-server's own console is informational. The
    /// controller already runs behind the repo trust prompt.
    fn spawnShell(self: *PreviewController, root: []const u8, command: []const u8) !std.process.Child {
        _ = self;
        return std.process.spawn(io.get(), .{
            .argv = &.{ "/usr/bin/env", "bash", "-lc", command },
            .cwd = .{ .path = root },
            .environ_map = storage.environ(),
            .stdout = .ignore,
            .stderr = .ignore,
        });
    }

    // =====================================================================
    // ordered teardown — no orphans, free the well-known ports.
    // =====================================================================

    /// Idempotent. Order matters: detach the embed (un-reparent the emulator
    /// window) BEFORE killing the emulator, so GTK never holds a dangling child
    /// XID; then graceful-kill children, then free the ports.
    pub fn stop(self: *PreviewController) void {
        self.cancel.store(true, .seq_cst);

        // 1) Detach the embed. For android, un-reparent the emulator window back
        // to the root so destroying our panel surface doesn't kill it before we
        // ask the emulator to quit; for web, drop the webview. The X11 path is
        // comptime-gated so an -Dx11=false build never references the Xlib symbols
        // (reparented_xid can only be set by the equally-gated embedEmulatorWindow).
        if (comptime build_options.enable_x11) {
            if (self.reparented_xid != 0 and self.x_display != null) {
                // Reparent back to the X root window (best-effort). Without this, a
                // GTK-side surface destroy could take the emulator window with it.
                const root_xid = xRootWindow(self.x_display.?);
                _ = gtk.XReparentWindow(self.x_display.?, self.reparented_xid, root_xid, 0, 0);
                _ = gtk.XFlush(self.x_display.?);
                self.reparented_xid = 0;
                self.x_display = null;
            }
        }
        if (self.web_view != null) {
            if (comptime build_options.enable_webkit) gtk.webkit_web_view_stop_loading(self.web_view);
            self.web_view = null;
        }
        // Wayland stream fallback: the cancel flag (set above) stops streamWorker
        // within one frame; drop our picture ref so a late frame is a no-op (the
        // widget itself is removed by clearSurface below).
        self.stream_picture = null;
        self.android_streamed = false;
        self.stream_w = 0;
        self.stream_h = 0;
        self.pane.clearSurface();

        // 2) Graceful then hard kill the android emulator (`adb emu kill`, then
        // SIGTERM via Child.kill). For the dev server, SIGTERM.
        if (self.emulator_child) |*child| {
            // Ask the emulator to quit cleanly first (frees the AVD lock).
            self.runDetached(&.{ "adb", "emu", "kill" });
            child.kill(io.get());
            self.emulator_child = null;
        }
        if (self.run_child) |*child| {
            child.kill(io.get());
            self.run_child = null;
        }

        // 3) Reclaim the well-known localhost ports a crashed prior owner may
        // still hold (best-effort fuser -k; localhost-bound services only).
        self.freePort(web_port_default);
        self.freePort(emulator_grpc_port);
        self.freePort(ios_serve_port);

        // 4) Drop the run's arena + owned bits.
        if (self.web_url) |u| {
            self.gpa.free(u);
            self.web_url = null;
        }
        if (self.root_dir) |r| {
            self.gpa.free(r);
            self.root_dir = null;
        }
        if (self.repo_slug) |r| {
            self.gpa.free(r);
            self.repo_slug = null;
        }
        if (self.arena) |*ar| {
            ar.deinit();
            self.arena = null;
        }
        self.platform = null;
        if (self.phase != .needs_mac) self.phase = .idle;
    }

    /// Best-effort `fuser -k <port>/tcp` to reclaim a stale localhost port owner.
    fn freePort(self: *PreviewController, port: u16) void {
        const spec = std.fmt.allocPrint(self.gpa, "{d}/tcp", .{port}) catch return;
        defer self.gpa.free(spec);
        self.runDetached(&.{ "fuser", "-k", spec });
    }

    /// Fire a short command and reap it (silent, best-effort). Used for the
    /// emulator-quit + port-free teardown helpers.
    fn runDetached(self: *PreviewController, argv: []const []const u8) void {
        const res = runCapture(self.gpa, argv, null) orelse return;
        self.gpa.free(res.stdout);
        self.gpa.free(res.stderr);
    }
};

// ---------------------------------------------------------------------------
// Target deep-copy (into the run arena)
// ---------------------------------------------------------------------------

fn dupeTarget(a: std.mem.Allocator, t: cfg.RunTarget) !cfg.RunTarget {
    return .{
        .id = try a.dupe(u8, t.id),
        .name = try a.dupe(u8, t.name),
        .platform = t.platform,
        .enabled = t.enabled,
        .root_dir = try dupeOpt(a, t.root_dir),
        .setup = try dupeOpt(a, t.setup),
        .run = try dupeOpt(a, t.run),
        .url = try dupeOpt(a, t.url),
        .port = t.port,
        .ready_path = try dupeOpt(a, t.ready_path),
        .inject_widget = t.inject_widget,
        .build = try dupeOpt(a, t.build),
        .apk = try dupeOpt(a, t.apk),
        .install_command = try dupeOpt(a, t.install_command),
        .avd = try dupeOpt(a, t.avd),
        .application_id = try dupeOpt(a, t.application_id),
        .activity = try dupeOpt(a, t.activity),
        .scheme = try dupeOpt(a, t.scheme),
        .workspace = try dupeOpt(a, t.workspace),
        .simulator = try dupeOpt(a, t.simulator),
        .bundle_id = try dupeOpt(a, t.bundle_id),
    };
}

fn dupeOpt(a: std.mem.Allocator, s: ?[]const u8) !?[]const u8 {
    return if (s) |v| try a.dupe(u8, v) else null;
}

// ---------------------------------------------------------------------------
// web health-poll worker
// ---------------------------------------------------------------------------

const HealthJob = struct {
    gpa: std.mem.Allocator,
    ctrl: *PreviewController,
    url: []u8,
    reachable: bool = false,
};

fn healthWorker(job: *HealthJob) void {
    defer _ = gtk.g_idle_add(@ptrCast(&onHealthDone), job);
    const url_z = std.fmt.allocPrintSentinel(job.gpa, "{s}", .{job.url}, 0) catch return;
    defer job.gpa.free(url_z);
    // ≈300ms cadence, ~180s budget (600 attempts) — vite's first cold bind on a
    // large app can take a while. ANY successful HTTP round-trip means the port
    // is up and serving: we deliberately accept any status (the shared client
    // sends `Accept: application/json`, which a dev SSR server may answer with a
    // 500 — that still proves it's listening). The webview then loads the URL
    // with normal browser headers and gets the real page. Stop early on cancel.
    var i: usize = 0;
    while (i < 600) : (i += 1) {
        if (job.ctrl.cancel.load(.seq_cst)) return;
        if (http.get(job.gpa, url_z, null, 5, null)) |resp| {
            var r = resp;
            r.deinit();
            job.reachable = true;
            return;
        } else |_| {}
        sleepMs(300);
    }
}

fn onHealthDone(data: gtk.gpointer) callconv(.c) c_int {
    const job: *HealthJob = @ptrCast(@alignCast(data orelse return 0));
    defer {
        job.gpa.free(job.url);
        job.gpa.destroy(job);
    }
    if (!job.ctrl.cancel.load(.seq_cst)) job.ctrl.mountWeb(job.reachable);
    return 0; // G_SOURCE_REMOVE
}

// ---------------------------------------------------------------------------
// android boot/build/install/launch worker
// ---------------------------------------------------------------------------

const AndroidJob = struct {
    gpa: std.mem.Allocator,
    ctrl: *PreviewController,
    root: []u8,
    avd: []u8,
    build_cmd: ?[]u8,
    apk: ?[]u8,
    install_cmd: ?[]u8,
    application_id: ?[]u8,
    activity: ?[]u8,
    // The emulator child stays worker-owned until the main thread adopts it in
    // onAndroidDone (avoids a cross-thread write to ctrl.emulator_child that
    // would race stop()). If a teardown (cancel) lands before hand-off, the
    // worker kills it itself.
    emulator: ?std.process.Child = null,
    // outcome → marshaled back
    ok: bool = false,
    message: ?[]u8 = null,
};

/// If the run was torn down (cancel) while the emulator is still worker-owned,
/// kill it here so it never orphans (the main thread's stop() can't see a
/// child that was never handed off). `force` kills regardless of the flag
/// (used on the bail paths that already decided to abort).
fn killEmulatorIfCancelled(job: *AndroidJob, force: bool) void {
    if (job.emulator) |*child| {
        if (force or job.ctrl.cancel.load(.seq_cst)) {
            child.kill(io.get());
            job.emulator = null;
        }
    }
}

fn androidWorker(job: *AndroidJob) void {
    defer _ = gtk.g_idle_add(@ptrCast(&onAndroidDone), job);
    const gpa = job.gpa;

    // 1) Build the APK.
    if (job.build_cmd) |b| {
        marshalPhase(job.ctrl, .building);
        if (!bashOk(gpa, job.root, b)) {
            job.message = gpa.dupe(u8, "the android build failed") catch null;
            return;
        }
    }
    if (job.ctrl.cancel.load(.seq_cst)) return;

    // 2) Boot the emulator (owned child) + wait sys.boot_completed==1. Resolve
    // the AVD first: if the configured name isn't installed, fall back to the
    // first available one so the preview works regardless of local AVD naming.
    const avd = resolveAvd(gpa, job.avd) orelse {
        job.message = gpa.dupe(u8, "no Android AVDs found — create one in Android Studio (Device Manager) or with `avdmanager create avd`") catch null;
        return;
    };
    defer gpa.free(avd);
    marshalPhase(job.ctrl, .booting);
    var emu_buf: [std.fs.max_path_bytes]u8 = undefined;
    // -no-window: run headless. We mirror the framebuffer into the pane via
    // `adb screencap`, so the emulator's own window (and its floating toolbar)
    // would just be redundant clutter. screencap + `adb input` work headless.
    const emu = std.process.spawn(io.get(), .{
        .argv = &.{ emulatorBin(&emu_buf), "-avd", avd, "-grpc", "8554", "-no-snapshot-save", "-no-window" },
        .environ_map = storage.environ(),
        .stdout = .ignore,
        .stderr = .ignore,
    }) catch {
        job.message = gpa.dupe(u8, "couldn't start the emulator (is `emulator` on PATH?)") catch null;
        return;
    };
    // Worker-owned until onAndroidDone adopts it (no cross-thread write to the
    // controller). killEmulatorIfCancelled() reaps it on any early bail below.
    job.emulator = emu;

    if (!waitBootCompleted(gpa, job.ctrl)) {
        job.message = gpa.dupe(u8, "the emulator didn't finish booting in time") catch null;
        killEmulatorIfCancelled(job, true);
        return;
    }
    if (job.ctrl.cancel.load(.seq_cst)) {
        killEmulatorIfCancelled(job, true);
        return;
    }

    // 3) Install the APK.
    marshalPhase(job.ctrl, .installing);
    if (job.install_cmd) |c| {
        if (!bashOk(gpa, job.root, c)) {
            job.message = gpa.dupe(u8, "adb install failed") catch null;
            killEmulatorIfCancelled(job, true);
            return;
        }
    } else if (job.apk) |apk| {
        const apk_path = std.fs.path.join(gpa, &.{ job.root, apk }) catch return;
        defer gpa.free(apk_path);
        if (!cmdOk(gpa, &.{ "adb", "install", "-r", apk_path })) {
            job.message = gpa.dupe(u8, "adb install failed") catch null;
            killEmulatorIfCancelled(job, true);
            return;
        }
    }
    if (job.ctrl.cancel.load(.seq_cst)) {
        killEmulatorIfCancelled(job, true);
        return;
    }

    // 4) Launch the activity.
    marshalPhase(job.ctrl, .launching);
    if (job.application_id) |app_id| {
        const component = std.fmt.allocPrint(gpa, "{s}/{s}", .{ app_id, job.activity orelse ".MainActivity" }) catch return;
        defer gpa.free(component);
        _ = cmdOk(gpa, &.{ "adb", "shell", "am", "start", "-n", component });
    }

    job.ok = true;
}

fn onAndroidDone(data: gtk.gpointer) callconv(.c) c_int {
    const job: *AndroidJob = @ptrCast(@alignCast(data orelse return 0));
    const gpa = job.gpa;
    defer {
        gpa.free(job.root);
        gpa.free(job.avd);
        if (job.build_cmd) |b| gpa.free(b);
        if (job.apk) |b| gpa.free(b);
        if (job.install_cmd) |b| gpa.free(b);
        if (job.application_id) |b| gpa.free(b);
        if (job.activity) |b| gpa.free(b);
        if (job.message) |m| gpa.free(m);
        gpa.destroy(job);
    }
    const ctrl = job.ctrl;
    // Raced a stop()/new start: the worker may still own the emulator — kill it
    // so it doesn't orphan, then drop the result.
    if (ctrl.cancel.load(.seq_cst)) {
        if (job.emulator) |*child| child.kill(io.get());
        return 0;
    }
    if (!job.ok) {
        // Failed boot/build/install: reap any still-owned emulator (the worker
        // killed it on its own bail paths, but be defensive).
        if (job.emulator) |*child| child.kill(io.get());
        ctrl.setPhase(.err);
        ctrl.pane.setMessage(job.message orelse "android preview failed");
        return 0;
    }
    // Success: adopt the emulator child on the MAIN thread (stop()'s sole
    // reader/writer), so teardown owns it cleanly.
    ctrl.emulator_child = job.emulator;
    job.emulator = null;
    if (ctrl.android_streamed) {
        // Native Wayland: mirror the framebuffer into the GtkPicture (no reparent
        // possible); sets phase + message itself.
        startStreamWorker(ctrl);
    } else {
        // X11/XWayland: reparent the emulator's own window into the panel.
        embedEmulatorWindow(ctrl);
        ctrl.setPhase(.running);
    }
    return 0;
}

/// Marshal a phase label update to the main thread from a worker.
fn marshalPhase(ctrl: *PreviewController, phase: Phase) void {
    const msg = std.heap.page_allocator.create(PhaseMsg) catch return;
    msg.* = .{ .ctrl = ctrl, .phase = phase };
    _ = gtk.g_idle_add(@ptrCast(&onPhaseMsg), msg);
}

const PhaseMsg = struct { ctrl: *PreviewController, phase: Phase };

fn onPhaseMsg(data: gtk.gpointer) callconv(.c) c_int {
    const msg: *PhaseMsg = @ptrCast(@alignCast(data orelse return 0));
    defer std.heap.page_allocator.destroy(msg);
    if (!msg.ctrl.cancel.load(.seq_cst)) msg.ctrl.setPhase(msg.phase);
    return 0;
}

/// Poll `adb shell getprop sys.boot_completed` until it returns 1 (≈120s budget).
fn waitBootCompleted(gpa: std.mem.Allocator, ctrl: *PreviewController) bool {
    var i: usize = 0;
    while (i < 240) : (i += 1) {
        if (ctrl.cancel.load(.seq_cst)) return false;
        const res = runCapture(gpa, &.{ "adb", "shell", "getprop", "sys.boot_completed" }, null) orelse {
            sleepMs(500);
            continue;
        };
        defer gpa.free(res.stdout);
        defer gpa.free(res.stderr);
        if (std.mem.indexOfScalar(u8, res.stdout, '1') != null) return true;
        sleepMs(500);
    }
    return false;
}

fn bashOk(gpa: std.mem.Allocator, cwd: []const u8, command: []const u8) bool {
    const res = runCapture(gpa, &.{ "/usr/bin/env", "bash", "-lc", command }, cwd) orelse return false;
    defer gpa.free(res.stdout);
    defer gpa.free(res.stderr);
    return termOk(res.term);
}

fn cmdOk(gpa: std.mem.Allocator, argv: []const []const u8) bool {
    const res = runCapture(gpa, argv, null) orelse return false;
    defer gpa.free(res.stdout);
    defer gpa.free(res.stderr);
    return termOk(res.term);
}

/// Resolve which AVD to boot. Returns the configured one if installed, else the
/// first available (so a box with a differently-named AVD still works). gpa-owned
/// result; null only when no AVDs exist at all. If `emulator -list-avds` can't
/// run, optimistically returns the configured name (boot then surfaces the real
/// "is `emulator` on PATH?" error).
/// Absolute path to the SDK `emulator` binary — it lives under
/// `$ANDROID_HOME/emulator` and is NOT on PATH (unlike adb at /usr/bin). Returns
/// a slice into `buf`, or the bare "emulator" (PATH fallback) when the SDK root
/// is unset or the binary is missing.
fn emulatorBin(buf: []u8) []const u8 {
    const home = std.c.getenv("ANDROID_HOME") orelse std.c.getenv("ANDROID_SDK_ROOT") orelse return "emulator";
    return std.fmt.bufPrint(buf, "{s}/emulator/emulator", .{std.mem.span(home)}) catch "emulator";
}

fn resolveAvd(gpa: std.mem.Allocator, configured: []const u8) ?[]u8 {
    var emu_buf: [std.fs.max_path_bytes]u8 = undefined;
    const res = runCapture(gpa, &.{ emulatorBin(&emu_buf), "-list-avds" }, null) orelse
        return gpa.dupe(u8, configured) catch null;
    defer gpa.free(res.stdout);
    defer gpa.free(res.stderr);
    var first: ?[]const u8 = null;
    var it = std.mem.tokenizeAny(u8, res.stdout, "\r\n");
    while (it.next()) |line| {
        const name = std.mem.trim(u8, line, " \t\r\n");
        if (name.len == 0) continue;
        if (first == null) first = name;
        if (std.mem.eql(u8, name, configured)) return gpa.dupe(u8, configured) catch null;
    }
    if (first) |f| return gpa.dupe(u8, f) catch null;
    return null;
}

// ---------------------------------------------------------------------------
// Android Wayland fallback — stream the framebuffer into a GtkPicture and
// forward tap/swipe via `adb shell input` (native Wayland can't reparent).
// ---------------------------------------------------------------------------

const StreamFrame = struct { ctrl: *PreviewController, bytes: []u8 };

/// Attach a drag gesture to the stream picture that handles both tap (no
/// movement) and swipe, mapping widget coords → device pixels.
fn attachStreamInput(self: *PreviewController, pic: gtk.Object) void {
    const drag = gtk.gtk_gesture_drag_new();
    _ = gtk.g_signal_connect_data(drag, "drag-begin", @ptrCast(&onStreamDragBegin), self, null, 0);
    _ = gtk.g_signal_connect_data(drag, "drag-end", @ptrCast(&onStreamDragEnd), self, null, 0);
    gtk.gtk_widget_add_controller(pic, drag);
}

/// Spawn the screencast worker (main thread, after a successful Wayland boot).
fn startStreamWorker(self: *PreviewController) void {
    self.setPhase(.running);
    self.pane.setMessage("Mirroring the device — tap & swipe in the pane to control it.");
    const th = std.Thread.spawn(.{}, streamWorker, .{self}) catch return;
    th.detach();
}

/// Loop grabbing PNG frames (`adb exec-out screencap -p`) and marshal each to
/// the GtkPicture on the main thread. Exits when the controller is torn down.
fn streamWorker(ctrl: *PreviewController) void {
    while (!ctrl.cancel.load(.seq_cst)) {
        const res = runCapture(ctrl.gpa, &.{ "adb", "exec-out", "screencap", "-p" }, null) orelse {
            sleepMs(400);
            continue;
        };
        if (termOk(res.term) and res.stdout.len > 0) {
            ctrl.gpa.free(res.stderr);
            const frame = ctrl.gpa.create(StreamFrame) catch {
                ctrl.gpa.free(res.stdout);
                sleepMs(150);
                continue;
            };
            frame.* = .{ .ctrl = ctrl, .bytes = res.stdout };
            _ = gtk.g_idle_add(@ptrCast(&onStreamFrame), frame);
        } else {
            ctrl.gpa.free(res.stdout);
            ctrl.gpa.free(res.stderr);
        }
        sleepMs(150);
    }
}

fn onStreamFrame(data: gtk.gpointer) callconv(.c) c_int {
    const frame: *StreamFrame = @ptrCast(@alignCast(data orelse return 0));
    const ctrl = frame.ctrl;
    defer {
        ctrl.gpa.free(frame.bytes);
        ctrl.gpa.destroy(frame);
    }
    if (ctrl.cancel.load(.seq_cst) or ctrl.stream_picture == null) return 0;
    const gbytes = gtk.g_bytes_new(frame.bytes.ptr, frame.bytes.len);
    const tex = gtk.gdk_texture_new_from_bytes(gbytes, null);
    gtk.g_bytes_unref(gbytes);
    if (tex == null) return 0;
    ctrl.stream_w = gtk.gdk_texture_get_width(tex);
    ctrl.stream_h = gtk.gdk_texture_get_height(tex);
    gtk.gtk_picture_set_paintable(ctrl.stream_picture, tex);
    gtk.g_object_unref(tex);
    return 0;
}

const DevicePoint = struct { x: i64, y: i64 };

/// Map a widget-space point on the stream picture to emulator device pixels,
/// honoring GtkPicture's aspect-fit-contain scaling. Null if outside the frame.
fn streamWidgetToDevice(ctrl: *PreviewController, wx: f64, wy: f64) ?DevicePoint {
    if (ctrl.stream_picture == null or ctrl.stream_w <= 0 or ctrl.stream_h <= 0) return null;
    const ww: f64 = @floatFromInt(gtk.gtk_widget_get_width(ctrl.stream_picture));
    const wh: f64 = @floatFromInt(gtk.gtk_widget_get_height(ctrl.stream_picture));
    const iw: f64 = @floatFromInt(ctrl.stream_w);
    const ih: f64 = @floatFromInt(ctrl.stream_h);
    if (ww <= 0 or wh <= 0) return null;
    const scale = @min(ww / iw, wh / ih);
    if (scale <= 0) return null;
    const ox = (ww - iw * scale) / 2;
    const oy = (wh - ih * scale) / 2;
    const dx = (wx - ox) / scale;
    const dy = (wy - oy) / scale;
    if (dx < 0 or dy < 0 or dx > iw or dy > ih) return null;
    return .{ .x = @intFromFloat(dx), .y = @intFromFloat(dy) };
}

fn onStreamDragBegin(_: gtk.Object, sx: f64, sy: f64, data: gtk.gpointer) callconv(.c) void {
    const self: *PreviewController = @ptrCast(@alignCast(data orelse return));
    self.drag_sx = sx;
    self.drag_sy = sy;
}

fn onStreamDragEnd(_: gtk.Object, ox: f64, oy: f64, data: gtk.gpointer) callconv(.c) void {
    const self: *PreviewController = @ptrCast(@alignCast(data orelse return));
    const start = streamWidgetToDevice(self, self.drag_sx, self.drag_sy) orelse return;
    const moved = @abs(ox) + @abs(oy);
    if (moved < 12) {
        const cmd = std.fmt.allocPrint(self.gpa, "input tap {d} {d}", .{ start.x, start.y }) catch return;
        fireAdbShell(self.gpa, cmd);
    } else {
        const end = streamWidgetToDevice(self, self.drag_sx + ox, self.drag_sy + oy) orelse return;
        const cmd = std.fmt.allocPrint(self.gpa, "input swipe {d} {d} {d} {d} 200", .{ start.x, start.y, end.x, end.y }) catch return;
        fireAdbShell(self.gpa, cmd);
    }
}

const InputJob = struct { gpa: std.mem.Allocator, cmd: []u8 };

/// Run `adb shell <cmd>` on a detached worker so input never blocks the UI.
/// Takes ownership of `cmd_owned`.
fn fireAdbShell(gpa: std.mem.Allocator, cmd_owned: []u8) void {
    const job = gpa.create(InputJob) catch {
        gpa.free(cmd_owned);
        return;
    };
    job.* = .{ .gpa = gpa, .cmd = cmd_owned };
    const th = std.Thread.spawn(.{}, inputWorker, .{job}) catch {
        inputWorker(job);
        return;
    };
    th.detach();
}

fn inputWorker(job: *InputJob) void {
    defer {
        job.gpa.free(job.cmd);
        job.gpa.destroy(job);
    }
    const res = runCapture(job.gpa, &.{ "adb", "shell", job.cmd }, null) orelse return;
    job.gpa.free(res.stdout);
    job.gpa.free(res.stderr);
}

// ---------------------------------------------------------------------------
// X11 reparent (Android embed). Best-effort; XWayland/X11 only.
// ---------------------------------------------------------------------------

/// Find the emulator's top-level X11 window and reparent it into the panel's
/// embed surface. Requires the panel surface to be realized at a nonzero size
/// (the panel guarantees this). On native Wayland this is a no-op (the caller
/// already showed the fallback message).
fn embedEmulatorWindow(ctrl: *PreviewController) void {
    // Comptime-gate the entire Xlib body so an -Dx11=false build never references
    // the symbols (a runtime early-return would still link them).
    if (comptime !build_options.enable_x11) {
        ctrl.setPhase(.running);
        ctrl.pane.setMessage("Emulator booted in its own window (this build has no X11 reparent embed).");
        return;
    }

    const display = gtk.gdk_display_get_default();
    if (display == null) return;
    const xdisplay = gtk.gdk_x11_display_get_xdisplay(display) orelse return;

    // The panel's embed surface XID (our reparent target).
    const parent_xid = ctrl.pane.embedSurfaceXid() orelse {
        ctrl.pane.setMessage("Emulator booted in its own window (the embed surface wasn't realized).");
        return;
    };
    if (parent_xid == 0) {
        ctrl.pane.setMessage("Emulator booted in its own window (the embed surface wasn't realized).");
        return;
    }

    // Locate the emulator's top-level window: walk the X tree matching the window
    // title ("Android Emulator - …") or _NET_WM_PID. By boot-complete the window
    // exists, but can lag a beat — try a few times (short, post-boot).
    const emulator_pid: i32 = blk: {
        if (ctrl.emulator_child) |c| {
            if (c.id) |id| break :blk @intCast(id);
        }
        break :blk 0;
    };
    var child_xid: gtk.XID = 0;
    var tries: usize = 0;
    while (tries < 4) : (tries += 1) {
        if (findEmulatorWindow(xdisplay, emulator_pid)) |w| {
            child_xid = w;
            break;
        }
        sleepMs(150);
    }
    if (child_xid == 0) {
        ctrl.pane.setMessage("Emulator booted — couldn't locate its window to embed; it's running in its own window.");
        return;
    }

    _ = gtk.XReparentWindow(xdisplay, child_xid, parent_xid, 0, 0);
    _ = gtk.XMapWindow(xdisplay, child_xid);
    // Size the embedded window to the socket, then track future resizes so the
    // emulator follows the pane.
    const w = gtk.gtk_widget_get_width(ctrl.pane.embed_socket);
    const h = gtk.gtk_widget_get_height(ctrl.pane.embed_socket);
    if (w > 0 and h > 0) _ = gtk.XMoveResizeWindow(xdisplay, child_xid, 0, 0, @intCast(w), @intCast(h));
    _ = gtk.XFlush(xdisplay);
    ctrl.reparented_xid = child_xid;
    ctrl.x_display = xdisplay;
    if (!ctrl.embed_resize_connected) {
        ctrl.embed_resize_connected = true;
        _ = gtk.g_signal_connect_data(ctrl.pane.embed_socket, "resize", @ptrCast(&onEmbedResize), ctrl, null, 0);
    }
}

/// Keep the reparented emulator window sized to the embed socket as the pane
/// resizes (GtkDrawingArea "resize" → XMoveResizeWindow).
fn onEmbedResize(_: gtk.Object, width: c_int, height: c_int, data: gtk.gpointer) callconv(.c) void {
    const ctrl: *PreviewController = @ptrCast(@alignCast(data orelse return));
    if (ctrl.reparented_xid != 0 and ctrl.x_display != null and width > 0 and height > 0) {
        _ = gtk.XMoveResizeWindow(ctrl.x_display.?, ctrl.reparented_xid, 0, 0, @intCast(width), @intCast(height));
        _ = gtk.XFlush(ctrl.x_display.?);
    }
}

/// Walk the X window tree from the root and return the emulator's top-level
/// window — matched by its title containing "emulator" (the Android Emulator
/// window is titled "Android Emulator - <avd>:<port>") or by _NET_WM_PID equal
/// to the emulator process. Depth-limited; returns the first match.
fn findEmulatorWindow(xdisplay: *anyopaque, emulator_pid: i32) ?gtk.XID {
    const root = gtk.XDefaultRootWindow(xdisplay);
    const pid_atom = gtk.XInternAtom(xdisplay, "_NET_WM_PID", 1);
    return searchWindow(xdisplay, root, pid_atom, emulator_pid, 0);
}

fn searchWindow(xdisplay: *anyopaque, w: gtk.XID, pid_atom: gtk.Atom, target_pid: i32, depth: u32) ?gtk.XID {
    if (depth > 0 and windowMatchesEmulator(xdisplay, w, pid_atom, target_pid)) return w;
    if (depth >= 5) return null;
    var root_ret: gtk.XID = 0;
    var parent_ret: gtk.XID = 0;
    var children: ?[*]gtk.XID = null;
    var nchildren: c_uint = 0;
    if (gtk.XQueryTree(xdisplay, w, &root_ret, &parent_ret, &children, &nchildren) == 0) return null;
    defer {
        if (children) |c| {
            _ = gtk.XFree(@ptrCast(c));
        }
    }
    if (children) |c| {
        var i: c_uint = 0;
        while (i < nchildren) : (i += 1) {
            if (searchWindow(xdisplay, c[i], pid_atom, target_pid, depth + 1)) |found| return found;
        }
    }
    return null;
}

fn windowMatchesEmulator(xdisplay: *anyopaque, w: gtk.XID, pid_atom: gtk.Atom, target_pid: i32) bool {
    // Primary: WM_CLASS. Qt always sets it; the emulator's is
    // "qemu-system-x86_64"/"Emulator". (The title is set via _NET_WM_NAME — which
    // the legacy XFetchName/WM_NAME doesn't see — and _NET_WM_PID is the forked
    // qemu pid, not our launcher pid. So WM_CLASS is the one reliable signal.)
    if (readWindowClass(xdisplay, w)) |buf| {
        defer _ = gtk.XFree(@ptrCast(buf.ptr));
        if (containsIgnoreCase(buf, "emulator") or containsIgnoreCase(buf, "qemu")) return true;
    }
    // Secondary: legacy WM_NAME (older emulators set it).
    var name_ptr: ?[*:0]u8 = null;
    if (gtk.XFetchName(xdisplay, w, &name_ptr) != 0) {
        if (name_ptr) |np| {
            defer _ = gtk.XFree(@ptrCast(np));
            if (containsIgnoreCase(std.mem.span(np), "emulator")) return true;
        }
    }
    // Tertiary: _NET_WM_PID == the emulator process (covers retitled windows).
    if (pid_atom != 0 and target_pid > 0) {
        var actual_type: gtk.Atom = 0;
        var actual_format: c_int = 0;
        var nitems: c_ulong = 0;
        var bytes_after: c_ulong = 0;
        var prop: ?[*]u8 = null;
        if (gtk.XGetWindowProperty(xdisplay, w, pid_atom, 0, 1, 0, gtk.XA_CARDINAL, &actual_type, &actual_format, &nitems, &bytes_after, &prop) == 0) {
            if (prop) |p| {
                defer _ = gtk.XFree(@ptrCast(p));
                // CARDINAL/32 properties are returned as a C `long` array.
                if (actual_format == 32 and nitems >= 1) {
                    const pid_val = @as(*align(1) const c_long, @ptrCast(p)).*;
                    if (pid_val == target_pid) return true;
                }
            }
        }
    }
    return false;
}

/// Read WM_CLASS ("res_name\0res_class\0") as raw bytes. The caller must
/// `XFree(result.ptr)`. Null when the window has no WM_CLASS.
fn readWindowClass(xdisplay: *anyopaque, w: gtk.XID) ?[]u8 {
    const wm_class_atom = gtk.XInternAtom(xdisplay, "WM_CLASS", 1);
    if (wm_class_atom == 0) return null;
    var actual_type: gtk.Atom = 0;
    var actual_format: c_int = 0;
    var nitems: c_ulong = 0;
    var bytes_after: c_ulong = 0;
    var prop: ?[*]u8 = null;
    if (gtk.XGetWindowProperty(xdisplay, w, wm_class_atom, 0, 64, 0, gtk.XA_STRING, &actual_type, &actual_format, &nitems, &bytes_after, &prop) != 0) return null;
    if (prop) |p| {
        if (actual_format == 8 and nitems > 0) return p[0..nitems];
        _ = gtk.XFree(@ptrCast(p));
    }
    return null;
}

fn containsIgnoreCase(haystack: []const u8, needle: []const u8) bool {
    if (needle.len == 0 or needle.len > haystack.len) return false;
    var i: usize = 0;
    while (i + needle.len <= haystack.len) : (i += 1) {
        var j: usize = 0;
        while (j < needle.len) : (j += 1) {
            if (std.ascii.toLower(haystack[i + j]) != std.ascii.toLower(needle[j])) break;
        } else return true;
    }
    return false;
}

/// The X root window for the default screen (teardown's un-reparent target).
fn xRootWindow(xdisplay: *anyopaque) gtk.XID {
    return gtk.XDefaultRootWindow(xdisplay);
}

/// True under a native Wayland session (XDG_SESSION_TYPE=wayland and no DISPLAY
/// proxy via XWayland we can reparent into). We treat the presence of WAYLAND_
/// DISPLAY without a usable X DISPLAY as native Wayland.
fn isNativeWayland() bool {
    const session = std.c.getenv("XDG_SESSION_TYPE");
    const is_wayland = session != null and std.mem.eql(u8, std.mem.span(session.?), "wayland");
    const has_display = std.c.getenv("DISPLAY") != null; // XWayland exposes one
    return is_wayland and !has_display;
}
