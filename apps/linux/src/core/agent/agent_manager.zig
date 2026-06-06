//! Drives the Rust `agent-core` from the desktop app: creates the core with a
//! config, registers an event callback, and starts the loop. The core emits
//! `run_request` events when it needs the coding CLI run.
//!
//! M7: instead of a headless `std.process.run` child, each run is launched in a
//! VISIBLE embedded ghostty terminal (ui/terminal.zig) so the user can watch and
//! steer the agent's `claude`/`codex` session. The CLI is wrapped in a per-run
//! bash script that tees its stdout to a capture file and records the exit code;
//! when the child exits we read those back and call `submit_run_result`, which
//! unblocks the parked pipeline thread in the core.
//!
//! Threading: `onEvent` fires on a core pipeline thread (NOT the GTK thread). It
//! copies the request and marshals to the GTK main loop via `g_idle_add`; all
//! GTK + ghostty work happens there. The pipeline thread then parks in the core's
//! `request_run` until `submit_run_result` is called from the child-exit handler.

const std = @import("std");
const ffi = @import("agent_core_ffi.zig");
const storage = @import("../storage.zig");
const gtk = @import("../../ui/gtk.zig");
const terminal = @import("../../ui/terminal.zig");

pub const Manager = struct {
    gpa: std.mem.Allocator,
    core: ffi.AgentCore,
    workspace_id: []u8,
    // Optional IDE-style terminal dock: when set, agent runs mount here instead
    // of a throwaway window. Opaque so this module needn't import the UI types.
    dock: ?*anyopaque = null,
    mount_fn: ?*const fn (dock: *anyopaque, term: gtk.Object, title: [*:0]const u8) void = null,
};

/// Point this manager's agent runs at the UI terminal dock (called after start).
pub fn setDock(mgr: *Manager, dock: ?*anyopaque, mount_fn: ?*const fn (dock: *anyopaque, term: gtk.Object, title: [*:0]const u8) void) void {
    mgr.dock = dock;
    mgr.mount_fn = mount_fn;
}

/// Create + start the core for `workspace_id` with the given config JSON.
pub fn start(gpa: std.mem.Allocator, config_json: []const u8, workspace_id: []const u8) ?*Manager {
    const cfg_z = gpa.dupeZ(u8, config_json) catch return null;
    defer gpa.free(cfg_z);
    const core = ffi.agent_core_create(cfg_z.ptr);
    if (core == null) return null;

    const mgr = gpa.create(Manager) catch {
        ffi.agent_core_free(core);
        return null;
    };
    mgr.gpa = gpa;
    mgr.core = core;
    mgr.workspace_id = gpa.dupe(u8, workspace_id) catch {
        ffi.agent_core_free(core);
        gpa.destroy(mgr);
        return null;
    };
    ffi.agent_core_set_event_callback(core, @ptrCast(mgr), &onEvent);
    _ = ffi.agent_core_start(core);
    return mgr;
}

pub fn stop(mgr: *Manager) void {
    _ = ffi.agent_core_stop(mgr.core);
    ffi.agent_core_free(mgr.core);
    mgr.gpa.free(mgr.workspace_id);
    mgr.gpa.destroy(mgr);
}

/// Desktop "AI" button: start an interactive plan session for `issue_id`. The
/// core emits a `run_request` (interactive:true) we launch in a terminal.
pub fn requestInteractive(mgr: *Manager, issue_id: []const u8) void {
    const z = mgr.gpa.dupeZ(u8, issue_id) catch return;
    defer mgr.gpa.free(z);
    _ = ffi.agent_core_request_interactive(mgr.core, z.ptr);
}

/// Desktop "Approve & continue here": resume the interactive session to
/// implement the (already human-approved) plan in the reused worktree.
pub fn approveInteractive(mgr: *Manager, issue_id: []const u8) void {
    const z = mgr.gpa.dupeZ(u8, issue_id) catch return;
    defer mgr.gpa.free(z);
    _ = ffi.agent_core_approve_interactive(mgr.core, z.ptr);
}

/// Desktop "Cancel" button: stop the run currently in flight for `issue_id`
/// (the core maps the issue to its run and unblocks the parked pipeline).
pub fn cancelIssue(mgr: *Manager, issue_id: []const u8) void {
    const z = mgr.gpa.dupeZ(u8, issue_id) catch return;
    defer mgr.gpa.free(z);
    _ = ffi.agent_core_cancel_issue(mgr.core, z.ptr);
}

