//! EXP-1495: `mosaic` (General components): THE Exponential UI Mosaic (the
//! plan page's wide picture group), painted by the gpui painter from the
//! catalog specimen (`packages/exponential-ui/fixtures/specimens.json`
//! `exponential-ui-mosaic`: four pictures = the lead, two side tiles and the
//! `+1` overlay), so the twin cannot drift from the web entry
//! (`apps/styleguide/src/entries/mosaic.tsx`).

use gpui::{div, App, Div, ParentElement as _, Styled as _, Window};

use crate::exponential_ui_screen::{specimen_node, styleguide_surface};

pub(crate) const ID: &str = "mosaic";
pub(crate) const OWNER: &str = "EXP-1495";

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let tree = specimen_node("exponential-ui-mosaic", "specimen-mosaic").expect("the Mosaic specimen");
    div().w_full().child(styleguide_surface("sg-mosaic", tree, window, cx))
}
