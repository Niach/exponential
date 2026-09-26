//! EXP-1094: the ONE merge control a Reviews row carries, x4 (web
//! `lib/reviews-merge.ts`, iOS `ReviewsMerge.swift`, Android
//! `ReviewsMerge.kt`), byte-locked by
//! `packages/domain-contract/fixtures/reviews-merge.json`.
//!
//! First match wins:
//! 1. a workflow's FINAL PR row merges only while that PR is open;
//! 2. a workflow NODE PR (its workflow running or paused) merges through the
//!    workflow, never from the row;
//! 3. a stack's bottom row merges the whole stack; an upper member merges
//!    with it;
//! 4. anything else is a plain Merge.

use std::collections::HashMap;

pub use crate::pr_stack::MERGE_STACK_LABEL;

/// A plain row's button.
pub const MERGE_LABEL: &str = "Merge";
/// An upper stack member's quiet caption.
pub const REVIEW_MERGES_WITH_STACK: &str = "merges with its stack";
/// A workflow node PR's quiet caption.
pub const REVIEW_MERGES_THROUGH_WORKFLOW: &str = "merges through the workflow";

/// Where a row sits in its PR stack: `Bottom` = the root row that merges the
/// whole stack, `Upper` = any member above it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStackPosition {
    #[default]
    None,
    Bottom,
    Upper,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ReviewMergeInput {
    pub stack: ReviewStackPosition,
    /// The status of the workflow whose node covers this PR's issue.
    pub workflow_status: Option<String>,
    /// The row IS a workflow's final PR.
    pub final_pr: bool,
    /// The final PR's state (`open` | `closed` | `merged`); `None` off final rows.
    pub final_pr_state: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewMergeAction {
    Merge,
    MergeStack,
    None,
}

impl ReviewMergeAction {
    /// The fixture's wire word.
    pub fn as_wire(self) -> &'static str {
        match self {
            ReviewMergeAction::Merge => "merge",
            ReviewMergeAction::MergeStack => "merge_stack",
            ReviewMergeAction::None => "none",
        }
    }
}

fn live_workflow(status: Option<&str>) -> bool {
    matches!(status, Some("running") | Some("paused"))
}

pub fn review_row_merge_action(input: &ReviewMergeInput) -> ReviewMergeAction {
    if input.final_pr {
        return if input.final_pr_state.as_deref() == Some("open") {
            ReviewMergeAction::Merge
        } else {
            ReviewMergeAction::None
        };
    }
    if live_workflow(input.workflow_status.as_deref()) {
        return ReviewMergeAction::None;
    }
    match input.stack {
        ReviewStackPosition::Bottom => ReviewMergeAction::MergeStack,
        ReviewStackPosition::Upper => ReviewMergeAction::None,
        ReviewStackPosition::None => ReviewMergeAction::Merge,
    }
}

/// Why a row offers no merge control; `None` when it offers one or has
/// nothing to say (a final PR that is not open).
pub fn reviews_merge_disabled_reason(input: &ReviewMergeInput) -> Option<&'static str> {
    if input.final_pr {
        return None;
    }
    if live_workflow(input.workflow_status.as_deref()) {
        return Some(REVIEW_MERGES_THROUGH_WORKFLOW);
    }
    (input.stack == ReviewStackPosition::Upper).then_some(REVIEW_MERGES_WITH_STACK)
}

/// Issue id -> the status of the workflow whose node covers it (the node's
/// issue or a member issue). A live workflow wins over a finished one.
/// `workflows` = (id, status); `nodes` = (workflow id, issue id, members).
pub fn workflow_status_by_issue<'a>(
    workflows: impl IntoIterator<Item = (&'a str, &'a str)>,
    nodes: impl IntoIterator<Item = (&'a str, &'a str, Vec<String>)>,
) -> HashMap<String, String> {
    let status_by_id: HashMap<&str, &str> = workflows.into_iter().collect();
    let mut by_issue: HashMap<String, String> = HashMap::new();
    for (workflow_id, issue_id, members) in nodes {
        let Some(status) = status_by_id.get(workflow_id).copied() else {
            continue;
        };
        for issue in std::iter::once(issue_id.to_string()).chain(members) {
            let replace = match by_issue.get(&issue) {
                None => true,
                Some(current) => live_workflow(Some(status)) && !live_workflow(Some(current)),
            };
            if replace {
                by_issue.insert(issue, status.to_string());
            }
        }
    }
    by_issue
}

/// A PR's `workflow_status` over its linked issues: a live workflow first,
/// else any covering one, else `None`.
pub fn review_workflow_status<'a>(
    issue_ids: impl IntoIterator<Item = &'a str>,
    by_issue: &HashMap<String, String>,
) -> Option<String> {
    let statuses: Vec<&String> = issue_ids.into_iter().filter_map(|id| by_issue.get(id)).collect();
    statuses
        .iter()
        .find(|status| live_workflow(Some(status.as_str())))
        .or_else(|| statuses.first())
        .map(|status| status.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Deserialize)]
    struct Fixture {
        labels: Labels,
        reasons: Reasons,
        cases: Vec<Case>,
    }

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Labels {
        merge: String,
        merge_stack: String,
    }

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Reasons {
        merges_with_stack: String,
        merges_through_workflow: String,
    }

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Case {
        name: String,
        input: ReviewMergeInput,
        action: String,
        disabled_reason: Option<String>,
    }

    fn fixture() -> Fixture {
        serde_json::from_str(include_str!(
            "../../../../../packages/domain-contract/fixtures/reviews-merge.json"
        ))
        .expect("reviews-merge.json parses")
    }

    #[test]
    fn labels_and_reasons_match_the_fixture() {
        let fixture = fixture();
        assert_eq!(MERGE_LABEL, fixture.labels.merge);
        assert_eq!(MERGE_STACK_LABEL, fixture.labels.merge_stack);
        assert_eq!(REVIEW_MERGES_WITH_STACK, fixture.reasons.merges_with_stack);
        assert_eq!(REVIEW_MERGES_THROUGH_WORKFLOW, fixture.reasons.merges_through_workflow);
    }

    #[test]
    fn every_case_matches_the_fixture() {
        let fixture = fixture();
        assert!(!fixture.cases.is_empty());
        for case in fixture.cases {
            assert_eq!(review_row_merge_action(&case.input).as_wire(), case.action, "{}", case.name);
            assert_eq!(
                reviews_merge_disabled_reason(&case.input).map(str::to_string),
                case.disabled_reason,
                "{}",
                case.name
            );
        }
    }

    #[test]
    fn a_live_workflow_wins_the_issue() {
        let by_issue = workflow_status_by_issue(
            [("w-done", "done"), ("w-live", "running")],
            [
                ("w-done", "i1", vec![]),
                ("w-live", "i2", vec!["i1".to_string()]),
                ("w-gone", "i3", vec![]),
            ],
        );
        assert_eq!(by_issue.get("i1").map(String::as_str), Some("running"));
        assert_eq!(by_issue.get("i3"), None);
        assert_eq!(review_workflow_status(["i9", "i1"], &by_issue).as_deref(), Some("running"));
        assert_eq!(review_workflow_status(["i9"], &by_issue), None);
    }
}
