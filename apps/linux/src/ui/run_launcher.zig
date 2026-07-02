//! Host-side run-config launcher (masterplan §4c): given a parsed `command`
//! run target and the repo clone dir (`preview_config.repoCloneDir`), spawn
//! `argv` with cwd + env into a NEW terminal-dock tab so output is visible,
//! capture the child exit code directly, surface it in the tab (a status strip
//! above the ghostty surface), and record it in an in-memory run-history ring
//! per target (the play menu shows "exit 0 · 2m ago").
//!
//! Spawn shape mirrors the coding launcher's `.exp-run.sh` trick: the argv is
//! shell-quoted into a launcher script written at the clone root (git-excluded
//! so it never dirties `git status`), and the terminal runs `sh <script>` —
//! two plain tokens, safe regardless of how ghostty tokenises its `command`
//! string. Env vars are layered through ghostty's own env plumbing.
//!
//! Lifecycle: NO widget destroy hooks. `terminal.zig` fires `on_exit` exactly
//! once per terminal — either the real child exit (code >= 0; widgets alive →
//! update the strip + history) or the destroy fallback (-130 when the tab is
//! closed while running; widgets are mid-teardown → record only, touch no
//! widget). Entries are freed in `destroy()` (sign-out), where still-armed
//! terminals are disarmed first so no late callback can land in freed memory.

const std = @import("std");
const gtk = @import("gtk.zig");
const terminal = @import("terminal.zig");
const storage = @import("../core/storage.zig");
const preview_config = @import("preview/preview_config.zig");
const run_script = @import("run_script.zig");

/// Insert the ready tab child into the dock (main thread) — same shape as the
/// coding launcher's MountFn. `key` is the run-target id (the dock generates a
/// unique tab key when a second concurrent run of the same target lands).
pub const MountFn = *const fn (ctx: ?*anyopaque, widget: gtk.Object, title: [*:0]const u8, key: [*:0]const u8) void;

/// Exit code recorded when the tab is closed while the child is still running
/// (terminal.zig's destroy fallback).
pub const exit_stopped: i32 = -130;

const ActiveRun = struct {
    launcher: *RunLauncher,
    target_id: []u8, // owned
    /// The tab child handed to the dock (identity only after exit).
    wrapper: gtk.Object,
    /// The ghostty terminal area (for disarmExit at launcher teardown).
    term: gtk.Object,
    status_label: gtk.Object,
    status_strip: gtk.Object,
    exited: bool = false,
    exit_code: ?i32 = null,
};

