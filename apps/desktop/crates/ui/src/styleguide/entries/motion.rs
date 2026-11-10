//! EXP-1604: `motion` (General components): THE eUI Motion painted by the
//! gpui painter (`eui_gpui::motion`, the raster → RenderImage path). The
//! specimens are the web entry's own file
//! (`packages/exponential-ui/fixtures/motion.specimens.json`), so the twin cannot
//! drift: a spinner, a success check drawing on, a Slider scrubbing a
//! Motion through its bound `progress`, and a themed illustration in light
//! and dark; EXP-1605: the `interactive` state-machine specimens (a like
//! button toggled by clicks, a bar following a bound `inputs.level`).

use eui_core::theme::Mode;
use gpui::{div, px, App, Div, ParentElement as _, SharedString, Styled as _, Window};
use gpui_component::{h_flex, v_flex, ActiveTheme as _};
use serde_json::Value;

use crate::exponential_ui_screen::styleguide_surface_in;

pub(crate) const ID: &str = "motion";
pub(crate) const OWNER: &str = "EXP-1604";

const SPECIMENS: &str = include_str!("../../../../../../../packages/exponential-ui/fixtures/motion.specimens.json");

fn specimens() -> Value {
    serde_json::from_str(SPECIMENS).expect("motion.specimens.json")
}

fn illustration(key: &'static str, label: &str, mode: Mode, window: &mut Window, cx: &mut App) -> Div {
    let tree = specimens()["illustration"].clone();
    let view = styleguide_surface_in(key, tree, Some(mode), None, window, cx);
    v_flex()
        .gap_2()
        .child(div().text_xs().text_color(cx.theme().muted_foreground).child(SharedString::from(label.to_string())))
        .child(div().w(px(200.)).child(view))
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let doc = specimens();
    let cases = styleguide_surface_in("sg-motion-cases", doc["cases"].clone(), None, Some(doc["data"].clone()), window, cx);
    let interactive = styleguide_surface_in("sg-motion-interactive", doc["interactive"].clone(), None, Some(doc["data"].clone()), window, cx);
    let light = illustration("sg-motion-light", "Light", Mode::Light, window, cx);
    let dark = illustration("sg-motion-dark", "Dark", Mode::Dark, window, cx);
    v_flex().gap_6().child(div().w_full().child(cases)).child(div().w_full().child(interactive)).child(h_flex().gap_6().items_start().child(light).child(dark))
}
