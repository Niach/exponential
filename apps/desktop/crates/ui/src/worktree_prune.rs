//! The ONE issue-state → [`coding::prune::PrunePolicy`] derivation (EXP-465).
//!
//! Both prune surfaces — the Settings → Local repositories button and the
//! trunk-sync auto-clean pass — used to derive their candidate sets
//! separately and differently (one composed `<prefix><IDENTIFIER>`, one
//! didn't; one scoped to the repo's boards, one didn't; neither gated the
//! other's hazards). This module is the single derivation both feed to
//! [`coding::prune::prune_landed`]:
//!
//! * an issue that is NOT finished and has no merged PR protects its branch
//!   outright (`keep`) — a crashed session's uncommitted work must survive
//!   whatever git thinks about the branch;
//! * `pr_state == merged` lands in `merged` — a prune candidate whatever
//!   its prefix, though the engine still demands git-confirmed containment
//!   before deleting (EXP-358: merged sessions stay live, and their
//!   post-merge follow-up commits must survive);
//! * finished issues (done/cancelled/duplicate) land in `finished` — pruned
//!   only when git confirms the work landed;
//! * live sessions protect: synced rows via their issue mapping
//!   (heartbeat-stale rows count as absent, EXP-153; `merged` sessions are
//!   still LIVE, EXP-358), local batch/action runs via
//!   [`LocalSessions::held_branches`], busy terminal tabs via their cwd.
//!
//! Branches no synced issue maps to (deleted issues, trashed boards, stale
//! batch branches) end up in NO set — the engine falls back to pure git
//! truth for prefix-matched names.

use std::collections::HashSet;
use std::path::PathBuf;

use gpui::{App, Window};
use sync::Store;

use coding::branch_name;
use coding::prune::PrunePolicy;

use crate::coding_flow::{self, CodingHub, LocalSessions};
use crate::queries;

/// The hardcoded batch-branch namespace (`batch_launcher` ignores the user's
/// branch prefix), always swept alongside it.
const BATCH_PREFIX: &str = "exp/batch-";

/// What the derivation needs to know about one issue — plain data so the
/// classification is unit-testable without gpui.
#[derive(Clone, Debug)]
pub(crate) struct IssueFacts {
    /// `issues.branch` — set by `open_pr`, shared across a batch's issues.
    pub branch: Option<String>,
    pub identifier: String,
    /// Status category is completed/cancelled/duplicate.
    pub finished: bool,
    /// `issues.pr_state == merged`.
    pub pr_merged: bool,
    /// A live coding session (any device) is bound to this issue.
    pub session_live: bool,
}

/// Classify every issue's branch names into the policy sets
/// `(keep, merged, finished)`. Each issue contributes BOTH its recorded
/// `branch` and the locally-composed `<prefix><IDENTIFIER>` — a PR opened
/// from another device (or never opened) must still resolve. `keep` wins
/// collisions: a batch branch shared by one open and one merged issue stays.
pub(crate) fn policy_sets_from_facts(
    facts: &[IssueFacts],
    prefix: &str,
) -> (HashSet<String>, HashSet<String>, HashSet<String>) {
    let mut keep = HashSet::new();
    let mut merged = HashSet::new();
    let mut finished = HashSet::new();
    for fact in facts {
        let composed = branch_name(prefix, &fact.identifier);
        let names = std::iter::once(composed).chain(
            fact.branch
                .clone()
                .filter(|branch| !branch.is_empty()),
        );
        for name in names {
            if fact.session_live || (!fact.finished && !fact.pr_merged) {
                keep.insert(name);
            } else if fact.pr_merged {
                merged.insert(name);
            } else {
                finished.insert(name);
            }
        }
    }
    merged.retain(|name| !keep.contains(name));
    finished.retain(|name| !keep.contains(name));
    (keep, merged, finished)
}

/// The full policy for pruning ONE repo's clone, scoped to the boards backed
/// by `repository_id` (an unrelated same-identifier branch in another repo's
/// clone must never inherit this repo's issue state).
pub(crate) fn prune_policy_for_repo(
    repository_id: &str,
    default_branch: Option<String>,
    window: &Window,
    cx: &App,
) -> PrunePolicy {
    let prefix = branch_prefix(cx);
    let collections = Store::global(cx).collections().clone();
    let board_ids: HashSet<String> = collections
        .boards
        .read(cx)
        .iter()
        .filter(|board| board.repository_id.as_deref() == Some(repository_id))
        .map(|board| board.id.clone())
        .collect();
    let facts = issue_facts(cx, Some(&board_ids));
    let (keep, merged, finished) = policy_sets_from_facts(&facts, &prefix);
    assemble(prefix, default_branch, keep, merged, finished, window, cx)
}

/// The fallback policy for a clone the repo resolver cannot map (another
/// team's repo, resolver still loading): git truth only. EVERY synced issue's
/// unfinished branches populate `keep` (over-protective on purpose), and
/// nothing lands in `merged` — without a trustworthy issue↔repo scoping, a
/// same-identifier collision must not prune an unlanded branch, so even
/// merged-PR branches need git agreement here.
pub(crate) fn prune_policy_unscoped(window: &Window, cx: &App) -> PrunePolicy {
    let prefix = branch_prefix(cx);
    let facts = issue_facts(cx, None);
    let (keep, merged, mut finished) = policy_sets_from_facts(&facts, &prefix);
    finished.extend(merged);
    assemble(prefix, None, keep, HashSet::new(), finished, window, cx)
}

