//! #EXP-1629: `avatar` (General components): THE Exponential UI Avatar,
//! painted by the gpui painter from the catalog specimen
//! (`packages/exponential-ui/fixtures/specimens.json`
//! `exponential-ui-avatar`), so the twin cannot drift from the web entry
//! (`apps/styleguide/src/entries/avatar.tsx`).

use gpui::{div, App, Div, ParentElement as _, Styled as _, Window};

use crate::exponential_ui_screen::{specimen_node, styleguide_surface};

pub(crate) const ID: &str = "avatar";
pub(crate) const OWNER: &str = "EXP-1629";

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let tree = specimen_node("exponential-ui-avatar", "specimen_avatar").expect("the Avatar specimen");
    div().w_full().child(styleguide_surface("sg-avatar", tree, window, cx))
}
