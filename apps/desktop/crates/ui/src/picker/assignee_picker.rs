//! EXP-1029 contract — the assignee picker: the team's members by avatar +
//! name, the email as the description and a search keyword; `Unassigned`
//! first. `pickers::assignee_*` (EXP-288) moves onto this.

use gpui::AnyElement;

use domain::rows::User;

use super::{OnPickerChange, Picker, PickerItem, PickerMode};

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

/// `mode`: single for the issue chip / dialog (an empty `value` = unassigned;
/// `allows_none` offers the `Unassigned` row, which reports an EMPTY set),
/// multi for the board filter (`allows_none` ignored).
pub(crate) fn assignee_picker(
    members: &[User],
    mode: PickerMode,
    value: Vec<String>,
    allows_none: bool,
    trigger: AnyElement,
    on_change: OnPickerChange<String>,
) -> Picker<String> {
    // The avatar is what makes a member row a MEMBER row, and it is not a
    // `PickerItem` slot — a picker glyph is an icon, never a photo. So the
    // one row body that needs one draws it over the primitive's, and
    // `Unassigned` keeps the plain body: there is nobody to picture.
    let avatars: Vec<(String, String, Option<String>)> = members
        .iter()
        .map(|member| {
            let email = member.email.clone().unwrap_or_default();
            let name = member.name.clone().unwrap_or_else(|| email.clone());
            (member.id.clone(), name, member.image.clone())
        })
        .collect();
    let picker = match mode {
        PickerMode::Single => Picker::single(
            assignee_items(members, allows_none),
            value.into_iter().next(),
            trigger,
            on_change,
        ),
        PickerMode::Multi => {
            Picker::multi(assignee_items(members, false), value, trigger, on_change)
        }
    };
    picker
        .search(true)
        .empty_text("No members")
        .render_item(move |item, cx| {
            use gpui::{IntoElement as _, ParentElement as _, Styled as _};
            let Some((id, name, image)) = avatars.iter().find(|(id, _, _)| *id == item.value)
            else {
                return super::picker_item_body(item, cx);
            };
            let avatar = crate::user_avatar::user_avatar(
                id,
                name,
                image.as_deref(),
                gpui_component::Size::XSmall,
                cx,
            );
            let body = super::picker_item_body(item, cx);
            gpui_component::h_flex()
                .flex_1()
                .min_w_0()
                .items_center()
                .gap_2()
                .child(avatar)
                .child(body)
                .into_any_element()
        })
}