fn onEvent(ctx: ?*anyopaque, json_ptr: [*c]const u8, len: usize) callconv(.c) void {
    const mgr: *Manager = @ptrCast(@alignCast(ctx orelse return));
    if (json_ptr == null or len == 0) return;
    const json = json_ptr[0..len];

    var parsed = std.json.parseFromSlice(std.json.Value, mgr.gpa, json, .{}) catch return;
    defer parsed.deinit();
    const obj = switch (parsed.value) {
        .object => |o| o,
        else => return,
    };
    const typ = objStr(obj, "type") orelse return;
    if (std.mem.eql(u8, typ, "run_request")) {
        handleRunRequest(mgr, obj);
    } else if (std.mem.eql(u8, typ, "log")) {
        // Surface core logs to stderr (safe off the GTK thread — no GTK calls).
        const level = objStr(obj, "level") orelse "info";
        const msg = objStr(obj, "message") orelse "";
        std.debug.print("[agent-core] {s}: {s}\n", .{ level, msg });
    }
}

// -------------------------------------------------------------------------
// Run request → embedded terminal
// -------------------------------------------------------------------------

/// A run marshaled from the pipeline thread to the GTK main loop. Owns deep
/// copies of everything (the source JSON is freed when onEvent returns).
const PendingRun = struct {
    mgr: *Manager,
    run_id: []u8,
    program: []u8,
    argv: std.ArrayListUnmanaged([]u8),
    env: std.ArrayListUnmanaged([2][]u8),
    cwd: ?[]u8,
    prompt: []u8, // combined system + user prompt
    // Interactive runs (desktop AI button / approve-and-continue): the user
    // watches + steers claude live, the plan is delivered via MCP, so we DON'T
    // wrap with tee/PIPESTATUS and submit an empty result on exit.
    interactive: bool = false,
};

/// Live state for an in-flight terminal run, used by the child-exit handler.
const RunCtx = struct {
    mgr: *Manager,
    run_id: [:0]u8,
    prompt_path: []u8,
    script_path: []u8,
    window: gtk.Object,
};

/// Pipeline thread: copy the request and hand it to the main loop. On any setup
/// failure, submit a failure result so the parked pipeline thread unblocks.
fn handleRunRequest(mgr: *Manager, obj: std.json.ObjectMap) void {
    const gpa = mgr.gpa;
    const run_id = objStr(obj, "runId") orelse return;
    const program = objStr(obj, "program") orelse {
        submitFailure(mgr, run_id);
        return;
    };

    const pending = gpa.create(PendingRun) catch return;
    pending.* = .{
        .mgr = mgr,
        .run_id = gpa.dupe(u8, run_id) catch {
            gpa.destroy(pending);
            return;
        },
        .program = gpa.dupe(u8, program) catch "",
        .argv = .empty,
        .env = .empty,
        .cwd = if (objStr(obj, "cwd")) |c| (gpa.dupe(u8, c) catch null) else null,
        .prompt = buildPrompt(gpa, obj),
        .interactive = if (obj.get("interactive")) |v| (v == .bool and v.bool) else false,
    };

    if (obj.get("argv")) |av| {
        if (av == .array) {
            for (av.array.items) |it| {
                if (it == .string) {
                    const s = gpa.dupe(u8, it.string) catch continue;
                    pending.argv.append(gpa, s) catch gpa.free(s);
                }
            }
        }
    }
    if (obj.get("env")) |ev| {
        if (ev == .object) {
            var it = ev.object.iterator();
            while (it.next()) |entry| {
                if (entry.value_ptr.* != .string) continue;
                const k = gpa.dupe(u8, entry.key_ptr.*) catch continue;
                const v = gpa.dupe(u8, entry.value_ptr.*.string) catch {
                    gpa.free(k);
                    continue;
                };
                pending.env.append(gpa, .{ k, v }) catch {
                    gpa.free(k);
                    gpa.free(v);
                };
            }
        }
    }

    _ = gtk.g_idle_add(startRunOnMain, pending);
}

fn buildPrompt(gpa: std.mem.Allocator, obj: std.json.ObjectMap) []u8 {
    const system = objStr(obj, "systemPrompt") orelse "";
    const user = objStr(obj, "userPrompt") orelse "";
    return std.fmt.allocPrint(gpa, "{s}\n\n<user_issue>\n{s}\n</user_issue>", .{ system, user }) catch
        (gpa.dupe(u8, user) catch "");
}

