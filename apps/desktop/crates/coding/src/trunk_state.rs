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
/// that navigates to Source Control). `dirty` + `has_upstream` feed the
/// auto-sync eligibility check ([`TrunkState::ff_eligible`]) and the git
/// bar's dirty dot / Publish action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrunkState {
    /// The trunk's checked-out branch (the branch chip + every transport
    /// target).
    pub branch: String,
    pub ahead: u32,
    pub behind: u32,
    /// `Some` while a rebase/merge is paused with conflicts (v4 §4.4).
    pub conflict: Option<ConflictState>,
    /// A clone/fetch is currently running (v4 §4.3 sync spinner).
    pub syncing: bool,
    /// Any working-tree change (staged, unstaged, or untracked).
    pub dirty: bool,
    /// The checked-out branch tracks an upstream (`# branch.upstream`).
    pub has_upstream: bool,
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
            dirty: false,
            has_upstream: false,
        }
    }

    /// Whether auto-sync may fast-forward: behind-only + clean + no conflict,
    /// on a real branch (not `(detached)`) with an upstream to fast-forward
    /// to. This is the ONLY state `clone_manager::auto_sync` mutates in.
    pub fn ff_eligible(&self) -> bool {
        self.conflict.is_none()
            && !self.dirty
            && self.ahead == 0
            && self.behind > 0
            && self.has_upstream
            && !self.branch.is_empty()
            && !self.branch.starts_with('(')
    }
}

/// Read the trunk's git state from disk: `git status --porcelain=v2 --branch`
/// for branch + upstream + ahead/behind + dirtiness, plus conflict detection
/// from `.git/rebase-merge` / `MERGE_HEAD` (v4 §4.2 rule 3). `syncing` is
/// layered on by the caller (it owns the in-flight clone/fetch job), so this
/// reads it as `false`.
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
        dirty: !status.changes.is_empty(),
        has_upstream: status.upstream.is_some(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scm::{ConflictKind, FileChange, FileStatus};

    fn summary(branch: &str, ahead: u32, behind: u32) -> StatusSummary {
        StatusSummary {
            branch: branch.to_string(),
            upstream: Some(format!("origin/{branch}")),
            ahead,
            behind,
            changes: vec![FileChange {
                path: "src/app.rs".to_string(),
                status: FileStatus::Modified,
                staged: false,
            }],
        }
    }

    fn clean(branch: &str, ahead: u32, behind: u32) -> StatusSummary {
        StatusSummary { changes: Vec::new(), ..summary(branch, ahead, behind) }
    }

    #[test]
    fn empty_is_clean_and_not_syncing() {
        let state = TrunkState::empty();
        assert!(state.branch.is_empty());
        assert_eq!(state.ahead, 0);
        assert_eq!(state.behind, 0);
        assert_eq!(state.conflict, None);
        assert!(!state.syncing);
        assert!(!state.dirty);
        assert!(!state.has_upstream);
        assert!(!state.ff_eligible()); // empty branch, nothing behind
    }

    #[test]
    fn compose_copies_branch_counts_and_derives_dirty_upstream() {
        // The changes list belongs to the Source Control screen, not the git
        // bar — `compose` lifts branch + ahead/behind + conflict and derives
        // the dirty/upstream booleans.
        let state = compose(summary("main", 1, 2), None);
        assert_eq!(state.branch, "main");
        assert_eq!(state.ahead, 1);
        assert_eq!(state.behind, 2);
        assert_eq!(state.conflict, None);
        assert!(!state.syncing); // caller owns the in-flight flag
        assert!(state.dirty); // one Modified change
        assert!(state.has_upstream);

        let clean_state = compose(clean("main", 0, 0), None);
        assert!(!clean_state.dirty);

        let unpublished = compose(
            StatusSummary { upstream: None, ..clean("feature", 0, 0) },
            None,
        );
        assert!(!unpublished.has_upstream);
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

    #[test]
    fn ff_eligible_is_behind_only_clean_real_branch_with_upstream() {
        // The one auto-mutation gate: behind-only + clean + upstream.
        assert!(compose(clean("main", 0, 2), None).ff_eligible());

        // Dirty tree → never.
        assert!(!compose(summary("main", 0, 2), None).ff_eligible());
        // Ahead (diverged or push-pending) → never.
        assert!(!compose(clean("main", 1, 2), None).ff_eligible());
        // Nothing behind → nothing to do.
        assert!(!compose(clean("main", 0, 0), None).ff_eligible());
        // No upstream → nothing to fast-forward to.
        assert!(!compose(
            StatusSummary { upstream: None, ..clean("main", 0, 2) },
            None
        )
        .ff_eligible());
        // Detached / empty branch → never.
        assert!(!compose(clean("(detached)", 0, 2), None).ff_eligible());
        assert!(!compose(clean("", 0, 2), None).ff_eligible());
        // Paused rebase/merge → never.
        let conflict = ConflictState { kind: ConflictKind::Merge, files: Vec::new() };
        assert!(!compose(clean("main", 0, 2), Some(conflict)).ff_eligible());
    }
}
