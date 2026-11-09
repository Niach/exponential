//! #EDE-65 lane O through the painter in a headless gpui window: a host's
//! claimed rows (`HostPlugin::host_leaf_prepaint`) paint at their real
//! height in the frame they first show, and a List kept in place
//! (`SurfaceView::keep_list_place`) settles in the frame it lays out: it
//! stays at its end while it follows (a wheel up stops that, the end re-arms
//! it), and otherwise keeps the reader's row where it was on screen when
//! rows load above it.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use eui_core::NestedNode;
use eui_gpui::host::HostPlugin;
use eui_gpui::view::{SurfaceView, SurfaceViewOptions};
#[allow(deprecated)]
use gpui::ScrollWheelEvent;
use gpui::{div, point, prelude::*, px, App, Entity, ScrollDelta, TestAppContext, VisualTestContext, Window};
use serde_json::{json, Value};

/// The host's rows: the height it answers (40 until it measured one) and
/// each row's real height, measured at prepaint.
#[derive(Clone, Default)]
struct Rows {
    answered: Rc<RefCell<HashMap<String, f32>>>,
}

/// Row `leaf.k<n>`'s real height.
fn real(id: &str) -> f32 {
    let n: u32 = id.rsplit('k').next().and_then(|n| n.parse().ok()).unwrap_or(0);
    24.0 + (n % 5) as f32 * 17.0
}

impl HostPlugin for Rows {
    fn host_leaf_height(&self, id: &str, _width: f32, _: &mut Window, _: &mut App) -> Option<f32> {
        id.starts_with("leaf.").then(|| self.answered.borrow().get(id).copied().unwrap_or(40.0))
    }
    fn host_leaf_prepaint(&self, id: &str, _width: f32, _: &mut Window, _: &mut App) -> bool {
        if !id.starts_with("leaf.") {
            return false;
        }
        self.answered.borrow_mut().insert(id.to_string(), real(id)) != Some(real(id))
    }
    fn host_leaf(&self, id: &str, _: &mut Window, _: &mut App) -> Option<gpui::AnyElement> {
        id.starts_with("leaf.").then(|| div().size_full().into_any_element())
    }
}

struct Pane {
    view: Entity<SurfaceView>,
}

impl Render for Pane {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        div().w(px(400.0)).child(self.view.clone())
    }
}

/// One frame, nothing deferred run after it.
fn frame_once(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

fn settle(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        frame_once(cx);
        cx.run_until_parked();
    }
}

fn rows(keys: impl IntoIterator<Item = u32>) -> Value {
    Value::Array(keys.into_iter().map(|n| json!({"id": format!("k{n}")})).collect())
}

/// A 300 px List of host rows over `/rows`.
fn mount(cx: &mut TestAppContext, data: Value) -> (Entity<SurfaceView>, &mut VisualTestContext) {
    cx.update(gpui_component::init);
    let host = Rc::new(Rows::default());
    let options = SurfaceViewOptions { surface_id: "keep".into(), theme: eui_core::themes::builtin_theme("neutral"), mode: eui_core::theme::Mode::Light, host, ..Default::default() };
    let tree = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column"}, "children": [
        {"id": "feed", "component": "List", "props": {"direction": "vertical", "gap": "none"}, "style": {"height": 300, "overflowY": "auto"}, "template": {"component": "row", "path": "/rows", "key": "id"}},
        {"id": "row", "component": "Box", "style": {"width": "100%"}, "children": [{"id": "leaf", "component": "Embed", "props": {"surface": {"@path": "surface"}}}]}
    ]});
    let (pane, vcx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| SurfaceView::new(options, window, cx));
        view.update(cx, |v, cx| {
            let out = v.set_nested(serde_json::from_value::<NestedNode>(tree).unwrap(), cx);
            assert!(out.issues.is_empty(), "{:?}", out.issues);
            v.record_bounds(true);
            v.set_data("/rows", Some(data), cx).unwrap();
        });
        Pane { view }
    });
    let view = pane.read_with(vcx, |p, _| p.view.clone());
    (view, vcx)
}

fn painted(view: &Entity<SurfaceView>, cx: &mut VisualTestContext, id: &str) -> gpui::Bounds<gpui::Pixels> {
    view.read_with(cx, |v, _| v.painted_bounds(id)).unwrap_or_else(|| panic!("{id} not painted"))
}

/// `(offset, viewport, content)` of the list.
fn extent(view: &Entity<SurfaceView>, cx: &mut VisualTestContext) -> (f32, f32, f32) {
    view.read_with(cx, |v, _| v.scroll_extent("feed")).expect("the list laid out")
}

fn at_end(view: &Entity<SurfaceView>, cx: &mut VisualTestContext) -> bool {
    let (offset, viewport, content) = extent(view, cx);
    (offset - (content - viewport).max(0.0)).abs() < 0.5
}

#[allow(deprecated)]
fn wheel(view: &Entity<SurfaceView>, cx: &mut VisualTestContext, dy: f32) {
    let list = painted(view, cx, "feed");
    let position = list.origin + point(px(20.0), px(150.0));
    cx.simulate_event(ScrollWheelEvent { position, delta: ScrollDelta::Pixels(point(px(0.0), px(dy))), ..Default::default() });
}

