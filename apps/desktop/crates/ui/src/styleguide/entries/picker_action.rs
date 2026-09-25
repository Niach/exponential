//! EXP-1021 — the `picker-action` styleguide entry: the action picker — the team's actions by curated icon + name.
//!
//! Filled by EXP-1021; never edits `entries/mod.rs` nor the index.

use gpui::{App, Div, Window};

use super::picker::{chip, column, demo, inert};

pub(crate) const ID: &str = "picker-action";
pub(crate) const OWNER: &str = "EXP-1021";

pub(crate) fn render(_window: &mut Window, _cx: &mut App) -> Div {
    column(vec![demo(
        "action — searchable; the curated icon leads, the description is muted",
        |window, cx| {
            let actions = vec![
                crate::picker::action_picker::ActionPickerAction {
                    id: "builtin:fix-conflicts".to_string(),
                    name: "Fix merge conflicts".to_string(),
                    icon: Some("wrench".to_string()),
                    description: Some("Rebase, resolve, force-push".to_string()),
                },
                crate::picker::action_picker::ActionPickerAction {
                    id: "builtin:create-action".to_string(),
                    name: "Create action".to_string(),
                    icon: Some("sparkles".to_string()),
                    description: None,
                },
            ];
            crate::picker::action_picker::action_picker(
                &actions,
                None,
                chip("sg-picker-action", "Pick an action", cx),
                inert(),
            )
            .id("sg-picker-action-surface")
            .render(window, cx)
        },
    )])
}
