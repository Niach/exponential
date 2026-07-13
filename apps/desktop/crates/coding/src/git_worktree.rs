//! Git via `argv`, never `gh`, never a git library (masterplan-v3 §7.1
//! step 3): every operation is `std::process::Command("git")` with explicit
//! argv — the installation token is never interpolated through a shell.
//!
//! Worktree layout on disk (§7.1):
//!
//! ```text
//! <repos_root>/<owner>/<name>                     # the clone (default branch)
//! <repos_root>/<owner>/<name>.worktrees/<branch>  # one worktree per branch;
//!                                                 # '/' in branch → '-'
//! ```
//!
//! So `exp/EXP-42` on `acme/web` lives at
//! `<repos_root>/acme/web.worktrees/exp-EXP-42` while the git branch stays
//! `exp/EXP-42`.
//!
//! **Redaction rule (§7.1 step 2):** the JIT installation token rides in the
//! initial clone's argv (`https://x-access-token:<token>@github.com/…`) and in
//! the clone's credential file ([`crate::git_credentials`] — EXP-73: `origin`
//! itself stays at the BARE URL), and git can echo either on failure (error
//! output, GIT_TRACE, helper stderr). [`TokenUrl`]'s `Debug`/`Display` are
//! redacted, and every captured stdout/stderr is scrubbed of the raw token
//! *and* the raw URL before it can reach a [`GitError`] — no code path
//! formats the secret.

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The token-embedded remote URL. Construct once per launch from the freshly
/// minted installation token; `Display`/`Debug` NEVER show the token.
#[derive(Clone)]
pub struct TokenUrl {
    full_name: String,
    token: String,
}

impl TokenUrl {
    /// `full_name` is `owner/name`; `token` is the raw JIT installation
    /// token (§7.1 step 2 — never persisted, never logged).
    pub fn new(full_name: impl Into<String>, token: impl Into<String>) -> Self {
        Self { full_name: full_name.into(), token: token.into() }
    }

    /// The raw URL — crate-private: it exists only to be handed to git argv.
    pub(crate) fn raw(&self) -> String {
        format!(
            "https://x-access-token:{}@github.com/{}.git",
            self.token, self.full_name
        )
    }

    /// The loggable form: `https://x-access-token:***@github.com/<full>.git`.
    pub fn redacted(&self) -> String {
        format!("https://x-access-token:***@github.com/{}.git", self.full_name)
    }

    /// The tokenless remote URL — what `origin` is set to (EXP-73: the token
    /// never rides the remote; ambient auth is [`crate::git_credentials`]).
    pub fn bare(&self) -> String {
        format!("https://github.com/{}.git", self.full_name)
    }

    /// The `owner/name` this token is scoped to.
    pub fn full_name(&self) -> &str {
        &self.full_name
    }

    /// The git-credential protocol answer for this token — exactly what the
    /// repo-local helper emits on `get` (values are raw in the protocol;
    /// installation tokens never contain newlines). Crate-private: only
    /// [`crate::git_credentials`] writes it.
    pub(crate) fn credential_file_contents(&self) -> String {
        format!("username=x-access-token\npassword={}\n", self.token)
    }

    /// Scrub the raw token and raw URL out of arbitrary text (git error
    /// output quotes the remote URL verbatim). Replaces BOTH the token-embedded
    /// URL and the bare token substring — a bare-token echo (GIT_TRACE, a
    /// credential-helper error) is scrubbed even when the full URL is not
    /// present. `pub(crate)` so every runner in the crate shares this one
    /// canonical redaction ([`crate::scm`], [`crate::clone_manager`]).
    pub(crate) fn scrub(&self, text: &str) -> String {
        let without_url = text.replace(&self.raw(), &self.redacted());
        if self.token.is_empty() {
            without_url
        } else {
            without_url.replace(&self.token, "***")
        }
    }
}

impl fmt::Display for TokenUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.redacted())
    }
}

