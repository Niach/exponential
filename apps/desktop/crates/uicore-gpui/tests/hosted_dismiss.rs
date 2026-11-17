//! #EDE-75: the taskbar's hover previews (a hosted Popover whose `change`
//! tells the shell) in headless gpui windows. The compositor's dismissal (a
//! press outside) closes the hosted layer in its surface, which fires the
//! Popover's `change` straight into the host plugin, and that reads the
//! shell: the dismissal runs with the shell free. The `change` says whether
//! the previews opened or closed, so a closed preview stops its thumbnails.

use std::cell::RefCell;
use std::rc::Rc;

use eui_core::surface::Layer;
use eui_gpui::host::{ActionEvent, FunctionCallEvent, HostPlugin};
use gpui::{App, TestAppContext, VisualTestContext};
use serde_json::{json, Value};
use uicore_data::Feed;
use uicore_gpui::{dismiss_hosted, Region, Surfaces, WindowedHost};

/// The shell as eDE holds it: one `RefCell` every host function reads.
type Shell = Rc<RefCell<Option<Surfaces>>>;

#[derive(Clone, Default)]
struct Plugin {
    shell: Shell,
    /// What the shell heard: the function's args or the event's context.
    heard: Rc<RefCell<Vec<(String, Value)>>>,
    hosted: Rc<RefCell<Vec<Layer>>>,
}

impl HostPlugin for Plugin {
    fn on_action(&self, event: &ActionEvent, _cx: &mut App) {
        let _shell = self.shell.borrow();
        self.heard.borrow_mut().push((event.name.clone(), event.context.clone()));
    }

    fn on_function_call(&self, event: &FunctionCallEvent, _cx: &mut App) {
        let _shell = self.shell.borrow();
        self.heard.borrow_mut().push((event.name.clone(), event.args.clone()));
    }

    fn on_hosted_popups(&self, layers: &[Layer]) {
        *self.hosted.borrow_mut() = layers.to_vec();
    }
}

fn node<'a>(n: &'a Value, id: &str) -> Option<&'a Value> {
    if n.get("id").and_then(Value::as_str) == Some(id) {
        return Some(n);
    }
    match n {
        Value::Object(m) => m.values().find_map(|v| node(v, id)),
        Value::Array(a) => a.iter().find_map(|v| node(v, id)),
        _ => None,
    }
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
}

/// A taskbar of one button whose previews Popover carries the FACTORY
/// taskbar's `on.change`, opened (a press here; the hover opens the same
/// layer) and hosted in its own window.
fn previews_open(cx: &mut TestAppContext) -> (Plugin, Region, VisualTestContext) {
    let factory = uicore_docs::doc("taskbar").expect("the factory taskbar").json();
    let on = node(&factory["root"], "previews").expect("the previews Popover")["on"].clone();
    assert!(on["change"].is_object(), "the previews tell the shell: {on}");
    previews_open_with(on, cx)
}

