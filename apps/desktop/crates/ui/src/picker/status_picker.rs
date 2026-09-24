//! EXP-1029 contract — the status picker: the team's `issue_statuses` rows
//! (EXP-314) in display order, each by its glyph in its colour
//! (`icons::resolved_status_icon`). `pickers::status_*` (EXP-288) moves onto
//! this.

use gpui::AnyElement;

use domain::statuses::ResolvedStatus;

use super::{OnPickerChange, Picker, PickerItem};

pub(crate) fn status_items(statuses: &[ResolvedStatus]) -> Vec<PickerItem<String>> {
    statuses
        .iter()
        .map(|status| PickerItem::new(status.group_key.clone(), status.name.clone()))
        .collect()
}

pub(crate) fn status_picker(
    statuses: &[ResolvedStatus],
    value: Option<String>,
    trigger: AnyElement,
    on_change: OnPickerChange<String>,
) -> Picker<String> {
    Picker::single(status_items(statuses), value, trigger, on_change)
}
