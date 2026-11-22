//! #EXP-2047: a hosted Popover FOLLOWS its anchor (interactions-round1.json
//! `overlayAnchor`, the hosted cases). A tray icon appearing left of the
//! button the popover hangs off moves the button: the layer stays open in
//! its surface and the shell's popup window opens again against the
//! button's new box (an xdg_popup's positioner is fixed once it maps).

use std::cell::RefCell;
use std::rc::Rc;

use eui_core::surface::Layer;
use eui_gpui::host::HostPlugin;
use gpui::{TestAppContext, VisualTestContext};
use serde_json::json;
use uicore_data::Feed;
use uicore_gpui::{Surfaces, WindowedHost};

#[derive(Clone, Default)]
struct Plugin {
    hosted: Rc<RefCell<Vec<Layer>>>,
}

impl HostPlugin for Plugin {
    fn on_hosted_popups(&self, layers: &[Layer]) {
        *self.hosted.borrow_mut() = layers.to_vec();
    }
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
}

#[gpui::test]
fn a_hosted_popover_follows_its_anchor_when_a_tray_icon_appears(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let factory = uicore_docs::doc("taskbar").expect("the factory taskbar").json();
    let source = json!({
        "surface": factory["surface"],
        "data": {"tray": false},
        "root": {"id": "taskbar", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "width": 800, "height": 48}, "children": [
            {"id": "tray", "component": "Box", "visible": {"@path": "/tray"}, "style": {"width": 24, "height": 24}},
            {"id": "clock", "component": "Button", "props": {"label": "12:00", "variant": "ghost"}},
            {"id": "spaces", "component": "Popover", "props": {"anchor": "clock", "side": "top", "align": "end", "open": true},
             "children": [{"id": "row", "component": "Text", "props": {"text": "Agent space 1"}}]}
        ]}
    });
    let doc = ede_ui::validate_document("taskbar", &source).expect("the document validates");
    let plugin = Plugin::default();
    let for_surface = plugin.clone();
    let mut surfaces = Surfaces::new(Box::new(WindowedHost), eui_core::themes::builtin_theme("exponential").expect("the theme"), Rc::new(move |_| Rc::new(for_surface.clone()) as Rc<dyn HostPlugin>), Rc::new(|_, _| {}), "test");
    assert!(cx.update(|cx| surfaces.open_layer(&doc, &Feed::new(), cx)), "the taskbar opened");
    let region = surfaces.region("taskbar").cloned().expect("the taskbar's region");
    let mut taskbar = VisualTestContext::from_window(region.handle.into(), cx);
    draw(&mut taskbar);
    let spaces = |plugin: &Plugin| plugin.hosted.borrow().iter().find(|l| l.owner == "spaces").cloned();
    let opened = spaces(&plugin).expect("the popover is the host's");
    cx.update(|cx| surfaces.sync_hosted("taskbar", &plugin.hosted.borrow().clone(), cx));
    let first = surfaces.hosted_window("taskbar", "spaces").expect("its popup window");

    // A tray icon appears left of the clock: the clock moves right.
    region.view.update(cx, |v, cx| v.set_data("/tray", Some(json!(true)), cx).expect("the data"));
    draw(&mut taskbar);
    draw(&mut taskbar);
    let moved = spaces(&plugin).expect("the popover stays open when its anchor moves");
    let (was, now) = (opened.anchor_frame.expect("an anchor"), moved.anchor_frame.expect("an anchor"));
    assert!((was.x - now.x).abs() >= 0.5 || (was.w - now.w).abs() >= 0.5 || (was.y - now.y).abs() >= 0.5, "the anchor moved: {was:?} → {now:?}");
    cx.update(|cx| surfaces.sync_hosted("taskbar", &plugin.hosted.borrow().clone(), cx));
    let second = surfaces.hosted_window("taskbar", "spaces").expect("its popup window, against the new box");
    assert_ne!(first, second, "the window opened again where the anchor went");

    // A pass that moves nothing keeps the window.
    draw(&mut taskbar);
    cx.update(|cx| surfaces.sync_hosted("taskbar", &plugin.hosted.borrow().clone(), cx));
    assert_eq!(surfaces.hosted_window("taskbar", "spaces"), Some(second), "nothing moved: the same window");
}
