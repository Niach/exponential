//! EXP-1021 — the `picker-assignee` styleguide entry: the assignee picker — the member's avatar + name, the email muted.
//!
//! Filled by EXP-1021; never edits `entries/mod.rs` nor the index.

use gpui::{App, Div, Window};

use super::picker::{chip, column, demo, inert};

pub(crate) const ID: &str = "picker-assignee";
pub(crate) const OWNER: &str = "EXP-1021";

pub(crate) fn render(_window: &mut Window, _cx: &mut App) -> Div {
    column(vec![
        demo("assignee — single; `Unassigned` is a real row, not a placeholder", |window, cx| {
            let members = super::picker::demo_members();
            crate::picker::assignee_picker::assignee_picker(
                &members,
                crate::picker::PickerMode::Single,
                vec!["user-1".to_string()],
                true,
                chip("sg-picker-assignee", "Ada Lovelace", cx),
                inert(),
            )
            .id("sg-picker-assignee-surface")
            .render(window, cx)
        }),
        demo("assignee — multi (a filter): `allows_none` is ignored", |window, cx| {
            let members = super::picker::demo_members();
            crate::picker::assignee_picker::assignee_picker(
                &members,
                crate::picker::PickerMode::Multi,
                vec!["user-2".to_string()],
                false,
                chip("sg-picker-assignees", "1 assignee", cx),
                inert(),
            )
            .id("sg-picker-assignees-surface")
            .render(window, cx)
        }),
    ])
}
