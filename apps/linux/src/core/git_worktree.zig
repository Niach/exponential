//! Host-side git plumbing for the "Start coding" launcher (§4a) — spawns the
//! `git` binary directly (argv arrays, never a shell, so paths/branches with
//! spaces or metacharacters are safe) and NEVER shells out to `gh`. Push auth is
//! a short-lived, token-embedded `origin` URL
//! (`https://x-access-token:<token>@github.com/<owner>/<name>.git`) minted per
//! session by `repositories.installationToken`; nothing is persisted.
//!
//! Layout under the configured repos root:
//!   <reposRoot>/<owner>/<name>              — the shared clone (fetched, never worked in)
//!   <reposRoot>/<owner>/<name>.worktrees/<branch>  — one linked worktree per issue
//!
//! One issue ⇒ one branch `<prefix><IDENTIFIER>` ⇒ one worktree: an existing
//! branch/worktree is REUSED, not recreated.
//!
//! The pure helpers (token URL, branch name, worktree path, sanitize) are split
//! out so they unit-test headlessly; the git ops need a real repo + network.

const std = @import("std");
const storage = @import("storage.zig");

// ---------------------------------------------------------------------------
// Pure helpers (unit-tested; no process/network)
// ---------------------------------------------------------------------------

/// `https://x-access-token:<token>@github.com/<full_name>.git`. `full_name` is
/// `owner/name`; a trailing `.git` (if the registry ever stores one) isn't
/// doubled. Caller owns the returned string.
pub fn tokenRemoteUrl(gpa: std.mem.Allocator, token: []const u8, full_name: []const u8) ![]u8 {
    const trimmed = if (std.mem.endsWith(u8, full_name, ".git"))
        full_name[0 .. full_name.len - ".git".len]
    else
        full_name;
    return std.fmt.allocPrint(gpa, "https://x-access-token:{s}@github.com/{s}.git", .{ token, trimmed });
}

/// `<prefix><identifier>` (e.g. `exp/EXP-123`). Caller owns the result.
pub fn branchName(gpa: std.mem.Allocator, prefix: []const u8, identifier: []const u8) ![]u8 {
    return std.fmt.allocPrint(gpa, "{s}{s}", .{ prefix, identifier });
}

/// A filesystem-safe leaf derived from a branch name: `/` and other path-unsafe
/// bytes become `-`. Caller owns the result.
pub fn sanitizeBranch(gpa: std.mem.Allocator, branch: []const u8) ![]u8 {
    const out = try gpa.alloc(u8, branch.len);
    for (branch, 0..) |c, i| {
        out[i] = switch (c) {
            '/', '\\', ':', ' ', '\t', '~', '^', '?', '*', '[' => '-',
            else => c,
        };
    }
    return out;
}

/// The worktree path for `branch` beside `clone_path`:
/// `<clone_path>.worktrees/<sanitized-branch>`. Caller owns the result.
pub fn worktreePath(gpa: std.mem.Allocator, clone_path: []const u8, branch: []const u8) ![]u8 {
    const leaf = try sanitizeBranch(gpa, branch);
    defer gpa.free(leaf);
    return std.fmt.allocPrint(gpa, "{s}.worktrees/{s}", .{ clone_path, leaf });
}

/// `<repos_root>/<full_name>` (== `<repos_root>/<owner>/<name>`). Caller owns it.
pub fn clonePath(gpa: std.mem.Allocator, repos_root: []const u8, full_name: []const u8) ![]u8 {
    return std.fs.path.join(gpa, &.{ repos_root, full_name });
}

// ---------------------------------------------------------------------------
// git operations
// ---------------------------------------------------------------------------

pub const Error = error{ GitFailed, OutOfMemory };

/// Ensure a working clone of `full_name` exists under `repos_root` with `origin`
/// pointed at the token URL, freshly fetched. Clones if missing, otherwise
/// re-points `origin` (refreshing the token) and `fetch --prune`. Returns the
/// clone path (caller owns it).
pub fn ensureClone(gpa: std.mem.Allocator, repos_root: []const u8, full_name: []const u8, token_url: []const u8) Error![]u8 {
    const path = clonePath(gpa, repos_root, full_name) catch return Error.OutOfMemory;
    errdefer gpa.free(path);

    if (isGitRepo(gpa, path)) {
        // Refresh the (expired) embedded token, then fetch.
        _ = runGit(gpa, null, &.{ "git", "-C", path, "remote", "set-url", "origin", token_url });
        if (!runGit(gpa, null, &.{ "git", "-C", path, "fetch", "--prune", "origin" })) return Error.GitFailed;
        return path;
    }

    // Fresh clone. Ensure the parent (`<repos_root>/<owner>`) exists first.
    if (std.fs.path.dirname(path)) |parent| {
        storage.ensureDir(parent) catch return Error.GitFailed;
    }
    if (!runGit(gpa, null, &.{ "git", "clone", token_url, path })) return Error.GitFailed;
    return path;
}