impl fmt::Debug for TokenUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TokenUrl")
            .field("url", &self.redacted())
            .finish()
    }
}

/// A failed git invocation. `detail` is already token-scrubbed — safe for
/// error surfaces and logs.
#[derive(Clone, Debug)]
pub struct GitError {
    /// What was being attempted, e.g. `git clone acme/web` (never the URL).
    pub op: String,
    /// Scrubbed stderr/stdout or the spawn error.
    pub detail: String,
}

impl fmt::Display for GitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.op, self.detail)
    }
}

impl std::error::Error for GitError {}

// ---- pure path/branch composition (unit-tested, no git) ----

/// `<repos_root>/<owner>/<name>` (§7.1). Defense-in-depth: `full_name` comes
/// from the server (workspace-owner-writable), so each segment is sanitized —
/// traversal (`..`), no-op (`.`/empty) and separator bytes must never let a
/// crafted repo name escape the repos root ([`sanitize_path_segment`]).
pub fn clone_path(repos_root: &Path, full_name: &str) -> PathBuf {
    let mut path = repos_root.to_path_buf();
    for segment in full_name.split('/') {
        path.push(sanitize_path_segment(segment));
    }
    path
}

/// Make one path segment safe to join under the repos root: `\` (a separator
/// on Windows, legal-but-hostile elsewhere) becomes `-`, and the traversal /
/// degenerate segments (`..`, `.`, empty) become `_`. Regular names pass
/// through untouched.
fn sanitize_path_segment(segment: &str) -> String {
    let flattened = segment.replace('\\', "-");
    match flattened.as_str() {
        "" | "." | ".." => "_".to_string(),
        _ => flattened,
    }
}

/// The coding branch: `<prefix><IDENTIFIER>` (default prefix `exp/`, so
/// `exp/EXP-42`) — one issue = one PR = one branch.
pub fn branch_name(prefix: &str, identifier: &str) -> String {
    format!("{prefix}{identifier}")
}

/// Directory-segment form of a branch: `/` → `-` (`exp/EXP-42` →
/// `exp-EXP-42`). Path segment only — the git branch keeps its slashes.
/// Runs through [`sanitize_path_segment`] so a degenerate branch name (a
/// user-set prefix like `..` with an empty identifier) can never traverse
/// out of the `.worktrees/` dir.
pub fn sanitize_branch_for_path(branch: &str) -> String {
    sanitize_path_segment(&branch.replace('/', "-"))
}

/// `<clone>.worktrees/` — sibling of the clone dir (§7.1 layout).
pub fn worktrees_dir(clone: &Path) -> PathBuf {
    let name = clone
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("repo");
    clone.with_file_name(format!("{name}.worktrees"))
}

/// `<clone>.worktrees/<branch-sanitized>` — where a branch's worktree lives.
pub fn worktree_path(clone: &Path, branch: &str) -> PathBuf {
    worktrees_dir(clone).join(sanitize_branch_for_path(branch))
}

/// `<clone>.cargo-target` — ONE cargo build cache shared by every coding
/// session on this repo (EXP-76). The launcher exports it as
/// `CARGO_TARGET_DIR` for the spawned `claude`, so per-issue worktrees stop
/// paying a full cold build (measured 7-8.5GB *each* on this app) and reuse
/// the previous sessions' compiled deps instead. Sibling of `.worktrees` —
/// outside every checkout, and no sanitized branch name can collide with it
/// (branch segments land under `.worktrees/`, never beside it). Trade-off:
/// cargo locks a target dir per build, so two sessions building at the same
/// moment serialize — warm-and-serialized beats cold-and-parallel for a tree
/// this size, by a lot.
pub fn shared_cargo_target_dir(clone: &Path) -> PathBuf {
    let name = clone
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("repo");
    clone.with_file_name(format!("{name}.cargo-target"))
}

// ---- argv git operations ----