/// GTK main thread: write the prompt + wrapper script, launch the CLI in an
/// embedded ghostty terminal, and show it in a window.
fn startRunOnMain(data: gtk.gpointer) callconv(.c) c_int {
    const pending: *PendingRun = @ptrCast(@alignCast(data orelse return 0));
    const mgr = pending.mgr;
    const gpa = mgr.gpa;
    defer freePending(pending);

    var arena_state = std.heap.ArenaAllocator.init(gpa);
    defer arena_state.deinit();
    const a = arena_state.allocator();

    const dir = storage.configDir(a) catch return failNow(pending);
    const runs_dir = std.fmt.allocPrint(a, "{s}/agent-runs", .{dir}) catch return failNow(pending);
    storage.ensureDir(runs_dir) catch {};

    // gpa-owned paths live in the RunCtx until the child exits.
    const prompt_path = std.fmt.allocPrint(gpa, "{s}/{s}.prompt", .{ runs_dir, pending.run_id }) catch return failNow(pending);
    const script_path = std.fmt.allocPrint(gpa, "{s}/{s}.sh", .{ runs_dir, pending.run_id }) catch return failNow(pending);

    storage.writeSecret(prompt_path, pending.prompt) catch return failNow(pending);

    // The host only launches INTERACTIVE agent sessions now — headless plan/code
    // runs execute in-core (agent-core spawns them itself). The user watches and
    // steers the session and the plan is delivered via MCP, so there's no stdout
    // capture: the prompt is read from its file as the final positional arg.
    const script = buildInteractiveScript(a, pending.program, pending.argv.items, prompt_path) catch return failNow(pending);
    storage.writeSecret(script_path, script) catch return failNow(pending);

    const command = std.fmt.allocPrint(a, "/usr/bin/env bash {s}", .{script_path}) catch return failNow(pending);

    // env pairs for the terminal (codex carries EXPONENTIAL_MCP_TOKEN etc.).
    var env_pairs = std.ArrayListUnmanaged([2][]const u8).empty;
    for (pending.env.items) |kv| env_pairs.append(a, .{ kv[0], kv[1] }) catch {};

    const ctx = gpa.create(RunCtx) catch return failNow(pending);
    ctx.* = .{
        .mgr = mgr,
        .run_id = gpa.dupeZ(u8, pending.run_id) catch return failNow(pending),
        .prompt_path = prompt_path,
        .script_path = script_path,
        .window = null,
    };

    const term = terminal.create(gpa, .{
        .cwd = pending.cwd,
        .command = command,
        .env = env_pairs.items,
        .wait_after_command = true,
        .on_exit = onRunExit,
        .on_exit_ctx = ctx,
    }) orelse {
        submitFailure(mgr, pending.run_id);
        freeCtx(ctx);
        return 0;
    };

    const title = std.fmt.allocPrintSentinel(a, "Agent — {s}", .{pending.run_id}, 0) catch "Agent run";

    // Prefer the docked terminal (IDE-style bottom pane); fall back to a
    // throwaway window if the dock isn't available.
    if (mgr.dock != null and mgr.mount_fn != null) {
        mgr.mount_fn.?(mgr.dock.?, term, title.ptr);
        ctx.window = null;
    } else {
        const win = gtk.gtk_window_new();
        gtk.gtk_window_set_title(win, title);
        gtk.gtk_window_set_default_size(win, 960, 640);
        gtk.gtk_window_set_child(win, term);
        gtk.gtk_window_present(win);
        ctx.window = win;
    }

    return 0; // G_SOURCE_REMOVE — fire once
}

/// Build the per-run bash wrapper. program/argv are shell-quoted by us; the
/// prompt is injected from its file as the final positional argument.
fn buildScript(a: std.mem.Allocator, program: []const u8, argv: []const []const u8, prompt_path: []const u8, out_path: []const u8, code_path: []const u8) ![]u8 {
    var buf = std.ArrayListUnmanaged(u8).empty;
    try buf.appendSlice(a, "#!/usr/bin/env bash\nset -o pipefail\n");
    try shquoteTo(a, &buf, program);
    for (argv) |arg| {
        try buf.append(a, ' ');
        try shquoteTo(a, &buf, arg);
    }
    // Final positional arg = the combined prompt, read from its file.
    try buf.appendSlice(a, " \"$(cat ");
    try shquoteTo(a, &buf, prompt_path);
    try buf.appendSlice(a, ")\" 2>&1 | tee ");
    try shquoteTo(a, &buf, out_path);
    try buf.appendSlice(a, "\necho \"${PIPESTATUS[0]}\" > ");
    try shquoteTo(a, &buf, code_path);
    try buf.append(a, '\n');
    return buf.toOwnedSlice(a);
}

