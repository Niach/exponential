//! EXP-1029 contract — `session-tree` (3 Special components). EXP-996 fills
//! this entry's demo and never edits `entries/mod.rs` or the section index.
//!
//! EXP-1049 draws the DESKTOP half of the rule: every session list (the rail's
//! Running section, the Recent panel) flattens
//! [`domain::session_tree::session_tree`] and paints the EXP-965 connector over
//! the depths. A GROUP row — a workflow, or a PR stack — leads with its concept
//! icon and folds behind the same chevron a parent run wears; it is NOT a
//! session, so it carries no state dot, no device glyph and no Stop. The demo
//! is static on purpose: the entry documents the SHAPE, never live rows.

use gpui::{div, px, App, Div, ParentElement as _, Styled as _, Window};
use gpui_component::{Icon, Sizable as _};

use crate::icons::{registry, ExpIcon};

pub(crate) const ID: &str = "session-tree";
pub(crate) const OWNER: &str = "EXP-996";

/// The list rows' base padding and per-level indent, as `run_rows` draws them.
const ROW_PAD: f32 = 12.;

/// One demo row: what it leads with, what it says, and whether it is folded.
struct Row {
    depth: usize,
    /// `Some` = a GROUP row's concept icon; `None` = a run.
    icon: Option<ExpIcon>,
    identifier: Option<&'static str>,
    label: String,
    /// `Some(true)` = a fold chevron pointing right (subtree hidden).
    folded: Option<bool>,
}

fn group(depth: usize, icon: ExpIcon, label: String, folded: bool) -> Row {
    Row {
        depth,
        icon: Some(icon),
        identifier: None,
        label,
        folded: Some(folded),
    }
}

fn run(depth: usize, identifier: &'static str, label: &'static str, folded: Option<bool>) -> Row {
    Row {
        depth,
        icon: None,
        identifier: Some(identifier),
        label: label.to_string(),
        folded,
    }
}

pub(crate) fn render(_window: &mut Window, _cx: &mut App) -> Div {
    let rows = vec![
        // A workflow's node runs, newest activity first.
        group(
            0,
            registry::NAV_WORKFLOWS,
            "Checkout rework".to_string(),
            false,
        ),
        run(1, "EXP-997", "fixture table", None),
        run(1, "EXP-996", "sessionTree ×4", None),
        // A stack, LINEAR: the lowest pull request first. Its label is the ×4
        // copy `domain` owns, never a string invented here.
        group(
            0,
            registry::PR_STACK,
            domain::session_tree::STACK_GROUP_LABEL.to_string(),
            false,
        ),
        run(1, "EXP-941", "rail polish", None),
        run(1, "EXP-942", "rail follow-up", None),
        // A parent run and the run it started (`sessions_start`).
        run(0, "EXP-1049", "desktop mirror", Some(false)),
        run(1, "EXP-1050", "doc sweep", None),
        // A folded group: the row stays, its runs are hidden.
        group(
            0,
            registry::NAV_WORKFLOWS,
            "Docs refresh".to_string(),
            true,
        ),
    ];
    // EXP-965: the connector comes off the VISIBLE depth sequence, exactly as
    // the real lists derive it.
    let guides = domain::tree_guides::guides_for(
        &rows.iter().map(|row| row.depth).collect::<Vec<_>>(),
    );
    let muted = theme::tokens::MUTED_FOREGROUND.to_hsla();
    let mut column = div().flex().flex_col().w_full().min_w_0();
    for (index, row) in rows.iter().enumerate() {
        let guides = guides.get(index).cloned().unwrap_or_default();
        let mut element = div()
            .flex()
            .w_full()
            .min_w_0()
            .relative()
            .items_center()
            .gap_2()
            .px_3()
            .py_2p5()
            .pl(px(ROW_PAD + crate::tree_guides::LEVEL_PITCH * guides.depth() as f32))
            .children(crate::tree_guides::guide_layer(&guides, ROW_PAD, 0.));
        if let Some(folded) = row.folded {
            element = element.child(
                div().flex_shrink_0().child(
                    Icon::from(if folded {
                        registry::UI_CHEVRON_RIGHT
                    } else {
                        registry::UI_CHEVRON_DOWN
                    })
                    .xsmall()
                    .text_color(muted),
                ),
            );
        }
        if let Some(icon) = row.icon.clone() {
            element = element.child(
                div()
                    .flex_shrink_0()
                    .child(Icon::from(icon).xsmall().text_color(muted)),
            );
        }
        if let Some(identifier) = row.identifier {
            element = element.child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(muted)
                    .child(identifier),
            );
        }
        // A group row is toned like a band; a run says its title in full.
        let title = div().flex_1().min_w_0().truncate().child(row.label.clone());
        let title = if row.icon.is_some() {
            title.text_xs().text_color(muted)
        } else {
            title.text_sm()
        };
        column = column.child(element.child(title));
    }
    column
}