/// Create (or reuse) the worktree for `branch` off `base_ref`. If the worktree
/// dir already exists it is returned as-is; if the branch already exists it is
/// checked out into a new worktree; otherwise a new branch is cut. Returns the
/// worktree path (caller owns it).
pub fn createWorktree(gpa: std.mem.Allocator, clone_path: []const u8, branch: []const u8, base_ref: []const u8) Error![]u8 {
    const wt = worktreePath(gpa, clone_path, branch) catch return Error.OutOfMemory;
    errdefer gpa.free(wt);

    // Reuse an existing worktree for this issue.
    if (dirExists(wt)) return wt;

    // `git worktree add` creates the leaf but not intermediate dirs.
    if (std.fs.path.dirname(wt)) |parent| {
        storage.ensureDir(parent) catch return Error.GitFailed;
    }

    const branch_ref = std.fmt.allocPrint(gpa, "refs/heads/{s}", .{branch}) catch return Error.OutOfMemory;
    defer gpa.free(branch_ref);
    const branch_exists = runGit(gpa, null, &.{ "git", "-C", clone_path, "rev-parse", "--verify", "--quiet", branch_ref });

    const ok = if (branch_exists)
        runGit(gpa, null, &.{ "git", "-C", clone_path, "worktree", "add", wt, branch })
    else
        runGit(gpa, null, &.{ "git", "-C", clone_path, "worktree", "add", wt, "-b", branch, base_ref });
    if (!ok) return Error.GitFailed;
    return wt;
}

/// Point the worktree's `origin` at the token URL so `git push` works with no
/// `gh` and no stored credentials. (Worktrees share the clone's config, so this
/// also refreshes the shared remote.) Best-effort.
pub fn setTokenRemote(gpa: std.mem.Allocator, worktree_path: []const u8, token_url: []const u8) void {
    _ = runGit(gpa, null, &.{ "git", "-C", worktree_path, "remote", "set-url", "origin", token_url });
}

// --- process helpers ---

fn isGitRepo(gpa: std.mem.Allocator, path: []const u8) bool {
    if (!dirExists(path)) return false;
    return runGit(gpa, null, &.{ "git", "-C", path, "rev-parse", "--git-dir" });
}

fn dirExists(path: []const u8) bool {
    return storage.fileExists(path);
}

/// Run `git` with `argv`, discarding output; returns whether it exited 0.
/// `cwd` is unused (we pass `-C` explicitly) but kept for signature clarity.
fn runGit(gpa: std.mem.Allocator, cwd: ?[]const u8, argv: []const []const u8) bool {
    _ = cwd;
    var arena = std.heap.ArenaAllocator.init(gpa);
    defer arena.deinit();
    const res = std.process.run(arena.allocator(), std.Io.Threaded.global_single_threaded.io(), .{
        .argv = argv,
        .stdout_limit = .limited(256 * 1024),
        .stderr_limit = .limited(256 * 1024),
    }) catch return false;
    return res.term == .exited and res.term.exited == 0;
}

// ---------------------------------------------------------------------------
// tests (pure helpers only)
// ---------------------------------------------------------------------------

test "tokenRemoteUrl embeds the token and strips a trailing .git" {
    const a = std.testing.allocator;
    const url_a = try tokenRemoteUrl(a, "ghs_abc", "octocat/Hello-World");
    defer a.free(url_a);
    try std.testing.expectEqualStrings("https://x-access-token:ghs_abc@github.com/octocat/Hello-World.git", url_a);

    const url_b = try tokenRemoteUrl(a, "tok", "octocat/Hello-World.git");
    defer a.free(url_b);
    try std.testing.expectEqualStrings("https://x-access-token:tok@github.com/octocat/Hello-World.git", url_b);
}

test "branchName and sanitizeBranch" {
    const a = std.testing.allocator;
    const b = try branchName(a, "exp/", "EXP-123");
    defer a.free(b);
    try std.testing.expectEqualStrings("exp/EXP-123", b);

    const leaf = try sanitizeBranch(a, "exp/EXP-123");
    defer a.free(leaf);
    try std.testing.expectEqualStrings("exp-EXP-123", leaf);
}

test "worktreePath and clonePath" {
    const a = std.testing.allocator;
    const cp = try clonePath(a, "/home/u/Exponential/repos", "octo/app");
    defer a.free(cp);
    try std.testing.expectEqualStrings("/home/u/Exponential/repos/octo/app", cp);

    const wt = try worktreePath(a, cp, "exp/EXP-7");
    defer a.free(wt);
    try std.testing.expectEqualStrings("/home/u/Exponential/repos/octo/app.worktrees/exp-EXP-7", wt);
}
