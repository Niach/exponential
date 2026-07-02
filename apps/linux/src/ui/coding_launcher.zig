//! The native "Start coding" launcher (§4a) — no Rust core, no FFI. Runs the
//! identical sequence whether triggered by the issue-detail play button (local)
//! or, later, a `start_session` control frame from the steer relay (§3/Phase 4):
//!
//!   repositories.forIssue → repositories.installationToken → host-side git
//!   (clone + `exp/<IDENTIFIER>` worktree + token-embedded remote, NO `gh`) →
//!   write `.mcp.json` (web `/api/mcp` + the user's personal apikey) → compose a
//!   plan-first prompt → spawn `claude --dangerously-skip-permissions` (cwd =
//!   worktree) in an embedded ghostty terminal → insert a `coding_sessions` row
//!   (`running`), flip it to `ended` when the child exits.
//!
//! The network + git work runs on a detached worker thread (like `preview.zig`);
//! only the terminal creation + mount is marshalled back to the GTK main thread
//! via `g_idle_add`. The caller supplies the already-resolved issue context
//! (identifier/title/description — read from the local synced DB) plus two
//! main-thread callbacks: `mount` (insert the ready terminal into the dock) and
//! `on_error` (surface a failure / the "link a repository" CTA). This keeps the
//! launcher decoupled from `AppState`, so the Phase-4 relay handler can reuse
//! `start()` verbatim.

const std = @import("std");
const gtk = @import("gtk.zig");
const terminal = @import("terminal.zig");
const trpc = @import("../core/api/trpc.zig");
const mutate = @import("../core/api/mutate.zig");
const storage = @import("../core/storage.zig");
const credentials = @import("../core/credentials.zig");
const git = @import("../core/git_worktree.zig");
const prompt = @import("../core/coding_prompt.zig");
const host_pty = @import("../core/steer/host_pty.zig");
const steer_pub = @import("../core/steer/publisher.zig");
const Database = @import("../core/db/database.zig").Database;

/// Insert a ready ghostty terminal into the UI (main thread). `title` is an
/// issue identifier like "EXP-123"; `key` is the tab key for the dock's
/// AdwTabView — the `coding_sessions.id` when a session row exists, else ""
/// (the dock generates a unique key).
pub const MountFn = *const fn (ctx: ?*anyopaque, term: gtk.Object, title: [*:0]const u8, key: [*:0]const u8) void;
/// Surface a launcher error / CTA (main thread).
pub const ErrorFn = *const fn (ctx: ?*anyopaque, message: [*:0]const u8) void;

/// One launch, identified by `issue_id` alone — the issue's identifier / title /
/// description are resolved from the local synced DB inside `start()`. This is
/// what lets the SAME entry point serve both the issue-detail play button and
/// the Phase-4 relay `start_session` handler.
pub const Request = struct {
    /// Thread-safe (the app uses `std.heap.page_allocator`).
    gpa: std.mem.Allocator,
    instance: []const u8,
    token: ?[]const u8,
    /// The signed-in user's id (Better Auth) — used to tell a REMOTE steerer
    /// from the local user steering via their own viewer (§3.4 banner).
    user_id: ?[]const u8 = null,
    /// The local synced DB — read (on the calling/main thread) for the issue's
    /// identifier / title / description. Never touched off the main thread.
    db: *Database,
    issue_id: []const u8,
    mount: MountFn,
    mount_ctx: ?*anyopaque = null,
    on_error: ErrorFn,
    error_ctx: ?*anyopaque = null,
};

// ---------------------------------------------------------------------------
// Launch pipeline
// ---------------------------------------------------------------------------

const Job = struct {
    gpa: std.mem.Allocator,
    instance: []u8,
    token: ?[]u8,
    user_id: ?[]u8,
    issue_id: []u8,
    identifier: []u8,
    title: []u8,
    description: []u8,
    mount: MountFn,
    mount_ctx: ?*anyopaque,
    on_error: ErrorFn,
    error_ctx: ?*anyopaque,
};

/// Kick off the launcher from `issue_id` alone. MUST be called on the GTK main
/// thread: it snapshots the issue's identifier / title / description from the
/// local DB here (fast, mutex-guarded) so the detached worker never touches the
/// DB — safe against a concurrent sign-out. The network + git work then runs on
/// a detached worker so the GTK loop never blocks.
pub fn start(req: Request) void {
    const gpa = req.gpa;

    // Snapshot the issue context on this (main) thread.
    var arena = std.heap.ArenaAllocator.init(gpa);
    defer arena.deinit();
    const issue = (req.db.getIssue(arena.allocator(), req.issue_id) catch null) orelse {
        req.on_error(req.error_ctx, "Couldn't load this issue.");
        return;
    };

    // Dup into locals first, so a mid-way OOM frees exactly what succeeded (a
    // struct-literal `catch` can't free fields the struct hasn't taken yet).
    const instance = gpa.dupe(u8, req.instance) catch return;
    const issue_id = gpa.dupe(u8, req.issue_id) catch return free1(gpa, instance);
    const identifier = gpa.dupe(u8, issue.identifier) catch return free2(gpa, instance, issue_id);
    const title = gpa.dupe(u8, issue.title) catch return free3(gpa, instance, issue_id, identifier);
    const description = gpa.dupe(u8, issue.description) catch return free4(gpa, instance, issue_id, identifier, title);
    const token: ?[]u8 = if (req.token) |t| (gpa.dupe(u8, t) catch null) else null;
    const user_id: ?[]u8 = if (req.user_id) |u| (gpa.dupe(u8, u) catch null) else null;

    const job = gpa.create(Job) catch {
        free4(gpa, instance, issue_id, identifier, title);
        gpa.free(description);
        if (token) |t| gpa.free(t);
        if (user_id) |u| gpa.free(u);
        return;
    };
    job.* = .{
        .gpa = gpa,
        .instance = instance,
        .token = token,
        .user_id = user_id,
        .issue_id = issue_id,
        .identifier = identifier,
        .title = title,
        .description = description,
        .mount = req.mount,
        .mount_ctx = req.mount_ctx,
        .on_error = req.on_error,
        .error_ctx = req.error_ctx,
    };
    const th = std.Thread.spawn(.{}, worker, .{job}) catch {
        worker(job); // fallback: inline (still correct, just blocks once)
        return;
    };
    th.detach();
}

