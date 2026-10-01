//! The session's CHANGES vocabulary (EXP-678/688/698, extracted by EXP-746,
//! repurposed by EXP-850 §11).
//!
//! The bottom "Changes" band is GONE: the session's diff is the run's DIFF
//! FACE now ([`crate::diff_pane`]), opened by the work header's toggle
//! (EXP-877). What survived the band is everything that was never chrome —
//! the diff PARSE and its cache ([`sync`], [`ChangesSnapshot`], whose counts
//! are the contract's [`domain::diff::totals`]) and the ONE merge-target rule
//! every session surface
//! applies ([`MergeTarget`], [`merge_target_for_run`],
//! [`merge_meta_for_session`], [`merge_when_live`]); the Merge pill itself is
//! `work_header::merge_pill`.

use gpui::App;

/// Re-parse a relay-delivered diff only when the host actually delivered a new
/// one (the raw string is the cache key): a unified-diff parse per repaint of
/// a live feed is real work for no new information. Returns the snapshot to
/// install, or `None` when the caller's snapshot is already current.
///
/// `Some(None)` means "clear it" (no diff for this session at all).
#[allow(clippy::option_option)] // "unchanged" and "cleared" are different answers
pub(crate) fn sync(
    state: Option<&ChangesSnapshot>,
    session_id: &str,
    raw: Option<&str>,
) -> Option<Option<ChangesSnapshot>> {
    let same = state
        .is_some_and(|state| state.session_id == session_id && state.raw.as_deref() == raw);
    if same {
        return None;
    }
    let Some(raw) = raw else {
        return Some(None);
    };
    // The expanded flag is the USER's, so it survives a re-parse of the same
    // session's diff; a different session starts collapsed.
    let expanded = state
        .filter(|state| state.session_id == session_id)
        .is_some_and(|state| state.expanded);
    let files = coding::scm::parse_unified_diff(raw);
    // EXP-895: the `+adds −dels` arithmetic is the CONTRACT's, shared with
    // every other client — nothing sums a diff locally any more.
    let totals = domain::diff::totals(&files);
    Some(Some(ChangesSnapshot {
        session_id: session_id.to_string(),
        raw: Some(raw.to_string()),
        files,
        additions: totals.additions,
        deletions: totals.deletions,
        expanded,
    }))
}

/// A relay-delivered Changes snapshot. Nothing is polled for it — the
/// host publishes the worktree diff on the activity channel and the viewer's
/// feed keeps the latest one, so this is only the PARSE of that string plus
/// the bar's expanded flag.
pub(crate) struct ChangesSnapshot {
    /// Which session the snapshot belongs to.
    pub(crate) session_id: String,
    /// The raw unified diff it was parsed from — the cache key, so a repaint
    /// of an unchanged feed re-parses nothing.
    pub(crate) raw: Option<String>,
    pub(crate) files: Vec<coding::scm::DiffFile>,
    pub(crate) additions: u32,
    pub(crate) deletions: u32,
    pub(crate) expanded: bool,
}

/// A finished run offers no Merge — iOS and web gate their pill on the same
/// liveness, and the PR merges from Reviews once the session is over. Pure,
/// and shared by both placements so "over" can never mean two things.
pub(crate) fn merge_when_live(merge: Option<MergeTarget>, over: bool) -> Option<MergeTarget> {
    merge.filter(|_| !over)
}

/// What the session's Merge pill acts on. An ISSUE target (EXP-498) is a
/// single-issue run's own issue with an open PR (`issues.mergePr`). A SESSION
/// target (EXP-734) is an issue-less run — a batch, chat or action run — whose
/// PR lives on its own `coding_sessions` row (every run that opened a PR
/// carries it there) and merges through `codingSessions.mergePr`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MergeTarget {
    Issue { issue_id: String },
    Session { session_id: String },
}

impl MergeTarget {
    /// The shared two-click arm/in-flight key for this target.
    pub(crate) fn key(&self) -> String {
        match self {
            MergeTarget::Issue { issue_id } => issue_id.clone(),
            MergeTarget::Session { session_id } => crate::pr_merge::session_merge_key(session_id),
        }
    }

