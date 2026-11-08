//! #EDE-65: `box-hover` (General components): an eUI Box's `on.hover`,
//! fired as the pointer enters (Motion's rule, no payload). The specimen is
//! the web entry's own file (`apps/styleguide/src/entries/box-hover.specimens.json`),
//! so the twin cannot drift: three tiles whose hover sets /selected, the
//! selected one on `$color.accent` (the eDE Alt-Tab hover-select).

use gpui::{div, App, Div, ParentElement as _, Styled as _, Window};
use serde_json::Value;

use crate::exponential_ui_screen::styleguide_surface_in;

pub(crate) const ID: &str = "box-hover";
pub(crate) const OWNER: &str = "EDE-65";

const SPECIMENS: &str = include_str!("../../../../../../styleguide/src/entries/box-hover.specimens.json");

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let doc: Value = serde_json::from_str(SPECIMENS).expect("box-hover.specimens.json");
    div().w_full().child(styleguide_surface_in("sg-box-hover", doc["tiles"].clone(), None, Some(doc["data"].clone()), window, cx))
}