fn free1(gpa: std.mem.Allocator, a: []u8) void {
    gpa.free(a);
}
fn free2(gpa: std.mem.Allocator, a: []u8, b: []u8) void {
    gpa.free(a);
    gpa.free(b);
}
fn free3(gpa: std.mem.Allocator, a: []u8, b: []u8, c: []u8) void {
    gpa.free(a);
    gpa.free(b);
    gpa.free(c);
}
fn free4(gpa: std.mem.Allocator, a: []u8, b: []u8, c: []u8, d: []u8) void {
    gpa.free(a);
    gpa.free(b);
    gpa.free(c);
    gpa.free(d);
}

fn freeJob(job: *Job) void {
    const gpa = job.gpa;
    gpa.free(job.instance);
    if (job.token) |t| gpa.free(t);
    if (job.user_id) |u| gpa.free(u);
    gpa.free(job.issue_id);
    gpa.free(job.identifier);
    gpa.free(job.title);
    gpa.free(job.description);
    gpa.destroy(job);
}

fn worker(job: *Job) void {
    defer freeJob(job);
    const gpa = job.gpa;

    var cred = credentials.Store.open(gpa) catch {
        return fail(job, "Couldn't open desktop settings.");
    };
    defer cred.deinit();

    const personal_key = cred.personalKey() orelse
        return fail(job, "Generate a personal API key in Settings → Coding first.");

    // 1. Resolve the repo (coding-first gate lives here too).
    const repo = resolveRepo(gpa, job) orelse
        return fail(job, "Link a repository to this project in workspace settings to start coding.");
    defer repo.deinit(gpa);

    // 2. Mint a short-lived push token.
    const tok = mintToken(gpa, job, repo.repository_id) orelse
        return fail(job, "Couldn't get a push token — reconnect this repository in settings.");
    defer tok.deinit(gpa);

    // 3. Host-side git: clone + worktree + token remote.
    const token_url = git.tokenRemoteUrl(gpa, tok.token, tok.full_name) catch return fail(job, "Out of memory.");
    defer gpa.free(token_url);
    const branch = git.branchName(gpa, cred.branchPrefix(), job.identifier) catch return fail(job, "Out of memory.");
    defer gpa.free(branch);
    const base_ref = std.fmt.allocPrint(gpa, "origin/{s}", .{tok.default_branch}) catch return fail(job, "Out of memory.");
    defer gpa.free(base_ref);

    const repos_root = cred.reposRoot(gpa) catch return fail(job, "Out of memory.");
    defer gpa.free(repos_root);

    const clone_path = git.ensureClone(gpa, repos_root, tok.full_name, token_url) catch
        return fail(job, "git clone/fetch failed — is `git` installed and the repo reachable?");
    defer gpa.free(clone_path);

    const worktree = git.createWorktree(gpa, clone_path, branch, base_ref) catch
        return fail(job, "Couldn't create the git worktree.");
    defer gpa.free(worktree);

    git.setTokenRemote(gpa, worktree, token_url);

    // 4-5. Write .mcp.json + the plan-first prompt + the launcher script, and
    // keep all three out of `git status` so Claude never commits the token.
    writeWorktreeFiles(gpa, job, &cred, worktree, branch, personal_key) catch
        return fail(job, "Couldn't write the worktree launch files.");

    // 6. Record the coding session (running). Best-effort — a null id just means
    // no `codingSessions.end` call fires when the terminal closes.
    const session_id_opt = startSession(gpa, job);
    defer if (session_id_opt) |s| gpa.free(s);

    // 6b. Live steer (masterplan §3): only when the relay subsystem is enabled
    // on this instance AND we have a session row to key the relay room. Purely
    // additive — disabled/unreachable ⇒ the plain local coding path, zero
    // sockets, zero behavior change.
    const steer = session_id_opt != null and steerEnabled(gpa, job);

    // 7. Marshal the terminal spawn back to the main thread.
    scheduleMount(job, worktree, session_id_opt orelse "", steer);
}

/// steer.config → { enabled, relayUrl } (false on any error/404 — graceful-off).
fn steerEnabled(gpa: std.mem.Allocator, job: *Job) bool {
    var resp = trpc.query(gpa, job.instance, "steer.config", job.token, 15) catch return false;
    defer resp.deinit();
    if (!resp.ok()) return false;
    const obj = dataObject(&resp) orelse return false;
    return trpc.objBool(obj, "enabled");
}

