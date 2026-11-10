//! #EXP-1760: `genui-color-picker` (General components): THE Exponential UI ColorPicker
//! macro, painted by the gpui painter from the catalog specimen
//! (`packages/exponential-ui/fixtures/specimens.json`
//! `exponential-ui-color-picker`: a pick, none, a child trigger, the inline
//! grid, the open grid; #EXP-1775), so the twin cannot
//! drift from the web entry (`apps/styleguide/src/entries/genui-color-picker.tsx`).

use gpui::{div, App, Div, ParentElement as _, Styled as _, Window};

use crate::exponential_ui_screen::{specimen_node, styleguide_surface};

pub(crate) const ID: &str = "genui-color-picker";
pub(crate) const OWNER: &str = "EXP-1760";

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let tree = specimen_node("exponential-ui-color-picker", "specimen_color_picker").expect("the ColorPicker specimen");
    div().w_full().child(styleguide_surface("sg-color-picker", tree, window, cx))
}
