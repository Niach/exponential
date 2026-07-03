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
//! remote URL (`https://x-access-token:<token>@github.com/<full>.git`) and in
//! git's own error output (git echoes the remote URL on clone/fetch
//! failures). [`TokenUrl`]'s `Debug`/`Display` are redacted, and every
//! captured stdout/stderr is scrubbed of the raw token *and* the raw URL
//! before it can reach a [`GitError`] — no code path formats the secret.

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

    /// Scrub the raw token and raw URL out of arbitrary text (git error
    /// output quotes the remote URL verbatim).
    fn scrub(&self, text: &str) -> String {
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

/// `<repos_root>/<owner>/<name>` (§7.1). Tolerates a malformed `full_name`
/// by nesting it verbatim.
pub fn clone_path(repos_root: &Path, full_name: &str) -> PathBuf {
    let mut path = repos_root.to_path_buf();
    for segment in full_name.split('/') {
        path.push(segment);
    }
    path
}

/// The coding branch: `<prefix><IDENTIFIER>` (default prefix `exp/`, so
/// `exp/EXP-42`) — one issue = one PR = one branch.
pub fn branch_name(prefix: &str, identifier: &str) -> String {
    format!("{prefix}{identifier}")
}

/// Directory-segment form of a branch: `/` → `-` (`exp/EXP-42` →
/// `exp-EXP-42`). Path segment only — the git branch keeps its slashes.
pub fn sanitize_branch_for_path(branch: &str) -> String {
    branch.replace('/', "-")
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
        url,
        &format!("git clone {full_name}"),
    )?;
    Ok(clone)
}

/// `git remote set-url origin <token-url>` — **re-run on EVERY launch**
/// (§7.1): the worktree outlives the ~55-min token, so the previously
/// embedded token is dead on relaunch. Remotes are repo-level config shared
/// by every worktree, so setting it on the clone covers them all.
pub fn set_token_remote(repo: &Path, url: &TokenUrl) -> Result<(), GitError> {
    run_git(
        Some(repo),
        &["remote", "set-url", "origin", &url.raw()],
        url,
        "git remote set-url origin",
    )?;
    Ok(())
}

/// Best-effort `git fetch origin <default_branch>` so `origin/<branch>` is
/// fresh when a NEW branch is cut on a reused clone. Callers may ignore the
/// error: a stale-but-present base ref still produces a valid worktree.
pub fn fetch_base(clone: &Path, default_branch: &str, url: &TokenUrl) -> Result<(), GitError> {
    run_git(
        Some(clone),
        &["fetch", "origin", default_branch],
        url,
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
    let _ = run_git(Some(clone), &["worktree", "prune"], url, "git worktree prune");

    let worktree_str = worktree.to_string_lossy().into_owned();
    if branch_exists(clone, branch, url) {
        run_git(
            Some(clone),
            &["worktree", "add", &worktree_str, branch],
            url,
            &format!("git worktree add ({branch})"),
        )?;
    } else {
        run_git(
            Some(clone),
            &["worktree", "add", "-b", branch, &worktree_str, base_ref],
            url,
            &format!("git worktree add -b {branch} from {base_ref}"),
        )?;
    }
    Ok(worktree)
}

fn branch_exists(clone: &Path, branch: &str, url: &TokenUrl) -> bool {
    run_git(
        Some(clone),
        &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")],
        url,
        "git rev-parse --verify",
    )
    .is_ok()
}

/// Run one git command (explicit argv, no shell), capturing output. Any
/// failure detail is scrubbed through [`TokenUrl::scrub`] before it becomes
/// a [`GitError`] — the token cannot leak via error strings.
fn run_git(cwd: Option<&Path>, args: &[&str], url: &TokenUrl, op: &str) -> Result<String, GitError> {
    let mut command = Command::new("git");
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let output = command.output().map_err(|e| GitError {
        op: op.to_string(),
        detail: if e.kind() == std::io::ErrorKind::NotFound {
            "git not found on PATH".to_string()
        } else {
            url.scrub(&e.to_string())
        },
    })?;
    if output.status.success() {
        Ok(url.scrub(&String::from_utf8_lossy(&output.stdout)))
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
        Err(GitError { op: op.to_string(), detail: url.scrub(&detail) })
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
        run_git(Some(&dir.0), &["init", "--quiet"], &url, "git init").unwrap();
        let err = run_git(
            Some(&dir.0),
            &["fetch", &url.raw()],
            &url,
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
                &url,
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
    fn set_token_remote_reset_covers_worktrees_via_shared_config() {
        let dir = temp_dir("remote");
        let origin = seed_origin(&dir.0);
        let repos_root = dir.0.join("repos");
        let clone = clone_path(&repos_root, "acme/web");
        fs::create_dir_all(clone.parent().unwrap()).unwrap();
        git(
            &dir.0,
            &["clone", "--quiet", origin.to_str().unwrap(), clone.to_str().unwrap()],
        );

        let url = TokenUrl::new("acme/web", "ghs_fresh456");
        set_token_remote(&clone, &url).unwrap();

        let remote = Command::new("git")
            .args(["remote", "get-url", "origin"])
            .current_dir(&clone)
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&remote.stdout).trim(), url.raw());
    }
}