/// If `<repos_root>/<owner>/<name>` is not a git repo, `git clone` it (via
/// the token URL); otherwise no-op (reuse). Returns the clone path.
pub fn ensure_clone(
    repos_root: &Path,
    full_name: &str,
    url: &TokenUrl,
) -> Result<PathBuf, GitError> {
    let clone = clone_path(repos_root, full_name);
    if clone.join(".git").exists() {
        return Ok(clone); // reuse (§7.1: idempotent relaunch)
    }
    if let Some(parent) = clone.parent() {
        std::fs::create_dir_all(parent).map_err(|e| GitError {
            op: format!("prepare clone dir for {full_name}"),
            detail: e.to_string(),
        })?;
    }
    let clone_str = clone.to_string_lossy().into_owned();
    run_git(
        None,
        &["clone", &url.raw(), &clone_str],
        Some(url),
        &format!("git clone {full_name}"),
    )?;
    Ok(clone)
}

/// Best-effort `git fetch origin <default_branch>` so `origin/<branch>` is
/// fresh when a NEW branch is cut on a reused clone. Callers may ignore the
/// error: a stale-but-present base ref still produces a valid worktree.
pub fn fetch_base(clone: &Path, default_branch: &str, url: &TokenUrl) -> Result<(), GitError> {
    run_git(
        Some(clone),
        &["fetch", "origin", default_branch],
        Some(url),
        &format!("git fetch origin {default_branch}"),
    )?;
    Ok(())
}

/// Create (or reuse) the worktree for `branch` at the §7.1 layout path.
/// Idempotent: an existing worktree for the branch is reattached, an
/// existing branch without a worktree gets one (no `-b`), and only a truly
/// new branch is cut from `base_ref` (`origin/<defaultBranch>`).
pub fn create_worktree(
    clone: &Path,
    branch: &str,
    base_ref: &str,
    url: &TokenUrl,
) -> Result<PathBuf, GitError> {
    let worktree = worktree_path(clone, branch);
    if worktree.join(".git").exists() {
        return Ok(worktree); // reuse — one issue = one worktree
    }
    if let Some(parent) = worktree.parent() {
        std::fs::create_dir_all(parent).map_err(|e| GitError {
            op: format!("prepare worktrees dir for {branch}"),
            detail: e.to_string(),
        })?;
    }
    // A manually deleted worktree dir leaves a stale registration that blocks
    // `worktree add` — prune is cheap and safe.
    let _ = run_git(Some(clone), &["worktree", "prune"], Some(url), "git worktree prune");

    let worktree_str = worktree.to_string_lossy().into_owned();
    if branch_exists(clone, branch, url) {
        run_git(
            Some(clone),
            &["worktree", "add", &worktree_str, branch],
            Some(url),
            &format!("git worktree add ({branch})"),
        )?;
    } else {
        run_git(
            Some(clone),
            &["worktree", "add", "-b", branch, &worktree_str, base_ref],
            Some(url),
            &format!("git worktree add -b {branch} from {base_ref}"),
        )?;
    }
    Ok(worktree)
}

/// Append `entries` to the repo-local ignore file (`.git/info/exclude`) if
/// missing. The common git dir is shared by every worktree, so one write
/// covers them all — and unlike `.gitignore` it is never committed.
///
/// This is a TOKEN-LEAK guard, not cosmetics: `.mcp.json` carries the raw
/// `expu_` personal key, and the spawned `claude` is told to commit + push —
/// without the exclude, a `git add -A` would ship the key to GitHub.
/// Best-effort: a missing/at-odds `.git` layout only skips the write (the
/// launch itself must not fail on it).
pub fn ensure_local_excludes(clone: &Path, entries: &[&str]) -> std::io::Result<()> {
    let git_dir = clone.join(".git");
    if !git_dir.is_dir() {
        return Ok(()); // unexpected layout (bare/worktree) — skip, don't fail
    }
    let info_dir = git_dir.join("info");
    std::fs::create_dir_all(&info_dir)?;
    let exclude = info_dir.join("exclude");
    let current = std::fs::read_to_string(&exclude).unwrap_or_default();
    let existing: std::collections::HashSet<&str> =
        current.lines().map(str::trim).collect();
    let mut additions = String::new();
    for entry in entries {
        if !existing.contains(entry) {
            additions.push_str(entry);
            additions.push('\n');
        }
    }
    if additions.is_empty() {
        return Ok(());
    }
    let mut content = current;
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(&additions);
    std::fs::write(&exclude, content)
}

