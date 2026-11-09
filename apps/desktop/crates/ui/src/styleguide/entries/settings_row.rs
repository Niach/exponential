//! VAPP-280 (round 7): `settings-row` (General components): THE Exponential
//! UI SettingsRow macro (one line of a settings page), painted by the gpui
//! painter from the catalog specimen
//! (`packages/exponential-ui/fixtures/specimens.json`
//! `exponential-ui-settings-row`: a Notifications group with a Switch, a
//! Select and a Slider row, then the stacked layout), so the twin cannot
//! drift from the web entry (`apps/styleguide/src/entries/settings-row.tsx`).

use gpui::{div, App, Div, ParentElement as _, Styled as _, Window};

use crate::exponential_ui_screen::{specimen_node, styleguide_surface};

pub(crate) const ID: &str = "settings-row";
pub(crate) const OWNER: &str = "VAPP-280";

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let tree = specimen_node("exponential-ui-settings-row", "specimen_settings_row").expect("the SettingsRow specimen");
    div().w_full().child(styleguide_surface("sg-settings-row", tree, window, cx))
}
