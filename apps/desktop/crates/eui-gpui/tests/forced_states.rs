//! #EXP-1781: the host's FORCED interaction states on gpui
//! (`SurfaceView::set_forced_states`, fixtures/forced-states.json): the core
//! restyles the forced nodes, a painter-drawn Segmented item takes its own
//! forced state, and nothing else happens (no action, no overlay).

use std::cell::RefCell;
use std::rc::Rc;

use eui_core::NestedNode;
use eui_gpui::host::{ActionEvent, HostPlugin};
use eui_gpui::view::{SurfaceView, SurfaceViewOptions};
use gpui::{div, prelude::*, px, App, Entity, TestAppContext, VisualTestContext, Window};
use serde_json::json;

#[derive(Clone, Default)]
struct Recorder {
    actions: Rc<RefCell<Vec<ActionEvent>>>,
}

impl HostPlugin for Recorder {
    fn on_action(&self, event: &ActionEvent, _cx: &mut App) {
        self.actions.borrow_mut().push(event.clone());
    }
}

struct Pane {
    view: Entity<SurfaceView>,
}

impl Render for Pane {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        div().w(px(480.0)).child(self.view.clone())
    }
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
}

fn bg(view: &Entity<SurfaceView>, cx: &mut VisualTestContext, id: &str) -> Option<String> {
    view.read_with(cx, |v, _| v.surface().visual(v.index_of(id).unwrap()).and_then(|s| s.background_color.clone()))
}

#[gpui::test]
fn forced_states_restyle_paint_only(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let log = Recorder::default();
    let host = Rc::new(log.clone());
    let options = SurfaceViewOptions { theme: eui_core::themes::builtin_theme("exponential"), mode: eui_core::theme::Mode::Light, host, ..Default::default() };
    let tree = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": 8}, "children": [
        {"id": "pad", "component": "Box", "style": {"height": 60}},
        {"id": "btn", "component": "Button", "props": {"label": "Save", "variant": "ghost"}},
        {"id": "seg", "component": "Segmented", "props": {"items": [{"label": "List", "value": "list"}, {"label": "Board", "value": "board"}], "value": "list"}},
        {"id": "tip", "component": "Tooltip", "props": {"content": "Copy link"}, "children": [{"id": "copy", "component": "Button", "props": {"label": "Copy"}}]}
    ]});
    let (pane, vcx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| SurfaceView::new(options, window, cx));
        view.update(cx, |v, cx| {
            let out = v.set_nested(serde_json::from_value::<NestedNode>(tree).unwrap(), cx);
            assert!(out.issues.is_empty(), "{:?}", out.issues);
        });
        Pane { view }
    });
    draw(vcx);
    // The test window's pointer rests at the origin: the pad keeps it off the button.
    let view = pane.read_with(vcx, |p, _| p.view.clone());
    assert_eq!(bg(&view, vcx, "btn").as_deref(), Some("#00000000"));
    let dropped = view.update(vcx, |v, cx| {
        v.set_forced_states([("btn".to_string(), vec!["hover".to_string()]), ("seg.item.1".to_string(), vec!["hover".to_string(), "open".to_string()]), ("tip".to_string(), vec!["hover".to_string()])], cx)
    });
    assert_eq!(dropped, vec!["seg.item.1:open".to_string()]);
    draw(vcx);
    draw(vcx);
    assert_eq!(bg(&view, vcx, "btn").as_deref(), Some("#f5f5f5"), "the ghost hover fill");
    view.read_with(vcx, |v, _| {
        assert_eq!(v.surface().forced_states_of("seg.item.1"), vec!["hover".to_string()], "the painter-drawn item's own forced state");
    });
    assert!(log.actions.borrow().is_empty(), "paint only: no action");
    view.update(vcx, |v, cx| v.set_forced_states(Vec::new(), cx));
    draw(vcx);
    assert_eq!(bg(&view, vcx, "btn").as_deref(), Some("#00000000"), "cleared");
}