    /// The op the confirmed click fires.
    pub(crate) fn op(&self) -> crate::pr_merge::MergeOp {
        match self {
            MergeTarget::Issue { issue_id } => crate::pr_merge::MergeOp::MergeIssuePr {
                issue_id: issue_id.clone(),
            },
            MergeTarget::Session { session_id } => crate::pr_merge::MergeOp::MergeSessionPr {
                session_id: session_id.clone(),
            },
        }
    }

    /// The button's resting tooltip — an issue merge completes the linked
    /// issues, a run's own PR has none to complete.
    pub(crate) fn tooltip(&self) -> &'static str {
        match self {
            MergeTarget::Issue { .. } => {
                "Merge: completes every linked issue and closes this coding session"
            }
            MergeTarget::Session { .. } => {
                "Merge: merges this run's pull request and closes the session"
            }
        }
    }
}

/// The ONE merge-target rule every session surface applies (EXP-498 /
/// EXP-734), pure so every arm can be tested against it:
///
/// 1. an ISSUE-linked run merges its OWN issue, when that issue's PR is open;
/// 2. otherwise an issue-less run (batch, chat, action) merges its OWN PR off
///    the `coding_sessions` row — the run owns the PR it opened.
///
/// `row` is the synced session row (absent while a just-started run has not
/// synced yet — then only the issue rule can fire).
pub(crate) fn merge_target_for_run<'a>(
    issue_id: Option<&str>,
    row: Option<&domain::rows::CodingSession>,
    mut issues: impl Iterator<Item = &'a domain::rows::Issue>,
) -> Option<MergeTarget> {
    if let Some(issue_id) = issue_id {
        if issues.any(|issue| issue.id == issue_id && issue_has_open_pr(issue)) {
            return Some(MergeTarget::Issue {
                issue_id: issue_id.to_string(),
            });
        }
    }
    let row = row?;
    row.has_open_pr().then(|| MergeTarget::Session {
        session_id: row.id.clone(),
    })
}

/// [`merge_target_for_run`] for a run with no local session to consult: the
/// merge target of a synced `coding_sessions` row. Same rules every client
/// applies (iOS `AgentSessionModel.mergeTarget`, web `use-agents-data`) —
/// including EXP-734's, which is why an issue-less run is not turned away
/// here: its own PR is right on the row.
pub(crate) fn merge_meta_for_session(
    session: &domain::rows::CodingSession,
    cx: &App,
) -> Option<MergeTarget> {
    let store = sync::Store::try_global(cx)?;
    let issues = store.collections().issues.read(cx);
    merge_target_for_run(
        session.issue_id.as_deref(),
        Some(session),
        issues.iter(),
    )
}

