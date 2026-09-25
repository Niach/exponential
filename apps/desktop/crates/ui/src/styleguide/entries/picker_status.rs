//! EXP-1021 — the `picker-status` styleguide entry: the status picker — the team's rows, each by its glyph in its colour.
//!
//! Filled by EXP-1021; never edits `entries/mod.rs` nor the index.

use gpui::{App, Div, Window};

use super::picker::{chip, column, demo, inert};

pub(crate) const ID: &str = "picker-status";
pub(crate) const OWNER: &str = "EXP-1021";

pub(crate) fn render(_window: &mut Window, _cx: &mut App) -> Div {
    column(vec![
        demo("status — single; vocabulary order, Backlog leads (EXP-448)", |window, cx| {
            let statuses = domain::statuses::default_resolved_statuses();
            crate::picker::status_picker::status_picker(
                &statuses,
                crate::picker::PickerMode::Single,
                vec![statuses[0].group_key.clone()],
                chip("sg-picker-status", "Backlog", cx),
                inert(),
            )
            .id("sg-picker-status-surface")
            .render(window, cx)
        }),
        demo("status — multi (a filter)", |window, cx| {
            let statuses = domain::statuses::default_resolved_statuses();
            crate::picker::status_picker::status_picker(
                &statuses,
                crate::picker::PickerMode::Multi,
                vec![statuses[1].group_key.clone()],
                chip("sg-picker-statuses", "1 status", cx),
                inert(),
            )
            .id("sg-picker-statuses-surface")
            .render(window, cx)
        }),
    ])
}
