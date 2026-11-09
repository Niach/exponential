//! `fixtures/embed.json` (#EDE-65 lane M) through the painter in a headless
//! gpui window: an Embed paints a nested `SurfaceView` in place (its own
//! data and ids), as tall as its content; `render` = the painted texts in
//! pre-order with the nested surface in place of its Embed; `events` = a
//! press on an inner node reaches the host as the Embed's `action` only,
//! an inner host function is denied, an inner `set` stays inside.

use std::cell::RefCell;
use std::rc::Rc;

use eui_core::NestedNode;
use eui_gpui::host::{ActionEvent, FunctionCallEvent, HostPlugin};
use eui_gpui::view::{SurfaceView, SurfaceViewOptions};
use gpui::{div, prelude::*, px, App, Entity, TestAppContext, VisualTestContext, Window};
use serde_json::{json, Value};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/fixtures");

fn fixture() -> Value {
    serde_json::from_str(&std::fs::read_to_string(format!("{FIXTURES}/embed.json")).expect("fixture")).expect("json")
}

#[derive(Default)]
struct Log {
    actions: Vec<ActionEvent>,
    functions: Vec<FunctionCallEvent>,
}

#[derive(Clone, Default)]
struct Recorder(Rc<RefCell<Log>>);

impl HostPlugin for Recorder {
    fn on_action(&self, event: &ActionEvent, _cx: &mut App) {
        self.0.borrow_mut().actions.push(event.clone());
    }
    fn on_function_call(&self, event: &FunctionCallEvent, _cx: &mut App) {
        self.0.borrow_mut().functions.push(event.clone());
    }
    fn open_url(&self, _url: &str, _cx: &mut App) {}
}

/// The window's root: the surface under test at 400 px.
struct Holder {
    view: Option<Entity<SurfaceView>>,
}

impl Render for Holder {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        div().w(px(400.0)).children(self.view.clone())
    }
}

fn draw(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
    }
}

/// A view over the case's `root` + `data` (neutral theme, light), drawn.
fn mount<'a>(cx: &'a mut TestAppContext, c: &Value) -> (Entity<SurfaceView>, Recorder, &'a mut VisualTestContext) {
    cx.update(gpui_component::init);
    let log = Recorder::default();
    let host = Rc::new(log.clone());
    let options = SurfaceViewOptions { surface_id: "host".into(), theme: eui_core::themes::builtin_theme("neutral"), mode: eui_core::theme::Mode::Light, host, ..Default::default() };
    let (holder, vcx) = cx.add_window_view(|_, _| Holder { view: None });
    let view = vcx.update(|window, cx| cx.new(|cx| SurfaceView::new(options, window, cx)));
    let tree: NestedNode = serde_json::from_value(c["root"].clone()).unwrap();
    view.update(vcx, |v, cx| {
        v.trace_paint(true);
        let out = v.set_nested(tree, cx);
        assert!(out.issues.is_empty(), "{:?}", out.issues);
        for (k, value) in c["data"].as_object().unwrap() {
            v.set_data(&format!("/{k}"), Some(value.clone()), cx).unwrap();
        }
    });
    let v = view.clone();
    holder.update(vcx, |h, cx| {
        h.view = Some(v);
        cx.notify();
    });
    draw(vcx);
    (view, log, vcx)
}

/// The painted Embeds of `view` in paint order: (id, its nested view).
fn painted_embeds(view: &Entity<SurfaceView>, cx: &App) -> Vec<(String, Entity<SurfaceView>)> {
    let v = view.read(cx);
    v.paint_trace()
        .into_iter()
        .filter_map(|i| v.placed_nodes().get(i as usize))
        .filter(|n| n.component == "Embed")
        .map(|n| (n.id.clone(), v.embedded(&n.id).unwrap_or_else(|| panic!("no nested view for {}", n.id))))
        .collect()
}

