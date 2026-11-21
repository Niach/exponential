//! #EDE-196: the factory toasts under a pointer, in a headless gpui window.
//! An action button calls `ede_notify_action` with its toast's id and its
//! key (it sent the key alone, so no app ever heard a press); the app's
//! `default` action is no button but the card itself (`ede_notify_open`);
//! a button on the card is that button's press alone.

use std::cell::RefCell;
use std::rc::Rc;

use eui_gpui::host::{FunctionCallEvent, HostPlugin};
use gpui::{App, Modifiers, TestAppContext, VisualTestContext};
use serde_json::{json, Value};
use uicore_data::Feed;
use uicore_gpui::{Region, Surfaces, WindowedHost};

/// The shell's half: what the toasts called.
#[derive(Clone, Default)]
struct Plugin {
    heard: Rc<RefCell<Vec<(String, Value)>>>,
}

impl HostPlugin for Plugin {
    fn on_function_call(&self, event: &FunctionCallEvent, _cx: &mut App) {
        self.heard.borrow_mut().push((event.name.clone(), event.args.clone()));
    }
}

impl Plugin {
    fn take(&self) -> Vec<(String, Value)> {
        std::mem::take(&mut *self.heard.borrow_mut())
    }
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
}

/// snapd-desktop-integration's question as the shell feeds it: every
/// action carries its toast's id, the `default` action is no button.
fn feed() -> Feed {
    let mut feed = Feed::new();
    feed.set(
        "/notifications",
        json!({"toasts": [{"id": 7, "app": "snapd-desktop-integration", "summary": "Some required themes are missing.", "body": "Would you like to install them now?", "icon": "",
            "actions": [{"id": 7, "key": "yes", "label": "Yes"}, {"id": 7, "key": "no", "label": "No"}], "default": true, "urgency": 1, "at": "12:00"}], "list": [], "count": 1}),
    );
    feed
}

fn toasts(cx: &mut TestAppContext) -> (Plugin, Surfaces, Region, VisualTestContext) {
    cx.update(gpui_component::init);
    let source = uicore_docs::doc("notifications").expect("the factory toasts").json();
    let mut doc = ede_ui::validate_document("notifications", &source).expect("the document validates");
    // As the shell opens it: the root takes the layer's size.
    if let ede_ui::Body::Nested(root) = &mut doc.body {
        let style = root.style.get_or_insert_with(Default::default);
        style.insert("width".into(), json!(380));
        style.insert("height".into(), json!(420));
    }
    let plugin = Plugin::default();
    let for_surface = plugin.clone();
    let mut surfaces = Surfaces::new(Box::new(WindowedHost), eui_core::themes::builtin_theme("exponential").expect("the theme"), Rc::new(move |_| Rc::new(for_surface.clone()) as Rc<dyn HostPlugin>), Rc::new(|_, _| {}), "test");
    assert!(cx.update(|cx| surfaces.open_layer(&doc, &feed(), cx)), "the toasts opened");
    let region = surfaces.region("notifications").cloned().expect("the toasts' region");
    let mut vcx = VisualTestContext::from_window(region.handle.into(), cx);
    draw(&mut vcx);
    draw(&mut vcx);
    (plugin, surfaces, region, vcx)
}

/// A pointer click on the middle of what `id` painted.
fn click(region: &Region, vcx: &mut VisualTestContext, id: &str) {
    let bounds = region.view.read_with(vcx, |v, _| v.painted_bounds(id)).unwrap_or_else(|| panic!("{id} is drawn"));
    vcx.simulate_click(bounds.center(), Modifiers::none());
    draw(vcx);
}

#[gpui::test]
fn a_press_on_a_toast_names_the_toast(cx: &mut TestAppContext) {
    let (plugin, _surfaces, region, mut vcx) = toasts(cx);
    let drawn = |vcx: &mut VisualTestContext, id: &str| region.view.read_with(vcx, |v, _| v.painted_bounds(id)).is_some();
    assert!(drawn(&mut vcx, "toast_action.7.yes") && drawn(&mut vcx, "toast_action.7.no"), "the app's buttons");
    assert!(!drawn(&mut vcx, "toast_action.7.default"), "the default action is no button");

    click(&region, &mut vcx, "toast_action.7.yes");
    assert_eq!(plugin.take(), [("ede_notify_action".to_string(), json!({"id": 7, "key": "yes"}))], "the action with its toast's id");

    click(&region, &mut vcx, "toast_summary.7");
    assert_eq!(plugin.take(), [("ede_notify_open".to_string(), json!({"id": 7}))], "the card itself is the default action");

    click(&region, &mut vcx, "toast_close.7");
    assert_eq!(plugin.take(), [("ede_notify_close".to_string(), json!({"id": 7, "toast": true}))], "a button on the card is its own press alone");
}
