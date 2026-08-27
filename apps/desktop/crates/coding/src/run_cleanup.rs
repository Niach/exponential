//! EXP-637 — auto-remove a RUN's worktree once the run is over.
//!
//! Decision 1: every repo-backed action/chat run gets its own worktree +
//! branch instead of writing to the trunk clone. That trades one problem
//! (agents leaving uncommitted work on master, autopull parked on
//! `SkippedDirty`) for another (a worktree per run piling up), so a finished
//! run cleans up after itself — but ONLY when it provably left nothing
//! behind:
//!
//! - tracked changes → keep (real work, and the ended strip warns about it);
//! - commits the base branch does not have → keep (unpushed work);
//! - the clone's launch gate is held by another launch → skip, the prune's
//!   own pass picks it up later.
//!
//! Untracked-only debris (build artifacts, the launcher's own seed files) is
//! removable — same rule the prune uses.

use std::path::{Path, PathBuf};

use crate::git_worktree::{run_git, validate_branch_arg};
use crate::prune::{worktree_dirty_state, DirtyState};

/// Everything the cleanup needs about one finished run — recorded by the
/// launcher on [`crate::launcher::PreparedLaunch`] and carried by the host
/// (desktop `LocalSessions`, CLI reaper) to the run's exit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunCleanup {
    /// The clone the worktree hangs off (the `git worktree remove` cwd and
    /// the launch gate's key).
    pub clone: PathBuf,
    pub worktree: PathBuf,
    /// The run branch (`exp/<slug>-<id8>` / `exp/chat-<id8>`).
    pub branch: String,
    /// What the branch was cut from — the "did anything land here" compare.
    pub base_branch: String,
}

/// What one [`remove_if_clean`] pass did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CleanupOutcome {
    /// Worktree removed, branch deleted.
    Removed,
    /// Tracked changes survive in the worktree — the run left work behind,
    /// which the ended strip reports as an amber warning (decision 7).
    KeptDirty(DirtyState),
    /// `n` commits the base branch does not have.
    KeptAhead(usize),
    /// Nothing was attempted; the reason is for logs, not the user.
    Skipped(SkipReason),
}