/// One registered worktree of a clone (`git worktree list --porcelain`).
#[derive(Clone, Debug, PartialEq)]
pub struct WorktreeEntry {
    pub path: PathBuf,
    /// The checked-out branch (short name, e.g. `exp/EXP-42`); `None` when
    /// detached or bare.
    pub branch: Option<String>,
}

/// All registered worktrees of `clone`, the main working tree FIRST (git
/// guarantees that ordering). Purely local, tokenless.
pub fn list_worktrees(clone: &Path) -> Result<Vec<WorktreeEntry>, GitError> {
    let output = run_git(
        Some(clone),
        &["worktree", "list", "--porcelain"],
        None,
        "git worktree list",
    )?;
    let mut entries = Vec::new();
    let mut path: Option<PathBuf> = None;
    let mut branch: Option<String> = None;
    for line in output.lines().chain(std::iter::once("")) {
        if line.is_empty() {
            if let Some(path) = path.take() {
                entries.push(WorktreeEntry { path, branch: branch.take() });
            }
            branch = None;
        } else if let Some(rest) = line.strip_prefix("worktree ") {
            path = Some(PathBuf::from(rest));
        } else if let Some(rest) = line.strip_prefix("branch refs/heads/") {
            branch = Some(rest.to_string());
        }
    }
    Ok(entries)
}

/// Remove session worktrees whose branch's work has landed (EXP-76 disk
/// hygiene): each removal reclaims the worktree's ignored build caches
/// (node_modules, stray target/ dirs, …) that git never tracks but disk
/// definitely pays for.
///
/// `prunable_branches` is caller-derived (the ui layer maps synced issues
/// with `pr_state == merged` and no running session to their `branch`) — this
/// function contributes the git-side safety only:
/// * the MAIN working tree is never touched (first entry, plus a path check);
/// * detached / unknown-branch worktrees are skipped;
/// * removal is `git worktree remove` WITHOUT `--force`, so git refuses any
///   worktree with modified or untracked files — ignored-only content (build
///   caches) does not block it, which is exactly the split we want. A refusal
///   just leaves that worktree for the user.
///
/// Local branches are deliberately left behind: a clean-but-unpushed commit
/// survives on its branch even after the worktree is gone.
///
/// Returns the removed paths (empty on listing failure — pruning is
/// best-effort by design).
pub fn prune_merged_worktrees(clone: &Path, prunable_branches: &[String]) -> Vec<PathBuf> {
    let Ok(entries) = list_worktrees(clone) else {
        return Vec::new();
    };
    let mut removed = Vec::new();
    for entry in entries.iter().skip(1) {
        if entry.path == clone {
            continue;
        }
        let Some(branch) = &entry.branch else { continue };
        if !prunable_branches.iter().any(|candidate| candidate == branch) {
            continue;
        }
        let path = entry.path.to_string_lossy().into_owned();
        if run_git(
            Some(clone),
            &["worktree", "remove", &path],
            None,
            &format!("git worktree remove ({branch})"),
        )
        .is_ok()
        {
            removed.push(entry.path.clone());
        }
    }
    removed
}

fn branch_exists(clone: &Path, branch: &str, url: &TokenUrl) -> bool {
    run_git(
        Some(clone),
        &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")],
        Some(url),
        "git rev-parse --verify",
    )
    .is_ok()
}