pub(crate) fn issue_has_open_pr(issue: &domain::rows::Issue) -> bool {
    issue.pr_state.as_deref() == Some("open")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EXP-850 §10: the Merge control is offered only while the run is LIVE
    /// — the header drops it the moment the session is over, and the PR
    /// merges from Reviews after that. iOS and web gate their pill on the
    /// same liveness.
    #[test]
    fn merge_is_offered_only_while_the_run_is_live() {
        let target = MergeTarget::Issue {
            issue_id: "i-1".to_string(),
        };
        assert_eq!(
            merge_when_live(Some(target.clone()), false),
            Some(target.clone())
        );
        assert_eq!(merge_when_live(Some(target), true), None);
        assert_eq!(merge_when_live(None, false), None);
    }

    /// EXP-734: the ONE merge-target rule. An issue-less run (batch, chat,
    /// action) carries its own PR on the SESSION row; an issue run still
    /// prefers its own issue; a run whose PR already merged offers nothing.
    #[test]
    fn merge_target_for_run_falls_back_to_the_runs_own_pr() {
        let issue =
            |id: &str, branch: Option<&str>, pr_state: Option<&str>| -> domain::rows::Issue {
                serde_json::from_value(serde_json::json!({
                    "id": id, "board_id": "b-1", "number": 1,
                    "identifier": "EXP-1", "title": "t", "status": "in_review",
                    "branch": branch, "pr_state": pr_state,
                }))
                .unwrap()
            };
        let row = |pr_state: Option<&str>| -> domain::rows::CodingSession {
            serde_json::from_value(serde_json::json!({
                "id": "cs-1", "team_id": "t-1", "status": "in_review",
                "action_id": "act-1", "branch": "exp/chat-a1b2c3d4",
                "pr_url": "https://github.com/o/r/pull/12",
                "pr_number": "12", "pr_state": pr_state,
            }))
            .unwrap()
        };
        let linked = issue("i-1", Some("exp/EXP-1"), Some("open"));

        // Action/chat run with an OPEN chore PR → the session itself.
        let open_row = row(Some("open"));
        assert_eq!(
            merge_target_for_run(None, Some(&open_row), std::iter::empty()),
            Some(MergeTarget::Session {
                session_id: "cs-1".to_string()
            })
        );
        // …and its key/op route to `codingSessions.mergePr`, never an issue.
        let target = MergeTarget::Session {
            session_id: "cs-1".to_string(),
        };
        assert_eq!(target.key(), "session:cs-1");
        assert!(matches!(
            target.op(),
            crate::pr_merge::MergeOp::MergeSessionPr { .. }
        ));

        // An ISSUE run prefers its own issue even with a row in hand.
        assert_eq!(
            merge_target_for_run(Some("i-1"), Some(&open_row), [&linked].into_iter()),
            Some(MergeTarget::Issue {
                issue_id: "i-1".to_string()
            })
        );

        // A BATCH run merges its own row's PR — never an issue sniffed off
        // its branch, even when one carries the same branch.
        let batch_issue = issue("i-2", Some("exp/batch-a1b2c3d4"), Some("open"));
        assert_eq!(
            merge_target_for_run(None, Some(&open_row), [&batch_issue].into_iter()),
            Some(MergeTarget::Session {
                session_id: "cs-1".to_string()
            })
        );
        assert_eq!(
            merge_target_for_run(None, Some(&row(None)), [&batch_issue].into_iter()),
            None
        );

        // A merged (or absent) run PR is not a merge target.
        let merged_row = row(Some("merged"));
        assert_eq!(
            merge_target_for_run(None, Some(&merged_row), std::iter::empty()),
            None
        );
        assert_eq!(
            merge_target_for_run(None, None, std::iter::empty()),
            None
        );
    }

    /// EXP-746: the parse cache. An unchanged raw diff re-parses nothing; a
    /// new one re-parses and KEEPS the user's expanded flag; a different
    /// session starts collapsed; `None` clears.
    #[test]
    fn the_diff_parse_cache_only_reparses_a_new_diff() {
        // Written with explicit escapes: a `\`-continued literal would eat
        // the context line's LEADING SPACE, and a body line without its
        // marker ends the hunk (the contract parser reads a patch strictly).
        const RAW: &str =
            "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1,2 @@\n one\n+two\n";

        let first = sync(None, "sess-1", Some(RAW))
            .expect("a first diff installs a snapshot")
            .expect("…a non-empty one");
        assert_eq!(first.additions, 1);
        assert!(!first.expanded, "a fresh snapshot starts collapsed");

        // Same session, same raw string → nothing to do.
        assert!(sync(Some(&first), "sess-1", Some(RAW)).is_none());

        // The user expanded it; a NEW diff for the same session keeps that.
        let mut expanded = first;
        expanded.expanded = true;
        let next = sync(Some(&expanded), "sess-1", Some("diff --git a/b.rs b/b.rs\n"))
            .expect("a new diff installs a snapshot")
            .expect("…a non-empty one");
        assert!(next.expanded, "the expanded flag is the user's");

        // A DIFFERENT session starts collapsed again.
        let other = sync(Some(&next), "sess-2", Some(RAW))
            .expect("another session installs its own snapshot")
            .expect("…a non-empty one");
        assert!(!other.expanded);

        // No diff at all clears.
        assert!(sync(Some(&other), "sess-2", None)
            .expect("a cleared diff is an answer")
            .is_none());
    }
}
