//! EXP-1029 contract — the account picker, and (EXP-1021) THE one the IDE
//! draws: the automation editor's "Account" row goes through it.
//!
//! The rows are EXP-991's — the agent's brand MARK plus the login's email,
//! never the profile name — and a dead credential rides as the row's muted
//! description. `coding_selects::account_picker` keeps the surfaces this
//! picker is not the shape for (EXP-1019 / EXP-1030 own those): the composer's
//! inline options line and the settings panes, with EXP-992's usage preview
//! in their tooltips.

use gpui::AnyElement;

use super::{OnPickerChange, Picker, PickerItem};

pub(crate) fn account_items(options: &[coding::AccountOption]) -> Vec<PickerItem<String>> {
    options
        .iter()
        .map(|option| {
            // EXP-862: an account row says which agent it is with the
            // brand MARK, never with the agent's name beside the login.
            let mut item = PickerItem::new(option.account_option_key(), option.email.clone())
                .icon(crate::coding_selects::agent_mark(option.agent))
                .keywords(vec![option.email.clone().into(), option.agent.label().into()]);
            // EXP-991's health badge: a login the device can no longer spend
            // still LISTS — picking it is how a machine gets repaired — so it
            // says so in the row's own muted line.
            if let Some(badge) = option.health.badge_label() {
                item = item.description(badge);
            }
            item
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
