//! #EXP-2045: the taskbar's Agent spaces list (the FACTORY taskbar's
//! press-opened hosted Popover) in headless gpui windows: its own popup
//! window draws all of it, with what the taskbar's surface laid out last,
//! whichever of the two windows draws first. On eDE about one open in four
//! showed nothing (the popup painted its enter at opacity 0 with the
//! taskbar window's clock and was never drawn again) or kept the row's
//! fallback icon (it drew from the caches before the taskbar's pass).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use eui_core::surface::Layer;
use eui_gpui::host::HostPlugin;
use gpui::{TestAppContext, VisualTestContext};
use serde_json::{json, Value};
use uicore_data::Feed;
use uicore_gpui::{Region, Surfaces, WindowedHost};

#[derive(Clone, Default)]
struct Plugin {
    hosted: Rc<RefCell<Vec<Layer>>>,
}

impl HostPlugin for Plugin {
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
    draw_only(cx);
    cx.run_until_parked();
}

/// One frame of this window and nothing else (parked, the test platform
/// draws every dirty window).
fn draw_only(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

fn spaces(picture: &str) -> Value {
    json!({"count": 1, "rows": [{"id": "space-1", "session": "s1", "title": "Agent space 1", "subtitle": null, "picture": picture}]})
}

struct Open {
    /// The shell's surfaces: the windows live with them.
    _surfaces: Surfaces,
    region: Region,
    /// The taskbar's window.
    taskbar: VisualTestContext,
    /// The list's own popup window.
    list: VisualTestContext,
    list_window: gpui::AnyWindowHandle,
}

/// The factory list in a taskbar, opened by a press on its button and
/// given its popup window, as the shell's main loop does.
fn list_open(cx: &mut TestAppContext) -> Open {
    cx.update(gpui_component::init);
    let factory = uicore_docs::doc("taskbar").expect("the factory taskbar").json();
    let list = node(&factory["root"], "agent_spaces").expect("the Agent spaces list").clone();
    let source = json!({
        "surface": factory["surface"],
        "data": {"spaces": spaces(""), "spacesList": {"open": false}},
        "root": {"id": "taskbar", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "width": 800, "height": 48}, "children": [list]}
    });
    let doc = ede_ui::validate_document("taskbar", &source).expect("the document validates");
    let plugin = Plugin::default();
    let for_surface = plugin.clone();
    let mut surfaces = Surfaces::new(Box::new(WindowedHost), eui_core::themes::builtin_theme("exponential").expect("the theme"), Rc::new(move |_| Rc::new(for_surface.clone()) as Rc<dyn HostPlugin>), Rc::new(|_, _| {}), "test");
    assert!(cx.update(|cx| surfaces.open_layer(&doc, &Feed::new(), cx)), "the taskbar opened");
    let region = surfaces.region("taskbar").cloned().expect("the taskbar's region");
    let mut taskbar = VisualTestContext::from_window(region.handle.into(), cx);
    draw(&mut taskbar);
    let button = region.view.read_with(&taskbar, |v, _| v.index_of("agent_spaces_button").expect("the Agent spaces button"));
    region.view.update_in(&mut taskbar, |v, window, cx| v.press(button, window, cx));
    draw(&mut taskbar);
    let layers = plugin.hosted.borrow().clone();
    assert!(layers.iter().any(|l| l.owner == "agent_spaces"), "the list is the host's: {layers:?}");
    cx.update(|cx| surfaces.sync_hosted("taskbar", &layers, cx));
    let window = surfaces.hosted_window("taskbar", "agent_spaces").expect("the list's popup window");
    let list = VisualTestContext::from_window(window, cx);
    Open { _surfaces: surfaces, region, taskbar, list, list_window: window }
}

/// The list's window draws after its enter is over while the taskbar's
/// window has not drawn since the press (nothing in it changed): the list
/// shows all of itself. It read the taskbar window's clock, from the press:
/// its enter at opacity 0, and nothing drew it again.
#[gpui::test]
fn the_list_shows_all_of_itself_without_the_taskbar_drawing(cx: &mut TestAppContext) {
    let Open { _surfaces: _keep, region, taskbar: _taskbar, mut list, .. } = list_open(cx);
    let passes = region.view.read_with(cx, |v, _| v.stats().passes);
    // Time passes; nothing tells the taskbar.
    cx.executor().advance_clock(Duration::from_secs(2));
    draw(&mut list);
    assert_eq!(region.view.read_with(cx, |v, _| v.stats().passes), passes, "the taskbar window did not draw");
    assert_eq!(region.view.read_with(cx, |v, _| v.hosted_popup_shown("agent_spaces")), Some(1.0), "the enter is over in the list's own window");
}

/// While the list enters, its own window draws the next frame by itself
/// (the taskbar window need not draw then): drawn once at the start of the
/// enter, it has drawn the whole list a moment later, and then stops.
#[gpui::test]
fn the_list_draws_its_enter_in_its_own_window(cx: &mut TestAppContext) {
    let Open { _surfaces: _keep, region, taskbar: _taskbar, mut list, .. } = list_open(cx);
    draw_only(&mut list);
    let first = region.view.read_with(cx, |v, _| v.hosted_popup_shown("agent_spaces")).expect("painted");
    assert!(first < 1.0, "the enter runs: {first}");
    for _ in 0..30 {
        cx.executor().advance_clock(Duration::from_millis(16));
        cx.run_until_parked();
    }
    assert_eq!(region.view.read_with(cx, |v, _| v.hosted_popup_shown("agent_spaces")), Some(1.0), "the list's window drew its enter to the end");
    // Nothing moves any more: its window draws no more frames.
    let paints = region.view.read_with(cx, |v, _| v.hosted_popup_paints("agent_spaces"));
    for _ in 0..20 {
        cx.executor().advance_clock(Duration::from_millis(16));
        cx.run_until_parked();
    }
    assert_eq!(region.view.read_with(cx, |v, _| v.hosted_popup_paints("agent_spaces")), paints, "the list's frames stopped with its enter");
}

/// A new picture for the row while the list is open. Both windows woke;
/// the list's drew first, from the caches before the taskbar's pass (the
/// old picture: the fallback icon). The pass that lays the new picture out
/// has it drawn again: the row's Image holds the new picture.
#[gpui::test]
fn a_list_drawn_before_the_taskbars_pass_draws_again_after_it(cx: &mut TestAppContext) {
    let Open { _surfaces: _keep, region, mut taskbar, mut list, list_window: window } = list_open(cx);
    // The enter is over, both windows drew: nothing moves.
    region.view.update(cx, |v, cx| v.advance_clock(Duration::from_secs(2), cx));
    draw(&mut taskbar);
    draw(&mut list);
    let held = eui_gpui::media::held_data_pictures;
    assert_eq!(held(), 0, "no picture yet");
    cx.update(|cx| {
        region.view.update(cx, |v, cx| v.set_data("/spaces", Some(spaces("data:image/png;base64,iVBORw0KGgo=")), cx).expect("the data"));
        window.update(cx, |_, window, cx| window.draw(cx).clear(cx)).expect("the list's window");
        assert_eq!(held(), 0, "the list's window drew from the caches before the taskbar's pass");
    });
    cx.run_until_parked();
    assert_eq!(held(), 1, "after the taskbar's pass the list's window drew the new picture");
}
