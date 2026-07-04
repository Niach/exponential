//! The trunk's derived git state (masterplan v4 §4.2/§4.3): what the git bar
//! and Source Control screen read. Always derived from the repo on disk
//! (`git status`, `.git/rebase-merge`, `MERGE_HEAD`) — never from session
//! bookkeeping — so it survives restarts and out-of-band fixes (v4 §4.2 rule
//! 3).

use std::path::Path;

use crate::git_worktree::GitError;
use crate::scm::{self, ConflictState, StatusSummary};

/// The trunk's git state for the chrome (v4 §4.3 git bar). `syncing` is true
/// while a clone/fetch is in flight (the chip's spinner); `conflict` is
/// `Some` while a rebase/merge sits paused (the amber `⚠ N conflicts` chip
/// that navigates to Source Control).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrunkState {
    /// The trunk's checked-out branch (the default branch — nothing to switch;
    /// v4 §4.3 static chip).
    pub branch: String,
    pub ahead: u32,
    pub behind: u32,
    /// `Some` while a rebase/merge is paused with conflicts (v4 §4.4).
    pub conflict: Option<ConflictState>,
    /// A clone/fetch is currently running (v4 §4.3 sync spinner).
    pub syncing: bool,
}

impl TrunkState {
    /// A placeholder state for a trunk that has not been read yet (or whose
    /// clone does not exist): empty branch, clean, not syncing.
    pub fn empty() -> Self {
        Self {
            branch: String::new(),
            ahead: 0,
            behind: 0,
            conflict: None,
            syncing: false,
        }
    }
}

/// Read the trunk's git state from disk: `git status --porcelain=v2 --branch`
/// for branch + ahead/behind, plus conflict detection from
/// `.git/rebase-merge` / `MERGE_HEAD` (v4 §4.2 rule 3). `syncing` is layered
/// on by the caller (it owns the in-flight clone/fetch job), so this reads it
/// as `false`.
///
/// Purely a composition of the two disk-derived [`scm`] reads — no session
/// bookkeeping — so the state survives restarts and out-of-band fixes
/// (v4 §4.2 rule 3). `detect_conflict` is infallible (absence of a paused
/// rebase/merge = `None`); only `status` can fail (a missing/corrupt clone),
/// and that failure propagates so the caller can show the git bar's error/
/// spinner instead of a misleading "clean" state.
pub fn read(clone: &Path) -> Result<TrunkState, GitError> {
    let status = scm::status(clone)?;
    let conflict = scm::detect_conflict(clone);
    Ok(compose(status, conflict))
}

/// Pure assembly of a [`TrunkState`] from the two disk reads (unit-tested
/// without a git repo). `syncing` is always `false` here — the caller layers
/// it on from its own in-flight clone/fetch job (v4 §4.3).
fn compose(status: StatusSummary, conflict: Option<ConflictState>) -> TrunkState {
    TrunkState {
        branch: status.branch,
        ahead: status.ahead,
        behind: status.behind,
        conflict,
        syncing: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scm::{ConflictKind, FileChange, FileStatus};

    fn summary(branch: &str, ahead: u32, behind: u32) -> StatusSummary {
        StatusSummary {
            branch: branch.to_string(),
            ahead,
            behind,
            changes: vec![FileChange {
                path: "src/app.rs".to_string(),
                status: FileStatus::Modified,
                staged: false,
            }],
        }
    }

    #[test]
    fn empty_is_clean_and_not_syncing() {
        let state = TrunkState::empty();
        assert!(state.branch.is_empty());
        assert_eq!(state.ahead, 0);
        assert_eq!(state.behind, 0);
        assert_eq!(state.conflict, None);
        assert!(!state.syncing);
    }

    #[test]
    fn compose_copies_branch_and_counts_drops_change_list() {
        // The changes list belongs to the Source Control screen, not the git
        // bar — `compose` only lifts branch + ahead/behind + conflict.
        let state = compose(summary("main", 1, 2), None);
        assert_eq!(state.branch, "main");
        assert_eq!(state.ahead, 1);
        assert_eq!(state.behind, 2);
        assert_eq!(state.conflict, None);
        assert!(!state.syncing); // caller owns the in-flight flag
    }

    #[test]
    fn compose_passes_conflict_through() {
        let conflict = ConflictState {
            kind: ConflictKind::Rebase,
            files: vec!["src/app.rs".to_string(), "Cargo.lock".to_string()],
        };
        let state = compose(summary("main", 0, 3), Some(conflict.clone()));
        assert_eq!(state.conflict, Some(conflict));
        // Counts still ride alongside — the git bar swaps them for the amber
        // chip in conflict mode, but the model carries both (v4 §4.3).
        assert_eq!(state.behind, 3);
    }
}
