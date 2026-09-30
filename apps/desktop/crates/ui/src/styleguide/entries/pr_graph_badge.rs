//! EXP-1079/EXP-1058/SLOP-16 — `pr-graph-badge` (Special components): the
//! work header's graph badge, the IDE half of the styleguide entry the web
//! page draws.
//!
//! The badge (`crate::pr_graph::badge`) is a muted ICON BUTTON (SLOP-16
//! round 2): the glyph names the shape — `pr-stack`, `pr-batch`,
//! `session-tree`, `relation-blocked-by` — with a mono `+N` beside it when
//! others ride along; the shape's name is its tooltip. A click opens the
//! "Related work" dialog (round 3): per section a group band over the
//! product's own flat rows (issue, pull request, run) or the compact blocks
//! graph, the same layout and copy ×4.
//!
//! EXP-1092: the demo draws the REAL badge — `crate::pr_graph::badge` over a
//! `BadgeSpec` whose graph comes from `domain::pr_graph::pr_graph`, the header's
//! own call — in each state it has (stack, batch, a run family, blocked-by
//! alone), over static synced-shape rows, never live ones. A lone pull
//! request with no relation draws NO badge (`badge` answers `None`), which is
//! a state too.

use gpui::{div, App, Div, ParentElement as _, Styled as _, Window};

use domain::pr_graph::pr_graph;
use domain::rows::{CodingSession, Issue};

use crate::pr_graph::{badge, BadgeFace, BadgeSpec};

pub(crate) const ID: &str = "pr-graph-badge";
pub(crate) const OWNER: &str = "EXP-1058";

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
    // One pull request closing two issues.
    let batch_pr = Some("https://github.com/o/r/pull/20");
    let batch = vec![
        issue("EXP-20", "Batch of fixes", Some("exp/batch-1"), Some("master"), batch_pr),
        issue("EXP-21", "Second fix", Some("exp/batch-1"), Some("master"), batch_pr),
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
        blocks_graph: domain::issue_graph::IssueGraph::default(),
        subject_issue_id: None,
        subject_session_id: None,
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
            {
                // EXP-1097: the open blockers ride the graph.
                let mut graph = pr_graph(Some(&waiting), None, std::slice::from_ref(&waiting), &[]);
                graph.blocked_by = vec![blocker];
                plain(graph, BadgeFace::Issue)
            },
        ),
        (
            "styleguide-pr-graph-badge-run",
            plain(pr_graph(None, Some(&runs[0]), &[], &runs), BadgeFace::Run),
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
    // The real badge, one per state; the plain one answers `None`.
    let mut badges = div().flex().flex_wrap().items_center().gap_4().pt_2();
    for (id, spec) in specs() {
        badges = badges.children(badge(id, spec, cx));
    }
    div().flex().flex_col().gap_2().child(badges)
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::pr_graph::BadgeShape;

    /// Every state but the lone pull request earns an icon button, with the
    /// glyph's shape and the `+N` the header would draw.
    #[test]
    fn every_state_but_the_plain_one_draws_an_icon_button() {
        let faces: Vec<_> = specs()
            .iter()
            .map(|(_, spec)| crate::pr_graph::badge_face(spec))
            .collect();
        assert_eq!(
            faces,
            vec![
                Some((BadgeShape::Stack, 2)),
                Some((BadgeShape::Batch, 1)),
                Some((BadgeShape::Blocked, 0)),
                Some((BadgeShape::Runs, 2)),
                None,
            ]
        );
    }
}
