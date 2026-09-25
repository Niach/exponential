//! EXP-1021 — the `picker-priority` styleguide entry: the priority picker — contract `issuePriority`, each glyph in its tone.
//!
//! Filled by EXP-1021; never edits `entries/mod.rs` nor the index.

use gpui::{App, Div, Window};

use super::picker::{chip, column, demo, inert};

pub(crate) const ID: &str = "picker-priority";
pub(crate) const OWNER: &str = "EXP-1021";

pub(crate) fn render(_window: &mut Window, _cx: &mut App) -> Div {
    column(vec![demo(
        "priority — single; the contract table's order, never the picker's",
        |window, cx| {
            crate::picker::priority_picker::priority_picker(
                crate::picker::PickerMode::Single,
                vec![domain::IssuePriority::High],
                chip("sg-picker-priority", "High", cx),
                inert(),
            )
            .id("sg-picker-priority-surface")
            .render(window, cx)
        },
    )])
}