// --- step 1: repositories.forIssue ---

const Repo = struct {
    repository_id: []u8,
    fn deinit(self: Repo, gpa: std.mem.Allocator) void {
        gpa.free(self.repository_id);
    }
};

fn resolveRepo(gpa: std.mem.Allocator, job: *Job) ?Repo {
    const input = std.fmt.allocPrint(gpa, "{{\"issueId\":\"{s}\"}}", .{job.issue_id}) catch return null;
    defer gpa.free(input);
    var resp = trpc.queryInput(gpa, job.instance, "repositories.forIssue", input, job.token, 30) catch return null;
    defer resp.deinit();
    if (!resp.ok()) return null;
    const obj = dataObject(&resp) orelse return null; // null repo ⇒ not linked
    const rid = trpc.objString(obj, "repositoryId") orelse return null;
    return .{ .repository_id = gpa.dupe(u8, rid) catch return null };
}

// --- step 2: repositories.installationToken ---

const Token = struct {
    token: []u8,
    full_name: []u8,
    default_branch: []u8,
    fn deinit(self: Token, gpa: std.mem.Allocator) void {
        gpa.free(self.token);
        gpa.free(self.full_name);
        gpa.free(self.default_branch);
    }
};

fn mintToken(gpa: std.mem.Allocator, job: *Job, repository_id: []const u8) ?Token {
    const input = std.fmt.allocPrint(gpa, "{{\"repositoryId\":\"{s}\"}}", .{repository_id}) catch return null;
    defer gpa.free(input);
    var resp = trpc.call(gpa, job.instance, "repositories.installationToken", input, job.token, 30) catch return null;
    defer resp.deinit();
    if (!resp.ok()) return null;
    const obj = dataObject(&resp) orelse return null;
    const token = trpc.objString(obj, "token") orelse return null;
    const full_name = trpc.objString(obj, "fullName") orelse return null;
    const default_branch = trpc.objString(obj, "defaultBranch") orelse "main";
    return .{
        .token = gpa.dupe(u8, token) catch return null,
        .full_name = gpa.dupe(u8, full_name) catch return null,
        .default_branch = gpa.dupe(u8, default_branch) catch return null,
    };
}

// --- step 6: codingSessions.start ---

/// Returns the new `coding_sessions` row id (caller-owned; "" on failure).
fn startSession(gpa: std.mem.Allocator, job: *Job) ?[]u8 {
    const label = credentials.hostDeviceLabel(gpa);
    defer gpa.free(label);
    const input = std.fmt.allocPrint(gpa, "{{\"issueId\":\"{s}\",\"deviceLabel\":\"{s}\"}}", .{ job.issue_id, label }) catch return null;
    defer gpa.free(input);
    var resp = trpc.call(gpa, job.instance, "codingSessions.start", input, job.token, 30) catch return null;
    defer resp.deinit();
    if (!resp.ok()) return null;
    // Shape: { session: { id, … } } (after the transformer `json` unwrap).
    const outer = dataObject(&resp) orelse return null;
    const session = trpc.asObject(outer.get("session") orelse return null) orelse return null;
    const id = trpc.objString(session, "id") orelse return null;
    return gpa.dupe(u8, id) catch null;
}

// --- worktree files ---

fn writeWorktreeFiles(
    gpa: std.mem.Allocator,
    job: *Job,
    cred: *const credentials.Store,
    worktree: []const u8,
    branch: []const u8,
    personal_key: []const u8,
) !void {
    // .mcp.json (contains the bearer token — 0600 + git-ignored).
    const mcp = try prompt.mcpJson(gpa, job.instance, personal_key);
    defer gpa.free(mcp);
    try writeInWorktree(gpa, worktree, ".mcp.json", mcp);

    const prompt_md = try prompt.composePrompt(gpa, job.identifier, job.title, job.description, branch);
    defer gpa.free(prompt_md);
    try writeInWorktree(gpa, worktree, ".exp-prompt.md", prompt_md);

    const script = try prompt.launchScript(gpa, cred.claudePath());
    defer gpa.free(script);
    try writeInWorktree(gpa, worktree, ".exp-run.sh", script);
    // Make the launcher script executable (writeSecret is 0600).
    chmodInWorktree(gpa, worktree, ".exp-run.sh", 0o700);

    // Keep our scratch files out of `git status` so Claude never commits the token.
    excludeInWorktree(gpa, worktree);
}

fn writeInWorktree(gpa: std.mem.Allocator, worktree: []const u8, name: []const u8, data: []const u8) !void {
    const path = try std.fs.path.join(gpa, &.{ worktree, name });
    defer gpa.free(path);
    try storage.writeSecret(path, data);
}

extern "c" fn chmod(path: [*:0]const u8, mode: c_uint) c_int;

fn chmodInWorktree(gpa: std.mem.Allocator, worktree: []const u8, name: []const u8, mode: c_uint) void {
    const path = std.fs.path.joinZ(gpa, &.{ worktree, name }) catch return;
    defer gpa.free(path);
    _ = chmod(path.ptr, mode);
}

