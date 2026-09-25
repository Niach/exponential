//! EXP-1029 contract — the status picker: the team's `issue_statuses` rows
//! (EXP-314) in display order, each by its glyph in its colour
//! (`icons::resolved_status_icon`). `pickers::status_*` (EXP-288) moves onto
//! this.

use gpui::AnyElement;

use domain::statuses::ResolvedStatus;

use crate::icons::{glyph_icon, static_status_tint_color};

use super::{OnPickerChange, Picker, PickerItem, PickerMode};

/// The team's statuses as rows, in the ONE vocabulary order the list groups
/// and the settings pane already render (EXP-448) — a picker never re-sorts.
pub(crate) fn status_items(statuses: &[ResolvedStatus]) -> Vec<PickerItem<String>> {
    statuses
        .iter()
        .map(|status| {
            PickerItem::new(status.group_key.clone(), status.name.clone())
                .icon(glyph_icon(status.glyph))
                .color(static_status_tint_color(&status.tint))
        })
        .collect()
}

/// `mode`: single for the issue chip / dialog, multi for the board filter.
pub(crate) fn status_picker(
    statuses: &[ResolvedStatus],
    mode: PickerMode,
    value: Vec<String>,
    trigger: AnyElement,
    on_change: OnPickerChange<String>,
) -> Picker<String> {
    match mode {
        PickerMode::Single => {
            Picker::single(status_items(statuses), value.into_iter().next(), trigger, on_change)
        }
        PickerMode::Multi => Picker::multi(status_items(statuses), value, trigger, on_change),
    }
}
