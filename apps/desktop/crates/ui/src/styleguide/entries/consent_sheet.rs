//! VAPP-262 (Engine E2): `consent-sheet`, the Engine's default ConsentUi
//! (`packages/eengine/fixtures/consent.json`, docs/spec/apps.md §13.2) under
//! the gpui painter: the ask sheet and the Permissions sheet, the fixture's
//! `expected` flat trees (what `consentSheet` / `manageSheet` return and the
//! eengine crate replays), so the twin cannot drift from the web entry.

use gpui::{div, px, App, Div, ParentElement as _, Styled as _, Window};
use gpui_component::{v_flex, ActiveTheme as _};
use serde_json::{json, Map, Value};

use crate::exponential_ui_screen::styleguide_surface_in;

pub(crate) const ID: &str = "consent-sheet";
pub(crate) const OWNER: &str = "VAPP-262";

const FIXTURE: &str = include_str!("../../../../../../../packages/eengine/fixtures/consent.json");

/// A Drawer is a layer at the bottom of the surface's viewport, so each sheet
/// gets a Box of that height and a viewport to match (the web paints inline).
const ASK_HEIGHT: f32 = 440.;
const MANAGE_HEIGHT: f32 = 300.;

const NODE_KEYS: [&str; 8] = ["id", "component", "children", "slots", "on", "style", "visible", "accessibility"];

/// A flat A2UI list (root = `root`) as the nested tree the painter takes.
fn nest(components: &[Value]) -> Value {
    fn node(components: &[Value], id: &str) -> Value {
        let Some(c) = components.iter().find(|c| c["id"] == id) else { return Value::Null };
        let mut out = Map::new();
        out.insert("id".into(), c["id"].clone());
        out.insert("component".into(), c["component"].clone());
        let props: Map<String, Value> = c.as_object().into_iter().flatten().filter(|(k, _)| !NODE_KEYS.contains(&k.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect();
        out.insert("props".into(), Value::Object(props));
        if let Some(on) = c.get("on") {
            out.insert("on".into(), on.clone());
        }
        if let Some(children) = c["children"].as_array() {
            out.insert("children".into(), Value::Array(children.iter().filter_map(Value::as_str).map(|k| node(components, k)).collect()));
        }
        if let Some(slots) = c["slots"].as_object() {
            out.insert("slots".into(), Value::Object(slots.iter().filter_map(|(k, v)| Some((k.clone(), node(components, v.as_str()?)))).collect()));
        }
        Value::Object(out)
    }
    node(components, "root")
}

/// The sheet inside a Box of `height`: the box its Drawer sits at the bottom of.
fn framed(sheet: Value, height: f32) -> Value {
    json!({ "id": "frame", "component": "Box", "style": { "height": height }, "children": [sheet] })
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let doc: Value = serde_json::from_str(FIXTURE).expect("consent.json");
    let sheet = &doc["sheetCases"][0];
    let manage = &doc["manageCases"][0];
    let ask = framed(nest(sheet["expected"].as_array().map(Vec::as_slice).unwrap_or_default()), ASK_HEIGHT);
    let permissions = framed(nest(manage["expected"].as_array().map(Vec::as_slice).unwrap_or_default()), MANAGE_HEIGHT);
    let muted = cx.theme().muted_foreground;
    let caption = |text: String| div().text_xs().text_color(muted).child(text);
    let ask_caption = caption(format!("{} · {}", sheet["app"].as_str().unwrap_or_default(), sheet["entry"]["id"].as_str().unwrap_or_default()));
    let manage_caption = caption(doc["copy"]["manageTitle"].as_str().unwrap_or_default().to_string());
    let ask = styleguide_surface_in("sg-consent-sheet-ask", ask, None, Some(sheet["data"].clone()), window, cx);
    let permissions = styleguide_surface_in("sg-consent-sheet-manage", permissions, None, None, window, cx);
    ask.update(cx, |view, cx| view.set_viewport_height(ASK_HEIGHT, cx));
    permissions.update(cx, |view, cx| view.set_viewport_height(MANAGE_HEIGHT, cx));
    v_flex()
        .gap_3()
        .child(ask_caption)
        .child(div().w(px(480.)).child(ask))
        .child(manage_caption)
        .child(div().w(px(480.)).child(permissions))
}