/// Every painted string (Text `text`, a Button `label`) in paint order,
/// each nested surface in place of its Embed.
fn painted_texts(view: &Entity<SurfaceView>, cx: &App) -> Vec<String> {
    let v = view.read(cx);
    let mut out = Vec::new();
    for i in v.paint_trace() {
        let Some(n) = v.placed_nodes().get(i as usize) else { continue };
        if n.component == "Embed" {
            let child = v.embedded(&n.id).unwrap_or_else(|| panic!("no nested view for {}", n.id));
            out.extend(painted_texts(&child, cx));
            continue;
        }
        for key in ["text", "label"] {
            if let Some(Value::String(s)) = n.props.get(key) {
                out.push(s.clone());
            }
        }
    }
    out
}

fn assert_texts(name: &str, got: &[String], c: &Value) {
    let mut at = 0;
    for want in c["texts"].as_array().unwrap() {
        let want = want.as_str().unwrap();
        match got[at..].iter().position(|t| t == want) {
            Some(p) => at += p + 1,
            None => panic!("{name}: `{want}` not painted (in order) in {got:?}"),
        }
    }
    for absent in c["absent"].as_array().unwrap() {
        assert!(!got.iter().any(|t| t == absent.as_str().unwrap()), "{name}: `{absent}` painted in {got:?}");
    }
}

fn case(key: &str, name: &str) -> Value {
    fixture()[key].as_array().unwrap().iter().find(|c| c["name"] == json!(name)).unwrap_or_else(|| panic!("{key}/{name}")).clone()
}

fn names(key: &str) -> Vec<String> {
    fixture()[key].as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap().to_string()).collect()
}

fn replay_render(cx: &mut TestAppContext, name: &str) {
    let c = case("render", name);
    let (view, _log, cx) = mount(cx, &c);
    let got = cx.read(|cx| painted_texts(&view, cx));
    assert_texts(name, &got, &c);
    // Each Embed is as tall as its nested surface, its id the host's + its own.
    cx.read(|cx| {
        for (id, child) in painted_embeds(&view, cx) {
            let v = view.read(cx);
            let frame = v.frame(v.index_of(&id).unwrap()).unwrap();
            let inner = child.read(cx);
            assert!(inner.surface_height() > 0.0, "{name}: {id} paints nothing");
            assert!((frame.h - inner.surface_height()).abs() < 1.0, "{name}: {id} is {} tall, its surface {}", frame.h, inner.surface_height());
            assert_eq!(inner.surface().id, format!("host/{id}"), "{name}");
        }
    });
}

fn replay_events(cx: &mut TestAppContext, name: &str) {
    let c = case("events", name);
    let (view, log, cx) = mount(cx, &c);
    let k = c["press"]["embed"].as_u64().unwrap() as usize;
    let node = c["press"]["node"].as_str().unwrap().to_string();
    let embeds = cx.read(|cx| painted_embeds(&view, cx));
    let (_, child) = embeds.get(k).unwrap_or_else(|| panic!("{name}: no painted Embed {k}")).clone();
    cx.update(|window, cx| {
        child.update(cx, |v, cx| {
            let i = v.index_of(&node).unwrap_or_else(|| panic!("{name}: no inner {node}"));
            v.press(i, window, cx);
        })
    });
    draw(cx);
    let log = log.0.borrow();
    let actions: Vec<Value> = log.actions.iter().map(|a| json!({"name": a.name, "context": a.context})).collect();
    assert_eq!(Value::Array(actions), c["actions"], "{name}: actions");
    for a in &log.actions {
        assert_eq!(a.surface_id, "host", "{name}: the host's own event");
        assert_eq!(a.event, "action", "{name}");
    }
    let functions: Vec<Value> = log.functions.iter().map(|f| json!({"name": f.name, "args": f.args})).collect();
    assert_eq!(Value::Array(functions), c["functions"], "{name}: functions");
    drop(log);
    let got = cx.read(|cx| painted_texts(&view, cx));
    assert_texts(name, &got, &c);
}

#[gpui::test]
fn the_fixture_lists_the_cases_replayed_here(_cx: &mut TestAppContext) {
    assert_eq!(names("render"), ["in-place", "own-data", "no-host-data", "same-ids-as-host", "bound-surface", "template-rows", "bad-surface-error-box", "unbound-surface-error-box", "nested-embed"]);
    assert_eq!(names("events"), ["reply-bubbles", "which-row", "author-key-loses", "no-on-action", "inner-bound-context", "set-stays-inner", "host-function-denied"]);
}

