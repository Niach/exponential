//! EXP-1029 contract — the account picker under the shared picker API.
//!
//! `coding_selects::account_picker` (EXP-991: brand mark + login email per
//! row, the EXP-992 rate-limit preview under the picked login) moves onto
//! [`Picker`] in EXP-1021, keeping its trigger variants and its preview.
//! Until then this file declares the typed constructor over the same
//! options; the rows' preview bars ride as each item's `description`.

use gpui::AnyElement;

use super::{OnPickerChange, Picker, PickerItem};

pub(crate) fn account_items(options: &[coding::AccountOption]) -> Vec<PickerItem<String>> {
    options
        .iter()
        .map(|option| {
            PickerItem::new(option.account_option_key(), option.email.clone())
                .keywords(vec![option.email.clone().into(), option.agent.label().into()])
        })
        .collect()
}

pub(crate) fn account_picker(
    options: &[coding::AccountOption],
    value: Option<String>,
    trigger: AnyElement,
    on_change: OnPickerChange<String>,
) -> Picker<String> {
    Picker::single(account_items(options), value, trigger, on_change)
}