/// Append our scratch filenames to the worktree's git exclude file.
fn excludeInWorktree(gpa: std.mem.Allocator, worktree: []const u8) void {
    // `git rev-parse --git-path info/exclude` resolves the correct file even for
    // linked worktrees (whose `.git` is a file pointing at the common dir).
    var arena = std.heap.ArenaAllocator.init(gpa);
    defer arena.deinit();
    const a = arena.allocator();
    const res = std.process.run(a, std.Io.Threaded.global_single_threaded.io(), .{
        .argv = &.{ "git", "-C", worktree, "rev-parse", "--git-path", "info/exclude" },
        .stdout_limit = .limited(64 * 1024),
        .stderr_limit = .limited(64 * 1024),
    }) catch return;
    if (!(res.term == .exited and res.term.exited == 0)) return;
    const rel = std.mem.trim(u8, res.stdout, " \t\r\n");
    if (rel.len == 0) return;
    // The path is relative to the worktree; resolve it.
    const abs = if (std.fs.path.isAbsolute(rel)) a.dupe(u8, rel) catch return else std.fs.path.join(a, &.{ worktree, rel }) catch return;
    const existing = storage.readFileAlloc(a, abs) orelse "";
    if (std.mem.indexOf(u8, existing, ".mcp.json") != null) return; // already added
    const merged = std.fmt.allocPrint(a, "{s}\n.mcp.json\n.exp-prompt.md\n.exp-run.sh\n", .{existing}) catch return;
    std.Io.Dir.cwd().writeFile(std.Io.Threaded.global_single_threaded.io(), .{ .sub_path = abs, .data = merged }) catch {};
}

// --- main-thread marshaling ---

const MountData = struct {
    gpa: std.mem.Allocator,
    mount: MountFn,
    mount_ctx: ?*anyopaque,
    instance: []u8,
    token: ?[]u8,
    user_id: ?[]u8, // §3.4: distinguish remote steerers from the local user
    session_id: []u8, // "" ⇒ no coding_sessions.end call
    issue_id: []u8, // for the publisher's hello frame
    steer: bool, // relay enabled ⇒ host-PTY terminal + publisher
    cwd: []u8,
    command: []u8,
    title: [:0]u8,
    key: [:0]u8, // dock-tab key (the session id; "" ⇒ dock generates one)
};

fn scheduleMount(job: *Job, worktree: []const u8, session_id: []const u8, steer: bool) void {
    buildMountData(job, worktree, session_id, steer) catch {};
}

/// Build the MountData incrementally so `errdefer` frees exactly what succeeded
/// (a struct-literal `catch` can't free fields the struct hasn't taken yet).
fn buildMountData(job: *Job, worktree: []const u8, session_id: []const u8, steer: bool) !void {
    const gpa = job.gpa;
    const md = try gpa.create(MountData);
    errdefer gpa.destroy(md);
    md.gpa = gpa;
    md.mount = job.mount;
    md.mount_ctx = job.mount_ctx;
    md.steer = steer;
    md.instance = try gpa.dupe(u8, job.instance);
    errdefer gpa.free(md.instance);
    md.token = if (job.token) |t| try gpa.dupe(u8, t) else null;
    errdefer if (md.token) |t| gpa.free(t);
    md.user_id = if (job.user_id) |u| try gpa.dupe(u8, u) else null;
    errdefer if (md.user_id) |u| gpa.free(u);
    md.session_id = try gpa.dupe(u8, session_id); // "" ⇒ 0-len heap slice
    errdefer gpa.free(md.session_id);
    md.issue_id = try gpa.dupe(u8, job.issue_id);
    errdefer gpa.free(md.issue_id);
    md.cwd = try gpa.dupe(u8, worktree);
    errdefer gpa.free(md.cwd);
    // `sh .exp-run.sh` — two space-separated tokens, no intra-token spaces, so
    // it's safe regardless of how ghostty tokenises `command`. The claude path
    // was already baked into the launcher script.
    md.command = try gpa.dupe(u8, "sh .exp-run.sh");
    errdefer gpa.free(md.command);
    md.title = try gpa.dupeZ(u8, job.identifier);
    errdefer gpa.free(md.title);
    md.key = try gpa.dupeZ(u8, session_id);
    _ = gtk.g_idle_add(@ptrCast(&onMountMain), md);
}

/// The end-of-session context, kept alive for the terminal's lifetime so the
/// child-exit callback can flip the `coding_sessions` row to `ended`.
const EndCtx = struct {
    gpa: std.mem.Allocator,
    instance: []u8,
    token: ?[]u8,
    session_id: []u8,
};

