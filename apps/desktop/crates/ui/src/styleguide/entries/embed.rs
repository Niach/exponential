//! #EDE-65: `embed` (General components): THE Exponential UI Embed (a
//! nested genUI surface drawn in place, sandboxed), painted by the gpui
//! painter from the catalog specimen
//! (`packages/exponential-ui/fixtures/specimens.json` `exponential-ui-embed`:
//! the example, a result card with its own data, a question card whose
//! reply reaches the host as `action`, a bad surface's error box), so the
//! twin cannot drift from the web entry (`apps/styleguide/src/entries/embed.tsx`).

use gpui::{div, App, Div, ParentElement as _, Styled as _, Window};

use crate::exponential_ui_screen::{specimen_node, styleguide_surface};

pub(crate) const ID: &str = "embed";
pub(crate) const OWNER: &str = "EDE-65";

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let tree = specimen_node("exponential-ui-embed", "specimen_embed").expect("the Embed specimen");
    div().w_full().child(styleguide_surface("sg-embed", tree, window, cx))
}
