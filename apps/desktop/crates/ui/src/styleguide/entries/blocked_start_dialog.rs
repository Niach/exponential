//! EXP-980/SLOP-3: `blocked-start-dialog` (3 Special components): what
//! starting a BLOCKED issue asks first, Cancel · Start anyway · Stacked PR.
//!
//! The IDE's dialog is a native alert window
//! (`ChatScreenView::prompt_blocked_start`); inside the composer dialog the
//! same question takes over its body. A window cannot be embedded, so the
//! demo draws the alert's REAL view (`native_dialog::alert_specimen`) over
//! the SAME spec the product opens (`chat_screen::blocked_start_alert`): the
//! title in the titlebar band, the sentence, THE blocks mini-graph
//! (`issue_graph::graph_in_dialog`), the caption under it and the three
//! buttons.
//!
//! Nothing is retyped. Static rows (never live ones) go through the
//! product's own rules: `block_graph` lays the graph out,
//! `open_blockers_of_set` names who the sentence is about,
//! `blocked_start::stack_line` + `stack_plan` decide the stacked start, and
//! `chat_screen::stack_outcome` turns the plan into the note. The two states
//! are the web specimen's, the fixture's own cases
//! (`blocked-start.json`): a line of two blockers, where Stacked PR is
//! enabled and the plan note says what starts first; and a fork, where it is
//! disabled and the reason takes the plan note's place.

use gpui::{div, App, Div, ParentElement as _, Styled as _, Window};
use gpui_component::ActiveTheme as _;

use domain::blocked_start::{self, StackMember, StackSubject};
use domain::issue_graph::{
    block_graph, open_blockers_of_set, GraphIssue, GraphRelation, IssueGraph,
};

use crate::chat_screen::{blocked_start_alert, stack_outcome, BlockedStart};

pub(crate) const ID: &str = "blocked-start-dialog";
pub(crate) const OWNER: &str = "EXP-980";

/// The issue both states start.
const SUBJECT: &str = "APP-20";
/// Every issue here lives on a board of this one repository.
const REPOSITORY: &str = "r1";

/// One open issue as the rules read it. Its id IS its identifier, so the
/// graph's chips name it with no synced row behind them.
fn issue(identifier: &'static str) -> GraphIssue<'static> {
    GraphIssue {
        id: identifier,
        identifier,
        status: "backlog",
    }
}

fn blocks(blocker: &'static str, blocked: &'static str) -> GraphRelation<'static> {
    GraphRelation {
        kind: "blocks",
        issue_id: blocker,
        related_issue_id: blocked,
    }
}

/// A line of two: APP-10 blocks APP-11, which blocks the subject.
fn line() -> (Vec<GraphIssue<'static>>, Vec<GraphRelation<'static>>) {
    (
        vec![issue("APP-10"), issue("APP-11"), issue(SUBJECT)],
        vec![blocks("APP-10", "APP-11"), blocks("APP-11", SUBJECT)],
    )
}

/// A fork: the subject's one blocker has two open blockers of its own.
fn fork() -> (Vec<GraphIssue<'static>>, Vec<GraphRelation<'static>>) {
    (
        vec![issue("APP-8"), issue("APP-9"), issue("APP-11"), issue(SUBJECT)],
        vec![
            blocks("APP-8", "APP-11"),
            blocks("APP-9", "APP-11"),
            blocks("APP-11", SUBJECT),
        ],
    )
}