fn onMountMain(data: gtk.gpointer) callconv(.c) c_int {
    const md: *MountData = @ptrCast(@alignCast(data));
    const gpa = md.gpa;
    defer {
        gpa.free(md.instance);
        if (md.token) |t| gpa.free(t);
        if (md.user_id) |u| gpa.free(u);
        gpa.free(md.session_id); // always heap (dup of "" when no row)
        gpa.free(md.issue_id);
        gpa.free(md.cwd);
        gpa.free(md.command);
        gpa.free(md.title);
        gpa.free(md.key);
        gpa.destroy(md);
    }

    // Steer path: host-owned PTY + relay publisher (masterplan §3.3). Any
    // failure falls through to the plain local path below.
    if (md.steer and md.session_id.len > 0) {
        if (mountSteerTerminal(md)) return 0;
    }

    // Wire the session-end call to the child exit (only when we have a row id).
    const end_ctx: ?*EndCtx = if (md.session_id.len > 0) buildEndCtx(md) catch null else null;

    const term = terminal.create(gpa, .{
        .cwd = md.cwd,
        .command = md.command,
        .wait_after_command = true,
        .on_exit = if (end_ctx != null) onSessionExit else null,
        .on_exit_ctx = end_ctx,
    }) orelse {
        // Terminal failed — end the session immediately so it doesn't dangle.
        if (end_ctx) |ec| {
            endSession(ec);
            freeEndCtx(ec);
        }
        return 0;
    };
    md.mount(md.mount_ctx, term, md.title.ptr, md.key.ptr);
    return 0; // G_SOURCE_REMOVE
}

/// Build the EndCtx incrementally (errdefer frees exactly what succeeded).
fn buildEndCtx(md: *MountData) !*EndCtx {
    const gpa = md.gpa;
    const ec = try gpa.create(EndCtx);
    errdefer gpa.destroy(ec);
    ec.gpa = gpa;
    ec.instance = try gpa.dupe(u8, md.instance);
    errdefer gpa.free(ec.instance);
    ec.token = if (md.token) |t| try gpa.dupe(u8, t) else null;
    errdefer if (ec.token) |t| gpa.free(t);
    ec.session_id = try gpa.dupe(u8, md.session_id);
    return ec;
}

fn onSessionExit(ctx: ?*anyopaque, exit_code: i32) void {
    _ = exit_code;
    const ec: *EndCtx = @ptrCast(@alignCast(ctx orelse return));
    endSession(ec);
    freeEndCtx(ec);
}

fn endSession(ec: *EndCtx) void {
    var arena = std.heap.ArenaAllocator.init(ec.gpa);
    defer arena.deinit();
    const json = std.fmt.allocPrint(arena.allocator(), "{{\"id\":\"{s}\"}}", .{ec.session_id}) catch return;
    mutate.fire(ec.gpa, ec.instance, ec.token, "codingSessions.end", json);
}

fn freeEndCtx(ec: *EndCtx) void {
    const gpa = ec.gpa;
    gpa.free(ec.instance);
    if (ec.token) |t| gpa.free(t);
    gpa.free(ec.session_id);
    gpa.destroy(ec);
}

// --- error marshaling ---

const ErrData = struct {
    gpa: std.mem.Allocator,
    on_error: ErrorFn,
    error_ctx: ?*anyopaque,
    message: [:0]u8,
};

fn fail(job: *Job, message: []const u8) void {
    const gpa = job.gpa;
    const ed = gpa.create(ErrData) catch return;
    ed.* = .{
        .gpa = gpa,
        .on_error = job.on_error,
        .error_ctx = job.error_ctx,
        .message = gpa.dupeZ(u8, message) catch {
            gpa.destroy(ed);
            return;
        },
    };
    _ = gtk.g_idle_add(@ptrCast(&onErrorMain), ed);
}

fn onErrorMain(data: gtk.gpointer) callconv(.c) c_int {
    const ed: *ErrData = @ptrCast(@alignCast(data));
    ed.on_error(ed.error_ctx, ed.message.ptr);
    ed.gpa.free(ed.message);
    ed.gpa.destroy(ed);
    return 0;
}

// --- shared JSON helper ---

/// The `result.data` object, transparently unwrapping a `{ json: … }` layer if
/// the server wraps it (mirrors `prDiffWorker`). Returns null for a JSON-null
/// (e.g. `forIssue` → not linked) or a non-object payload.
fn dataObject(resp: *const trpc.Response) ?std.json.ObjectMap {
    const dv = resp.data() orelse return null;
    var obj = trpc.asObject(dv) orelse return null;
    if (obj.get("json")) |inner| {
        if (trpc.asObject(inner)) |inner_obj| obj = inner_obj;
    }
    return obj;
}

// ---------------------------------------------------------------------------
// Steer host-PTY session (masterplan §3.3)
// ---------------------------------------------------------------------------
//
// When the relay is enabled, the launcher owns the `claude` child on a host
// PTY instead of letting ghostty exec it: the PTY reader tees every output
// chunk to (a) the local ghostty surface (manual-IO, marshalled to the GTK
// main loop) and (b) the relay publisher (binary 0x01 frames). Local ghostty
// keystrokes and remote steerer `input` frames are written to the SAME PTY
// master fd. One SteerSession per terminal tab, keyed by coding_sessions.id.
//
// Lifetime: refcounted. The terminal widget holds the owner ref; every
// cross-thread g_idle_add marshal holds one. Teardown (tab close) marks the
// session ended, SIGKILLs the child group, and defers the joins to a follow-up
// idle so libghostty's surface is fully freed first (destroyTerm frees the
// surface right after firing on_exit).
//
// §3.4 banner: the mounted tab child is a wrapper box (hidden "Remote
// steering — <name>" strip + the GLArea) so presence can surface the remote
// claim; "Take over" sends release-then-claim on the publisher socket.

