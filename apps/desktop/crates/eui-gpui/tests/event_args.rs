//! #EDE-264 / #EDE-270 through the painter: interactions-round1.json
//! `eventArgs` with REAL pointer clicks in a headless gpui window: an
//! action's relative paths read the event's payload first (a Menu
//! `select`'s {value} in a host function's args, a `set`'s value, the
//! event's context), then the scope; a Toggle with only `on.change` takes
//! the click (the eDE Voice tray's Mute) and each press runs its function.

use std::cell::RefCell;
use std::rc::Rc;

use eui_core::NestedNode;
use eui_gpui::host::{ActionEvent, FunctionCallEvent, HostPlugin};
use eui_gpui::view::{SurfaceView, SurfaceViewOptions};
use gpui::{App, Modifiers, TestAppContext, VisualTestContext};
use serde_json::{json, Value};

#[derive(Clone, Default)]
struct Recorder {
    actions: Rc<RefCell<Vec<ActionEvent>>>,
    calls: Rc<RefCell<Vec<FunctionCallEvent>>>,
}

impl HostPlugin for Recorder {
    fn on_action(&self, event: &ActionEvent, _cx: &mut App) {
        self.actions.borrow_mut().push(event.clone());
    }
    fn on_function_call(&self, event: &FunctionCallEvent, _cx: &mut App) {
        self.calls.borrow_mut().push(event.clone());
    }
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
}

#[gpui::test]
fn event_args_read_the_payload_and_a_toggle_takes_the_click(cx: &mut TestAppContext) {
    let f: Value = serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/fixtures/interactions-round1.json")).unwrap()).unwrap();
    let fx = &f["eventArgs"];
    cx.update(gpui_component::init);
    for c in fx["cases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap().to_string();
        let mut tree = fx["surfaces"][c["surface"].as_str().unwrap()].clone();
        tree["children"][0]["on"] = c["on"].clone();
        let log = Recorder::default();
        let host = Rc::new(log.clone());
        let options = SurfaceViewOptions { theme: eui_core::themes::builtin_theme("neutral"), mode: eui_core::theme::Mode::Light, host, ..Default::default() };
        let (view, vcx) = cx.add_window_view(move |window, cx| SurfaceView::new(options, window, cx));
        let data = c["data"].clone();
        view.update(vcx, |v, cx| {
            v.record_bounds(true);
            let out = v.set_nested(serde_json::from_value::<NestedNode>(tree).unwrap(), cx);
            assert!(out.issues.is_empty(), "{:?}", out.issues);
            for (k, val) in data.as_object().unwrap() {
                v.set_data(&format!("/{k}"), Some(val.clone()), cx).unwrap();
            }
        });
        draw(vcx);
        draw(vcx);
        for step in c["steps"].as_array().unwrap() {
            let target = if step == "pick" { "m.item.0" } else { "t" };
            let b = view.read_with(vcx, |v, _| v.painted_bounds(target)).unwrap_or_else(|| panic!("{name}: {target} not painted"));
            vcx.simulate_click(b.center(), Modifiers::default());
            draw(vcx);
            draw(vcx);
        }
        let calls: Vec<Value> = log.calls.borrow().iter().map(|e| json!({"name": e.name, "args": e.args})).collect();
        assert_eq!(Value::Array(calls), c["calls"], "{name}: calls");
        let events: Vec<Value> = log.actions.borrow().iter().map(|e| json!({"name": e.name, "context": e.context})).collect();
        assert_eq!(Value::Array(events), c["events"], "{name}: events");
        for (k, v) in c.get("after").and_then(Value::as_object).into_iter().flatten() {
            let got = view.read_with(vcx, |view, _| view.surface().get_data(&format!("/{k}")).cloned());
            assert_eq!(got.as_ref(), Some(v), "{name}: /{k}");
        }
    }
}