pub const RunLauncher = struct {
    gpa: std.mem.Allocator,
    mount: MountFn,
    mount_ctx: ?*anyopaque,
    active: std.ArrayListUnmanaged(*ActiveRun) = .empty,
    history: run_script.RunHistory,

    pub fn create(gpa: std.mem.Allocator, mount: MountFn, mount_ctx: ?*anyopaque) ?*RunLauncher {
        const self = gpa.create(RunLauncher) catch return null;
        self.* = .{
            .gpa = gpa,
            .mount = mount,
            .mount_ctx = mount_ctx,
            .history = run_script.RunHistory.init(gpa),
        };
        return self;
    }

    /// Sign-out teardown. MUST run while still-docked tab widgets are alive
    /// (i.e. after the dock's destroy but before the window content is
    /// swapped): live terminals are disarmed so their destroy-fallback
    /// `on_exit` can't fire into a freed entry.
    pub fn destroy(self: *RunLauncher) void {
        for (self.active.items) |run| {
            if (!run.exited) terminal.disarmExit(run.term);
            self.gpa.free(run.target_id);
            self.gpa.destroy(run);
        }
        self.active.deinit(self.gpa);
        self.history.deinit();
        self.gpa.destroy(self);
    }

    /// Launch a `command` target from the repo cloned at `clone_dir`. Main
    /// thread (the spawn itself is ghostty's, nothing blocks). Returns a
    /// static error message, or null on success.
    pub fn launch(self: *RunLauncher, target: preview_config.RunTarget, clone_dir: []const u8) ?[]const u8 {
        const gpa = self.gpa;
        const argv = target.argv orelse return "This run config has no argv.";
        if (argv.len == 0) return "This run config has no argv.";

        // Working dir: `cwd` wins over the common `rootDir`; both are
        // repo-relative and re-validated here (defense in depth — the parser
        // already drops `..`/absolute cwd, but rootDir arrives unchecked).
        const rel: ?[]const u8 = target.cwd orelse target.root_dir;
        if (rel) |r| {
            if (!preview_config.isSafeRelDir(r)) return "This run config's working directory is invalid.";
        }

        if (!storage.fileExists(clone_dir))
            return "Repo not cloned yet — start coding once (or run a preview) to clone it.";

        // Compose + write the launcher script at the clone root.
        const script = run_script.buildRunScript(gpa, rel, argv) catch return "Out of memory.";
        defer gpa.free(script);
        var id_buf: [48]u8 = undefined;
        const safe_id = run_script.sanitizeId(&id_buf, target.id);
        const script_name = std.fmt.allocPrint(gpa, ".exp-cmd-{s}.sh", .{safe_id}) catch return "Out of memory.";
        defer gpa.free(script_name);
        const script_path = std.fs.path.join(gpa, &.{ clone_dir, script_name }) catch return "Out of memory.";
        defer gpa.free(script_path);
        storage.writeSecret(script_path, script) catch return "Couldn't write the run script into the clone.";
        excludeRunScripts(gpa, clone_dir);

        const command = std.fmt.allocPrint(gpa, "sh {s}", .{script_name}) catch return "Out of memory.";
        defer gpa.free(command);

        const run = gpa.create(ActiveRun) catch return "Out of memory.";
        run.* = .{
            .launcher = self,
            .target_id = gpa.dupe(u8, target.id) catch {
                gpa.destroy(run);
                return "Out of memory.";
            },
            .wrapper = null,
            .term = null,
            .status_label = null,
            .status_strip = null,
        };

        const term = terminal.create(gpa, .{
            .cwd = clone_dir,
            .command = command,
            .env = target.env,
            .wait_after_command = true, // output stays inspectable after exit
            .on_exit = onRunExit,
            .on_exit_ctx = run,
        }) orelse {
            gpa.free(run.target_id);
            gpa.destroy(run);
            return "Couldn't create the terminal for this run.";
        };

        // Tab child: a wrapper box with a (hidden) exit-status strip above the
        // terminal, so the exit code shows in the tab when the process ends —
        // and survives a tab detach (the wrapper reparents as one widget).
        const wrapper = gtk.gtk_box_new(gtk.ORIENTATION_VERTICAL, 0);
        const strip = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 8);
        gtk.gtk_widget_add_css_class(strip, "exp-run-strip");
        const label = gtk.gtk_label_new("");
        gtk.gtk_widget_set_halign(label, gtk.ALIGN_START);
        gtk.gtk_widget_set_hexpand(label, 1);
        gtk.gtk_label_set_ellipsize(label, gtk.ELLIPSIZE_END);
        gtk.gtk_box_append(strip, label);
        gtk.gtk_widget_set_visible(strip, 0); // shown when the process ends
        gtk.gtk_box_append(wrapper, strip);
        gtk.gtk_box_append(wrapper, term);

        run.wrapper = wrapper;
        run.term = term;
        run.status_label = label;
        run.status_strip = strip;
        self.active.append(gpa, run) catch {
            // Untracked-by-list is survivable: the run still shows in its tab;
            // only Stop/menu state misses it. Disarm so exit can't UAF... the
            // entry stays alive (leaked to the arena of the process) instead.
        };

        var title_buf: [128]u8 = undefined;
        const title = std.fmt.bufPrintZ(&title_buf, "{s}", .{target.name[0..@min(target.name.len, 96)]}) catch "Run";
        var key_buf: [64]u8 = undefined;
        const key = std.fmt.bufPrintZ(&key_buf, "{s}", .{safe_id}) catch "run";
        self.mount(self.mount_ctx, wrapper, title.ptr, key.ptr);
        return null;
    }

    /// Whether `target_id` has a run whose child is still alive.
    pub fn isRunning(self: *const RunLauncher, target_id: []const u8) bool {
        for (self.active.items) |run| {
            if (!run.exited and std.mem.eql(u8, run.target_id, target_id)) return true;
        }
        return false;
    }

    /// The tab child of the newest still-running launch of `target_id`
    /// (identity for the dock's close-tab lookup), or null.
    pub fn activeWidget(self: *const RunLauncher, target_id: []const u8) gtk.Object {
        var i = self.active.items.len;
        while (i > 0) {
            i -= 1;
            const run = self.active.items[i];
            if (!run.exited and std.mem.eql(u8, run.target_id, target_id)) return run.wrapper;
        }
        return null;
    }

    /// The most recent finished run of `target_id` (exit code + when).
    pub fn lastRun(self: *const RunLauncher, target_id: []const u8) ?run_script.Record {
        return self.history.lastFor(target_id);
    }
};