const SteerSession = struct {
    gpa: std.mem.Allocator,
    refs: std.atomic.Value(u32),
    instance: []u8,
    token: ?[]u8,
    user_id: ?[]u8, // the local signed-in user (for the §3.4 banner)
    session_id: []u8,
    pty: ?*host_pty.HostPty = null,
    publisher: ?*steer_pub.Publisher = null,
    area: gtk.Object = null,
    banner: gtk.Object = null, // §3.4 "Remote steering" strip (main thread)
    banner_label: gtk.Object = null,
    term_alive: bool = false, // main thread only
    ended: bool = false, // main thread only: codingSessions.end already fired
    child_exited: bool = false, // main thread mirror
    torn_down: bool = false, // main thread: final teardown already scheduled

    fn ref(self: *SteerSession) void {
        _ = self.refs.fetchAdd(1, .acq_rel);
    }

    fn unref(self: *SteerSession) void {
        if (self.refs.fetchSub(1, .acq_rel) != 1) return;
        const gpa = self.gpa;
        gpa.free(self.instance);
        if (self.token) |t| gpa.free(t);
        if (self.user_id) |u| gpa.free(u);
        gpa.free(self.session_id);
        gpa.destroy(self);
    }
};

/// Build the steer session + host PTY + publisher + manual-IO terminal and
/// mount it. Returns false on any setup failure — the caller falls back to the
/// plain exec-mode path (which then also owns the codingSessions.end call).
fn mountSteerTerminal(md: *MountData) bool {
    const gpa = md.gpa;

    const instance = gpa.dupe(u8, md.instance) catch return false;
    const token: ?[]u8 = if (md.token) |t| (gpa.dupe(u8, t) catch null) else null;
    const user_id: ?[]u8 = if (md.user_id) |u| (gpa.dupe(u8, u) catch null) else null;
    const session_id = gpa.dupe(u8, md.session_id) catch {
        gpa.free(instance);
        if (token) |t| gpa.free(t);
        if (user_id) |u| gpa.free(u);
        return false;
    };
    const session = gpa.create(SteerSession) catch {
        gpa.free(instance);
        if (token) |t| gpa.free(t);
        if (user_id) |u| gpa.free(u);
        gpa.free(session_id);
        return false;
    };
    session.* = .{
        .gpa = gpa,
        .refs = std.atomic.Value(u32).init(1), // the terminal/main owner
        .instance = instance,
        .token = token,
        .user_id = user_id,
        .session_id = session_id,
    };

    // Publisher first (it dials out on its own thread; remote input lands on
    // the PTY through the session). A null publisher is fine — the session
    // simply runs local-only.
    session.publisher = steer_pub.Publisher.create(gpa, .{
        .instance = md.instance,
        .token = md.token,
        .session_id = md.session_id,
        .issue_id = md.issue_id,
        .on_input = onRemoteInput,
        .on_kill = onRemoteKill,
        .on_remote_resize = onRemoteResize,
        .on_presence = onPresenceFrame,
        .ctx = session,
    });

    session.pty = host_pty.HostPty.spawn(gpa, .{
        .argv = &.{ "/bin/sh", ".exp-run.sh" },
        .cwd = md.cwd,
        .on_output = onPtyOutput,
        .on_exit = onPtyExit,
        .ctx = session,
    }) catch {
        if (session.publisher) |p| p.destroy();
        session.publisher = null;
        session.unref();
        return false;
    };

    const term = terminal.create(gpa, .{
        .cwd = md.cwd,
        .manual_io = true,
        .io_write = onLocalIoWrite,
        .io_write_ctx = session,
        .on_grid = onGridChanged,
        .on_grid_ctx = session,
        .on_exit = onSteerTermClosed,
        .on_exit_ctx = session,
    }) orelse {
        // No terminal ⇒ kill the child and hand the session back to the
        // fallback exec path (its EndCtx owns codingSessions.end from here).
        if (session.pty) |p| p.destroy();
        session.pty = null;
        if (session.publisher) |p| p.destroy();
        session.publisher = null;
        session.unref();
        return false;
    };

    // §3.4 local steering banner: the tab child is a wrapper box holding a
    // hidden "Remote steering — <name>" strip above the terminal, so a tab
    // detach reparents banner + ghostty surface together. Local typing is
    // never gated — the banner only surfaces the claim + offers "Take over".
    const wrapper = gtk.gtk_box_new(gtk.ORIENTATION_VERTICAL, 0);
    const banner = gtk.gtk_box_new(gtk.ORIENTATION_HORIZONTAL, 8);
    gtk.gtk_widget_add_css_class(banner, "exp-steer-banner");
    const banner_label = gtk.gtk_label_new("Remote steering");
    gtk.gtk_widget_set_halign(banner_label, gtk.ALIGN_START);
    gtk.gtk_widget_set_hexpand(banner_label, 1);
    gtk.gtk_label_set_ellipsize(banner_label, gtk.ELLIPSIZE_END);
    gtk.gtk_box_append(banner, banner_label);
    const takeover_btn = gtk.gtk_button_new_with_label("Take over");
    gtk.gtk_widget_add_css_class(takeover_btn, "flat");
    gtk.gtk_widget_set_tooltip_text(takeover_btn, "Reclaim steering from the remote viewer");
    _ = gtk.g_signal_connect_data(takeover_btn, "clicked", @ptrCast(&onTakeOverClicked), session, null, 0);
    gtk.gtk_box_append(banner, takeover_btn);
    gtk.gtk_widget_set_visible(banner, 0); // shown only while a remote steers
    gtk.gtk_box_append(wrapper, banner);
    gtk.gtk_box_append(wrapper, term);

    session.area = term;
    session.banner = banner;
    session.banner_label = banner_label;
    session.term_alive = true;
    md.mount(md.mount_ctx, wrapper, md.title.ptr, md.key.ptr);
    return true;
}

