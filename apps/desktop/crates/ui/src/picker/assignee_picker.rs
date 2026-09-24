//! EXP-1029 contract — the assignee picker: the team's members by avatar +
//! name, the email as the description and a search keyword; `Unassigned`
//! first. `pickers::assignee_*` (EXP-288) moves onto this.

use gpui::AnyElement;

use domain::rows::User;

use super::{OnPickerChange, Picker, PickerItem};

/// The row that clears the pick.
pub(crate) const UNASSIGNED_VALUE: &str = "";

pub(crate) fn assignee_items(members: &[User], allows_none: bool) -> Vec<PickerItem<String>> {
    let rows = members.iter().map(|member| {
        let email = member.email.clone().unwrap_or_default();
        let name = member.name.clone().unwrap_or_else(|| email.clone());
        PickerItem::new(member.id.clone(), name.clone())
            .description(email.clone())
            .keywords(vec![name.into(), email.into()])
    });
    if allows_none {
        std::iter::once(PickerItem::new(UNASSIGNED_VALUE.to_string(), "Unassigned"))
            .chain(rows)
            .collect()
    } else {
        rows.collect()
    }
}

pub(crate) fn assignee_picker(
    members: &[User],
    value: Option<String>,
    allows_none: bool,
    trigger: AnyElement,
    on_change: OnPickerChange<String>,
) -> Picker<String> {
    Picker::single(assignee_items(members, allows_none), value, trigger, on_change)
        .search(true)
        .empty_text("No members")
}