fn previews_open_with(on: Value, cx: &mut TestAppContext) -> (Plugin, Region, VisualTestContext) {
    cx.update(gpui_component::init);
    let factory = uicore_docs::doc("taskbar").expect("the factory taskbar").json();
    let source = json!({
        "surface": factory["surface"],
        "data": {"app": "firefox"},
        "root": {"id": "taskbar", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "width": 800, "height": 48}, "children": [
            {"id": "previews", "component": "Popover", "props": {"side": "top", "align": "center"}, "on": on,
             "slots": {"trigger": {"id": "button", "component": "Button", "props": {"label": "Firefox"}}},
             "children": [{"id": "tile", "component": "Text", "props": {"text": "A window"}}]}
        ]}
    });
    let doc = ede_ui::validate_document("taskbar", &source).expect("the document validates");
    let plugin = Plugin::default();
    let for_surface = plugin.clone();
    let surfaces = Surfaces::new(Box::new(WindowedHost), eui_core::themes::builtin_theme("exponential").expect("the theme"), Rc::new(move |_| Rc::new(for_surface.clone()) as Rc<dyn HostPlugin>), Rc::new(|_, _| {}), "test");
    *plugin.shell.borrow_mut() = Some(surfaces);
    let opened = cx.update(|cx| plugin.shell.borrow_mut().as_mut().unwrap().open_layer(&doc, &Feed::new(), cx));
    assert!(opened, "the taskbar opened");
    let region = plugin.shell.borrow().as_ref().unwrap().region("taskbar").cloned().expect("the taskbar's region");
    let mut vcx = VisualTestContext::from_window(region.handle.into(), cx);
    draw(&mut vcx);
    let button = region.view.read_with(&vcx, |v, _| v.index_of("button").expect("the trigger"));
    region.view.update_in(&mut vcx, |v, window, cx| v.press(button, window, cx));
    draw(&mut vcx);
    let layers = plugin.hosted.borrow().clone();
    assert!(layers.iter().any(|l| l.hosted && l.owner == "previews"), "the previews are the host's: {layers:?}");
    // The main loop's half: the hosted layer gets its window.
    cx.update(|cx| plugin.shell.borrow_mut().as_mut().unwrap().sync_hosted("taskbar", &layers, cx));
    (plugin, region, vcx)
}

fn heard_open(plugin: &Plugin) -> Vec<(String, Option<bool>)> {
    plugin.heard.borrow().iter().map(|(name, said)| (name.clone(), said.get("open").and_then(Value::as_bool))).collect()
}

/// The compositor dismissed the popups while the shell handles the press:
/// the Popover's `change` reaches the host function, which borrows the shell.
#[gpui::test]
fn a_dismissal_reaches_the_host_with_the_shell_free(cx: &mut TestAppContext) {
    let (plugin, _region, mut vcx) = previews_open(cx);
    let before = plugin.heard.borrow().len();
    let layers = plugin.shell.borrow().as_ref().unwrap().hosted_layers();
    assert_eq!(layers.len(), 1, "the previews' window");
    cx.update(|cx| dismiss_hosted(layers, cx));
    draw(&mut vcx);
    assert_eq!(plugin.heard.borrow().len(), before + 1, "the dismissal told the shell: {:?}", plugin.heard.borrow());
    assert!(plugin.hosted.borrow().iter().all(|l| l.owner != "previews"), "the layer closed");
}

/// The shell hears the previews open and close (it stops the thumbnails on
/// the close): `open` reaches it with the factory document's handler.
#[gpui::test]
fn the_previews_change_says_open_or_closed(cx: &mut TestAppContext) {
    let (plugin, region, mut vcx) = previews_open(cx);
    region.view.update_in(&mut vcx, |v, window, cx| v.dismiss_hosted_popup("previews", window, cx));
    draw(&mut vcx);
    assert_eq!(heard_open(&plugin), vec![("ede_previews".to_string(), Some(true)), ("ede_previews".to_string(), Some(false))]);
    assert_eq!(plugin.heard.borrow()[1].1["app"], "firefox", "the close names its app");
}

/// A person's or an agent's copy of the taskbar from before #EDE-75 calls
/// the function: a dismissal is as safe, and its `open` still never arrives
/// (the shell reads such a change as an open, as it always did: those
/// previews go with the next press, not on leave).
#[gpui::test]
fn an_edited_copy_with_the_function_call_dismisses_without_open(cx: &mut TestAppContext) {
    let on = json!({"change": {"functionCall": {"@call": "ede_previews", "args": {"app": {"@path": "app"}, "open": {"@path": "open"}}}}});
    let (plugin, _region, mut vcx) = previews_open_with(on, cx);
    let layers = plugin.shell.borrow().as_ref().unwrap().hosted_layers();
    cx.update(|cx| dismiss_hosted(layers, cx));
    draw(&mut vcx);
    assert_eq!(heard_open(&plugin), vec![("ede_previews".to_string(), None), ("ede_previews".to_string(), None)]);
    assert_eq!(plugin.heard.borrow()[1].1["app"], "firefox");
}