/// Flip the coding_sessions row to ended + close the relay room. Main thread;
/// the first caller's outcome wins.
fn endSteerSession(session: *SteerSession, outcome: []const u8) void {
    if (session.ended) return;
    session.ended = true;
    var arena = std.heap.ArenaAllocator.init(session.gpa);
    defer arena.deinit();
    if (std.fmt.allocPrint(arena.allocator(), "{{\"id\":\"{s}\"}}", .{session.session_id})) |json| {
        mutate.fire(session.gpa, session.instance, session.token, "codingSessions.end", json);
    } else |_| {}
    if (session.publisher) |p| p.stop(outcome);
}

// --- callbacks from ghostty (local input / grid) ---

/// ghostty io_write_cb: local keystrokes → the PTY master. May fire off the
/// main thread; writeInput is thread-safe and no-ops after child exit.
fn onLocalIoWrite(ctx: ?*anyopaque, data: [*c]const u8, len: usize) callconv(.c) void {
    const session: *SteerSession = @ptrCast(@alignCast(ctx orelse return));
    if (data == null or len == 0) return;
    const pty = session.pty orelse return;
    pty.writeInput(data[0..len]);
}

/// Main thread: the surface's cell grid changed — propagate to the PTY winsize
/// (child gets SIGWINCH) and tell remote viewers to reflow.
fn onGridChanged(ctx: ?*anyopaque, cols: u16, rows: u16) void {
    const session: *SteerSession = @ptrCast(@alignCast(ctx orelse return));
    if (session.pty) |p| p.setWinsize(cols, rows);
    if (session.publisher) |p| p.sendResize(cols, rows);
}

// --- callbacks from the host PTY (reader thread) ---

const FeedData = struct { session: *SteerSession, bytes: []u8 };

/// PTY reader thread: tee one output chunk to the relay (direct — the
/// publisher is thread-safe) and to the local surface (via the main loop).
fn onPtyOutput(ctx: ?*anyopaque, bytes: []const u8) void {
    const session: *SteerSession = @ptrCast(@alignCast(ctx orelse return));
    if (session.publisher) |p| p.feed(bytes);

    const fd = session.gpa.create(FeedData) catch return;
    fd.* = .{
        .session = session,
        .bytes = session.gpa.dupe(u8, bytes) catch {
            session.gpa.destroy(fd);
            return;
        },
    };
    session.ref();
    _ = gtk.g_idle_add(@ptrCast(&feedTerminalIdle), fd);
}

fn feedTerminalIdle(data: gtk.gpointer) callconv(.c) c_int {
    const fd: *FeedData = @ptrCast(@alignCast(data));
    const session = fd.session;
    if (session.term_alive) terminal.feedOutput(session.area, fd.bytes);
    session.gpa.free(fd.bytes);
    session.gpa.destroy(fd);
    session.unref();
    return 0; // G_SOURCE_REMOVE
}

/// PTY reader thread: the child exited (naturally or killed).
fn onPtyExit(ctx: ?*anyopaque, exit_code: i32) void {
    _ = exit_code;
    const session: *SteerSession = @ptrCast(@alignCast(ctx orelse return));
    session.ref();
    _ = gtk.g_idle_add(@ptrCast(&ptyExitIdle), session);
}

fn ptyExitIdle(data: gtk.gpointer) callconv(.c) c_int {
    const session: *SteerSession = @ptrCast(@alignCast(data));
    session.child_exited = true;
    endSteerSession(session, "ended");
    session.unref();
    return 0;
}

// --- callbacks from the relay publisher (socket thread) ---

/// Steerer keystrokes → the same PTY master local keys use. Socket thread;
/// safe because the pty struct outlives the publisher (teardown order).
fn onRemoteInput(ctx: ?*anyopaque, bytes: []const u8) void {
    const session: *SteerSession = @ptrCast(@alignCast(ctx orelse return));
    const pty = session.pty orelse return;
    pty.writeInput(bytes);
}

fn onRemoteKill(ctx: ?*anyopaque) void {
    const session: *SteerSession = @ptrCast(@alignCast(ctx orelse return));
    session.ref();
    _ = gtk.g_idle_add(@ptrCast(&remoteKillIdle), session);
}

fn remoteKillIdle(data: gtk.gpointer) callconv(.c) c_int {
    const session: *SteerSession = @ptrCast(@alignCast(data));
    // End first so the outcome reads "killed", then nuke the child group; the
    // PTY exit path handles the rest. The terminal tab stays open showing the
    // final output (matching the local-exit behavior).
    endSteerSession(session, "killed");
    if (session.pty) |p| p.kill(host_pty.SIGKILL);
    session.unref();
    return 0;
}

// --- §3.4 presence → the "Remote steering — <name>" banner ---

const PresenceData = struct {
    session: *SteerSession,
    steerer_id: ?[]u8,
    steerer_name: ?[]u8,
};