/// A claimed row is measured where it is about to paint: its first painted
/// frame already has its real height (never the 40 it answered unmeasured).
#[gpui::test]
fn a_host_row_paints_at_its_real_height_on_its_first_frame(cx: &mut TestAppContext) {
    let (view, cx) = mount(cx, rows(0..4));
    frame_once(cx);
    for n in 0..4 {
        let id = format!("leaf.k{n}");
        assert_eq!(f32::from(painted(&view, cx, &id).size.height), real(&id), "{id} on its first frame");
    }
    // A row arriving later: its first frame too.
    view.update(cx, |v, cx| v.set_data("/rows", Some(rows(0..5)), cx)).unwrap();
    frame_once(cx);
    assert_eq!(f32::from(painted(&view, cx, "leaf.k4").size.height), real("leaf.k4"));
}

/// FOLLOW: while it follows, a new row lands at the bottom in the frame it
/// arrives; a wheel up stops following (a new row leaves the reader where
/// they are); scrolling back to the end follows again.
#[gpui::test]
fn a_following_list_stays_at_its_end_in_the_frame_a_row_arrives(cx: &mut TestAppContext) {
    let (view, cx) = mount(cx, rows(0..30));
    view.update(cx, |v, cx| v.keep_list_place("feed", true, 0, cx));
    settle(cx);
    assert!(at_end(&view, cx), "it opens at its end: {:?}", extent(&view, cx));
    for n in 30..33 {
        view.update(cx, |v, cx| v.set_data("/rows", Some(rows(0..=n)), cx)).unwrap();
        frame_once(cx);
        assert!(at_end(&view, cx), "row {n}: at the end in the frame it arrived: {:?}", extent(&view, cx));
        let (list, last) = (painted(&view, cx, "feed"), painted(&view, cx, &format!("leaf.k{n}")));
        assert!((f32::from(last.bottom()) - f32::from(list.bottom())).abs() < 0.5, "the new row is the bottom one: {last:?} in {list:?}");
        cx.run_until_parked();
    }
    wheel(&view, cx, 120.0);
    settle(cx);
    assert_eq!(view.read_with(cx, |v, _| v.list_follows("feed")), Some(false), "a wheel up stops following");
    let before = extent(&view, cx).0;
    view.update(cx, |v, cx| v.set_data("/rows", Some(rows(0..=33)), cx)).unwrap();
    frame_once(cx);
    assert!((extent(&view, cx).0 - before).abs() < 0.5, "a reader above the end stays put");
    cx.run_until_parked();
    wheel(&view, cx, -100_000.0);
    settle(cx);
    assert_eq!(view.read_with(cx, |v, _| v.list_follows("feed")), Some(true), "back at the end it follows again");
    view.update(cx, |v, cx| v.set_data("/rows", Some(rows(0..=34)), cx)).unwrap();
    frame_once(cx);
    assert!(at_end(&view, cx), "following again: {:?}", extent(&view, cx));
}

/// Where row `id` sits under the list's top now (its laid-out frame less
/// the scroll; `None` = not laid out this pass).
fn screen_y(view: &Entity<SurfaceView>, cx: &mut VisualTestContext, id: &str) -> Option<f32> {
    let offset = extent(view, cx).0;
    view.read_with(cx, |v, _| {
        let index = v.index_of(id)?;
        if v.placed_nodes().get(index as usize).is_none_or(|n| n.removed) {
            return None;
        }
        let list = v.frame(v.index_of("feed")?)?;
        Some(v.frame(index)?.y - list.y - offset)
    })
}

/// PAGING: rows loading ABOVE the reader (their heights only estimated,
/// then measured as they enter the window) leave the row the reader looks
/// at exactly where it was on screen, in the frame they arrive.
#[gpui::test]
fn rows_loading_above_keep_the_readers_row_in_place(cx: &mut TestAppContext) {
    let (view, cx) = mount(cx, rows(20..60));
    view.update(cx, |v, cx| v.keep_list_place("feed", false, 0, cx));
    settle(cx);
    view.update(cx, |v, cx| v.scroll_to_index("feed", 3, Some("start"), cx));
    settle(cx);
    assert!(extent(&view, cx).0 > 0.0, "scrolled into the list");
    // The row at the reader's top, and one further down.
    let reader = (20..60).map(|n| format!("leaf.k{n}")).find(|id| screen_y(&view, cx, id).is_some_and(|y| y >= -0.5)).expect("a row in view");
    let y = screen_y(&view, cx, &reader).expect("in view");
    let below = screen_y(&view, cx, "leaf.k26").expect("in view");
    view.update(cx, |v, cx| v.set_data("/rows", Some(rows(0..60)), cx)).unwrap();
    frame_once(cx);
    let now = screen_y(&view, cx, &reader);
    assert!(now.is_some_and(|now| (now - y).abs() < 0.5), "{reader} stays where it was: {y} → {now:?}");
    let now = screen_y(&view, cx, "leaf.k26");
    assert!(now.is_some_and(|now| (now - below).abs() < 0.5), "and so does what is under it: {below} → {now:?}");
    let painted_y = painted(&view, cx, &reader).origin.y;
    cx.run_until_parked();
    settle(cx);
    let now = screen_y(&view, cx, &reader);
    assert!(now.is_some_and(|now| (now - y).abs() < 0.5), "{reader} never drifts after: {y} → {now:?}");
    assert!((f32::from(painted(&view, cx, &reader).origin.y - painted_y)).abs() < 0.5, "painted where it was");
}
