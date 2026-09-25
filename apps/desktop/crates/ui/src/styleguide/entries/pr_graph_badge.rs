//! EXP-1079/EXP-1058 — `pr-graph-badge` (Special components): the work
//! header's graph badge, the IDE half of the styleguide entry the web page
//! draws.
//!
//! The badge (`crate::pr_graph::badge`) is the STACKED issue chip
//! (`crate::issue_chip`'s deck, EXP-1058): the front chip names the subject
//! pull request's representative issue and `+N` counts every other issue on
//! the stack or batch; on the Run face of a run with no issue the same chip
//! box leads with the `session-tree` concept and names the run. Hovering or
//! clicking it opens the face's overlay. The demo draws the REAL element
//! ([`crate::pr_graph::chip_face`]) over static data — never live rows.

use gpui::{div, Div, ParentElement as _, SharedString, Styled as _};

use crate::pr_graph::{chip_face, ChipFace};

pub(crate) const ID: &str = "pr-graph-badge";
pub(crate) const OWNER: &str = "EXP-1058";

/// One badge state: the front chip's words, whether it is a run's, and how
/// many ride behind it.
struct State {
    identifier: &'static str,
    title: &'static str,
    runs: bool,
    count: usize,
}

pub(crate) fn render() -> Div {
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
    let mut row = div().flex().flex_wrap().items_center().gap_4().pt_2();
    for (index, state) in states.iter().enumerate() {
        row = row.child(chip_face(
            &format!("styleguide-pr-graph-badge-{index}"),
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
    row
}
