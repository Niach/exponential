//! FEED-63: save a STOPPED run's uncommitted work as a local WIP commit.
//!
//! A run ended by a person (a web/mobile Stop, MCP `exponential_sessions_kill`)
//! used to leave its worktree dirty and nothing else: the next start or the
//! prune could lose it, and nobody was told. The host now commits whatever
//! the run left behind (tracked changes and untracked files alike) on the
//! run's own branch, so the work survives as history. Never pushed: the
//! commit is a local safety net, the branch owner decides what to do with it.

use std::path::Path;

use crate::git_worktree::run_git;
use crate::prune::{worktree_dirty_state, DirtyState};

/// The message every saved WIP commit carries.
pub const WIP_COMMIT_MESSAGE: &str =
    "WIP: uncommitted changes saved when the run was stopped (Exponential)";

/// The identity a commit falls back to when the worktree has none configured.
const FALLBACK_NAME: &str = "user.name=Exponential";
const FALLBACK_EMAIL: &str = "user.email=noreply@exponential.local";

/// What one [`save_wip_commit`] pass did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WipSave {
    /// No git work tree here (a repo-less scratch dir, or the worktree is
    /// already gone): nothing to save.
    NotARepo,
    /// Nothing uncommitted.
    Clean,
    /// Committed on `branch` (`HEAD` when detached).
    Committed { branch: String },
    /// `git add` or `git commit` failed; the detail is for the log.
    Failed(String),
}

/// Commit everything uncommitted in `worktree` as one WIP commit, with the
/// worktree's own identity, or the Exponential fallback identity when the
/// first attempt fails for a missing one. Hooks and signing are skipped: a
/// save at stop time must not hang on a prompt or be vetoed by a linter.
pub fn save_wip_commit(worktree: &Path) -> WipSave {
    // A worktree has a `.git` FILE, a clone a `.git` dir; a scratch dir has
    // neither (and may sit inside some unrelated repo, which is not ours).
    if !worktree.join(".git").exists() {
        return WipSave::NotARepo;
    }
    match worktree_dirty_state(worktree) {
        DirtyState::Clean => return WipSave::Clean,
        DirtyState::UntrackedOnly | DirtyState::TrackedChanges => {}
    }
    if let Err(err) = run_git(Some(worktree), &["add", "-A"], None, "git add -A") {
        return WipSave::Failed(err.detail);
    }
    let commit = |identity: &[&str]| {
        let mut args: Vec<&str> = identity.to_vec();
        args.extend([
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "--no-verify",
            "-m",
            WIP_COMMIT_MESSAGE,
        ]);
        run_git(Some(worktree), &args, None, "git commit (WIP)")
    };
    if let Err(err) = commit(&[]) {
        if !is_missing_identity(&err.detail) {
            return WipSave::Failed(err.detail);
        }
        if let Err(err) = commit(&["-c", FALLBACK_NAME, "-c", FALLBACK_EMAIL]) {
            return WipSave::Failed(err.detail);
        }
    }
    let branch = run_git(
        Some(worktree),
        &["rev-parse", "--abbrev-ref", "HEAD"],
        None,
        "git rev-parse",
    )
    .map(|out| out.trim().to_string())
    .unwrap_or_else(|_| "HEAD".to_string());
    WipSave::Committed { branch }
}

/// Whether a failed `git commit` failed for want of an author/committer
/// identity (unset, empty, or `user.useConfigOnly` with none configured).
fn is_missing_identity(detail: &str) -> bool {
    let detail = detail.to_ascii_lowercase();
    ["tell me who you are", "identity unknown", "empty ident", "unable to auto-detect email", "no name was given"]
        .iter()
        .any(|needle| detail.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .expect("git");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).to_string()
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "exp-wip-save-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A repo with one commit on `exp/APP-1` and a configured identity.
    fn repo(tag: &str) -> PathBuf {
        let dir = temp_dir(tag);
        git(&dir, &["init", "-q", "--initial-branch=exp/APP-1", "."]);
        git(&dir, &["config", "user.name", "Run Owner"]);
        git(&dir, &["config", "user.email", "owner@example.com"]);
        git(&dir, &["config", "commit.gpgsign", "false"]);
        std::fs::write(dir.join("README.md"), "hello\n").unwrap();
        git(&dir, &["add", "README.md"]);
        git(&dir, &["commit", "-q", "-m", "init"]);
        dir
    }

    #[test]
    fn a_dirty_worktree_is_committed_with_its_own_identity() {
        let dir = repo("dirty");
        std::fs::write(dir.join("README.md"), "changed\n").unwrap();
        std::fs::write(dir.join("new.txt"), "untracked\n").unwrap();
        assert_eq!(
            save_wip_commit(&dir),
            WipSave::Committed { branch: "exp/APP-1".to_string() }
        );
        assert_eq!(worktree_dirty_state(&dir), DirtyState::Clean);
        let log = git(&dir, &["log", "-1", "--format=%s|%an"]);
        assert_eq!(log.trim(), format!("{WIP_COMMIT_MESSAGE}|Run Owner"));
        let files = git(&dir, &["show", "--name-only", "--format=", "HEAD"]);
        assert!(files.contains("README.md") && files.contains("new.txt"), "{files}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn untracked_only_is_saved_too() {
        let dir = repo("untracked");
        std::fs::write(dir.join("notes.md"), "draft\n").unwrap();
        assert!(matches!(save_wip_commit(&dir), WipSave::Committed { .. }));
        assert_eq!(worktree_dirty_state(&dir), DirtyState::Clean);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_clean_worktree_and_a_scratch_dir_are_left_alone() {
        let dir = repo("clean");
        let head = git(&dir, &["rev-parse", "HEAD"]);
        assert_eq!(save_wip_commit(&dir), WipSave::Clean);
        assert_eq!(git(&dir, &["rev-parse", "HEAD"]), head);
        let _ = std::fs::remove_dir_all(&dir);

        let scratch = temp_dir("scratch");
        std::fs::write(scratch.join("file.txt"), "x\n").unwrap();
        assert_eq!(save_wip_commit(&scratch), WipSave::NotARepo);
        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[test]
    fn a_missing_identity_falls_back_to_exponential() {
        let dir = repo("no-identity");
        // An EMPTY local name outranks any global one, so the first commit
        // fails for the identity on every machine; `-c` outranks it again.
        git(&dir, &["config", "user.name", ""]);
        std::fs::write(dir.join("README.md"), "changed\n").unwrap();
        assert!(matches!(save_wip_commit(&dir), WipSave::Committed { .. }));
        let log = git(&dir, &["log", "-1", "--format=%an <%ae>"]);
        assert_eq!(log.trim(), "Exponential <noreply@exponential.local>");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
