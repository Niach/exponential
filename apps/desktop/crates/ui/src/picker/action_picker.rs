//! EXP-1029 contract — the action picker: the team's actions (and the two
//! listed builtins) by curated icon + name. The composer's action chip and
//! the automation editor pick one.

use gpui::AnyElement;

use crate::icons::action_icon;

use super::{OnPickerChange, Picker, PickerItem};

/// An action row as the picker reads it (a synced `actions` row or a
/// client-constructed builtin, `api::actions::builtin_*`).
pub(crate) struct ActionPickerAction {
    pub id: String,
    pub name: String,
    /// Contract `boardIcon` (the curated action set); `None` = the default.
    pub icon: Option<String>,
    pub description: Option<String>,
}

pub(crate) fn action_items(actions: &[ActionPickerAction]) -> Vec<PickerItem<String>> {
    actions
        .iter()
        .map(|action| {
            let mut item = PickerItem::new(action.id.clone(), action.name.clone())
                .icon(action_icon(action.icon.as_deref()));
            if let Some(description) = &action.description {
                item = item.description(description.clone());
            }
            item
        })
        .collect()
}

pub(crate) fn action_picker(
    actions: &[ActionPickerAction],
    value: Option<String>,
    trigger: AnyElement,
    on_change: OnPickerChange<String>,
) -> Picker<String> {
    Picker::single(action_items(actions), value, trigger, on_change)
        .search(true)
        .empty_text("No actions")
}
