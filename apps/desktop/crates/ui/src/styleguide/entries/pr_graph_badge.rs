//! EXP-1079/EXP-1058/SLOP-16: `pr-graph-badge` (Special components): the
//! issue header's graph badge, the IDE half of the styleguide entry the web
//! page draws.
//!
//! The badge (`crate::pr_graph::badge`) is a muted ICON BUTTON (SLOP-16
//! round 2): the glyph names the shape: `pr-stack`, `pr-batch`,
//! `relation-blocked-by`: with a mono `+N` beside it when others ride
//! along; the shape's name is its tooltip. A click opens the "Related work"
//! dialog (round 5): EXACTLY the relations card's foldable bands: "Blocked
//! by", "Same pull request", "Pull request stack": over its issue rows (a
//! pull request in the same row shape), capped at three with "Show N more",
//! the same layout and copy ×4.
//!
//! EXP-1092: the demo draws the REAL badge: `crate::pr_graph::badge` over a
//! `BadgeSpec` whose graph comes from `domain::pr_graph::pr_graph`, the header's
//! own call: in each state it has (stack, batch, blocked-by alone), over
//! static synced-shape `pull_requests` rows (EXP-1294), never live ones. A
//! lone pull request with no relation draws NO badge (`badge` answers
//! `None`), which is a state too.

use gpui::{div, App, Div, ParentElement as _, Styled as _, Window};

use domain::pr_graph::pr_graph;
use domain::rows::{Issue, PullRequestIssueRow};

use crate::pr_graph::{badge, BadgeSpec};

use super::fixture_rows::{self, PrDemo};

pub(crate) const ID: &str = "pr-graph-badge";
pub(crate) const OWNER: &str = "EXP-1058";

/// A synced-shape issue row with no pull request.
fn issue(identifier: &str, title: &str) -> Issue {
    fixture_rows::row(&serde_json::json!({
        "id": format!("sg-{identifier}"),
        "boardId": "sg-board",
        "number": 1,
        "identifier": identifier,
        "title": title,
        "status": "in_progress",
    }))
}

/// The header's spec for `identifier`'s issue over `demo`.
fn issue_spec(demo: &PrDemo, identifier: &str) -> BadgeSpec {
    let subject = demo.issues.iter().find(|issue| issue.identifier == identifier);
    BadgeSpec { graph: demo.with_set(|set| pr_graph(subject, None, set)) }
}

/// The badge's states, each the header's own spec shape.
fn specs() -> Vec<(&'static str, BadgeSpec)> {
    // A stack of three: EXP-11 on master, EXP-12 on EXP-11, EXP-13 on EXP-12.
    let stack = PrDemo::open(&[
        ("EXP-13", "Stacked follow-up", 13, "exp/EXP-12"),
        ("EXP-12", "Middle of the stack", 12, "exp/EXP-11"),
        ("EXP-11", "Bottom of the stack", 11, "master"),
    ]);
    // One pull request closing two issues.
    let mut batch = PrDemo::open(&[("EXP-20", "Batch of fixes", 20, "master")]);
    batch.issues.push(issue("EXP-21", "Second fix"));
    batch.links.push(fixture_rows::row::<PullRequestIssueRow>(&serde_json::json!({
        "id": "sg-link-EXP-21",
        "pullRequestId": "sg-pr-EXP-20",
        "issueId": "sg-EXP-21",
    })));
    // An issue with no pull request, blocked by another.
    let waiting = PrDemo {
        pulls: Vec::new(),
        links: Vec::new(),
        issues: vec![issue("EXP-30", "Waiting on a blocker")],
    };
    // A lone pull request: no relation.
    let lone = PrDemo::open(&[("EXP-40", "A lone fix", 40, "master")]);
    vec![
        ("styleguide-pr-graph-badge-stack", issue_spec(&stack, "EXP-12")),
        ("styleguide-pr-graph-badge-batch", issue_spec(&batch, "EXP-20")),
        ("styleguide-pr-graph-badge-blocked", {
            // EXP-1097: the open blockers ride the graph.
            let mut spec = issue_spec(&waiting, "EXP-30");
            spec.graph.blocked_by = vec![issue("EXP-31", "The blocker")];
            spec
        }),
        // A lone pull request: no relation, so no badge at all.
        ("styleguide-pr-graph-badge-plain", issue_spec(&lone, "EXP-40")),
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

    /// Every state but the lone pull request earns an icon button, with
    /// the glyph's shape and the `+N` the header would draw.
    #[test]
    fn every_related_state_draws_an_icon_button() {
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
                None,
            ]
        );
    }
}
