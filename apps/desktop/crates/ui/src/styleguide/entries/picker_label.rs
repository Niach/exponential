//! EXP-1021 — the `picker-label` styleguide entry: the label picker — ALWAYS multi, each row its colour dot.
//!
//! Filled by EXP-1021; never edits `entries/mod.rs` nor the index.

use gpui::Div;

use super::picker::{chip, column, demo, inert};

pub(crate) const ID: &str = "picker-label";
pub(crate) const OWNER: &str = "EXP-1021";

pub(crate) fn render() -> Div {
    column(vec![demo(
        "label — a colour and NO glyph is what makes a row a coloured dot",
        |window, cx| {
            let labels = super::picker::demo_labels();
            crate::picker::label_picker::label_picker(
                &labels,
                vec!["label-1".to_string()],
                chip("sg-picker-label", "bug", cx),
                inert(),
            )
            .id("sg-picker-label-surface")
            .render(window, cx)
        },
    )])
}
