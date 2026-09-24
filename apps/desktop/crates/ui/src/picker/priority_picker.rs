//! EXP-1029 contract — the priority picker: contract `issuePriority` in its
//! order (`domain::options::ISSUE_PRIORITY_OPTIONS`), each by the priority
//! glyph in its tone. `pickers::priority_*` (EXP-288) moves onto this.

use gpui::AnyElement;

use domain::options::ISSUE_PRIORITY_OPTIONS;
use domain::IssuePriority;

use super::{OnPickerChange, Picker, PickerItem};

pub(crate) fn priority_items() -> Vec<PickerItem<IssuePriority>> {
    ISSUE_PRIORITY_OPTIONS
        .iter()
        .map(|option| PickerItem::new(option.value, option.label))
        .collect()
}

pub(crate) fn priority_picker(
    value: Option<IssuePriority>,
    trigger: AnyElement,
    on_change: OnPickerChange<IssuePriority>,
) -> Picker<IssuePriority> {
    Picker::single(priority_items(), value, trigger, on_change)
}
