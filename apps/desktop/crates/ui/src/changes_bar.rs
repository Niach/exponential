//! The session's CHANGES vocabulary (EXP-678/688/698, extracted by EXP-746,
//! repurposed by EXP-850 §11).
//!
//! The bottom "Changes" band is GONE: the session's diff opens as a
//! right-hand pane now ([`crate::diff_pane`]), toggled by the header's Diff
//! pill. What survived the band is everything that was never chrome — the
//! diff PARSE and its cache ([`sync`], [`ChangesSnapshot`], [`changes_totals`]),
//! the Merge button ([`merge_button`], which the session header wears) and
//! the ONE merge-target rule every session surface applies
//! ([`merge_target_for_run`], [`merge_meta_for_session`], [`merge_when_live`]).

use gpui::{AnyElement, App, ClickEvent, Context, Entity, IntoElement, Render, Styled as _};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    Disableable as _, Sizable as _,
};

use crate::icons::ExpIcon;

/// The session's Merge pill: the same two-click arm/confirm machinery every
/// other Merge surface drives ([`crate::pr_merge`]). A failed merge (typically
/// conflicts) jumps to the Reviews PAGE, where the shared error caption + the
/// Fix-conflicts button render exactly as a Reviews-originated failure.
/// Nothing runs on the confirm itself: the server ends the session on merge
/// (EXP-498).
pub(crate) fn merge_button<V: Render>(
    merge: &MergeTarget,
    merge_state: &Entity<crate::pr_merge::MergeState>,
    cx: &Context<V>,
) -> AnyElement {
    let key = merge.key();
    let (armed, merging) = {
        let state = merge_state.read(cx);
        (state.armed(&key), state.merging(&key))
    };
    // EXP-484: one button per surface (the dock's toolbar renders the ACTIVE
    // tab only), so the id no longer carries a strip index.
    let mut button = Button::new("merge-session-changes").xsmall();
    if merging {
        button = button
            .outline()
            .cursor_pointer()
            .label("Merging…")
            .loading(true)
            .disabled(true);
    } else if armed {
        button = button
            .outline()
            .cursor_pointer()
            .label("Confirm merge")
            .danger();
    } else {
        button = button
            .ghost()
            .cursor_pointer()
            .icon(ExpIcon::GitMerge)
            .label("Merge")
            .tooltip(merge.tooltip());
    }
    let target = merge.clone();
    let button = button.on_click(cx.listener(move |_, _: &ClickEvent, window, cx| {
        cx.stop_propagation();
        let handle = window.window_handle();
        crate::pr_merge::two_click(
            target.op(),
            Some(Box::new(move |cx: &mut App| {
                let _ = handle.update(cx, |_, window, cx| {
                    // EXP-706: Reviews is a full-page screen now, not a rail
                    // tool window.
                    crate::navigation::navigate(window, cx, crate::navigation::Screen::Reviews);
                });
            })),
            None,
            cx,
        );
    }));
    button.into_any_element()
}

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
    let (additions, deletions) = changes_totals(&files);
    Some(Some(ChangesSnapshot {
        session_id: session_id.to_string(),
        raw: Some(raw.to_string()),
        files,
        additions,
        deletions,
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

/// `+adds -dels` over every file in the snapshot. Pure.
pub(crate) fn changes_totals(files: &[coding::scm::DiffFile]) -> (u32, u32) {
    files.iter().fold((0, 0), |(adds, dels), file| {
        (adds + file.additions, dels + file.deletions)
    })
}

/// What the session's Merge pill acts on. An ISSUE target (EXP-498) is the
/// representative synced issue with an open PR — `issues.mergePr` on it fans
/// out to every issue sharing the prUrl, so any batch sibling merges the whole
/// PR. A SESSION target (EXP-734) is a run whose PR links NO issue at all (an
/// action or chat run's chore PR) — it lives on the `coding_sessions` row and
/// merges through `codingSessions.mergePr`.
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
/// 2. otherwise a BATCH run resolves the representative open-PR issue through
///    the head branch `pr_open` stamped on it (every batch sibling shares the
///    one prUrl, so any of them merges the whole PR);
/// 3. otherwise (EXP-734) an ACTION or CHAT run merges its OWN chore PR off
///    the `coding_sessions` row — no issue links it, so nothing else can.
///
/// `row` is the synced session row (absent while a just-started run has not
/// synced yet — then only the issue/branch rules can fire).
pub(crate) fn merge_target_for_run<'a>(
    issue_id: Option<&str>,
    branch: &str,
    row: Option<&domain::rows::CodingSession>,
    issues: impl Iterator<Item = &'a domain::rows::Issue>,
) -> Option<MergeTarget> {
    let issues: Vec<&domain::rows::Issue> = issues.collect();
    if let Some(issue_id) = issue_id {
        if let Some(issue) = issues
            .iter()
            .copied()
            .find(|issue| issue.id == issue_id && issue_has_open_pr(issue))
        {
            return Some(MergeTarget::Issue {
                issue_id: issue.id.clone(),
            });
        }
    }
    if let Some(issue) = open_pr_issue_on_branch(branch, issues.into_iter()) {
        return Some(MergeTarget::Issue {
            issue_id: issue.id.clone(),
        });
    }
    let row = row?;
    row.has_open_pr().then(|| MergeTarget::Session {
        session_id: row.id.clone(),
    })
}

/// [`merge_target_for_run`] for a run with no local session to consult: the
/// merge target of a synced `coding_sessions` row. Same rules every client
/// applies (iOS `AgentSessionModel.mergeTarget`, web `use-agents-data`) —
/// including EXP-734's third one, which is why an ACTION or CHAT run is not
/// turned away here: its own chore PR is right on the row.
pub(crate) fn merge_meta_for_session(
    session: &domain::rows::CodingSession,
    cx: &App,
) -> Option<MergeTarget> {
    let store = sync::Store::try_global(cx)?;
    let issues = store.collections().issues.read(cx);
    merge_target_for_run(
        session.issue_id.as_deref(),
        session.branch.as_deref().unwrap_or_default(),
        Some(session),
        issues.iter(),
    )
}

pub(crate) fn issue_has_open_pr(issue: &domain::rows::Issue) -> bool {
    issue.pr_state.as_deref() == Some("open")
}

/// Any synced open-PR issue on `branch` — a batch run's representative merge
/// target. Pure (unit-tested); an empty branch never matches (trunk/scratch
/// runs record no branch).
pub(crate) fn open_pr_issue_on_branch<'a>(
    branch: &str,
    issues: impl Iterator<Item = &'a domain::rows::Issue>,
) -> Option<&'a domain::rows::Issue> {
    if branch.is_empty() {
        return None;
    }
    let mut issues =
        issues.filter(|issue| issue.branch.as_deref() == Some(branch) && issue_has_open_pr(issue));
    issues.next()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diff_file(path: &str, additions: u32, deletions: u32) -> coding::scm::DiffFile {
        coding::scm::DiffFile {
            path: path.to_string(),
            previous_path: None,
            status: coding::scm::FileStatus::Modified,
            additions,
            deletions,
            hunks: Vec::new(),
            binary: false,
        }
    }

    /// EXP-688: the bar's `+adds -dels` counts the WHOLE snapshot, not the
    /// first file.
    #[test]
    fn changes_totals_sum_every_file() {
        assert_eq!(changes_totals(&[]), (0, 0));
        assert_eq!(
            changes_totals(&[
                diff_file("f", 3, 1),
                diff_file("f", 0, 7),
                diff_file("f", 10, 0)
            ]),
            (13, 8)
        );
    }


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

    /// EXP-498: the batch run's merge target — any synced OPEN-PR issue on
    /// the session's branch; nothing else qualifies.
    #[test]
    fn open_pr_issue_on_branch_picks_only_open_prs_on_the_branch() {
        let issue =
            |id: &str, branch: Option<&str>, pr_state: Option<&str>| -> domain::rows::Issue {
                serde_json::from_value(serde_json::json!({
                    "id": id, "board_id": "b-1", "number": 1,
                    "identifier": "EXP-1", "title": "t", "status": "in_review",
                    "branch": branch, "pr_state": pr_state,
                }))
                .unwrap()
            };
        let open = issue("i-open", Some("exp/batch-a1b2c3d4"), Some("open"));
        let merged = issue("i-merged", Some("exp/batch-a1b2c3d4"), Some("merged"));
        let other = issue("i-other", Some("exp/EXP-9"), Some("open"));
        let branchless = issue("i-none", None, Some("open"));

        let found = open_pr_issue_on_branch(
            "exp/batch-a1b2c3d4",
            [&merged, &other, &open, &branchless].into_iter(),
        );
        assert_eq!(found.map(|issue| issue.id.as_str()), Some("i-open"));
        assert!(open_pr_issue_on_branch(
            "exp/batch-a1b2c3d4",
            [&merged, &other, &branchless].into_iter()
        )
        .is_none());
        // Trunk/scratch sessions record no branch — never a merge target.
        assert!(open_pr_issue_on_branch("", [&open].into_iter()).is_none());
    }

    /// EXP-734: the ONE merge-target rule. An action/chat run carries its own
    /// chore PR on the SESSION row (no issue links it); an issue run still
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
            merge_target_for_run(
                None,
                "exp/chat-a1b2c3d4",
                Some(&open_row),
                std::iter::empty()
            ),
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
            merge_target_for_run(
                Some("i-1"),
                "exp/EXP-1",
                Some(&open_row),
                [&linked].into_iter()
            ),
            Some(MergeTarget::Issue {
                issue_id: "i-1".to_string()
            })
        );

        // A BATCH run still resolves through its branch (no issue, no row PR).
        let batch_issue = issue("i-2", Some("exp/batch-a1b2c3d4"), Some("open"));
        let plain_row = row(None);
        assert_eq!(
            merge_target_for_run(
                None,
                "exp/batch-a1b2c3d4",
                Some(&plain_row),
                [&batch_issue].into_iter()
            ),
            Some(MergeTarget::Issue {
                issue_id: "i-2".to_string()
            })
        );

        // A merged (or absent) run PR is not a merge target.
        let merged_row = row(Some("merged"));
        assert_eq!(
            merge_target_for_run(
                None,
                "exp/chat-a1b2c3d4",
                Some(&merged_row),
                std::iter::empty()
            ),
            None
        );
        assert_eq!(
            merge_target_for_run(None, "exp/chat-a1b2c3d4", None, std::iter::empty()),
            None
        );
    }

    /// EXP-746: the parse cache. An unchanged raw diff re-parses nothing; a
    /// new one re-parses and KEEPS the user's expanded flag; a different
    /// session starts collapsed; `None` clears.
    #[test]
    fn the_diff_parse_cache_only_reparses_a_new_diff() {
        const RAW: &str = "diff --git a/a.rs b/a.rs\n\
--- a/a.rs\n\
+++ b/a.rs\n\
@@ -1 +1,2 @@\n\
 one\n\
+two\n";

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
