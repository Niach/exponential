//! Round 9 through the gpui painter (`packages/exponential-ui/docs/
//! round-9-contract.md`): a field's check severities (§6.6), `accessibility`
//! `live` and `hidden` (§5.1), the Markdown inline profile (§9.1).

use std::cell::Cell;
use std::rc::Rc;

use eui_core::theme::{self, Mode};
use eui_core::themes::builtin_theme;
use eui_core::NestedNode;
use eui_gpui::host::HostPlugin;
use eui_gpui::paint::markdown::{MdStyles, TextSpec};
use eui_gpui::view::{SurfaceView, SurfaceViewOptions};
use gpui::{prelude::*, App, Entity, Hsla, Role, TestAppContext, VisualTestContext, Window};
use serde_json::{json, Value};

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
}

fn surface_with(cx: &mut TestAppContext, tree: Value, data: Value, host: Rc<dyn HostPlugin>) -> (Entity<SurfaceView>, &mut VisualTestContext) {
    cx.update(gpui_component::init);
    let options = SurfaceViewOptions { theme: builtin_theme("neutral"), mode: Mode::Light, host, ..Default::default() };
    let (view, vcx) = cx.add_window_view(move |window, cx| SurfaceView::new(options, window, cx));
    view.update(vcx, |v, cx| {
        v.record_bounds(true);
        v.set_data("", Some(data), cx).unwrap();
        let out = v.set_nested(serde_json::from_value::<NestedNode>(tree).unwrap(), cx);
        assert!(out.issues.is_empty(), "{:?}", out.issues);
    });
    draw(vcx);
    draw(vcx);
    (view, vcx)
}

struct Plain;
impl HostPlugin for Plain {}

fn surface(cx: &mut TestAppContext, tree: Value, data: Value) -> (Entity<SurfaceView>, &mut VisualTestContext) {
    surface_with(cx, tree, data, Rc::new(Plain))
}

fn press(view: &Entity<SurfaceView>, cx: &mut VisualTestContext, id: &str) {
    let id = id.to_string();
    cx.update(|window, cx| view.update(cx, |v, cx| {
        let i = v.index_of(&id).unwrap();
        v.press(i, window, cx);
    }));
    draw(cx);
    draw(cx);
}

fn token(name: &str) -> Hsla {
    let t = builtin_theme("neutral").unwrap();
    eui_gpui::paint::parts::theme_color(Some(&t), Mode::Light, name).unwrap()
}

fn ink(view: &Entity<SurfaceView>, cx: &mut VisualTestContext, id: &str) -> Hsla {
    view.read_with(cx, |v, _| v.painted_colors(id)).unwrap_or_else(|| panic!("no {id}")).1
}

fn border(view: &Entity<SurfaceView>, cx: &mut VisualTestContext, id: &str) -> String {
    format!("{:?}", view.read_with(cx, |v, _| v.painted_style(id)).unwrap_or_else(|| panic!("no {id}")))
}

fn node_states(view: &Entity<SurfaceView>, cx: &mut VisualTestContext, id: &str) -> Vec<String> {
    view.read_with(cx, |v, _| v.placed_nodes().iter().find(|n| n.id == id && !n.removed).map(|n| n.states.clone()).unwrap_or_default())
}

fn check(path: &str) -> Value {
    json!([{"condition": {"@path": path}}])
}

fn severities() -> Value {
    json!({
        "e": {"valid": false, "severity": "error", "message": "Not an email"},
        "w": {"valid": false, "severity": "warning", "message": "Weak"},
        "i": {"valid": false, "severity": "info", "message": "Shown"}
    })
}

