//! EXP-1079/EXP-1058 — `pr-graph-badge` (Special components): the work
//! header's graph badge, the IDE half of the styleguide entry the web page
//! draws.
//!
//! The badge (`crate::pr_graph::badge`) is the STACKED issue chip
//! (`crate::issue_chip`'s deck, EXP-1058): the front chip names the subject
//! pull request's representative issue and `+N` counts every other issue on
//! the stack or batch; on the Run face of a run with no issue the same chip
//! box leads with the `session-tree` concept and names the run. Hovering or
//! clicking it opens the face's overlay.
//!
//! EXP-1092: the demo draws the REAL badge — `crate::pr_graph::badge` over a
//! `BadgeSpec` whose graph comes from `domain::pr_graph::pr_graph`, the header's
//! own call — in each state it has (stack, batch, blocked-by alone on the
//! Issue face, a run family named by its run title), over static synced-shape
//! rows, never live ones. A lone pull request with no relation draws NO badge
//! (`badge` answers `None`), which is a state too. The bare chip faces
//! ([`crate::pr_graph::chip_face`]) stay as the first row: the element the
//! badge wraps, before the overlay behaviour.

use gpui::{div, App, Div, ParentElement as _, SharedString, Styled as _, Window};

use domain::pr_graph::pr_graph;
use domain::rows::{CodingSession, Issue};

use crate::pr_graph::{badge, chip_face, BadgeFace, BadgeSpec, ChipFace};

pub(crate) const ID: &str = "pr-graph-badge";
pub(crate) const OWNER: &str = "EXP-1058";

/// One chip-face state: the front chip's words, whether it is a run's, and
/// how many ride behind it.
struct State {
    identifier: &'static str,
    title: &'static str,
    runs: bool,
    count: usize,
}

/// A synced-shape issue row. `head`/`base` = its PR branch and the branch
/// that PR targets (the stack edge); `pr` = its pull request (a batch shares
/// one).
fn issue(identifier: &str, title: &str, head: Option<&str>, base: Option<&str>, pr: Option<&str>) -> Issue {
    serde_json::from_value(serde_json::json!({
        "id": format!("sg-{identifier}"),
        "board_id": "sg-board",
        "number": 1,
        "identifier": identifier,
        "title": title,
        "status": "in_progress",
        "branch": head,
        "pr_base_branch": base,
        "pr_url": pr,
        "pr_state": pr.map(|_| "open"),
    }))
    .expect("a styleguide issue row deserializes")
}

fn run(id: &str, parent: Option<&str>) -> CodingSession {
    serde_json::from_value(serde_json::json!({
        "id": id,
        "action_name": "Chat",
        "parent_session_id": parent,
        "status": "running",
        "started_at": "2026-09-26T09:00:00Z",
    }))
    .expect("a styleguide run row deserializes")
}

