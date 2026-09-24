//! EXP-1029 contract — the issue picker (single or multi): rows read
//! `IDENT Title`, search matches the identifier and the title. The composer
//! picks several (a batch); relations, duplicates and the stack dialog pick
//! one. `issue_picker.rs` (EXP-892's search sheet) moves onto this.

use gpui::AnyElement;

use domain::rows::Issue;

use super::{OnPickerChange, Picker, PickerItem};

pub(crate) fn issue_items(issues: &[Issue]) -> Vec<PickerItem<String>> {
    issues
        .iter()
        .map(|issue| {
            PickerItem::new(issue.id.clone(), format!("{} {}", issue.identifier, issue.title))
                .keywords(vec![issue.identifier.clone().into(), issue.title.clone().into()])
        })
        .collect()
}

pub(crate) fn issue_picker(
    issues: &[Issue],
    value: Option<String>,
    trigger: AnyElement,
    on_change: OnPickerChange<String>,
) -> Picker<String> {
    Picker::single(issue_items(issues), value, trigger, on_change)
        .search(true)
        .empty_text("No issues")
}

pub(crate) fn issue_multi_picker(
    issues: &[Issue],
    value: Vec<String>,
    trigger: AnyElement,
    on_change: OnPickerChange<String>,
) -> Picker<String> {
    Picker::multi(issue_items(issues), value, trigger, on_change)
        .search(true)
        .empty_text("No issues")
}
