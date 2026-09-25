//! EXP-1029 contract — the label picker: ALWAYS multi, searchable, each row
//! its colour dot + name; the surface stays open across toggles.
//! `pickers::label_picker_popover` (EXP-288) moves onto this.

use gpui::AnyElement;

use domain::rows::Label;

use super::{OnPickerChange, Picker, PickerItem};

pub(crate) fn label_items(labels: &[Label]) -> Vec<PickerItem<String>> {
    labels
        .iter()
        .map(|label| PickerItem::new(label.id.clone(), label.name.clone()))
        .collect()
}

pub(crate) fn label_picker(
    labels: &[Label],
    value: Vec<String>,
    trigger: AnyElement,
    on_change: OnPickerChange<String>,
) -> Picker<String> {
    Picker::multi(label_items(labels), value, trigger, on_change)
        .search(true)
        .empty_text("No labels")
}
