//! VAPP-217 (eApps U2): `record-form` (General components): THE Exponential UI
//! RecordForm macro, painted by the gpui painter from the catalog specimen
//! (`packages/exponential-ui/fixtures/specimens.json` `exponential-ui-record-form`),
//! so the twin cannot drift from the web entry
//! (`apps/styleguide/src/entries/record-form.tsx`).

use gpui::{div, App, Div, ParentElement as _, Styled as _, Window};

use crate::exponential_ui_screen::{specimen_node, styleguide_surface};

pub(crate) const ID: &str = "record-form";
pub(crate) const OWNER: &str = "VAPP-217";

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let tree = specimen_node("exponential-ui-record-form", "specimen-record-form").expect("the RecordForm specimen");
    div().w_full().child(styleguide_surface("sg-record-form", tree, window, cx))
}