impl CleanupOutcome {
    /// Decision 7: did the run leave uncommitted changes behind?
    pub fn left_dirty(&self) -> bool {
        matches!(self, Self::KeptDirty(_))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkipReason {
    /// Another launch holds the clone's gate (EXP-478) — retry later.
    GateHeld,
    /// The worktree is already gone (a previous pass, a manual removal).
    Missing,
    /// A malformed branch/base never reaches git argv.
    InvalidBranch,
    /// `git worktree remove` / `branch -D` failed (locked, permissions, …).
    GitFailed(String),
}

/// Remove the run's worktree + branch iff it provably left nothing behind.
/// Callers follow a [`CleanupOutcome::Removed`] with
/// `run_registry::remove(session_id)`.
pub fn remove_if_clean(cleanup: &RunCleanup) -> CleanupOutcome {
    match crate::launch_gate::try_exclusive(&cleanup.clone, || remove_locked(cleanup)) {
        Some(outcome) => outcome,
        None => CleanupOutcome::Skipped(SkipReason::GateHeld),
    }
}

fn remove_locked(cleanup: &RunCleanup) -> CleanupOutcome {
    if !cleanup.worktree.is_dir() {
        return CleanupOutcome::Skipped(SkipReason::Missing);
    }
    if validate_branch_arg(&cleanup.branch, "run cleanup").is_err()
        || validate_branch_arg(&cleanup.base_branch, "run cleanup base").is_err()
    {
        return CleanupOutcome::Skipped(SkipReason::InvalidBranch);
    }
    // Tracked work is sacred — a failing `git status` reads as dirty, so an
    // uninspectable worktree is never force-removed.
    let dirty = worktree_dirty_state(&cleanup.worktree);
    if dirty == DirtyState::TrackedChanges {
        return CleanupOutcome::KeptDirty(dirty);
    }
    // Anything committed on the run branch outlives the run: it is either
    // pushed (the PR keeps it) or local-only (deleting it would lose it).
    // Compared against the LOCAL base tip first, then origin's — a run whose
    // fetch failed must not read as "1000 commits ahead".
    let range = |base: &str| format!("{base}..{}", cleanup.branch);
    let ahead = count_commits(&cleanup.clone, &range(&format!("origin/{}", cleanup.base_branch)))
        .or_else(|| count_commits(&cleanup.clone, &range(&cleanup.base_branch)));
    match ahead {
        Some(0) => {}
        Some(n) => return CleanupOutcome::KeptAhead(n),
        // No comparable base at all: keep, the same conservative stance the
        // prune takes with `NoDefaultBranch`.
        None => {
            return CleanupOutcome::Skipped(SkipReason::GitFailed(
                "no comparable base branch".to_string(),
            ))
        }
    }
    let path = cleanup.worktree.to_string_lossy().into_owned();
    let args: &[&str] = match dirty {
        DirtyState::Clean => &["worktree", "remove", &path],
        // Untracked-only debris goes with the worktree (git refuses it
        // without --force).
        _ => &["worktree", "remove", "--force", &path],
    };
    if let Err(err) = run_git(
        Some(&cleanup.clone),
        args,
        None,
        &format!("git worktree remove ({})", cleanup.branch),
    ) {
        return CleanupOutcome::Skipped(SkipReason::GitFailed(err.detail));
    }
    // The branch carried nothing the base does not have — it goes too.
    let _ = run_git(
        Some(&cleanup.clone),
        &["branch", "-D", &cleanup.branch],
        None,
        &format!("git branch -D {}", cleanup.branch),
    );
    CleanupOutcome::Removed
}

/// `git rev-list --count <range>` — `None` when the range does not resolve.
fn count_commits(clone: &Path, range: &str) -> Option<usize> {
    run_git(
        Some(clone),
        &["rev-list", "--count", range],
        None,
        "git rev-list --count",
    )
    .ok()
    .and_then(|out| out.trim().parse::<usize>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.com")
            .output()
            .expect("git");
        assert!(
            status.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&status.stderr)
        );
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "exp-run-cleanup-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A real clone with an `origin/main` remote-tracking ref and one run
    /// worktree on `exp/chat-1a2b3c4d`.
    fn fixture(tag: &str) -> (PathBuf, RunCleanup) {
        let root = temp_dir(tag);
        let origin = root.join("origin.git");
        std::fs::create_dir_all(&origin).unwrap();
        git(&origin, &["init", "--bare", "--initial-branch=main", "."]);

        let clone = root.join("clone");
        std::fs::create_dir_all(&clone).unwrap();
        git(&clone, &["init", "--initial-branch=main", "."]);
        std::fs::write(clone.join("README.md"), "hello\n").unwrap();
        git(&clone, &["add", "."]);
        git(&clone, &["commit", "-m", "base"]);
        git(&clone, &["remote", "add", "origin", origin.to_str().unwrap()]);
        git(&clone, &["push", "-u", "origin", "main"]);

        let worktree = root.join("wt");
        git(
            &clone,
            &[
                "worktree",
                "add",
                "-b",
                "exp/chat-1a2b3c4d",
                worktree.to_str().unwrap(),
                "origin/main",
            ],
        );
        let cleanup = RunCleanup {
            clone,
            worktree,
            branch: "exp/chat-1a2b3c4d".to_string(),
            base_branch: "main".to_string(),
        };
        (root, cleanup)
    }

    fn branch_exists(clone: &Path, branch: &str) -> bool {
        run_git(
            Some(clone),
            &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")],
            None,
            "verify",
        )
        .is_ok()
    }

    #[test]
    fn a_clean_run_worktree_is_removed_with_its_branch() {
        let (root, cleanup) = fixture("clean");
        assert_eq!(remove_if_clean(&cleanup), CleanupOutcome::Removed);
        assert!(!cleanup.worktree.exists());
        assert!(!branch_exists(&cleanup.clone, &cleanup.branch));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn untracked_debris_does_not_save_a_worktree() {
        let (root, cleanup) = fixture("untracked");
        std::fs::write(cleanup.worktree.join("scratch.log"), "noise").unwrap();
        assert_eq!(remove_if_clean(&cleanup), CleanupOutcome::Removed);
        assert!(!cleanup.worktree.exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_commit_on_the_run_branch_keeps_it() {
        let (root, cleanup) = fixture("ahead");
        std::fs::write(cleanup.worktree.join("work.txt"), "real work\n").unwrap();
        git(&cleanup.worktree, &["add", "."]);
        git(&cleanup.worktree, &["commit", "-m", "run work"]);
        assert_eq!(remove_if_clean(&cleanup), CleanupOutcome::KeptAhead(1));
        assert!(cleanup.worktree.exists());
        assert!(branch_exists(&cleanup.clone, &cleanup.branch));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn tracked_changes_keep_it_and_report_dirty() {
        let (root, cleanup) = fixture("dirty");
        std::fs::write(cleanup.worktree.join("README.md"), "edited\n").unwrap();
        let outcome = remove_if_clean(&cleanup);
        assert_eq!(outcome, CleanupOutcome::KeptDirty(DirtyState::TrackedChanges));
        assert!(outcome.left_dirty(), "the ended strip warns on this");
        assert!(cleanup.worktree.exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_held_launch_gate_skips_the_pass() {
        let (root, cleanup) = fixture("gate");
        let hold = crate::launch_gate::hold(&cleanup.clone);
        assert_eq!(
            remove_if_clean(&cleanup),
            CleanupOutcome::Skipped(SkipReason::GateHeld)
        );
        assert!(cleanup.worktree.exists());
        drop(hold);
        // ... and the next pass, once the launch released it, removes it.
        assert_eq!(remove_if_clean(&cleanup), CleanupOutcome::Removed);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_already_removed_worktree_is_a_no_op() {
        let (root, cleanup) = fixture("missing");
        assert_eq!(remove_if_clean(&cleanup), CleanupOutcome::Removed);
        assert_eq!(
            remove_if_clean(&cleanup),
            CleanupOutcome::Skipped(SkipReason::Missing)
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