/// Build the INTERACTIVE wrapper: run the CLI with the prompt as the final
/// positional arg, NO tee/PIPESTATUS capture (the user watches + steers the
/// session live; the plan is delivered out-of-band via the Exponential MCP).
fn buildInteractiveScript(a: std.mem.Allocator, program: []const u8, argv: []const []const u8, prompt_path: []const u8) ![]u8 {
    var buf = std.ArrayListUnmanaged(u8).empty;
    try buf.appendSlice(a, "#!/usr/bin/env bash\n");
    try shquoteTo(a, &buf, program);
    for (argv) |arg| {
        try buf.append(a, ' ');
        try shquoteTo(a, &buf, arg);
    }
    try buf.appendSlice(a, " \"$(cat ");
    try shquoteTo(a, &buf, prompt_path);
    try buf.appendSlice(a, ")\"\n");
    return buf.toOwnedSlice(a);
}

/// Append `t` as a single-quoted shell token (escaping embedded single quotes).
fn shquoteTo(a: std.mem.Allocator, buf: *std.ArrayListUnmanaged(u8), t: []const u8) !void {
    try buf.append(a, '\'');
    for (t) |c| {
        if (c == '\'') try buf.appendSlice(a, "'\\''") else try buf.append(a, c);
    }
    try buf.append(a, '\'');
}

/// GTK main thread: the agent CLI's child has exited. Read the captured output +
/// exit code and submit them to the core (unblocking the pipeline thread).
fn onRunExit(ctx_ptr: ?*anyopaque, action_exit_code: i32) void {
    const ctx: *RunCtx = @ptrCast(@alignCast(ctx_ptr orelse return));
    // The host only runs interactive sessions now: the plan/code is delivered via
    // MCP (no stdout capture) and the claude session id is recovered in-core from
    // its logs, so just submit an empty result to unblock the pipeline thread.
    // The terminal window stays open so the user can inspect the session.
    _ = ffi.agent_core_submit_run_result(ctx.mgr.core, ctx.run_id.ptr, @intCast(action_exit_code), "", null);
    storage.deleteFile(ctx.prompt_path);
    storage.deleteFile(ctx.script_path);
    freeCtx(ctx);
}

fn submitFailure(mgr: *Manager, run_id: []const u8) void {
    const rid = mgr.gpa.dupeZ(u8, run_id) catch return;
    defer mgr.gpa.free(rid);
    _ = ffi.agent_core_submit_run_result(mgr.core, rid.ptr, -1, "", null);
}

/// Submit a failure for a pending run and free it (used on main-thread setup
/// errors). Returns 0 so it can be `return failNow(...)` from the idle callback.
fn failNow(pending: *PendingRun) c_int {
    submitFailure(pending.mgr, pending.run_id);
    return 0;
}

fn freePending(pending: *PendingRun) void {
    const gpa = pending.mgr.gpa;
    gpa.free(pending.run_id);
    if (pending.program.len > 0) gpa.free(pending.program);
    for (pending.argv.items) |s| gpa.free(s);
    pending.argv.deinit(gpa);
    for (pending.env.items) |kv| {
        gpa.free(kv[0]);
        gpa.free(kv[1]);
    }
    pending.env.deinit(gpa);
    if (pending.cwd) |c| gpa.free(c);
    if (pending.prompt.len > 0) gpa.free(pending.prompt);
    gpa.destroy(pending);
}

fn freeCtx(ctx: *RunCtx) void {
    const gpa = ctx.mgr.gpa;
    gpa.free(ctx.run_id);
    gpa.free(ctx.prompt_path);
    gpa.free(ctx.script_path);
    gpa.destroy(ctx);
}

fn objStr(obj: std.json.ObjectMap, key: []const u8) ?[]const u8 {
    const v = obj.get(key) orelse return null;
    return switch (v) {
        .string => |s| s,
        else => null,
    };
}

