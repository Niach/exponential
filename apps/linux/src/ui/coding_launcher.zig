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

/// Insert a ready ghostty terminal into the UI (main thread). `title` is a issue
/// identifier like "EXP-123".
pub const MountFn = *const fn (ctx: ?*anyopaque, term: gtk.Object, title: [*:0]const u8) void;
/// Surface a launcher error / CTA (main thread).
pub const ErrorFn = *const fn (ctx: ?*anyopaque, message: [*:0]const u8) void;

pub const Request = struct {
    /// Thread-safe (the app uses `std.heap.page_allocator`).
    gpa: std.mem.Allocator,
    instance: []const u8,
    token: ?[]const u8,
    issue_id: []const u8,
    identifier: []const u8, // e.g. "EXP-123" — drives the branch name + tab title
    title: []const u8,
    description: []const u8, // issue markdown ("" when none)
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
    issue_id: []u8,
    identifier: []u8,
    title: []u8,
    description: []u8,
    mount: MountFn,
    mount_ctx: ?*anyopaque,
    on_error: ErrorFn,
    error_ctx: ?*anyopaque,
};

/// Kick off the launcher. Dups every input, then runs the network + git work on
/// a detached worker (never blocks the GTK loop). Safe to call from any thread.
pub fn start(req: Request) void {
    const gpa = req.gpa;
    // Dup into locals first, so a mid-way OOM frees exactly what succeeded (a
    // struct-literal `catch` can't free fields the struct hasn't taken yet).
    const instance = gpa.dupe(u8, req.instance) catch return;
    const issue_id = gpa.dupe(u8, req.issue_id) catch return free1(gpa, instance);
    const identifier = gpa.dupe(u8, req.identifier) catch return free2(gpa, instance, issue_id);
    const title = gpa.dupe(u8, req.title) catch return free3(gpa, instance, issue_id, identifier);
    const description = gpa.dupe(u8, req.description) catch return free4(gpa, instance, issue_id, identifier, title);
    const token: ?[]u8 = if (req.token) |t| (gpa.dupe(u8, t) catch null) else null;

    const job = gpa.create(Job) catch {
        free4(gpa, instance, issue_id, identifier, title);
        gpa.free(description);
        if (token) |t| gpa.free(t);
        return;
    };
    job.* = .{
        .gpa = gpa,
        .instance = instance,
        .token = token,
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

    // 6. Record the coding session (running).
    const session_id = startSession(gpa, job) orelse ""; // best-effort; empty ⇒ no end call

    // 7. Marshal the terminal spawn back to the main thread.
    scheduleMount(job, &cred, worktree, session_id);
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
    const label = deviceLabel(gpa);
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

fn deviceLabel(gpa: std.mem.Allocator) []u8 {
    if (storage.readFileAlloc(gpa, "/etc/hostname")) |bytes| {
        defer gpa.free(bytes);
        const trimmed = std.mem.trim(u8, bytes, " \t\r\n");
        if (trimmed.len > 0) return gpa.dupe(u8, trimmed) catch (gpa.dupe(u8, "Linux desktop") catch unreachable);
    }
    return gpa.dupe(u8, "Linux desktop") catch unreachable;
}

// --- main-thread marshaling ---

const MountData = struct {
    gpa: std.mem.Allocator,
    mount: MountFn,
    mount_ctx: ?*anyopaque,
    instance: []u8,
    token: ?[]u8,
    session_id: []u8, // "" ⇒ no coding_sessions.end call
    cwd: []u8,
    command: []u8,
    title: [:0]u8,
};

fn scheduleMount(job: *Job, cred: *const credentials.Store, worktree: []const u8, session_id: []const u8) void {
    const gpa = job.gpa;
    const md = gpa.create(MountData) catch return;
    md.* = .{
        .gpa = gpa,
        .mount = job.mount,
        .mount_ctx = job.mount_ctx,
        .instance = gpa.dupe(u8, job.instance) catch return gpa.destroy(md),
        .token = if (job.token) |t| (gpa.dupe(u8, t) catch null) else null,
        .session_id = gpa.dupe(u8, session_id) catch "",
        .cwd = gpa.dupe(u8, worktree) catch return freeMountPartial(md),
        // `sh .exp-run.sh` — two space-separated tokens, no intra-token spaces,
        // so it's safe regardless of how ghostty tokenises `command`.
        .command = gpa.dupe(u8, "sh .exp-run.sh") catch return freeMountPartial(md),
        .title = gpa.dupeZ(u8, job.identifier) catch return freeMountPartial(md),
    };
    _ = cred; // claude_path was already baked into the launcher script
    _ = gtk.g_idle_add(@ptrCast(&onMountMain), md);
}

fn freeMountPartial(md: *MountData) void {
    const gpa = md.gpa;
    gpa.free(md.instance);
    if (md.token) |t| gpa.free(t);
    gpa.destroy(md);
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
        if (md.session_id.len > 0) gpa.free(md.session_id);
        gpa.free(md.cwd);
        gpa.free(md.command);
        gpa.free(md.title);
        gpa.destroy(md);
    }

    // Wire the session-end call to the child exit (only when we have a row id).
    var end_ctx: ?*EndCtx = null;
    if (md.session_id.len > 0) {
        if (gpa.create(EndCtx)) |ec| {
            ec.* = .{
                .gpa = gpa,
                .instance = gpa.dupe(u8, md.instance) catch {
                    gpa.destroy(ec);
                    return 0;
                },
                .token = if (md.token) |t| (gpa.dupe(u8, t) catch null) else null,
                .session_id = gpa.dupe(u8, md.session_id) catch {
                    gpa.free(ec.instance);
                    gpa.destroy(ec);
                    return 0;
                },
            };
            end_ctx = ec;
        } else |_| {}
    }

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
    md.mount(md.mount_ctx, term, md.title.ptr);
    return 0; // G_SOURCE_REMOVE
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
