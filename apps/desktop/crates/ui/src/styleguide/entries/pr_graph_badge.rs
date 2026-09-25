//! EXP-1079 — `pr-graph-badge` (Special components): the work header's graph
//! badge, the IDE half of the styleguide entry the web page draws.
//!
//! The badge (`crate::pr_graph::badge`) is ONE glass pill that says what a
//! piece of work is PART OF: the `pr-stack` concept with `2 of 3` for a
//! stacked pull request, `pr-batch` with `3 issues` for a batch, both for a
//! batch inside a stack, and `session-tree` alone on the Run face of a run
//! with a family. It stands beside the face toggle, so it wears the toggle's
//! rung — [`crate::pr_graph::badge_size`] = the header action size (EXP-926),
//! never a size of its own. The demo is static on purpose: it documents the
//! glyph-per-relation rule and the rung, never live rows.

use gpui::{div, px, App, Div, ParentElement as _, Styled as _, Window};
use gpui_component::{Icon, Sizable as _};

use crate::icons::{registry, ExpIcon};

pub(crate) const ID: &str = "pr-graph-badge";
pub(crate) const OWNER: &str = "EXP-1079";

/// One badge state: its concept glyph(s) and the label beside them.
struct State {
    glyphs: Vec<ExpIcon>,
    label: Option<&'static str>,
}

/// The capsule at the header rung: the glyphs at the rung's own glyph size,
/// the label after them — the shape `glass_pill_button` draws. Calling
/// `crate::pr_graph::badge` itself instead is EXP-1092.
fn capsule(state: &State) -> Div {
    let size = crate::pr_graph::badge_size();
    let muted = theme::tokens::MUTED_FOREGROUND.to_hsla();
    let mut pill = div()
        .flex()
        .items_center()
        .flex_shrink_0()
        .h(px(size.height()))
        .px_3()
        .gap_1p5()
        .rounded_full()
        .border_1()
        .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
        .bg(theme::tokens::glass::FILL_CARD.to_hsla());
    for glyph in &state.glyphs {
        pill = pill.child(
            Icon::new(glyph.clone())
                .with_size(px(size.glyph()))
                .text_color(muted),
        );
    }
    if let Some(label) = state.label {
        pill = pill.child(div().text_sm().text_color(muted).child(label));
    }
    pill
}

pub(crate) fn render(_window: &mut Window, _cx: &mut App) -> Div {
    let states = vec![
        // The Run face of a run with a family and no pull-request relation.
        State {
            glyphs: vec![registry::SESSION_TREE],
            label: None,
        },
        // A stacked pull request: its position.
        State {
            glyphs: vec![registry::PR_STACK],
            label: Some("2 of 3"),
        },
        // A pull request that closes several issues.
        State {
            glyphs: vec![registry::PR_BATCH],
            label: Some("3 issues"),
        },
        // A batch INSIDE a stack wears both concepts (EXP-897 §4).
        State {
            glyphs: vec![registry::PR_STACK, registry::PR_BATCH],
            label: Some("2 of 3"),
        },
    ];
    let mut row = div().flex().flex_wrap().items_center().gap_2();
    for state in &states {
        row = row.child(capsule(state));
    }
    row
}