macro_rules! render_case {
    ($test:ident, $name:literal) => {
        #[gpui::test]
        fn $test(cx: &mut TestAppContext) {
            replay_render(cx, $name);
        }
    };
}

macro_rules! events_case {
    ($test:ident, $name:literal) => {
        #[gpui::test]
        fn $test(cx: &mut TestAppContext) {
            replay_events(cx, $name);
        }
    };
}

render_case!(render_in_place, "in-place");
render_case!(render_own_data, "own-data");
render_case!(render_no_host_data, "no-host-data");
render_case!(render_same_ids_as_host, "same-ids-as-host");
render_case!(render_bound_surface, "bound-surface");
render_case!(render_template_rows, "template-rows");
render_case!(render_bad_surface_error_box, "bad-surface-error-box");
render_case!(render_unbound_surface_error_box, "unbound-surface-error-box");
render_case!(render_nested_embed, "nested-embed");

events_case!(events_reply_bubbles, "reply-bubbles");
events_case!(events_which_row, "which-row");
events_case!(events_author_key_loses, "author-key-loses");
events_case!(events_no_on_action, "no-on-action");
events_case!(events_inner_bound_context, "inner-bound-context");
events_case!(events_set_stays_inner, "set-stays-inner");
events_case!(events_host_function_denied, "host-function-denied");

/// A new `surface` value rebuilds the nested view; an Embed that goes drops it.
#[gpui::test]
fn a_new_value_rebuilds_and_a_gone_embed_drops(cx: &mut TestAppContext) {
    let c = case("render", "bound-surface");
    let (view, _log, cx) = mount(cx, &c);
    let first = view.read_with(cx, |v, _| v.embedded("card")).expect("mounted");
    view.update(cx, |v, cx| v.set_data("/card/root/props/text", Some(json!("Changed")), cx).unwrap());
    draw(cx);
    let second = view.read_with(cx, |v, _| v.embedded("card")).expect("mounted");
    assert_ne!(first.entity_id(), second.entity_id(), "a new value = a new nested surface");
    assert!(cx.read(|cx| painted_texts(&view, cx)).contains(&"Changed".to_string()));
    view.update(cx, |v, cx| {
        v.set_nested(serde_json::from_value(json!({"id": "host", "component": "Stack", "children": [{"id": "t", "component": "Text", "props": {"text": "No card"}}]})).unwrap(), cx);
    });
    draw(cx);
    assert!(view.read_with(cx, |v, _| v.embedded("card")).is_none(), "dropped with its node");
}

/// The nested surface grows in place: the host lays out around it again.
#[gpui::test]
fn the_host_follows_the_nested_height(cx: &mut TestAppContext) {
    let c = json!({"root": {"id": "host", "component": "Stack", "children": [
        {"id": "card", "component": "Embed", "props": {"surface": {"root": {"id": "inner", "component": "Stack", "children": [
            {"id": "more", "component": "Button", "props": {"label": "More"}, "on": {"press": {"functionCall": {"@call": "set", "args": {"path": "/many", "value": true}}}}},
            {"id": "extra", "component": "Text", "visible": {"@path": "/many"}, "props": {"text": "Extra line"}}
        ]}, "data": {"many": false}}}},
        {"id": "after", "component": "Text", "props": {"text": "After"}}
    ]}, "data": {}});
    let (view, _log, cx) = mount(cx, &c);
    let after_y = |view: &Entity<SurfaceView>, cx: &mut VisualTestContext| view.read_with(cx, |v, _| v.frame(v.index_of("after").unwrap()).unwrap().y);
    let before = after_y(&view, cx);
    let child = view.read_with(cx, |v, _| v.embedded("card")).unwrap();
    cx.update(|window, cx| child.update(cx, |v, cx| v.press(v.index_of("more").unwrap(), window, cx)));
    draw(cx);
    let grown = after_y(&view, cx);
    assert!(grown > before + 10.0, "the host's next node moved down: {before} → {grown}");
    assert!(cx.read(|cx| painted_texts(&view, cx)).contains(&"Extra line".to_string()));
}
