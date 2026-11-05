//! #EDE-10 / #EDE-11 / #EDE-12: eDE's factory shell documents (apps/ede/ui)
//! under the gpui painter, the way ede-shell draws them on ede-compositor
//! (the shots `ede-panel`, `ede-panel-menu`, `ede-switcher` and `ede-osd` are
//! the compositor's own captures). The `surface` header (where the shell
//! puts a document, which host functions it may call) is not part of the tree.

use gpui::{div, px, App, Div, ParentElement as _, Styled as _, Window};
use gpui_component::v_flex;
use serde_json::{json, Value};

use crate::exponential_ui_screen::styleguide_surface_in;

pub(crate) const ID: &str = "ede-panel";
pub(crate) const OWNER: &str = "EDE-10";

const PANEL: &str = include_str!("../../../../../../ede/ui/panel.json");
const MENU: &str = include_str!("../../../../../../ede/ui/menu.json");
const SWITCHER: &str = include_str!("../../../../../../ede/ui/switcher.json");
const OSD: &str = include_str!("../../../../../../ede/ui/osd.json");

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
    let apps = json!({"query": "", "apps": [
        {"id": "org.gnome.Calculator.desktop", "name": "Calculator", "comment": "Perform arithmetic, scientific or financial calculations", "exec": "gnome-calculator", "icon": ""},
        {"id": "nemo.desktop", "name": "Files", "comment": "Access and organize files", "exec": "nemo", "icon": ""},
        {"id": "firefox.desktop", "name": "Firefox", "comment": "Browse the web", "exec": "firefox", "icon": ""},
        {"id": "terminal.desktop", "name": "Terminal", "comment": "Use the command line", "exec": "x-terminal-emulator", "icon": ""}
    ]});
    let (switcher, _) = document(SWITCHER);
    let windows = json!({"switcher": {"windows": [
        {"id": "w1", "title": "Firefox", "selected": false, "thumbnail": ""},
        {"id": "w2", "title": "Terminal", "selected": true, "thumbnail": ""},
        {"id": "w3", "title": "Files", "selected": false, "thumbnail": ""}
    ]}});
    let (osd, _) = document(OSD);
    let level = json!({"osd": {"icon": "media-volume-down", "level": 45, "label": "45%"}});
    v_flex()
        .gap_3()
        .child(div().w_full().child(styleguide_surface_in("sg-ede-panel", panel, None, Some(data), window, cx)))
        .child(div().w(px(360.0)).child(styleguide_surface_in("sg-ede-menu", menu, None, Some(apps), window, cx)))
        .child(div().w(px(760.0)).child(styleguide_surface_in("sg-ede-switcher", switcher, None, Some(windows), window, cx)))
        .child(div().w(px(280.0)).child(styleguide_surface_in("sg-ede-osd", osd, None, Some(level), window, cx)))
}
