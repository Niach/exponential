import Foundation

/// Host-side git plumbing for the "Start coding" launcher (masterplan §4a) —
/// clone/fetch, worktree + branch, and a token-embedded push remote. Runs `git`
/// via `Foundation.Process` argv (through `PreviewShell.run`, which resolves
/// `git` on the augmented PATH). NEVER `gh`, NEVER a shell string. Every call is
/// blocking — invoke from a detached Task, off the main actor.
enum GitWorktree {
    struct Failure: LocalizedError, Sendable {
        let step: String
        let detail: String
        var errorDescription: String? {
            detail.isEmpty ? "git \(step) failed" : "git \(step) failed: \(detail)"
        }
    }

    @discardableResult
    private static func git(_ args: [String], cwd: URL? = nil) throws -> String {
        let r = PreviewShell.run("git", args, cwd: cwd)
        guard r.ok else {
            let detail = (r.stderr.isEmpty ? r.stdout : r.stderr)
                .trimmingCharacters(in: .whitespacesAndNewlines)
            throw Failure(step: args.first ?? "git", detail: detail)
        }
        return r.stdout
    }

    private static func gitSucceeds(_ args: [String], cwd: URL? = nil) -> Bool {
        PreviewShell.run("git", args, cwd: cwd).ok
    }

    /// Ensure a local clone of `fullName` (`owner/name`) under `reposRoot`. Clones
    /// with the token URL when missing; otherwise repoints `origin` at the fresh
    /// token (the previous one has expired) and fetches. Returns the clone path.
    static func ensureClone(reposRoot: URL, fullName: String, tokenUrl: String) throws -> URL {
        let clonePath = reposRoot.appendingPathComponent(fullName, isDirectory: true)
        let gitDir = clonePath.appendingPathComponent(".git")
        if FileManager.default.fileExists(atPath: gitDir.path) {
            try git(["remote", "set-url", "origin", tokenUrl], cwd: clonePath)
            try git(["fetch", "origin"], cwd: clonePath)
        } else {
            try FileManager.default.createDirectory(
                at: clonePath.deletingLastPathComponent(), withIntermediateDirectories: true)
            try git(["clone", tokenUrl, clonePath.path])
        }
        return clonePath
    }

    /// Create (or reuse) a worktree for `branch` off `baseRef`. One issue = one
    /// worktree: an existing worktree dir is reused; an existing branch is checked
    /// out into a new worktree; otherwise the branch is created off `baseRef`.
    /// Worktrees live in a `<name>.worktrees/` sibling of the clone. Returns the
    /// worktree path.
    static func createWorktree(clonePath: URL, branch: String, baseRef: String) throws -> URL {
        let leaf = clonePath.lastPathComponent
        let branchSlug = branch.replacingOccurrences(of: "/", with: "-")
        let worktreePath = clonePath.deletingLastPathComponent()
            .appendingPathComponent("\(leaf).worktrees", isDirectory: true)
            .appendingPathComponent(branchSlug, isDirectory: true)

        if FileManager.default.fileExists(atPath: worktreePath.appendingPathComponent(".git").path) {
            return worktreePath
        }
        try FileManager.default.createDirectory(
            at: worktreePath.deletingLastPathComponent(), withIntermediateDirectories: true)

        let branchExists = gitSucceeds(
            ["show-ref", "--verify", "--quiet", "refs/heads/\(branch)"], cwd: clonePath)
        if branchExists {
            try git(["worktree", "add", worktreePath.path, branch], cwd: clonePath)
        } else {
            try git(["worktree", "add", "-b", branch, worktreePath.path, baseRef], cwd: clonePath)
        }
        return worktreePath
    }

    /// Point the worktree's `origin` at the token-embedded URL so a later
    /// `git push` works with no `gh` and no personal credentials. (Worktrees share
    /// the clone's config, so this sets the shared `origin`.)
    static func setTokenRemote(worktreePath: URL, tokenUrl: String) throws {
        try git(["remote", "set-url", "origin", tokenUrl], cwd: worktreePath)
    }
}