/// Run one git command (explicit argv, no shell), capturing output. The
/// canonical crate-wide git runner (`pub(crate)`): [`crate::scm`] and
/// [`crate::clone_manager`] route through this instead of reimplementing it.
///
/// `url` is `Some` for any op that can touch the token remote (git echoes the
/// remote URL — and, via GIT_TRACE / credential helpers, the bare token — on
/// failure): every captured stdout/stderr and spawn error is then scrubbed
/// through [`TokenUrl::scrub`] before it becomes a [`GitError`]. Purely local,
/// tokenless ops pass `None` (nothing to scrub). Shared hardening either way:
/// no shell, and `GIT_TERMINAL_PROMPT=0` so a rejected/expired token FAILS the
/// command rather than parking a GUI app behind an invisible credential prompt.
pub(crate) fn run_git(
    cwd: Option<&Path>,
    args: &[&str],
    url: Option<&TokenUrl>,
    op: &str,
) -> Result<String, GitError> {
    let mut command = Command::new("git");
    command.args(args);
    command.env("GIT_TERMINAL_PROMPT", "0");
    // C-locale messages: `scm::checkout_blocked_by_local_changes` classifies
    // git errors by their English text — localized git would break it.
    command.env("LC_ALL", "C");
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let scrub = |text: &str| match url {
        Some(url) => url.scrub(text),
        None => text.to_string(),
    };
    let output = command.output().map_err(|e| GitError {
        op: op.to_string(),
        detail: if e.kind() == std::io::ErrorKind::NotFound {
            "git not found on PATH".to_string()
        } else {
            scrub(&e.to_string())
        },
    })?;
    if output.status.success() {
        Ok(scrub(&String::from_utf8_lossy(&output.stdout)))
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut detail = stderr.trim().to_string();
        if detail.is_empty() {
            detail = stdout.trim().to_string();
        }
        if detail.is_empty() {
            detail = format!("exit code {}", output.status.code().unwrap_or(-1));
        }
        Err(GitError { op: op.to_string(), detail: scrub(&detail) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // ---- pure composition ----

    #[test]
    fn branch_composition_uses_prefix_and_identifier() {
        assert_eq!(branch_name("exp/", "EXP-42"), "exp/EXP-42");
        assert_eq!(branch_name("feat/", "GATE-7"), "feat/GATE-7");
        assert_eq!(branch_name("", "EXP-1"), "EXP-1");
    }

    #[test]
    fn sanitize_replaces_slashes_only_in_the_path_segment() {
        assert_eq!(sanitize_branch_for_path("exp/EXP-42"), "exp-EXP-42");
        assert_eq!(sanitize_branch_for_path("a/b/c"), "a-b-c");
        assert_eq!(sanitize_branch_for_path("plain"), "plain");
        // Degenerate branch names can never traverse out of `.worktrees/`.
        assert_eq!(sanitize_branch_for_path(".."), "_");
        assert_eq!(sanitize_branch_for_path(""), "_");
        assert_eq!(sanitize_branch_for_path("..\\up"), "..-up");
    }

    #[test]
    fn clone_path_never_escapes_the_repos_root() {
        // full_name is server data a workspace owner can influence — `..`,
        // `.`, empty and backslash segments must all stay under the root.
        let root = PathBuf::from("/home/u/Exponential/repos");
        for (full_name, expect) in [
            ("../..", "/home/u/Exponential/repos/_/_"),
            ("./evil", "/home/u/Exponential/repos/_/evil"),
            ("a/..", "/home/u/Exponential/repos/a/_"),
            ("..\\up/name", "/home/u/Exponential/repos/..-up/name"),
        ] {
            let path = clone_path(&root, full_name);
            assert_eq!(path, PathBuf::from(expect), "for {full_name:?}");
            assert!(path.starts_with(&root));
        }
        // Regular names are untouched.
        assert_eq!(
            clone_path(&root, "acme/web"),
            PathBuf::from("/home/u/Exponential/repos/acme/web")
        );
    }

    #[test]
    fn worktree_layout_matches_the_spec_example() {
        // §7.1: exp/EXP-42 on acme/web → <root>/acme/web.worktrees/exp-EXP-42
        let root = PathBuf::from("/home/u/Exponential/repos");
        let clone = clone_path(&root, "acme/web");
        assert_eq!(clone, PathBuf::from("/home/u/Exponential/repos/acme/web"));
        assert_eq!(
            worktrees_dir(&clone),
            PathBuf::from("/home/u/Exponential/repos/acme/web.worktrees")
        );
        assert_eq!(
            worktree_path(&clone, "exp/EXP-42"),
            PathBuf::from("/home/u/Exponential/repos/acme/web.worktrees/exp-EXP-42")
        );
    }

    // ---- redaction (§7.1 step 2) ----

    #[test]
    fn token_url_display_and_debug_are_redacted() {
        let url = TokenUrl::new("acme/web", "ghs_secret123");
        assert_eq!(
            url.to_string(),
            "https://x-access-token:***@github.com/acme/web.git"
        );
        let debug = format!("{url:?}");
        assert!(!debug.contains("ghs_secret123"), "token leaked: {debug}");
        assert!(debug.contains("***"));
        // The raw form exists solely for git argv.
        assert_eq!(
            url.raw(),
            "https://x-access-token:ghs_secret123@github.com/acme/web.git"
        );
    }

    #[test]
    fn scrub_removes_token_and_raw_url_from_git_output() {
        let url = TokenUrl::new("acme/web", "ghs_secret123");
        let git_noise = format!(
            "fatal: unable to access '{}': Could not resolve host (token ghs_secret123)",
            url.raw()
        );
        let scrubbed = url.scrub(&git_noise);
        assert!(!scrubbed.contains("ghs_secret123"), "leak: {scrubbed}");
        assert!(scrubbed.contains("https://x-access-token:***@github.com/acme/web.git"));
    }

    #[test]
    fn git_error_display_never_carries_the_token() {
        // Force a real git failure whose stderr quotes the remote URL.
        let dir = temp_dir("git-error");
        let url = TokenUrl::new("acme/definitely-missing", "ghs_secret123");
        run_git(Some(&dir.0), &["init", "--quiet"], Some(&url), "git init").unwrap();
        let err = run_git(
            Some(&dir.0),
            &["fetch", &url.raw()],
            Some(&url),
            "git fetch acme/definitely-missing",
        )
        .unwrap_err();
        let rendered = format!("{err} / {err:?}");
        assert!(!rendered.contains("ghs_secret123"), "leak: {rendered}");
    }

    // ---- real-git integration (hermetic: file:// remotes, no network) ----

    struct TempDir(PathBuf);

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn temp_dir(tag: &str) -> TempDir {
        let mut path = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!("exp-coding-git-{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }

    fn git(cwd: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.com")
            .output()
            .unwrap();
        assert!(status.status.success(), "git {args:?} failed");
    }

    /// A local "origin" repo with one commit on `main`, plus a TokenUrl whose
    /// raw() would never resolve — proving reuse paths don't hit the remote.
    fn seed_origin(dir: &Path) -> PathBuf {
        let origin = dir.join("origin-src");
        fs::create_dir_all(&origin).unwrap();
        git(&origin, &["init", "--quiet", "-b", "main"]);
        fs::write(origin.join("README.md"), "seed\n").unwrap();
        git(&origin, &["add", "."]);
        git(&origin, &["commit", "--quiet", "-m", "seed"]);
        origin
    }

    #[test]
    fn ensure_clone_clones_once_then_reuses() {
        let dir = temp_dir("clone");
        let origin = seed_origin(&dir.0);
        let repos_root = dir.0.join("repos");

        // First launch: a real clone (from the file:// origin — run_git only
        // sees the URL string, so substituting a local one exercises the same
        // code path without network).
        let clone = {
            let target = clone_path(&repos_root, "acme/web");
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            let url = TokenUrl::new("acme/web", "ghs_dead");
            run_git(
                None,
                &["clone", "--quiet", origin.to_str().unwrap(), target.to_str().unwrap()],
                Some(&url),
                "git clone acme/web",
            )
            .unwrap();
            target
        };
        assert!(clone.join(".git").exists());

        // Relaunch: ensure_clone must reuse without touching any remote — the
        // token URL is unroutable garbage, so a network attempt would fail.
        let reused = ensure_clone(
            &repos_root,
            "acme/web",
            &TokenUrl::new("acme/web", "ghs_dead"),
        )
        .unwrap();
        assert_eq!(reused, clone);
    }

    #[test]
    fn create_worktree_cuts_branch_then_reuses_idempotently() {
        let dir = temp_dir("worktree");
        let origin = seed_origin(&dir.0);
        let repos_root = dir.0.join("repos");
        let clone = clone_path(&repos_root, "acme/web");
        fs::create_dir_all(clone.parent().unwrap()).unwrap();
        git(
            &dir.0,
            &["clone", "--quiet", origin.to_str().unwrap(), clone.to_str().unwrap()],
        );

        let url = TokenUrl::new("acme/web", "ghs_dead");
        let branch = branch_name("exp/", "EXP-42");

        // First launch: new branch from origin/main, worktree at the layout path.
        let worktree = create_worktree(&clone, &branch, "origin/main", &url).unwrap();
        assert_eq!(worktree, worktree_path(&clone, "exp/EXP-42"));
        assert!(worktree.join(".git").exists());
        assert!(worktree.join("README.md").exists());

        // The checked-out branch keeps its slash.
        let head = Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(&worktree)
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&head.stdout).trim(), "exp/EXP-42");

        // Relaunch: same issue → same worktree, no error (idempotent reuse).
        let again = create_worktree(&clone, &branch, "origin/main", &url).unwrap();
        assert_eq!(again, worktree);
    }

    #[test]
    fn local_excludes_hide_the_seed_files_from_git_add() {
        let dir = temp_dir("exclude");
        let origin = seed_origin(&dir.0);
        let clone = dir.0.join("clone");
        git(
            &dir.0,
            &["clone", "--quiet", origin.to_str().unwrap(), clone.to_str().unwrap()],
        );

        ensure_local_excludes(&clone, &[".mcp.json", "PROMPT.md"]).unwrap();
        // Idempotent — no duplicate lines on relaunch.
        ensure_local_excludes(&clone, &[".mcp.json", "PROMPT.md"]).unwrap();
        let exclude = fs::read_to_string(clone.join(".git/info/exclude")).unwrap();
        assert_eq!(exclude.matches(".mcp.json").count(), 1);
        assert_eq!(exclude.matches("PROMPT.md").count(), 1);

        // The real assertion: git itself must consider both files ignored —
        // a `git add -A` by the spawned claude can never stage the raw key.
        fs::write(clone.join(".mcp.json"), "{\"secret\":true}").unwrap();
        fs::write(clone.join("PROMPT.md"), "seed").unwrap();
        let status = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&clone)
            .output()
            .unwrap();
        let listing = String::from_utf8_lossy(&status.stdout).into_owned();
        assert!(!listing.contains(".mcp.json"), "not ignored: {listing}");
        assert!(!listing.contains("PROMPT.md"), "not ignored: {listing}");
    }

    #[test]
    fn shared_cargo_target_dir_sits_beside_the_worktrees_dir() {
        let clone = PathBuf::from("/home/u/Exponential/repos/acme/web");
        assert_eq!(
            shared_cargo_target_dir(&clone),
            PathBuf::from("/home/u/Exponential/repos/acme/web.cargo-target")
        );
    }

    /// A clone with two session worktrees: `exp/EXP-1` (clean, plus an
    /// IGNORED junk dir standing in for a build cache) and `exp/EXP-2`
    /// (an untracked file — in-progress work).
    fn seed_prune_fixture(dir: &Path) -> (PathBuf, PathBuf, PathBuf) {
        let origin = seed_origin(dir);
        let clone = dir.join("clone");
        git(dir, &["clone", "--quiet", origin.to_str().unwrap(), clone.to_str().unwrap()]);
        let url = TokenUrl::new("acme/web", "ghs_dead");
        let merged = create_worktree(&clone, "exp/EXP-1", "origin/main", &url).unwrap();
        let dirty = create_worktree(&clone, "exp/EXP-2", "origin/main", &url).unwrap();
        // Ignored build-cache stand-in: must NOT protect the worktree.
        ensure_local_excludes(&clone, &["junk-cache/"]).unwrap();
        fs::create_dir_all(merged.join("junk-cache")).unwrap();
        fs::write(merged.join("junk-cache/blob.o"), "artifacts").unwrap();
        // Untracked real work: MUST protect the worktree.
        fs::write(dirty.join("wip.txt"), "not committed").unwrap();
        (clone, merged, dirty)
    }

    #[test]
    fn list_worktrees_reports_trunk_first_with_branches() {
        let dir = temp_dir("list");
        let (clone, merged, dirty) = seed_prune_fixture(&dir.0);
        let entries = list_worktrees(&clone).unwrap();
        assert_eq!(entries.len(), 3);
        // canonicalize: git prints resolved paths (macOS /tmp → /private/tmp).
        assert_eq!(entries[0].path, clone.canonicalize().unwrap());
        assert_eq!(entries[0].branch.as_deref(), Some("main"));
        let branches: Vec<_> = entries[1..].iter().map(|e| e.branch.clone()).collect();
        assert!(branches.contains(&Some("exp/EXP-1".into())), "{branches:?}");
        assert!(branches.contains(&Some("exp/EXP-2".into())), "{branches:?}");
        assert!(entries.iter().any(|e| e.path == merged.canonicalize().unwrap()));
        assert!(entries.iter().any(|e| e.path == dirty.canonicalize().unwrap()));
    }

    #[test]
    fn prune_removes_clean_merged_worktrees_and_keeps_the_rest() {
        let dir = temp_dir("prune");
        let (clone, merged, dirty) = seed_prune_fixture(&dir.0);

        // Canonicalize while the path still exists (git reports resolved
        // paths; macOS /tmp is a symlink) — it is gone after the prune.
        let merged_resolved = merged.canonicalize().unwrap();

        // Both branches nominated; only the clean one may go. The trunk's own
        // branch is nominated too and must survive regardless.
        let removed = prune_merged_worktrees(
            &clone,
            &["exp/EXP-1".to_string(), "exp/EXP-2".to_string(), "main".to_string()],
        );
        assert_eq!(removed, vec![merged_resolved]);
        assert!(!merged.exists(), "ignored junk must not protect a clean worktree");
        assert!(dirty.exists(), "untracked work must protect a worktree");
        assert!(clone.join("README.md").exists(), "trunk must never be pruned");

        // The branch outlives its worktree (unpushed commits stay reachable).
        assert!(branch_exists(&clone, "exp/EXP-1", &TokenUrl::new("acme/web", "ghs_dead")));

        // Not nominated → untouched even when clean.
        let removed = prune_merged_worktrees(&clone, &[]);
        assert!(removed.is_empty());
        assert!(dirty.exists());
    }

    #[test]
    fn token_url_bare_and_credential_forms() {
        let url = TokenUrl::new("acme/web", "ghs_secret123");
        assert_eq!(url.bare(), "https://github.com/acme/web.git");
        assert_eq!(url.full_name(), "acme/web");
        assert_eq!(
            url.credential_file_contents(),
            "username=x-access-token\npassword=ghs_secret123\n"
        );
    }
}
