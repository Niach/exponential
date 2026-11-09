//! #EXP-1629: `key-recorder` (General components): THE Exponential UI
//! KeyRecorder macro (the shortcut field), painted by the gpui painter from
//! the catalog specimen (`packages/exponential-ui/fixtures/specimens.json`
//! `exponential-ui-key-recorder`: a binding, none, recording, a held
//! modifier, an inline error), so the twin cannot drift from the web entry
//! (`apps/styleguide/src/entries/key-recorder.tsx`).

use gpui::{div, App, Div, ParentElement as _, Styled as _, Window};

use crate::exponential_ui_screen::{specimen_node, styleguide_surface};

pub(crate) const ID: &str = "key-recorder";
pub(crate) const OWNER: &str = "EXP-1629";

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let tree = specimen_node("exponential-ui-key-recorder", "specimen-key-recorder").expect("the KeyRecorder specimen");
    div().w_full().child(styleguide_surface("sg-key-recorder", tree, window, cx))
}
