//! #EDE-263: the keys of a hosted Menu's popup (the FACTORY window menu,
//! Alt+Space's) in headless gpui windows: the arrows move the focus and
//! Enter picks the focused entry, never the first.

use std::cell::RefCell;
use std::rc::Rc;

use eui_core::surface::Layer;
use eui_gpui::host::{ActionEvent, FunctionCallEvent, HostPlugin};
use gpui::{App, TestAppContext, VisualTestContext};
use serde_json::{json, Value};
use uicore_data::Feed;
use uicore_gpui::{Surfaces, WindowedHost};

#[derive(Clone, Default)]
struct Plugin {
    heard: Rc<RefCell<Vec<(String, Value)>>>,
    hosted: Rc<RefCell<Vec<Layer>>>,
}

impl HostPlugin for Plugin {
    fn on_action(&self, event: &ActionEvent, _cx: &mut App) {
        self.heard.borrow_mut().push((event.name.clone(), event.context.clone()));
    }

    fn on_function_call(&self, event: &FunctionCallEvent, _cx: &mut App) {
        self.heard.borrow_mut().push((event.name.clone(), event.args.clone()));
    }

    fn on_hosted_popups(&self, layers: &[Layer]) {
        *self.hosted.borrow_mut() = layers.to_vec();
    }
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
}

/// The picks the shell heard (`ede_window_menu`'s values).
fn picks(plugin: &Plugin) -> Vec<String> {
    plugin.heard.borrow().iter().filter(|(name, _)| name == "ede_window_menu").filter_map(|(_, said)| said.get("value").and_then(Value::as_str).map(String::from)).collect()
}

fn keys_then_picks(keys: &[&str], cx: &mut TestAppContext) -> Vec<String> {
    cx.update(gpui_component::init);
    let source = uicore_docs::doc("window-menu").expect("the factory window menu").json();
    let doc = ede_ui::validate_document("window-menu", &source).expect("the document validates");
    let plugin = Plugin::default();
    let for_surface = plugin.clone();
    let mut surfaces = Surfaces::new(Box::new(WindowedHost), eui_core::themes::builtin_theme("exponential").expect("the theme"), Rc::new(move |_| Rc::new(for_surface.clone()) as Rc<dyn HostPlugin>), Rc::new(|_, _| {}), "test");
    let mut feed = Feed::new();
    let items = json!([
        {"label": "Minimize", "value": "minimize"},
        {"label": "Maximize", "value": "maximize"},
        {"label": "Move", "value": "move"},
        {"kind": "separator"},
        {"label": "Close", "value": "close"}
    ]);
    feed.set("/windowMenu", json!({"items": items, "open": false}));
    assert!(cx.update(|cx| surfaces.open_layer(&doc, &feed, cx)), "the window menu opened");
    let region = surfaces.region("window-menu").cloned().expect("its region");
    let mut surface = VisualTestContext::from_window(region.handle.into(), cx);
    draw(&mut surface);
    // The compositor's `window.menu`: the shell opens it.
    feed.set("/windowMenu", json!({"items": items, "open": true}));
    cx.update(|cx| surfaces.feed_all(&feed, cx));
    draw(&mut surface);
    let layers = plugin.hosted.borrow().clone();
    assert!(layers.iter().any(|l| l.hosted && l.kind == "Menu"), "the menu is the host's: {layers:?}");
    let owner = layers.iter().find(|l| l.kind == "Menu").unwrap().owner.clone();
    cx.update(|cx| surfaces.sync_hosted("window-menu", &layers, cx));
    let window = surfaces.hosted_window("window-menu", &owner).expect("the menu's popup window");
    let mut menu = VisualTestContext::from_window(window, cx);
    draw(&mut menu);
    for key in keys {
        menu.simulate_keystrokes(key);
        draw(&mut menu);
        draw(&mut surface);
    }
    let heard = picks(&plugin);
    drop(surfaces);
    heard
}

/// The menu opens on its first entry; Down moves on (over the separator)
/// and Enter picks the entry the focus is on (the pick closes it: the
/// `dismiss` of its `change`). Before #EDE-263 the surface window's pass
/// took the focus back to the first entry: Enter minimized.
#[gpui::test]
fn enter_picks_the_focused_entry(cx: &mut TestAppContext) {
    assert_eq!(keys_then_picks(&["enter"], cx), ["minimize", "dismiss"]);
    assert_eq!(keys_then_picks(&["down", "enter"], cx), ["maximize", "dismiss"]);
    assert_eq!(keys_then_picks(&["down", "down", "down", "enter"], cx), ["close", "dismiss"]);
    assert_eq!(keys_then_picks(&["down", "down", "up", "enter"], cx), ["maximize", "dismiss"]);
}
