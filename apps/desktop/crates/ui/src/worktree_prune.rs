//! The ONE issue-state → [`coding::prune::PrunePolicy`] derivation (EXP-465).
//!
//! Both prune surfaces — the Settings → Worktrees button and the
//! trunk-sync auto-clean pass — used to derive their candidate sets
//! separately and differently (one composed `<prefix><IDENTIFIER>`, one
//! didn't; one scoped to the repo's boards, one didn't; neither gated the
//! other's hazards). This module is the single derivation both feed to
//! [`coding::prune::prune_landed`]:
//!
//! * an issue that is NOT finished and has no merged PR protects its branch
//!   outright (`keep`) — a crashed session's uncommitted work must survive
//!   whatever git thinks about the branch;
//! * a merged pull request lands in `merged` — a prune candidate whatever
//!   its prefix, though the engine still demands git-confirmed containment
//!   before deleting (EXP-358: merged sessions stay live, and their
//!   post-merge follow-up commits must survive);
//! * finished issues (done/cancelled/duplicate) land in `finished` — pruned
//!   only when git confirms the work landed;
//! * live sessions protect: synced rows via their root's linked issues and
//!   their own branch (EXP-358: merging never ends a session), local runs
//!   via [`LocalSessions::held_branches`], busy shell tabs via their cwd.
//!
//! EXP-1294: PR and branch state live on `pull_requests` (linked through
//! `pull_request_issues`), never on the issue.
//!
//! Branches no synced issue maps to (deleted issues, trashed boards) end up
//! in NO set — the engine falls back to pure git truth for prefix-matched
//! names.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use gpui::{App, Window};
use sync::Store;

use coding::branch_name;
use coding::prune::PrunePolicy;

use crate::coding_flow::{self, CodingHub, LocalSessions};

/// What the derivation needs to know about one issue — plain data so the
/// classification is unit-testable without gpui.
#[derive(Clone, Debug)]
pub(crate) struct IssueFacts {
    /// The head branch of the issue's newest pull request (one PR may link
    /// several issues).
    pub branch: Option<String>,
    pub identifier: String,
    /// Status category is completed/cancelled/duplicate.
    pub finished: bool,
    /// A pull request linked to the issue is merged.
    pub pr_merged: bool,
    /// A live coding session (any device) is bound to this issue.
    pub session_live: bool,
}

/// Classify every issue's branch names into the policy sets
/// `(keep, merged, finished)`. Each issue contributes BOTH its recorded
/// `branch` and the locally-composed `<prefix><IDENTIFIER>` — a PR opened
/// from another device (or never opened) must still resolve. `keep` wins
/// collisions: a branch shared by one open and one merged issue stays.
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
    let (mut keep, merged, finished) = policy_sets_from_facts(&facts, &prefix);
    keep.extend(live_session_branches(cx));
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
    let (mut keep, merged, mut finished) = policy_sets_from_facts(&facts, &prefix);
    keep.extend(live_session_branches(cx));
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

/// The branches of every open session (any device) whose tree is not idle
/// with every PR merged (S-4, [`coding::open_branches`]): a working run's
/// tree, and an idle chat's the next message resumes in, are never prune
/// candidates.
fn live_session_branches(cx: &App) -> Vec<String> {
    open_branches(cx).keep.into_iter().collect()
}

/// S-4: [`coding::open_branches`] over the synced rows — the prune's and the
/// stale-worktree sweep's view of the chats nothing ends any more.
pub(crate) fn open_branches(cx: &App) -> coding::OpenBranches {
    let collections = Store::global(cx).collections().clone();
    let sessions = collections.sessions.read(cx);
    let pulls = collections.pull_requests.read(cx);
    coding::open_branches(sessions.iter(), pulls.iter())
}

/// Snapshot the synced issues (optionally board-scoped) as [`IssueFacts`].
fn issue_facts(cx: &App, board_ids: Option<&HashSet<String>>) -> Vec<IssueFacts> {
    let collections = Store::global(cx).collections().clone();
    // The issues a working session's root links.
    let working_roots: HashSet<String> = collections
        .sessions
        .read(cx)
        .iter()
        .filter(|session| crate::session_data::session_is_working(session))
        .map(|session| session.root_id_or_self().to_string())
        .collect();
    let live_issue_ids: HashSet<String> = collections
        .session_issues
        .read(cx)
        .iter()
        .filter(|link| working_roots.contains(&link.session_id))
        .map(|link| link.issue_id.clone())
        .collect();
    // issue id → (newest PR head branch, any linked PR merged).
    let pulls = collections.pull_requests.read(cx);
    let mut pr_facts: HashMap<String, (Option<(String, String)>, bool)> = HashMap::new();
    for link in collections.pull_request_issues.read(cx).iter() {
        let Some(pr) = pulls.get(&link.pull_request_id) else {
            continue;
        };
        let entry = pr_facts.entry(link.issue_id.clone()).or_default();
        entry.1 |= pr.is_merged();
        if let Some(head) = pr.head.clone().filter(|head| !head.is_empty()) {
            let at = pr.created_at.clone().unwrap_or_default();
            if entry.0.as_ref().is_none_or(|(newest, _)| at > *newest) {
                entry.0 = Some((at, head));
            }
        }
    }
    collections
        .issues
        .read(cx)
        .iter()
        .filter(|issue| {
            board_ids.is_none_or(|board_ids| board_ids.contains(issue.board_id.as_str()))
        })
        .map(|issue| {
            let (branch, pr_merged) = pr_facts
                .get(&issue.id)
                .map(|(head, merged)| (head.as_ref().map(|(_, head)| head.clone()), *merged))
                .unwrap_or((None, false));
            IssueFacts {
                branch,
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
                pr_merged,
                session_live: live_issue_ids.contains(issue.id.as_str()),
            }
        })
        .collect()
}

/// Fold in the process-local protections (local runs, busy terminal tabs)
/// and finish the policy.
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
    for hardcoded in [coding::SESSION_BRANCH_PREFIX] {
        if !prefixes.iter().any(|p| p == hardcoded) {
            prefixes.push(hardcoded.to_string());
        }
    }
    PrunePolicy {
        prefixes,
        default_branch,
        keep,
        busy_paths,
        merged,
        finished,
        delete_stale_branches: true,
        // EXP-637: the backstop for run worktrees the run cleanup could not
        // remove at exit (a held gate, a crash). Nomination only — git still
        // decides, and dirty/ahead worktrees survive.
        run_registry_dir: Some(coding_flow::coding_data_dir(cx)),
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
    fn a_branch_shared_by_open_and_merged_issue_stays_kept() {
        // One PR links several issues onto one branch; any open issue on it
        // protects the branch for all of them.
        let mut merged_issue = fact("EXP-5");
        merged_issue.pr_merged = true;
        merged_issue.finished = true;
        merged_issue.branch = Some("exp/shared-1b81d4c7".to_string());
        let mut open_issue = fact("EXP-6");
        open_issue.branch = Some("exp/shared-1b81d4c7".to_string());
        let (keep, merged, _) =
            policy_sets_from_facts(&[merged_issue, open_issue], "exp/");
        assert!(keep.contains("exp/shared-1b81d4c7"));
        assert!(!merged.contains("exp/shared-1b81d4c7"));
        // The merged issue's own composed name still prunes.
        assert!(merged.contains("exp/EXP-5"));
    }
}
