//! VAPP-232 (eApps U3): `calendar` (General components): THE eUI Calendar macro
//! painted by the gpui painter from the web entry's own specimens
//! (`apps/styleguide/src/entries/calendar.specimens.json`: the node + the
//! records value it reads), so the twin cannot drift from the web entry.

use gpui::{div, App, Div, ParentElement as _, Styled as _, Window};
use gpui_component::v_flex;
use serde_json::Value;

use crate::exponential_ui_screen::styleguide_surface_in;

pub(crate) const ID: &str = "calendar";
pub(crate) const OWNER: &str = "VAPP-232";

const SPECIMENS: &str = include_str!("../../../../../../styleguide/src/entries/calendar.specimens.json");

/// One specimen case's node (by id) and the shared data model.
pub(crate) fn case(id: &str) -> (Value, Value) {
    let doc: Value = serde_json::from_str(SPECIMENS).expect("calendar.specimens.json");
    let node = doc["cases"].as_array().into_iter().flatten().find(|c| c["id"] == id).map(|c| c["node"].clone()).unwrap_or(Value::Null);
    (node, doc["data"].clone())
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let (month, data) = case("calendar-month");
    let (week, _) = case("calendar-week");
    let month = styleguide_surface_in("sg-calendar-month", month, None, Some(data.clone()), window, cx);
    let week = styleguide_surface_in("sg-calendar-week", week, None, Some(data), window, cx);
    v_flex().gap_6().child(div().w_full().child(month)).child(div().w_full().child(week))
}
