//! Round 7 (`packages/exponential-ui/docs/round-7-contract.md`) through the
//! painter in a headless gpui window. VAPP-281: with
//! `SurfaceSettings::hosted_popups` an anchored Popover / Menu is the HOST's:
//! `HostPlugin::on_hosted_popups` hears the layer (anchor frame, side, align,
//! the unclipped root frame outside the surface), the view paints none of it
//! in the surface, `SurfaceView::hosted_popup_element` paints it at (0, 0) in
//! the host's own window, a click there routes back to the surface (an
//! action), and `dismiss_hosted_popup` closes it.

use std::cell::RefCell;
use std::rc::Rc;

use eui_core::surface::{Layer, SurfaceSettings};
use eui_core::NestedNode;
use eui_gpui::host::{ActionEvent, HostPlugin};
use eui_gpui::view::{SurfaceView, SurfaceViewOptions};
use gpui::{div, prelude::*, App, Entity, Modifiers, TestAppContext, VisualTestContext};
use serde_json::json;

#[derive(Clone, Default)]
struct Recorder {
    actions: Rc<RefCell<Vec<ActionEvent>>>,
    popups: Rc<RefCell<Vec<Vec<Layer>>>>,
}

impl HostPlugin for Recorder {
    fn on_action(&self, event: &ActionEvent, _cx: &mut App) {
        self.actions.borrow_mut().push(event.clone());
    }

    fn on_hosted_popups(&self, layers: &[Layer]) {
        self.popups.borrow_mut().push(layers.to_vec());
    }
}

struct Pane {
    view: Entity<SurfaceView>,
}

impl Render for Pane {
    fn render(&mut self, _window: &mut gpui::Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div().size_full().child(self.view.clone())
    }
}

/// The host's popup window: the hosted layer at (0, 0).
struct Popup {
    view: Entity<SurfaceView>,
    owner: String,
}

impl Render for Popup {
    fn render(&mut self, window: &mut gpui::Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let owner = self.owner.clone();
        let el = self.view.update(cx, |v, cx| v.hosted_popup_element(&owner, window, cx));
        div().size_full().children(el)
    }
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
}

#[gpui::test]
fn hosted_popups_reach_the_host_and_paint_in_its_window(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let log = Recorder::default();
    let host = Rc::new(log.clone());
    let tree = json!({
        "id": "panel", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "justifyContent": "space-between", "alignItems": "center", "width": 800, "height": 44, "paddingHorizontal": 4},
        "children": [
            {"id": "start", "component": "Button", "props": {"label": "Start"}},
            {"id": "clock", "component": "Button", "props": {"label": "12:00"}},
            {"id": "calendar", "component": "Popover", "props": {"anchor": "clock", "side": "top", "align": "end"},
             "children": [{"id": "today", "component": "Button", "props": {"label": "Today"}, "on": {"press": {"event": {"name": "today"}}}}]}
        ]
    });
    let options = SurfaceViewOptions {
        theme: eui_core::themes::builtin_theme("neutral"),
        mode: eui_core::theme::Mode::Light,
        host,
        settings: Some(SurfaceSettings { hosted_popups: true, mode: eui_core::theme::ModeSetting::Light, ..SurfaceSettings::default() }),
        ..Default::default()
    };
    let (pane, vcx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| SurfaceView::new(options, window, cx));
        view.update(cx, |v, cx| {
            let out = v.set_nested(serde_json::from_value::<NestedNode>(tree).unwrap(), cx);
            assert!(out.issues.is_empty(), "{:?}", out.issues);
            v.set_viewport_height(44.0, cx);
            v.record_bounds(true);
        });
        Pane { view }
    });
    draw(vcx);
    let view = pane.read_with(vcx, |p, _| p.view.clone());
    // The clock (the Popover's `anchor`) opens it: hosted, not painted here.
    let clock = view.read_with(vcx, |v, _| v.index_of("clock").unwrap());
    view.update_in(vcx, |v, window, cx| v.press(clock, window, cx));
    draw(vcx);
    let heard = log.popups.borrow().last().cloned().expect("the host heard the popups");
    assert_eq!(heard.len(), 1);
    let layer = &heard[0];
    assert!(layer.hosted);
    assert_eq!(layer.owner, "calendar");
    assert_eq!(layer.placement.map(|p| p.side.as_str()), Some("top"));
    assert_eq!(layer.align.map(|a| a.as_str()), Some("end"));
    let root = layer.frames[0];
    let anchor = layer.anchor_frame.unwrap();
    assert!(root.y < 0.0, "above the 44 px surface: {}", root.y);
    assert!((root.x + root.w - (anchor.x + anchor.w)).abs() < 0.6, "end-aligned");
    let trace = view.read_with(vcx, |v, _| v.paint_trace());
    assert!(!trace.contains(&layer.root), "the view paints no hosted layer");

    // The host's popup window paints it at (0, 0); a click on its button
    // routes back to the surface.
    let h = root.h;
    let popup_view = view.clone();
    let (_popup, pcx) = vcx.add_window_view(move |_, _| Popup { view: popup_view, owner: "calendar".into() });
    draw(pcx);
    let today = view.read_with(pcx, |v, _| v.painted_bounds("today")).expect("the hosted content painted");
    assert!(f32::from(today.origin.y) >= 0.0 && f32::from(today.origin.y) < h, "inside the popup: {today:?}");
    pcx.simulate_click(today.center(), Modifiers::default());
    pcx.run_until_parked();
    assert!(log.actions.borrow().iter().any(|a| a.name == "today"), "the press routed to the surface");

    // The compositor dismissed it: closed, and the host hears none.
    view.update_in(vcx, |v, window, cx| v.dismiss_hosted_popup("calendar", window, cx));
    draw(vcx);
    assert!(view.read_with(vcx, |v, _| v.layers().iter().all(|l| l.owner != "calendar")));
    assert_eq!(log.popups.borrow().last().map(Vec::len), Some(0));
}