/// The user's branch prefix — read-only (render paths must not create the
/// hub; both prune surfaces run long after it exists anyway).
fn branch_prefix(cx: &App) -> String {
    CodingHub::global_ref(cx)
        .map(|hub| hub.read(cx).settings.branch_prefix.clone())
        .unwrap_or_else(|| coding::Settings::default().branch_prefix)
}

/// Snapshot the synced issues (optionally board-scoped) as [`IssueFacts`].
fn issue_facts(cx: &App, board_ids: Option<&HashSet<String>>) -> Vec<IssueFacts> {
    let collections = Store::global(cx).collections().clone();
    // Heartbeat-stale rows count as absent (EXP-153) — a phantom row must not
    // park a landed branch forever.
    let now = chrono::Utc::now().timestamp();
    let live_issue_ids: HashSet<String> = collections
        .coding_sessions
        .read(cx)
        .iter()
        .filter(|session| queries::coding_session_is_live(session, now))
        .filter_map(|session| session.issue_id.clone())
        .collect();
    collections
        .issues
        .read(cx)
        .iter()
        .filter(|issue| {
            board_ids.is_none_or(|board_ids| board_ids.contains(issue.board_id.as_str()))
        })
        .map(|issue| IssueFacts {
            branch: issue.branch.clone(),
            identifier: issue.identifier.clone(),
            // The dual-written enum ANCHOR (EXP-314): a custom completed/
            // cancelled status anchors to done/cancelled, so this stays a
            // cheap enum test instead of a per-issue team-status resolution.
            finished: matches!(
                issue.status,
                domain::enums::IssueStatus::Done
                    | domain::enums::IssueStatus::Cancelled
                    | domain::enums::IssueStatus::Duplicate
            ),
            pr_merged: issue.pr_state.as_deref() == Some(domain::contract::PR_STATE_MERGED),
            session_live: live_issue_ids.contains(issue.id.as_str()),
        })
        .collect()
}

/// Fold in the process-local protections (batch/action runs, busy terminal
/// tabs) and finish the policy.
fn assemble(
    prefix: String,
    default_branch: Option<String>,
    mut keep: HashSet<String>,
    merged: HashSet<String>,
    finished: HashSet<String>,
    window: &Window,
    cx: &App,
) -> PrunePolicy {
    if let Some(sessions) = LocalSessions::global_ref(cx) {
        for branch in sessions.read(cx).held_branches() {
            keep.insert(branch.to_string());
        }
    }
    let busy_paths: Vec<PathBuf> = coding_flow::window_terminal_manager(window, cx)
        .map(|manager| {
            manager
                .read(cx)
                .tabs()
                .iter()
                .filter(|tab| tab.is_running())
                .filter_map(|tab| tab.cwd.clone())
                .collect()
        })
        .unwrap_or_default();
    let mut prefixes = vec![prefix];
    if !prefixes.contains(&BATCH_PREFIX.to_string()) {
        prefixes.push(BATCH_PREFIX.to_string());
    }
    PrunePolicy {
        prefixes,
        default_branch,
        keep,
        busy_paths,
        merged,
        finished,
        delete_stale_branches: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fact(identifier: &str) -> IssueFacts {
        IssueFacts {
            branch: None,
            identifier: identifier.to_string(),
            finished: false,
            pr_merged: false,
            session_live: false,
        }
    }

    #[test]
    fn unfinished_issues_protect_their_branches() {
        // Open issue, no PR fields at all → keep, via the COMPOSED name
        // (trunk_sync's old derivation dropped NULL-branch issues entirely).
        let (keep, merged, finished) = policy_sets_from_facts(&[fact("EXP-1")], "exp/");
        assert!(keep.contains("exp/EXP-1"));
        assert!(merged.is_empty() && finished.is_empty());
    }

    #[test]
    fn merged_pr_and_finished_status_split_correctly() {
        let mut done_with_pr = fact("EXP-2");
        done_with_pr.finished = true;
        done_with_pr.pr_merged = true;
        done_with_pr.branch = Some("exp/EXP-2".to_string());
        let mut done_no_pr = fact("EXP-3");
        done_no_pr.finished = true;
        let (keep, merged, finished) =
            policy_sets_from_facts(&[done_with_pr, done_no_pr], "exp/");
        assert!(keep.is_empty());
        assert!(merged.contains("exp/EXP-2"));
        assert!(finished.contains("exp/EXP-3"), "done-without-PR needs git agreement");
        assert!(!merged.contains("exp/EXP-3"));
    }

    #[test]
    fn live_session_wins_over_merged() {
        // EXP-358: a merged session keeps running — its branch must stay.
        let mut merged_but_live = fact("EXP-4");
        merged_but_live.pr_merged = true;
        merged_but_live.session_live = true;
        let (keep, merged, _) = policy_sets_from_facts(&[merged_but_live], "exp/");
        assert!(keep.contains("exp/EXP-4"));
        assert!(merged.is_empty());
    }

    #[test]
    fn batch_branch_shared_by_open_and_merged_issue_stays_kept() {
        // One batch PR links several issues onto one branch; any open issue
        // on it protects the branch for all of them.
        let mut merged_issue = fact("EXP-5");
        merged_issue.pr_merged = true;
        merged_issue.finished = true;
        merged_issue.branch = Some("exp/batch-1b81d4c7".to_string());
        let mut open_issue = fact("EXP-6");
        open_issue.branch = Some("exp/batch-1b81d4c7".to_string());
        let (keep, merged, _) =
            policy_sets_from_facts(&[merged_issue, open_issue], "exp/");
        assert!(keep.contains("exp/batch-1b81d4c7"));
        assert!(!merged.contains("exp/batch-1b81d4c7"));
        // The merged issue's own composed name still prunes.
        assert!(merged.contains("exp/EXP-5"));
    }
}
