//! VAPP-162: `qr-code` (General components): THE Exponential UI QrCode,
//! painted by the gpui painter from the catalog specimen
//! (`packages/exponential-ui/fixtures/specimens.json`
//! `exponential-ui-qr-code`: the pairing code with its label, then level H
//! at 120 px), so the twin cannot drift from the web entry
//! (`apps/styleguide/src/entries/qr-code.tsx`).

use gpui::{div, App, Div, ParentElement as _, Styled as _, Window};

use crate::exponential_ui_screen::{specimen_node, styleguide_surface};

pub(crate) const ID: &str = "qr-code";
pub(crate) const OWNER: &str = "VAPP-162";

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let tree = specimen_node("exponential-ui-qr-code", "specimen-qr-code").expect("the QrCode specimen");
    div().w_full().child(styleguide_surface("sg-qr-code", tree, window, cx))
}
