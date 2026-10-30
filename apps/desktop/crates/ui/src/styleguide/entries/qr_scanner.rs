//! VAPP-162: `qr-scanner` (General components): THE Exponential UI
//! QrScanner, painted by the gpui painter from the catalog specimen
//! (`fixtures/specimens.json` `exponential-ui-qr-scanner`: the viewfinder
//! with its hint, then disabled). The desktop has no camera host, so the
//! viewfinder shows the placeholder (`ui-scan` + "Camera unavailable"); a
//! host feed arrives through `HostPlugin::qr_scanner_feed`.

use gpui::{div, App, Div, ParentElement as _, Styled as _, Window};

use crate::exponential_ui_screen::{specimen_node, styleguide_surface};

pub(crate) const ID: &str = "qr-scanner";
pub(crate) const OWNER: &str = "VAPP-162";

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let tree = specimen_node("exponential-ui-qr-scanner", "specimen-qr-scanner").expect("the QrScanner specimen");
    div().w_full().child(styleguide_surface("sg-qr-scanner", tree, window, cx))
}
