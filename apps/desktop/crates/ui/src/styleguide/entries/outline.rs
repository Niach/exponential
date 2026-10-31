//! EXP-1495: `outline` (General components): THE Exponential UI Outline
//! (the plan page's anchor list), painted by the gpui painter from the
//! catalog specimen (`packages/exponential-ui/fixtures/specimens.json`
//! `exponential-ui-outline`: the caption, three rows, the second active with
//! its bar and count), so the twin cannot drift from the web entry
//! (`apps/styleguide/src/entries/outline.tsx`).

use gpui::{div, App, Div, ParentElement as _, Styled as _, Window};

use crate::exponential_ui_screen::{specimen_node, styleguide_surface};

pub(crate) const ID: &str = "outline";
pub(crate) const OWNER: &str = "EXP-1495";

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let tree = specimen_node("exponential-ui-outline", "specimen-outline").expect("the Outline specimen");
    div().w_full().child(styleguide_surface("sg-outline", tree, window, cx))
}
