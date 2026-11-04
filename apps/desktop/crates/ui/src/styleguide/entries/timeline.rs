//! VAPP-232 (eApps U3): `timeline` (General components): THE eUI Timeline macro
//! painted by the gpui painter from the web entry's own specimens
//! (`apps/styleguide/src/entries/calendar.specimens.json`: the node + the
//! records value it reads), so the twin cannot drift from the web entry.

use gpui::{div, App, Div, ParentElement as _, Styled as _, Window};
use gpui_component::v_flex;
use serde_json::Value;

use crate::exponential_ui_screen::styleguide_surface_in;

pub(crate) const ID: &str = "timeline";
pub(crate) const OWNER: &str = "VAPP-232";

const SPECIMENS: &str = include_str!("../../../../../../styleguide/src/entries/calendar.specimens.json");

/// One specimen case's node (by id) and the shared data model.
pub(crate) fn case(id: &str) -> (Value, Value) {
    let doc: Value = serde_json::from_str(SPECIMENS).expect("calendar.specimens.json");
    let node = doc["cases"].as_array().into_iter().flatten().find(|c| c["id"] == id).map(|c| c["node"].clone()).unwrap_or(Value::Null);
    (node, doc["data"].clone())
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let (node, data) = case("timeline");
    v_flex().child(div().w_full().child(styleguide_surface_in("sg-timeline", node, None, Some(data), window, cx)))
}
