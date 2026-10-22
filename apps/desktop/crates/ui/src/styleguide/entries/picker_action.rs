//! EXP-1021 — the `picker-action` styleguide entry: rows by curated icon +
//! name; a row with no icon of its own draws the default glyph. NEW-2:
//! prompts replaced actions and no app screen opens this picker any more; it
//! stays for the natives' mirrors until NEW-8.
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
                    id: "action-release-notes".to_string(),
                    name: "Release notes".to_string(),
                    icon: Some("rocket".to_string()),
                    description: Some("Draft the notes from the merged pull requests".to_string()),
                },
                crate::picker::action_picker::ActionPickerAction {
                    id: "action-triage".to_string(),
                    name: "Triage the inbox".to_string(),
                    icon: Some("bug".to_string()),
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
