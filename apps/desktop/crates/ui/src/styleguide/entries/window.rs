//! VAPP-290: `window` (General components): THE Exponential UI Window (the
//! window frame exponential.at puts its visuals in), painted by the gpui
//! painter from the catalog specimen
//! (`packages/exponential-ui/fixtures/specimens.json` `exponential-ui-window`:
//! a titled frame and an untitled one, each around a padded body), so the
//! twin cannot drift from the web entry (`apps/styleguide/src/entries/window.tsx`).

use gpui::{div, App, Div, ParentElement as _, Styled as _, Window};

use crate::exponential_ui_screen::{specimen_node, styleguide_surface};

pub(crate) const ID: &str = "window";
pub(crate) const OWNER: &str = "VAPP-290";

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let tree = specimen_node("exponential-ui-window", "specimen_window").expect("the Window specimen");
    div().w_full().child(styleguide_surface("sg-window", tree, window, cx))
}
