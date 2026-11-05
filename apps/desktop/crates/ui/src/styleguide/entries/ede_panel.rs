//! #EDE-10 / #EDE-11: eDE's factory shell documents (apps/ede/ui) under the
//! gpui painter, the way ede-shell draws them on ede-compositor (the shots
//! `ede-panel` and `ede-panel-menu` are the compositor's own captures). The
//! `surface` header (where the shell puts a document) is not part of the tree.

use gpui::{div, px, App, Div, ParentElement as _, Styled as _, Window};
use gpui_component::v_flex;
use serde_json::{json, Value};

use crate::exponential_ui_screen::styleguide_surface_in;

pub(crate) const ID: &str = "ede-panel";
pub(crate) const OWNER: &str = "EDE-10";

const PANEL: &str = include_str!("../../../../../../ede/ui/panel.json");
const MENU: &str = include_str!("../../../../../../ede/ui/menu.json");

fn document(source: &str) -> (Value, Value) {
    let doc: Value = serde_json::from_str(source).expect("an eDE factory document");
    (doc["root"].clone(), doc.get("data").cloned().unwrap_or(Value::Null))
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let (panel, mut data) = document(PANEL);
    if let Some(data) = data.as_object_mut() {
        data.insert("time".into(), json!("12:00"));
        data.insert("windows".into(), json!([{"id": "w1", "title": "Firefox", "focused": true}, {"id": "w2", "title": "Terminal", "focused": false}]));
    }
    let (menu, _) = document(MENU);
    v_flex()
        .gap_3()
        .child(div().w_full().child(styleguide_surface_in("sg-ede-panel", panel, None, Some(data), window, cx)))
        .child(div().w(px(280.0)).child(styleguide_surface_in("sg-ede-menu", menu, None, None, window, cx)))
}
