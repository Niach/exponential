//! #EXP-1760: `genui-icon-picker` (General components): THE Exponential UI IconPicker
//! macro, painted by the gpui painter from the catalog specimen
//! (`packages/exponential-ui/fixtures/specimens.json`
//! `exponential-ui-icon-picker`: a pick, none, the open grid), so the twin cannot
//! drift from the web entry (`apps/styleguide/src/entries/genui-icon-picker.tsx`).

use gpui::{div, App, Div, ParentElement as _, Styled as _, Window};

use crate::exponential_ui_screen::{specimen_node, styleguide_surface};

pub(crate) const ID: &str = "genui-icon-picker";
pub(crate) const OWNER: &str = "EXP-1760";

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let tree = specimen_node("exponential-ui-icon-picker", "specimen_icon_picker").expect("the IconPicker specimen");
    div().w_full().child(styleguide_surface("sg-icon-picker", tree, window, cx))
}