/// Every checkable field, one severity: the `error` part wears its colour.
#[gpui::test]
fn an_advisory_paints_the_error_part_and_keeps_the_control(cx: &mut TestAppContext) {
    let field = |id: &str, component: &str, path: &str| -> Value {
        let mut props = json!({"label": "L", "name": id, "checks": check(path)});
        if matches!(component, "Radio" | "ChoiceTiles" | "Select") {
            props["options"] = json!([{"label": "A", "value": "a"}, {"label": "B", "value": "b"}]);
        }
        if component == "ChoiceTiles" {
            props.as_object_mut().unwrap().remove("label");
        }
        json!({"id": id, "component": component, "props": props})
    };
    let mut children = vec![];
    for component in ["Input", "Checkbox", "Radio", "ChoiceTiles", "Select"] {
        children.push(field(&format!("w{component}"), component, "/w"));
        children.push(field(&format!("i{component}"), component, "/i"));
    }
    children.push(json!({"id": "go", "component": "Button", "props": {"label": "Send", "submit": true}}));
    let tree = json!({"id": "f", "component": "Form", "props": {"name": "f"}, "on": {"submit": {"event": {"name": "ok"}}}, "children": children});
    let (view, cx) = surface(cx, tree, severities());
    let before = border(&view, cx, "wInput.field");
    press(&view, cx, "go");
    for component in ["Input", "Checkbox", "Radio", "ChoiceTiles", "Select"] {
        let (w, i) = (format!("w{component}.error"), format!("i{component}.error"));
        assert_eq!(node_states(&view, cx, &w), ["warning"], "{w}");
        assert_eq!(node_states(&view, cx, &i), ["info"], "{i}");
        assert_eq!(ink(&view, cx, &w), token("warning"), "{w}");
        assert_eq!(ink(&view, cx, &i), token("mutedForeground"), "{i}");
        assert_eq!(view.read_with(cx, |v, _| v.accessible_info(&w)).unwrap().role, Role::Status, "{w}");
        // The same `sm` type as an error.
        let lh = view.read_with(cx, |v, _| v.surface().text_style(v.index_of(&w).unwrap()).unwrap().line_height);
        assert_eq!(lh, 20.0, "{w}");
    }
    let left = |cx: &mut VisualTestContext, id: &str| view.read_with(cx, |v, _| v.painted_bounds(id).unwrap().origin.x);
    assert_eq!(left(cx, "wCheckbox.error"), left(cx, "wCheckbox.label"), "a Checkbox's message sits under its label");
    let top = |cx: &mut VisualTestContext, id: &str| view.read_with(cx, |v, _| v.painted_bounds(id).unwrap().origin.y);
    assert_eq!(top(cx, "wCheckbox.box") - top(cx, "wCheckbox.label"), gpui::px(2.0), "the box stays on the first line");
    assert_eq!(border(&view, cx, "wInput.field"), before, "the control keeps its normal border");
    assert!(!node_states(&view, cx, "wInput.field").iter().any(|s| s == "invalid"));
}

#[gpui::test]
fn an_error_is_invalid_and_one_message_shows(cx: &mut TestAppContext) {
    let tree = json!({"id": "f", "component": "Form", "props": {"name": "f"}, "children": [
        {"id": "x", "component": "Input", "props": {"label": "X", "name": "x", "validateOn": "submit", "checks": [{"condition": {"@path": "/w"}}, {"condition": {"@path": "/e"}}, {"condition": {"@path": "/e2"}}]}},
        {"id": "go", "component": "Button", "props": {"label": "Send", "submit": true}}
    ]});
    let mut data = severities();
    data["e2"] = json!({"valid": false, "message": "Second"});
    let (view, cx) = surface(cx, tree, data);
    press(&view, cx, "go");
    assert_eq!(node_states(&view, cx, "x.error"), ["invalid"]);
    assert_eq!(ink(&view, cx, "x.error"), token("destructive"));
    assert_eq!(view.read_with(cx, |v, _| v.accessible_info("x.error")).unwrap().role, Role::Alert);
    assert!(node_states(&view, cx, "x.field").iter().any(|s| s == "invalid"));
    let text = view.read_with(cx, |v, _| v.placed_nodes().iter().find(|n| n.id == "x.error").unwrap().props["text"].clone());
    assert_eq!(text, "Not an email", "one message");
}

#[gpui::test]
fn only_an_error_disables_a_button(cx: &mut TestAppContext) {
    let tree = json!({"id": "root", "component": "Box", "children": [
        {"id": "e", "component": "Button", "props": {"label": "E", "checks": check("/e")}},
        {"id": "w", "component": "Button", "props": {"label": "W", "checks": check("/w")}},
        {"id": "i", "component": "Button", "props": {"label": "I", "checks": check("/i")}}
    ]});
    let (view, cx) = surface(cx, tree, severities());
    let disabled = |cx: &mut VisualTestContext, id: &str| view.read_with(cx, |v, _| v.placed_nodes().iter().find(|n| n.id == id).unwrap().props.get("disabled") == Some(&json!(true)));
    assert!(disabled(cx, "e"));
    assert!(!disabled(cx, "w"));
    assert!(!disabled(cx, "i"));
}