/// The badge's states, each the header's own spec shape.
fn specs() -> Vec<(&'static str, BadgeSpec)> {
    // A stack of three: EXP-11 on master, EXP-12 on EXP-11, EXP-13 on EXP-12.
    let stack = vec![
        issue("EXP-13", "Stacked follow-up", Some("exp/EXP-13"), Some("exp/EXP-12"), Some("https://github.com/o/r/pull/13")),
        issue("EXP-12", "Middle of the stack", Some("exp/EXP-12"), Some("exp/EXP-11"), Some("https://github.com/o/r/pull/12")),
        issue("EXP-11", "Bottom of the stack", Some("exp/EXP-11"), Some("master"), Some("https://github.com/o/r/pull/11")),
    ];
    // One pull request closing three issues.
    let batch_pr = Some("https://github.com/o/r/pull/20");
    let batch = vec![
        issue("EXP-20", "Batch of fixes", Some("exp/batch-1"), Some("master"), batch_pr),
        issue("EXP-21", "Second fix", Some("exp/batch-1"), Some("master"), batch_pr),
        issue("EXP-22", "Third fix", Some("exp/batch-1"), Some("master"), batch_pr),
    ];
    // An issue with no pull request, blocked by another.
    let waiting = issue("EXP-30", "Waiting on a blocker", None, None, None);
    let blocker = issue("EXP-31", "The blocker", None, None, None);
    // A chat run and the two runs it started.
    let runs = vec![
        run("sg-run", None),
        run("sg-run-child-1", Some("sg-run")),
        run("sg-run-child-2", Some("sg-run")),
    ];
    let plain = |graph, face| BadgeSpec {
        graph,
        face,
        blocked_by: Vec::new(),
        blocks_graph: domain::issue_graph::IssueGraph::default(),
        run_title: None,
    };
    vec![
        (
            "styleguide-pr-graph-badge-stack",
            plain(pr_graph(Some(&stack[1]), None, &stack, &[]), BadgeFace::Changes),
        ),
        (
            "styleguide-pr-graph-badge-batch",
            plain(pr_graph(Some(&batch[0]), None, &batch, &[]), BadgeFace::Issue),
        ),
        (
            "styleguide-pr-graph-badge-blocked",
            BadgeSpec {
                blocked_by: vec![blocker],
                ..plain(
                    pr_graph(Some(&waiting), None, std::slice::from_ref(&waiting), &[]),
                    BadgeFace::Issue,
                )
            },
        ),
        (
            "styleguide-pr-graph-badge-run",
            BadgeSpec {
                run_title: Some(SharedString::from("Chat: tidy the release notes")),
                ..plain(pr_graph(None, Some(&runs[0]), &[], &runs), BadgeFace::Run)
            },
        ),
        (
            // A lone pull request: no relation, so no badge at all.
            "styleguide-pr-graph-badge-plain",
            plain(
                pr_graph(Some(&stack[2]), None, std::slice::from_ref(&stack[2]), &[]),
                BadgeFace::Issue,
            ),
        ),
    ]
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let states = [
        // The Run face of a run with a family and no issue.
        State { identifier: "", title: "Chat: tidy the release notes", runs: true, count: 2 },
        // A stacked pull request: the others on the stack.
        State { identifier: "EXP-12", title: "Stacked follow-up", runs: false, count: 2 },
        // A pull request that closes several issues.
        State { identifier: "EXP-20", title: "Batch of fixes", runs: false, count: 2 },
        // Blockers alone on the Issue face: the subject, nothing behind it.
        State { identifier: "EXP-30", title: "Waiting on a blocker", runs: false, count: 0 },
    ];
    let muted = theme::tokens::MUTED_FOREGROUND.to_hsla();
    let mut faces = div().flex().flex_wrap().items_center().gap_4().pt_2();
    for (index, state) in states.iter().enumerate() {
        faces = faces.child(chip_face(
            &format!("styleguide-pr-graph-badge-face-{index}"),
            ChipFace {
                identifier: SharedString::from(state.identifier),
                title: SharedString::from(state.title),
                status: None,
                runs: state.runs,
                count: state.count,
            },
            muted,
        ));
    }
    // The real badge, one per state; the plain one answers `None`.
    let mut badges = div().flex().flex_wrap().items_center().gap_4().pt_2();
    for (id, spec) in specs() {
        badges = badges.children(badge(id, spec, cx));
    }
    div().flex().flex_col().gap_2().child(faces).child(badges)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every state but the lone pull request earns a chip, with the `+N` the
    /// header would draw.
    #[test]
    fn every_state_but_the_plain_one_draws_a_chip() {
        let counts: Vec<Option<usize>> = specs()
            .iter()
            .map(|(_, spec)| {
                domain::pr_graph::badge_chip(&spec.graph, spec.face)
                    .map(|chip| chip.count)
                    .or_else(|| (!spec.blocked_by.is_empty()).then_some(0))
            })
            .collect();
        assert_eq!(counts, vec![Some(2), Some(2), Some(0), Some(2), None]);
    }
}
