//! #VAPP-285: `tree` (General components): THE genUI Tree, painted by the
//! gpui painter from the catalog specimen
//! (`packages/exponential-ui/fixtures/specimens.json` `exponential-ui-tree`:
//! a repository with folders open and closed, a selected file, and a lazy
//! folder opened on its loading row), so the twin cannot drift from the web
//! entry (`apps/styleguide/src/entries/tree.tsx`).

use gpui::{div, App, Div, ParentElement as _, Styled as _, Window};

use crate::exponential_ui_screen::{specimen_node, styleguide_surface};

pub(crate) const ID: &str = "tree";
pub(crate) const OWNER: &str = "VAPP-285";

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let tree = specimen_node("exponential-ui-tree", "specimen_tree").expect("the Tree specimen");
    div().w_full().child(styleguide_surface("sg-tree", tree, window, cx))
}