#[gpui::test]
fn live_holds_on_any_component_and_wins_over_a_text_s_own(cx: &mut TestAppContext) {
    let tree = json!({"id": "root", "component": "Box", "children": [
        {"id": "a", "component": "Text", "props": {"text": "a"}, "accessibility": {"live": "polite"}},
        {"id": "b", "component": "Stack", "props": {"gap": "sm"}, "accessibility": {"live": "assertive"}},
        {"id": "c", "component": "Text", "props": {"text": "c", "live": "polite"}, "accessibility": {"live": "assertive"}},
        {"id": "d", "component": "Text", "props": {"text": "d", "live": "polite"}, "accessibility": {"live": "off"}},
        {"id": "e", "component": "Text", "props": {"text": "e", "live": "polite"}}
    ]});
    let (view, cx) = surface(cx, tree, json!({}));
    let read = |cx: &mut VisualTestContext, id: &str| view.read_with(cx, |v, _| v.accessible_info(id)).map(|i| (i.role, i.live.map(|l| l.to_string())));
    assert_eq!(read(cx, "a"), Some((Role::Status, Some("polite".into()))));
    assert_eq!(read(cx, "b"), Some((Role::Alert, Some("assertive".into()))));
    assert_eq!(read(cx, "c"), Some((Role::Alert, Some("assertive".into()))));
    assert_eq!(read(cx, "d"), Some((Role::Label, None)));
    assert_eq!(read(cx, "e"), Some((Role::Status, Some("polite".into()))));
}

#[gpui::test]
fn hidden_is_a_dynamic_boolean_over_the_subtree(cx: &mut TestAppContext) {
    let tree = json!({"id": "root", "component": "Box", "children": [
        {"id": "a", "component": "Text", "props": {"text": "a"}, "accessibility": {"hidden": true}},
        {"id": "b", "component": "Text", "props": {"text": "b"}, "accessibility": {"hidden": {"@path": "/on"}}},
        {"id": "c", "component": "Text", "props": {"text": "c"}, "accessibility": {"hidden": {"@path": "/off"}}},
        {"id": "box", "component": "Box", "accessibility": {"hidden": true}, "children": [
            {"id": "inner", "component": "Button", "props": {"label": "Inner"}}
        ]}
    ]});
    let (view, cx) = surface(cx, tree, json!({"on": "TRUE", "off": false}));
    let info = |cx: &mut VisualTestContext, id: &str| view.read_with(cx, |v, _| v.accessible_info(id));
    assert!(info(cx, "a").is_none());
    assert!(info(cx, "b").is_none(), "the §6.7 coercion");
    assert!(info(cx, "c").is_some());
    assert!(info(cx, "inner").is_none(), "the whole subtree");
}

fn styles(props: Value, chat: bool) -> MdStyles {
    let t = builtin_theme("neutral").unwrap();
    let body = TextSpec { size: 14.0, line_height: 20.0, weight: 400, family: None };
    let props = props.as_object().unwrap().clone();
    let props = eui_gpui::paint::markdown::density_props(&props, chat);
    MdStyles::resolve(Some(&t), Mode::Light, body, &props)
}

/// The fixture's `neutral` styles, token by token, in the block styles the
/// measurer and the painter both lay out with.
#[test]
fn the_inline_styles_are_the_fixture_s() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/fixtures/markdown-inline.json");
    let fixture: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let t = builtin_theme("neutral").unwrap();
    let px = |c: &Value, key: &str| -> f32 {
        let resolved = theme::resolve_recipe(&t, &eui_core::recipes::markdown_inline_recipes(c["part"].as_str().unwrap(), c["variant"].as_str(), c["level"].as_u64().map(|l| l as u8)).pop().unwrap(), Mode::Light);
        eui_gpui::paint::parts::px_prop(&resolved, key).unwrap()
    };
    for chat in [false, true] {
        for c in fixture["cases"].as_array().unwrap() {
            let spec = if let Some(level) = c["level"].as_u64() {
                styles(json!({"profile": "inline"}), chat).heading_levels[level as usize - 1].clone().unwrap()
            } else {
                styles(json!({"profile": "inline", "variant": c["variant"]}), chat).body
            };
            assert_eq!((spec.size, spec.line_height), (px(c, "fontSize"), px(c, "lineHeight")), "{c} chat {chat}");
            let weight = if c["neutral"]["fontWeight"] == "$type.weight.bold" { 700 } else if c["neutral"]["fontWeight"] == "$type.weight.regular" { 400 } else { 600 };
            assert_eq!(spec.weight, weight, "{c}");
        }
    }
    let inline = styles(json!({"profile": "inline"}), true);
    let document = styles(json!({}), false);
    assert_eq!((inline.block_gap, inline.heading_top, inline.heading_bottom), (document.block_gap, document.heading_top, document.heading_bottom), "the document gaps stay");
    assert_eq!(document.heading_levels[0].as_ref().unwrap().size, 18.0, "the document profile keeps Markdown/heading");
}

