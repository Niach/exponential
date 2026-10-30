//! EXP-1459: `sealed-locked` (Special components): THE locked state for
//! sealed session content this install cannot open (yet)
//! (`fixtures/sealed-content.json` `placeholder`), the twin of web
//! `sealed-locked.tsx`: the transcript's divider pill with the lock concept
//! and the reason's copy (`noKey`, `unreadable`), no time. The thread
//! screen paints the same pill (`surface_card::locked_pill`).

use domain::sealed::LockedReason;
use gpui::{px, App, Div, ParentElement as _, Styled as _, Window};
use gpui_component::v_flex;

pub(crate) const ID: &str = "sealed-locked";
pub(crate) const OWNER: &str = "EXP-1459";

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    v_flex()
        .w(px(520.))
        .gap_3()
        .child(crate::surface_card::locked_pill("no-key", LockedReason::NoKey, cx))
        .child(crate::surface_card::locked_pill("unreadable", LockedReason::Unreadable, cx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icons::registry;
    use domain::sealed::PLACEHOLDER;

    #[test]
    fn the_fixture_names_the_icon_and_both_labels() {
        assert!(registry::concept_by_name(&PLACEHOLDER.icon).is_some(), "{} is a concept", PLACEHOLDER.icon);
        assert_eq!(PLACEHOLDER.text(LockedReason::NoKey), "Waiting for the key");
        assert_eq!(PLACEHOLDER.text(LockedReason::Unreadable), "Can't be opened here");
    }
}