// -------------------------------------------------------------------------
// Smoke: launch a wrapped command in an embedded terminal and log the captured
// output + exit code on child-exit. Exercises the full Stage-2 chain (script
// build → terminal → tee capture → on_exit read) WITHOUT the core handshake.
// Invoked via `exponential run-smoke <program> [args...]`.
// -------------------------------------------------------------------------

const SmokeReq = struct {
    gpa: std.mem.Allocator,
    program: []const u8,
    args: []const []const u8,
    prompt: []const u8,
};
const SmokeCtx = struct {
    gpa: std.mem.Allocator,
    app: gtk.Object,
    out_path: []u8,
    code_path: []u8,
    paths: [2][]u8, // prompt, script — for cleanup
};

pub fn runSmoke(gpa: std.mem.Allocator, program: []const u8, args: []const []const u8) u8 {
    var req = SmokeReq{ .gpa = gpa, .program = program, .args = args, .prompt = "smoke-prompt-from-agent-run" };
    const app = gtk.adw_application_new("at.exponential.desktop.runsmoke", gtk.APP_DEFAULT_FLAGS);
    _ = gtk.g_signal_connect_data(app, "activate", @ptrCast(&onSmokeActivate), &req, null, 0);
    const status = gtk.g_application_run(app, 0, null);
    gtk.g_object_unref(app);
    return @intCast(status);
}

fn onSmokeActivate(app: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const req: *SmokeReq = @ptrCast(@alignCast(data orelse return));
    const gpa = req.gpa;
    var arena_state = std.heap.ArenaAllocator.init(gpa);
    defer arena_state.deinit();
    const a = arena_state.allocator();

    const dir = storage.configDir(a) catch return;
    const runs_dir = std.fmt.allocPrint(a, "{s}/agent-runs", .{dir}) catch return;
    storage.ensureDir(runs_dir) catch {};
    const prompt_path = std.fmt.allocPrint(gpa, "{s}/smoke.prompt", .{runs_dir}) catch return;
    const script_path = std.fmt.allocPrint(gpa, "{s}/smoke.sh", .{runs_dir}) catch return;
    const out_path = std.fmt.allocPrint(gpa, "{s}/smoke.out", .{runs_dir}) catch return;
    const code_path = std.fmt.allocPrint(gpa, "{s}/smoke.code", .{runs_dir}) catch return;

    storage.writeSecret(prompt_path, req.prompt) catch return;
    const script = buildScript(a, req.program, req.args, prompt_path, out_path, code_path) catch return;
    storage.writeSecret(script_path, script) catch return;
    const command = std.fmt.allocPrint(a, "/usr/bin/env bash {s}", .{script_path}) catch return;

    const ctx = gpa.create(SmokeCtx) catch return;
    ctx.* = .{ .gpa = gpa, .app = app, .out_path = out_path, .code_path = code_path, .paths = .{ prompt_path, script_path } };

    const term = terminal.create(gpa, .{
        .command = command,
        .wait_after_command = true,
        .on_exit = onSmokeExit,
        .on_exit_ctx = ctx,
    }) orelse return;

    // Use an app-associated window so the GApplication stays alive (a standalone
    // gtk_window_new() isn't tracked → the app would quit right after activate).
    const win = gtk.adw_application_window_new(app);
    gtk.gtk_window_set_title(win, "agent run-smoke");
    gtk.gtk_window_set_default_size(win, 900, 560);
    gtk.adw_application_window_set_content(win, term);
    gtk.gtk_window_present(win);
}

fn onSmokeExit(ctx_ptr: ?*anyopaque, action_exit_code: i32) void {
    const ctx: *SmokeCtx = @ptrCast(@alignCast(ctx_ptr orelse return));
    const gpa = ctx.gpa;
    var exit_code: i32 = action_exit_code;
    if (storage.readFileAlloc(gpa, ctx.code_path)) |raw| {
        defer gpa.free(raw);
        if (std.fmt.parseInt(i32, std.mem.trim(u8, raw, " \t\r\n"), 10)) |c| exit_code = c else |_| {}
    }
    const out = storage.readFileAlloc(gpa, ctx.out_path);
    defer if (out) |o| gpa.free(o);
    std.debug.print("RUN-SMOKE captured exit_code={d} output=<<<{s}>>>\n", .{ exit_code, out orelse "(none)" });
    storage.deleteFile(ctx.out_path);
    storage.deleteFile(ctx.code_path);
    storage.deleteFile(ctx.paths[0]);
    storage.deleteFile(ctx.paths[1]);
    gtk.g_application_quit(ctx.app);
}