/// What the composer builds for a blocked pick, over static rows: the
/// dialog's subject and the graph it draws. No member has a pull request or
/// a live run.
fn blocked(
    issues: &[GraphIssue<'static>],
    relations: &[GraphRelation<'static>],
) -> (BlockedStart, IssueGraph) {
    let graph = block_graph(&[SUBJECT], relations, issues);
    let blockers = open_blockers_of_set(&[SUBJECT], relations, issues)
        .into_iter()
        .map(|issue| issue.identifier.to_string())
        .collect();
    let found = blocked_start::stack_line(SUBJECT, relations, issues);
    let members: Vec<StackMember> = found
        .line
        .iter()
        .map(|issue| StackMember {
            identifier: issue.identifier.to_string(),
            pr_state: None,
            branch: None,
            repository_id: Some(REPOSITORY.to_string()),
            running: false,
        })
        .collect();
    let subject = StackSubject {
        identifier: SUBJECT.to_string(),
        repository_id: Some(REPOSITORY.to_string()),
    };
    let planned = blocked_start::stack_plan(1, &subject, &members, found.fork, found.cycle);
    let (stack, stack_note) = stack_outcome(planned, |first| Some(first.to_string()));
    (
        BlockedStart {
            picked: vec![SUBJECT.to_string()],
            blockers,
            stack,
            stack_note,
        },
        graph,
    )
}

/// One state: a caption naming it over the alert.
fn specimen(
    key: &'static str,
    caption: &'static str,
    rows: (Vec<GraphIssue<'static>>, Vec<GraphRelation<'static>>),
    window: &mut Window,
    cx: &mut App,
) -> Div {
    let (blocked, graph) = blocked(&rows.0, &rows.1);
    // A specimen answers nothing: `false` keeps the hosting window open.
    let spec = blocked_start_alert(&blocked, graph, |_, _| false, |_, _| false);
    let muted = cx.theme().muted_foreground;
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(div().text_xs().text_color(muted).child(caption))
        .child(crate::native_dialog::alert_specimen(key, spec, window, cx))
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_6()
        .child(specimen(
            "sg-blocked-start-line",
            "Stacked PR enabled: the plan note says what starts first.",
            line(),
            window,
            cx,
        ))
        .child(specimen(
            "sg-blocked-start-fork",
            "Stacked PR disabled: the reason takes the plan note's place.",
            fork(),
            window,
            cx,
        ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Fixture {
        copy: CopyText,
        plan_cases: Vec<PlanCase>,
    }

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct CopyText {
        body_prefix: String,
        body_suffix: String,
        body_suffix_stackable: String,
    }

    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct PlanCase {
        line: Vec<serde_json::Value>,
        reason: Option<String>,
        note: Option<String>,
        plan_note: Option<String>,
    }

    fn fixture() -> Fixture {
        serde_json::from_str(include_str!(
            "../../../../../../../packages/domain-contract/fixtures/blocked-start.json"
        ))
        .expect("blocked-start.json parses")
    }

    /// The line state is the fixture's two-blocker line: Stacked PR enabled,
    /// the stackable sentence, the plan note under the graph, and a graph of
    /// three issues with the subject at the bottom.
    #[test]
    fn the_line_state_is_stackable_with_the_fixtures_plan_note() {
        let fixture = fixture();
        let case = fixture
            .plan_cases
            .iter()
            .find(|case| case.plan_note.is_some() && case.line.len() == 2)
            .expect("a two-blocker line case");
        let (issues, relations) = line();
        let (blocked, graph) = blocked(&issues, &relations);
        let stack = blocked.stack.as_ref().expect("a line stacks");
        assert_eq!(stack.issue_id, "APP-10");
        assert_eq!(stack.plan.run, vec!["APP-10", "APP-11", SUBJECT]);
        assert_eq!(blocked.stack_note, case.plan_note);
        assert_eq!(
            blocked.description(),
            format!(
                "{}#APP-11{}",
                fixture.copy.body_prefix, fixture.copy.body_suffix_stackable
            )
        );
        assert_eq!(graph.nodes.len(), 3);
        let subject = graph.nodes.iter().find(|node| node.subject).expect("a subject");
        assert_eq!((subject.id.as_str(), subject.wave), (SUBJECT, 2));
    }

    /// The fork state is the fixture's `many` case: Stacked PR disabled, the
    /// plain sentence, the reason's note in the plan note's place, and both
    /// of the fork's blockers in the graph's top wave.
    #[test]
    fn the_fork_state_is_disabled_with_the_fixtures_reason() {
        let fixture = fixture();
        let case = fixture
            .plan_cases
            .iter()
            .find(|case| case.reason.as_deref() == Some("many"))
            .expect("a fork case");
        let (issues, relations) = fork();
        let (blocked, graph) = blocked(&issues, &relations);
        assert!(blocked.stack.is_none());
        assert_eq!(blocked.stack_note, case.note);
        assert_eq!(
            blocked.description(),
            format!("{}#APP-11{}", fixture.copy.body_prefix, fixture.copy.body_suffix)
        );
        assert_eq!(graph.nodes.len(), 4);
        assert_eq!(graph.nodes.iter().filter(|node| node.wave == 0).count(), 2);
    }
}