/// Publisher presence callback — SOCKET thread. Dupe + hop to the main loop.
fn onPresenceFrame(ctx: ?*anyopaque, steerer_id: ?[]const u8, steerer_name: ?[]const u8) void {
    const session: *SteerSession = @ptrCast(@alignCast(ctx orelse return));
    const gpa = session.gpa;
    const pd = gpa.create(PresenceData) catch return;
    pd.* = .{
        .session = session,
        // A failed id dupe degrades to "nobody steering" — hide, never crash.
        .steerer_id = if (steerer_id) |s| (gpa.dupe(u8, s) catch null) else null,
        .steerer_name = if (steerer_name) |s| (gpa.dupe(u8, s) catch null) else null,
    };
    session.ref();
    _ = gtk.g_idle_add(@ptrCast(&presenceIdle), pd);
}

fn presenceIdle(data: gtk.gpointer) callconv(.c) c_int {
    const pd: *PresenceData = @ptrCast(@alignCast(data));
    const session = pd.session;
    const gpa = session.gpa;
    defer {
        if (pd.steerer_id) |s| gpa.free(s);
        if (pd.steerer_name) |s| gpa.free(s);
        gpa.destroy(pd);
        session.unref();
    }
    if (!session.term_alive) return 0;
    const banner = session.banner orelse return 0;

    // Only a REMOTE steerer raises the banner — the local user steering via
    // their own viewer (same userId) is not "remote steering".
    const remote_who: ?[]const u8 = blk: {
        const id = pd.steerer_id orelse break :blk null;
        if (session.user_id) |own| {
            if (std.mem.eql(u8, own, id)) break :blk null;
        }
        break :blk (pd.steerer_name orelse id);
    };
    if (remote_who) |who| {
        if (std.fmt.allocPrintSentinel(gpa, "Remote steering — {s}", .{who}, 0)) |z| {
            defer gpa.free(z);
            gtk.gtk_label_set_text(session.banner_label, z.ptr);
        } else |_| {}
        gtk.gtk_widget_set_visible(banner, 1);
    } else {
        gtk.gtk_widget_set_visible(banner, 0);
    }
    return 0; // G_SOURCE_REMOVE
}

/// "Take over" (main thread): release-then-claim on the local user's behalf —
/// their machine wins (§3.4). The banner hides when the resulting presence
/// broadcast reports the remote claim gone.
fn onTakeOverClicked(_: gtk.Object, data: gtk.gpointer) callconv(.c) void {
    const session: *SteerSession = @ptrCast(@alignCast(data));
    if (session.publisher) |p| p.takeOver();
}

fn onRemoteResize(ctx: ?*anyopaque, cols: u16, rows: u16) void {
    const session: *SteerSession = @ptrCast(@alignCast(ctx orelse return));
    const rd = session.gpa.create(RemoteResizeData) catch return;
    rd.* = .{ .session = session, .cols = cols, .rows = rows };
    session.ref();
    _ = gtk.g_idle_add(@ptrCast(&remoteResizeIdle), rd);
}

const RemoteResizeData = struct { session: *SteerSession, cols: u16, rows: u16 };

fn remoteResizeIdle(data: gtk.gpointer) callconv(.c) c_int {
    const rd: *RemoteResizeData = @ptrCast(@alignCast(data));
    const session = rd.session;
    // Update the PTY winsize only — the local GTK grid keeps following its own
    // widget size (the remote view is a viewer, not the owner of the layout).
    if (session.pty) |p| p.setWinsize(rd.cols, rd.rows);
    session.gpa.destroy(rd);
    session.unref();
    return 0;
}

// --- teardown (terminal widget destroyed = tab closed / sign-out) ---

/// Fired by destroyTerm's fallback (manual-IO mode has no ghostty child), on
/// the main thread, exactly once per terminal.
fn onSteerTermClosed(ctx: ?*anyopaque, exit_code: i32) void {
    _ = exit_code;
    const session: *SteerSession = @ptrCast(@alignCast(ctx orelse return));
    if (session.torn_down) return;
    session.torn_down = true;
    session.term_alive = false;
    session.area = null;
    session.banner = null; // dies with the wrapper (same dispose cascade)
    session.banner_label = null;
    endSteerSession(session, "closed");
    // Signal-only here: destroyTerm frees the ghostty surface right after this
    // callback, so the joins are deferred to a follow-up idle. The master fd
    // stays open until then, keeping any concurrent PTY write harmless.
    if (session.pty) |p| p.kill(host_pty.SIGKILL);
    session.ref();
    _ = gtk.g_idle_add(@ptrCast(&steerTeardownIdle), session);
}

fn steerTeardownIdle(data: gtk.gpointer) callconv(.c) c_int {
    const session: *SteerSession = @ptrCast(@alignCast(data));
    // Ordered quiesce: (1) join the PTY reader (no more feeds/marshals), then
    // (2) join the publisher socket (no more remote input), then (3) free the
    // pty (nobody left who could write to the master fd).
    if (session.pty) |p| p.shutdown();
    if (session.publisher) |p| p.destroy();
    session.publisher = null;
    if (session.pty) |p| p.destroy();
    session.pty = null;
    session.unref(); // this idle's ref
    session.unref(); // the terminal/main owner ref
    return 0;
}