/// `documentPaint`: inline code = the recipe, lists = the web's column and disc.
#[test]
fn the_document_paints_the_fixture_s_code_and_lists() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/fixtures/markdown-inline.json");
    let fixture: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let paint = &fixture["documentPaint"];
    let t = builtin_theme("neutral").unwrap();
    let token = |key: &str| -> f32 {
        let path = paint["code"]["neutral"][key].as_str().unwrap().trim_start_matches('$').replace('.', "/");
        let tokens = serde_json::to_value(&t.tokens).unwrap();
        tokens.pointer(&format!("/{path}")).unwrap().as_f64().unwrap() as f32
    };
    let s = styles(json!({}), false);
    let code = s.inline_code;
    assert_eq!((code.size, code.pad_h, code.pad_v, code.radius), (Some(token("fontSize")), Some(token("paddingHorizontal")), 0.0, token("borderRadius")));
    let list = &paint["list"];
    let em = |key: &str| list[key].as_f64().unwrap() as f32;
    assert_eq!(s.list_indent, (14.0 * em("indentEm")).round());
    assert_eq!((eui_gpui::paint::markdown::BULLET_EM, eui_gpui::paint::markdown::BULLET_CENTRE_EM), (em("bulletEm"), em("bulletCentreEm")));
    assert_eq!((s.item_top, s.item_bottom), (em("itemGap"), em("itemGap")));
}

struct OwnMarkdown(Rc<Cell<u32>>);
impl HostPlugin for OwnMarkdown {
    fn markdown(&self, _text: &str, _style: &eui_core::measure::TextStyle, _width: f32, _window: &mut Window, _cx: &mut App) -> Option<gpui::AnyElement> {
        self.0.set(self.0.get() + 1);
        Some(gpui::div().into_any_element())
    }
}

#[gpui::test]
fn an_inline_markdown_skips_the_host_s_painter(cx: &mut TestAppContext) {
    let calls = Rc::new(Cell::new(0));
    let tree = json!({"id": "m", "component": "Markdown", "props": {"text": "plain", "profile": "inline"}});
    surface_with(cx, tree, json!({}), Rc::new(OwnMarkdown(calls.clone())));
    assert_eq!(calls.get(), 0);
}

#[gpui::test]
fn a_document_markdown_keeps_the_host_s_painter(cx: &mut TestAppContext) {
    let calls = Rc::new(Cell::new(0));
    let tree = json!({"id": "m", "component": "Markdown", "props": {"text": "plain"}});
    surface_with(cx, tree, json!({}), Rc::new(OwnMarkdown(calls.clone())));
    assert!(calls.get() > 0);
}

#[gpui::test]
fn a_caption_is_as_tall_as_its_text(cx: &mut TestAppContext) {
    let tree = json!({"id": "root", "component": "Box", "children": [
        {"id": "m", "component": "Markdown", "props": {"text": "plain", "profile": "inline", "variant": "caption"}},
        {"id": "t", "component": "Text", "props": {"text": "plain", "variant": "caption"}}
    ]});
    let (view, cx) = surface(cx, tree, json!({}));
    let h = |cx: &mut VisualTestContext, id: &str| view.read_with(cx, |v, _| v.painted_bounds(id).map(|b| f32::from(b.size.height))).unwrap();
    assert_eq!(h(cx, "m"), h(cx, "t"), "same recipe, same measure");
}