/// terminal.zig on_exit — main thread, exactly once per terminal. A real exit
/// (code >= 0) arrives with the widgets alive; the destroy fallback (-130)
/// arrives mid-teardown, so it must not touch any widget.
fn onRunExit(ctx: ?*anyopaque, exit_code: i32) void {
    const run: *ActiveRun = @ptrCast(@alignCast(ctx orelse return));
    if (run.exited) return;
    run.exited = true;
    run.exit_code = exit_code;
    const self = run.launcher;
    self.history.record(run.target_id, exit_code, run_script.nowMs());

    if (exit_code >= 0) {
        var buf: [96]u8 = undefined;
        if (std.fmt.bufPrintZ(&buf, "Process exited with code {d}", .{exit_code})) |z| {
            gtk.gtk_label_set_text(run.status_label, z.ptr);
        } else |_| {}
        if (exit_code != 0) gtk.gtk_widget_add_css_class(run.status_strip, "exp-run-strip-err");
        gtk.gtk_widget_set_visible(run.status_strip, 1);
    }
    // Widgets are done for this entry either way — drop the references.
    run.wrapper = null;
    run.term = null;
    run.status_label = null;
    run.status_strip = null;
}

/// Keep `.exp-cmd-*.sh` out of `git status` in the shared clone (mirrors the
/// coding launcher's worktree exclude). Best-effort.
fn excludeRunScripts(gpa: std.mem.Allocator, clone_dir: []const u8) void {
    var arena = std.heap.ArenaAllocator.init(gpa);
    defer arena.deinit();
    const a = arena.allocator();
    const res = std.process.run(a, std.Io.Threaded.global_single_threaded.io(), .{
        .argv = &.{ "git", "-C", clone_dir, "rev-parse", "--git-path", "info/exclude" },
        .stdout_limit = .limited(64 * 1024),
        .stderr_limit = .limited(64 * 1024),
    }) catch return;
    if (!(res.term == .exited and res.term.exited == 0)) return;
    const rel = std.mem.trim(u8, res.stdout, " \t\r\n");
    if (rel.len == 0) return;
    const abs = if (std.fs.path.isAbsolute(rel)) a.dupe(u8, rel) catch return else std.fs.path.join(a, &.{ clone_dir, rel }) catch return;
    const existing = storage.readFileAlloc(a, abs) orelse "";
    if (std.mem.indexOf(u8, existing, ".exp-cmd-*.sh") != null) return; // already added
    const merged = std.fmt.allocPrint(a, "{s}\n.exp-cmd-*.sh\n", .{existing}) catch return;
    std.Io.Dir.cwd().writeFile(std.Io.Threaded.global_single_threaded.io(), .{ .sub_path = abs, .data = merged }) catch {};
}
