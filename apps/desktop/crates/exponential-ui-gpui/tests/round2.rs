//! Round 2 (`packages/exponential-ui/docs/round-2-contract.md`) through the
//! painter in a headless gpui window: Resizable (drag from the start sizes,
//! keys, separator a11y), `position: sticky` and pinned section headers,
//! keyframe animations (reduced motion = rest), `scrollToIndex` (own
//! scroller and the host's), horizontal windowing (ltr and rtl), the list
//! a11y (heading level, place in set), the minute tick, the explicit media
//! and picker sizes and the Accordion count.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use exponential_ui::surface::SurfaceSettings;
use exponential_ui::NestedNode;
use exponential_ui_gpui::host::{ActionEvent, HostPlugin};
use exponential_ui_gpui::view::{SurfaceView, SurfaceViewOptions};
use gpui::{div, prelude::*, px, App, Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext, Window};
use serde_json::{json, Value};

#[derive(Clone, Default)]
struct Recorder {
    actions: Rc<RefCell<Vec<ActionEvent>>>,
    scrolled: Rc<RefCell<Vec<(f32, f32)>>>,
}

impl HostPlugin for Recorder {
    fn on_action(&self, event: &ActionEvent, _cx: &mut App) {
        self.actions.borrow_mut().push(event.clone());
    }
    fn scroll_surface(&self, x: f32, y: f32, _cx: &mut App) {
        self.scrolled.borrow_mut().push((x, y));
    }
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
}

fn surface_with(cx: &mut TestAppContext, tree: Value, settings: Option<SurfaceSettings>) -> (Entity<SurfaceView>, Recorder, &mut VisualTestContext) {
    cx.update(gpui_component::init);
    let log = Recorder::default();
    let host = Rc::new(log.clone());
    let options = SurfaceViewOptions { theme: exponential_ui::themes::builtin_theme("neutral"), mode: exponential_ui::theme::Mode::Light, host, settings, ..Default::default() };
    let (pane, vcx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| SurfaceView::new(options, window, cx));
        view.update(cx, |v, cx| {
            let out = v.set_nested(serde_json::from_value::<NestedNode>(tree).unwrap(), cx);
            assert!(out.issues.is_empty(), "{:?}", out.issues);
            v.record_bounds(true);
        });
        Pane { view }
    });
    draw(vcx);
    draw(vcx);
    let view = pane.read_with(vcx, |p, _| p.view.clone());
    (view, log, vcx)
}

fn surface(cx: &mut TestAppContext, tree: Value) -> (Entity<SurfaceView>, Recorder, &mut VisualTestContext) {
    surface_with(cx, tree, None)
}

fn frame(view: &Entity<SurfaceView>, cx: &mut VisualTestContext, id: &str) -> exponential_ui::surface::Frame {
    view.read_with(cx, |v, _| v.frame(v.index_of(id).unwrap_or_else(|| panic!("no {id}"))).unwrap())
}

fn prop(view: &Entity<SurfaceView>, cx: &mut VisualTestContext, id: &str, key: &str) -> Value {
    view.read_with(cx, |v, _| v.placed_nodes().iter().find(|n| n.id == id && !n.removed).and_then(|n| n.props.get(key).cloned()).unwrap_or(Value::Null))
}

fn painted(view: &Entity<SurfaceView>, cx: &mut VisualTestContext, id: &str) -> gpui::Bounds<gpui::Pixels> {
    view.read_with(cx, |v, _| v.painted_bounds(id)).unwrap_or_else(|| panic!("{id} not painted"))
}

fn sizes(view: &Entity<SurfaceView>, cx: &mut VisualTestContext) -> Vec<f64> {
    prop(view, cx, "split", "sizes").as_array().unwrap().iter().map(|v| (v.as_f64().unwrap() * 100.0).round() / 100.0).collect()
}

fn split() -> Value {
    json!({"id": "root", "component": "Box", "style": {"width": 400, "padding": 0}, "children": [
        {"id": "split", "component": "Resizable", "props": {"sizes": [40, 60], "panels": [{"min": 20}, {"min": 30, "collapsible": true}], "handle": true}, "style": {"height": 120},
         "on": {"change": {"event": {"name": "resized", "context": {"sizes": {"path": "sizes"}}}}},
         "children": [
            {"id": "a", "component": "Text", "props": {"text": "Left"}},
            {"id": "b", "component": "Text", "props": {"text": "Right"}}
        ]}
    ]})
}

#[gpui::test]
fn a_resizable_drags_from_its_start_sizes_and_follows_the_keys(cx: &mut TestAppContext) {
    let (view, log, cx) = surface(cx, split());
    let (p0, p1, h) = (frame(&view, cx, "split.panel.0"), frame(&view, cx, "split.panel.1"), frame(&view, cx, "split.handle.0"));
    assert_eq!(h.w, 1.0, "the handle is a hairline in layout");
    assert!((p0.w - 399.0 * 0.4).abs() < 0.6 && (p1.w - 399.0 * 0.6).abs() < 0.6, "panels split the space after the handle: {} / {}", p0.w, p1.w);
    let info = view.read_with(cx, |v, _| v.accessible_info("split.handle.0")).expect("a11y");
    assert_eq!(info.role, gpui::Role::Splitter);
    assert_eq!(info.label.as_deref(), Some("Resize"), "named by $string.resize");

    // The 8 px hit area centred on the hairline hovers and takes a press
    // 3 px beside it.
    let line = painted(&view, cx, "split.handle.0");
    let start = gpui::point(line.center().x + px(3.0), line.center().y);
    cx.simulate_mouse_move(start, None, Modifiers::default());
    draw(cx);
    assert!(view.read_with(cx, |v, _| v.node_states("split.handle.0")).iter().any(|s| s == "hover"), "the hit area shows the handle's hover state");
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(gpui::point(start.x + px(40.0), start.y), Some(MouseButton::Left), Modifiers::default());
    draw(cx);
    cx.simulate_mouse_move(gpui::point(start.x + px(80.0), start.y), Some(MouseButton::Left), Modifiers::default());
    draw(cx);
    let mid = sizes(&view, cx);
    assert!((mid[0] - 60.05).abs() < 0.2, "80 px of 399 from the START sizes (no drift): {mid:?}");
    assert!(log.actions.borrow().iter().all(|a| a.name != "resized"), "change fires at the end only");
    cx.simulate_mouse_up(gpui::point(start.x + px(80.0), start.y), MouseButton::Left, Modifiers::default());
    draw(cx);
    assert!(log.actions.borrow().iter().any(|a| a.name == "resized"), "the drag end fires change");

    // Keys on the focused handle (the press focused it).
    let key = |view: &Entity<SurfaceView>, cx: &mut VisualTestContext, k: &str| {
        let k = k.to_string();
        let handled = cx.update(|window, cx| view.update(cx, |v, cx| v.handle_key(&k, false, None, window, cx)));
        draw(cx);
        handled
    };
    let before = sizes(&view, cx);
    assert!(key(&view, cx, "left"));
    let after = sizes(&view, cx);
    assert!((before[0] - after[0] - 10.0).abs() < 0.01, "ArrowLeft = resizeStep: {before:?} → {after:?}");
    assert!(key(&view, cx, "home"));
    assert_eq!(sizes(&view, cx)[0], 20.0, "Home = the first panel's min");
    assert!(key(&view, cx, "enter"));
    assert_eq!(sizes(&view, cx), vec![100.0, 0.0], "Enter collapses the collapsible panel");
    assert!(view.read_with(cx, |v, _| v.surface().layout_node(v.surface().index_of("split.panel.1").unwrap()).unwrap().hidden), "a collapsed panel is hidden");
}

#[gpui::test]
fn a_resizable_flips_its_drag_and_arrows_in_rtl(cx: &mut TestAppContext) {
    let mut tree = split();
    tree["style"]["direction"] = json!("rtl");
    let (view, _log, cx) = surface(cx, tree);
    assert!(view.read_with(cx, |v, _| v.is_rtl()));
    let (p0, line) = (painted(&view, cx, "split.panel.0"), painted(&view, cx, "split.handle.0"));
    assert!(p0.origin.x > line.origin.x, "the first panel sits right of the handle in rtl");
    let start = line.center();
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(gpui::point(start.x + px(40.0), start.y), Some(MouseButton::Left), Modifiers::default());
    draw(cx);
    cx.simulate_mouse_up(gpui::point(start.x + px(40.0), start.y), MouseButton::Left, Modifiers::default());
    draw(cx);
    let after = sizes(&view, cx);
    assert!((after[0] - (40.0 - 40.0 * 100.0 / 399.0)).abs() < 0.2, "dragging right shrinks the first (right-hand) panel: {after:?}");
    let before = after[0];
    let handled = cx.update(|window, cx| view.update(cx, |v, cx| v.handle_key("left", false, None, window, cx)));
    draw(cx);
    assert!(handled);
    assert!((sizes(&view, cx)[0] - before - 10.0).abs() < 0.01, "ArrowLeft grows it in rtl");
}

#[gpui::test]
fn a_sticky_box_pins_inside_its_scroller(cx: &mut TestAppContext) {
    let rows: Vec<Value> = (0..30).map(|i| json!({"id": format!("r{i}"), "component": "Box", "style": {"height": 40, "flexShrink": 0}, "on": {"press": {"event": {"name": "row", "context": {"i": i}}}}})).collect();
    let mut children = vec![json!({"id": "pin", "component": "Box", "style": {"position": "sticky", "top": 0, "height": 30, "flexShrink": 0, "backgroundColor": "#ffffff"}})];
    children.extend(rows);
    let tree = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column"}, "children": [
        {"id": "pane", "component": "Box", "style": {"height": 200, "overflowY": "auto", "display": "flex", "flexDirection": "column"}, "children": children}
    ]});
    let (view, log, cx) = surface(cx, tree);
    view.update(cx, |v, _| v.trace_paint(true));
    let top = f32::from(painted(&view, cx, "pin").origin.y);
    let pane = painted(&view, cx, "pane");
    cx.simulate_event(gpui::ScrollWheelEvent { position: pane.center(), delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.0), px(-300.0))), ..Default::default() });
    draw(cx);
    draw(cx);
    let pinned = f32::from(painted(&view, cx, "pin").origin.y);
    assert!((pinned - top).abs() < 0.5, "the sticky box stays at the scroller's top: {top} → {pinned}");
    let r10 = f32::from(painted(&view, cx, "r10").origin.y);
    assert!(r10 < top + 200.0, "the rows moved under it: r10 at {r10}, top {top}, offset {:?}", view.read_with(cx, |v, _| v.surface().scroll_offset("pane")));
    // The pinned box keeps its DOM (= AccessKit) place before the rows; it
    // only paints on top.
    let (order, pin, r0) = view.read_with(cx, |v, _| (v.paint_trace(), v.index_of("pin").unwrap(), v.index_of("r0").unwrap()));
    let at = |i: u32| order.iter().position(|o| *o == i).unwrap();
    assert!(at(pin) < at(r0), "the pinned box stays first in element order: {order:?}");
    // A press on the pinned box never reaches the row scrolled under it.
    let pin_box = painted(&view, cx, "pin");
    cx.simulate_click(pin_box.center(), Modifiers::default());
    draw(cx);
    assert!(log.actions.borrow().is_empty(), "the row under the pinned box took the press: {:?}", log.actions.borrow());
    cx.simulate_click(gpui::point(pin_box.center().x, pin_box.bottom() + px(20.0)), Modifiers::default());
    draw(cx);
    assert!(log.actions.borrow().iter().any(|a| a.name == "row"), "a row below the pinned box takes its press");
}

#[gpui::test]
fn a_pinned_section_header_paints_at_the_lists_top_as_a_heading(cx: &mut TestAppContext) {
    let items: Vec<Value> = (0..120).map(|i| json!({"id": format!("i{i}"), "day": format!("d{}", i / 10)})).collect();
    let tree = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column"}, "children": [
        {"id": "l", "component": "List", "props": {"sectionBy": "day", "stickyHeaders": true}, "style": {"height": 400}, "template": {"component": "row", "path": "/items", "key": "id"},
         "slots": {"section": {"id": "hdr", "component": "Text", "props": {"text": {"path": "value"}}, "style": {"height": 24, "backgroundColor": "#ffffff"}}}},
        {"id": "row", "component": "Box", "style": {"height": 40, "flexShrink": 0}}
    ]});
    let (view, _log, cx) = surface(cx, tree);
    view.update(cx, |v, cx| v.set_data("/items", Some(Value::Array(items)), cx));
    draw(cx);
    draw(cx);
    let info = view.read_with(cx, |v, _| v.accessible_info("l.section.0")).expect("a11y");
    assert_eq!(info.role, gpui::Role::Heading, "a section header is a heading");
    assert_eq!(info.level, Some(3), "a section header is a level-3 heading (a11y.json)");
    view.update(cx, |v, _| v.trace_paint(true));
    let list = painted(&view, cx, "l");
    cx.simulate_event(gpui::ScrollWheelEvent { position: list.center(), delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.0), px(-500.0))), ..Default::default() });
    draw(cx);
    draw(cx);
    let pinned = view.read_with(cx, |v, _| v.placed_nodes().iter().filter(|n| !n.removed && n.id.starts_with("l.section.")).filter_map(|n| v.painted_bounds(&n.id).map(|b| (n.id.clone(), f32::from(b.origin.y - list.origin.y)))).collect::<Vec<_>>());
    // Section d1 starts at 424 (24 + 10 × 40): at 500 its header is the pinned one.
    assert!(pinned.iter().any(|(id, y)| id == "l.section.1" && y.abs() < 0.5), "d1's header is pinned at the list's top: {pinned:?}");
    // Pinned, it still precedes its rows in element (= AccessKit) order.
    let (order, header, rows) = view.read_with(cx, |v, _| (v.paint_trace(), v.index_of("l.section.1").unwrap(), (10..20).filter_map(|i| v.index_of(&format!("row.i{i}"))).collect::<Vec<_>>()));
    let at = |i: u32| order.iter().position(|o| *o == i);
    let first_row = rows.iter().filter_map(|r| at(*r)).min().expect("d1's rows are painted");
    assert!(at(header).expect("the header is painted") < first_row, "the pinned header reads before its rows: {order:?}");
}

#[gpui::test]
fn animations_paint_their_frames_and_rest_under_reduced_motion(cx: &mut TestAppContext) {
    let tree = json!({"id": "root", "component": "Box", "style": {"padding": 20}, "children": [
        {"id": "slide", "component": "Box", "style": {"width": 50, "height": 20, "animation": "slide-in-up", "backgroundColor": "#000000"}}
    ]});
    let (view, _log, cx) = surface(cx, tree);
    let rest = frame(&view, cx, "slide");
    let first = painted(&view, cx, "slide");
    let window_origin = view.read_with(cx, |v, _| v.origin());
    let dy0 = f32::from(first.origin.y - window_origin.y) - rest.y;
    assert!(dy0 > 7.0, "slide-in-up starts 8 px below: {dy0}");
    view.update(cx, |v, cx| v.advance_clock(Duration::from_millis(2000), cx));
    draw(cx);
    let done = f32::from(painted(&view, cx, "slide").origin.y - window_origin.y) - rest.y;
    assert!(done.abs() < 0.01, "it ends at rest: {done}");
}

#[gpui::test]
fn reduced_motion_paints_the_rest_frame(cx: &mut TestAppContext) {
    let tree = json!({"id": "root", "component": "Box", "style": {"padding": 20}, "children": [
        {"id": "slide", "component": "Box", "style": {"width": 50, "height": 20, "animation": "slide-in-up", "backgroundColor": "#000000"}}
    ]});
    let reduced = SurfaceSettings { reduced_motion: true, ..SurfaceSettings::default() };
    let (view, _log, cx) = surface_with(cx, tree, Some(reduced));
    let origin = view.read_with(cx, |v, _| v.origin());
    let dy = f32::from(painted(&view, cx, "slide").origin.y - origin.y) - frame(&view, cx, "slide").y;
    assert!(dy.abs() < 0.01, "reduced motion paints the rest frame: {dy}");
}

#[gpui::test]
fn scroll_to_index_moves_the_own_scroller(cx: &mut TestAppContext) {
    let rows: Vec<Value> = (0..200).map(|i| json!({"label": format!("Row {i}")})).collect();
    let tree = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column"}, "children": [
        {"id": "feed", "component": "List", "props": {"direction": "vertical"}, "style": {"height": 200}, "template": {"component": "row", "path": "/rows"}},
        {"id": "row", "component": "Box", "style": {"height": 40}, "children": [{"id": "row-label", "component": "Text", "props": {"text": {"path": "label"}}}]}
    ]});
    let (view, log, cx) = surface(cx, tree);
    view.update(cx, |v, cx| v.set_data("/rows", Some(Value::Array(rows)), cx));
    draw(cx);
    draw(cx);
    let live = |view: &Entity<SurfaceView>, cx: &mut VisualTestContext, id: &str| view.read_with(cx, |v, _| v.index_of(id).is_some_and(|i| v.surface().layout_node(i).is_some()));
    assert!(!live(&view, cx, "row.150"));
    let info = view.read_with(cx, |v, _| v.accessible_info("row.0")).expect("a11y");
    assert_eq!((info.role, info.position), (gpui::Role::ListItem, Some((1, 200))), "a windowed row keeps its place in the whole list");
    view.update(cx, |v, cx| v.scroll_to_index("feed", 150, Some("start"), cx));
    draw(cx);
    draw(cx);
    assert!(live(&view, cx, "row.150"), "the item is rendered");
    let (f, list) = (frame(&view, cx, "row.150"), frame(&view, cx, "feed"));
    let off = view.read_with(cx, |v, _| v.surface().scroll_offset("feed")).1;
    assert!((f.y - off - list.y).abs() < 0.5, "aligned to the start of the list's viewport: item {} offset {off} list {}", f.y, list.y);
    assert!(log.scrolled.borrow().is_empty(), "its own scroller moved, not the host's");
}

#[gpui::test]
fn scroll_to_index_asks_the_host_to_scroll_an_unbounded_list(cx: &mut TestAppContext) {
    let rows: Vec<Value> = (0..300).map(|i| json!({"label": format!("Row {i}")})).collect();
    let tree = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column"}, "children": [
        {"id": "intro", "component": "Box", "style": {"height": 100, "flexShrink": 0}},
        {"id": "feed", "component": "List", "props": {"direction": "vertical"}, "template": {"component": "row", "path": "/rows"}},
        {"id": "row", "component": "Box", "style": {"height": 40, "flexShrink": 0}, "children": [{"id": "row-label", "component": "Text", "props": {"text": {"path": "label"}}}]}
    ]});
    let (view, log, cx) = surface(cx, tree);
    view.update(cx, |v, cx| v.set_data("/rows", Some(Value::Array(rows)), cx));
    draw(cx);
    draw(cx);
    let list = frame(&view, cx, "feed");
    view.update(cx, |v, cx| v.scroll_to_index("feed", 200, Some("start"), cx));
    draw(cx);
    let scrolled = log.scrolled.borrow().clone();
    let &(x, y) = scrolled.last().unwrap_or_else(|| panic!("the host was not asked to scroll: list {list:?}"));
    assert_eq!(x, 0.0);
    assert!((y - (list.y + 200.0 * 40.0)).abs() < 0.5, "the host scrolls row 200 to its top: {y} (list at {})", list.y);
}

#[gpui::test]
fn a_horizontal_list_windows_on_x(cx: &mut TestAppContext) {
    let rows: Vec<Value> = (0..300).map(|i| json!({"label": format!("{i}")})).collect();
    let tree = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column"}, "children": [
        {"id": "strip", "component": "List", "props": {"direction": "horizontal"}, "style": {"width": 400}, "template": {"component": "cell", "path": "/cells"}},
        {"id": "cell", "component": "Box", "style": {"width": 50, "height": 30, "flexShrink": 0}}
    ]});
    let (view, _log, cx) = surface(cx, tree);
    view.update(cx, |v, cx| v.set_data("/cells", Some(Value::Array(rows)), cx));
    draw(cx);
    draw(cx);
    let live = |view: &Entity<SurfaceView>, cx: &mut VisualTestContext, id: &str| view.read_with(cx, |v, _| v.index_of(id).is_some_and(|i| v.surface().layout_node(i).is_some()));
    assert!(live(&view, cx, "cell.0") && !live(&view, cx, "cell.200"), "a window of the cells: {:?}", view.read_with(cx, |v, _| v.surface().layout_node(v.surface().index_of("strip").unwrap()).map(|n| n.children.len())));
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.surface_mut().scroll_to("strip", 10_000.0, 0.0);
            v.layout_now(window, cx);
            v.layout_now(window, cx);
            cx.notify();
        })
    });
    draw(cx);
    draw(cx);
    assert!(live(&view, cx, "cell.200") && !live(&view, cx, "cell.0"), "scrolling on x re-windows");
}

#[gpui::test]
fn a_horizontal_list_windows_from_the_inline_start_in_rtl(cx: &mut TestAppContext) {
    let rows: Vec<Value> = (0..300).map(|i| json!({"label": format!("{i}")})).collect();
    let tree = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "direction": "rtl"}, "children": [
        {"id": "strip", "component": "List", "props": {"direction": "horizontal"}, "style": {"width": 400}, "template": {"component": "cell", "path": "/cells"}},
        {"id": "cell", "component": "Box", "style": {"width": 50, "height": 30, "flexShrink": 0}}
    ]});
    let (view, _log, cx) = surface(cx, tree);
    view.update(cx, |v, cx| v.set_data("/cells", Some(Value::Array(rows)), cx));
    draw(cx);
    draw(cx);
    let live = |view: &Entity<SurfaceView>, cx: &mut VisualTestContext, id: &str| view.read_with(cx, |v, _| v.index_of(id).is_some_and(|i| v.surface().layout_node(i).is_some()));
    assert!(live(&view, cx, "cell.0") && !live(&view, cx, "cell.200"), "a window of the cells from the inline start");
    let (strip, first) = (painted(&view, cx, "strip"), painted(&view, cx, "cell.0"));
    assert!((f32::from(strip.right() - first.right())).abs() < 0.5, "cell 0 sits at the right edge in rtl: strip {strip:?} cell {first:?}");
    cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.surface_mut().scroll_to("strip", 10_000.0, 0.0);
            v.layout_now(window, cx);
            v.layout_now(window, cx);
            cx.notify();
        })
    });
    draw(cx);
    draw(cx);
    assert!(live(&view, cx, "cell.200") && !live(&view, cx, "cell.0"), "scrolling toward the inline end re-windows");
}

#[gpui::test]
fn only_a_surface_showing_a_relative_time_ticks(cx: &mut TestAppContext) {
    let still = json!({"id": "root", "component": "Box", "children": [{"id": "t", "component": "Text", "props": {"text": "Saved"}}]});
    let (view, _log, cx) = surface(cx, still);
    assert!(!view.read_with(cx, |v, _| v.ticks_minutely()), "nothing reads the clock");
    let moving = json!({"id": "root", "component": "Box", "children": [{"id": "t", "component": "Text", "props": {"text": {"call": "formatRelativeTime", "args": {"value": "2026-01-01T00:00:00Z"}}}}]});
    view.update(cx, |v, cx| {
        v.set_nested(serde_json::from_value::<NestedNode>(moving).unwrap(), cx);
    });
    draw(cx);
    assert!(view.read_with(cx, |v, _| v.ticks_minutely()), "a relative time re-binds every minute");
}

struct Pane {
    view: Entity<SurfaceView>,
}

impl Render for Pane {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        div().w(px(390.0)).child(self.view.clone())
    }
}

/// The dump's frames for a tree at 390 px under neutral, gpui's measure.
fn layout(cx: &mut TestAppContext, tree: Value) -> (Entity<SurfaceView>, &mut VisualTestContext) {
    cx.update(gpui_component::init);
    let options = SurfaceViewOptions { theme: exponential_ui::themes::builtin_theme("neutral"), mode: exponential_ui::theme::Mode::Light, ..Default::default() };
    let (pane, vcx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| SurfaceView::new(options, window, cx));
        view.update(cx, |v, cx| {
            let out = v.set_nested(serde_json::from_value::<NestedNode>(tree).unwrap(), cx);
            assert!(out.issues.is_empty(), "{:?}", out.issues);
        });
        Pane { view }
    });
    draw(vcx);
    draw(vcx);
    let view = pane.read_with(vcx, |p, _| p.view.clone());
    (view, vcx)
}

#[gpui::test]
fn explicit_sizes_replace_the_browser_defaults(cx: &mut TestAppContext) {
    let tree = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": 8}, "children": [
        {"id": "video", "component": "Video", "props": {"src": "https://example.com/v.mp4"}},
        {"id": "square", "component": "Image", "props": {"src": "", "alt": "x", "aspectRatio": 1}},
        {"id": "row", "component": "Box", "style": {"display": "flex", "flexDirection": "row"}, "children": [
            {"id": "badge", "component": "Badge", "props": {"text": "3"}},
            {"id": "toggles", "component": "Segmented", "props": {"items": [{"label": "A", "value": "a"}]}}
        ]},
        {"id": "tg", "component": "Segmented", "props": {"items": [{"label": "A", "value": "a"}]}},
        {"id": "radio", "component": "Radio", "props": {"label": "Tone", "name": "tone", "options": [{"label": "A", "value": "a"}], "orientation": "horizontal"}},
        {"id": "slider", "component": "Slider", "props": {"label": "Length", "min": 0, "max": 10, "value": 4}}
    ]});
    let (view, cx) = layout(cx, tree);
    let video = frame(&view, cx, "video");
    assert!((video.h - video.w / 1.7777778).abs() < 0.5, "Video: 16:9 by default: {}×{}", video.w, video.h);
    let sq = frame(&view, cx, "square");
    assert!((sq.h - sq.w).abs() < 0.5, "Image: its aspectRatio");
    let (badge, label) = (frame(&view, cx, "badge"), frame(&view, cx, "badge.label"));
    assert!((badge.w - (label.w + 16.0)).abs() < 0.01, "Badge = 2 × padding + content: {} vs {}", badge.w, label.w);
    let tg = frame(&view, cx, "tg");
    assert!(tg.w < 200.0, "a Segmented is content-sized: {}", tg.w);
    let (radio, items) = (frame(&view, cx, "radio"), frame(&view, cx, "radio.items"));
    assert_eq!(items.y - radio.y, 20.0 + 8.0, "the Radio/root gap ($spacing.sm)");
    let (slider, track) = (frame(&view, cx, "slider"), frame(&view, cx, "slider.track"));
    assert_eq!(slider.h, 20.0 + 4.0 + 16.0, "header + xs + a thumb-tall track row");
    assert_eq!((track.h, track.y - slider.y), (6.0, 24.0 + 5.0), "the 6 px rail centred in the row");
}

#[gpui::test]
fn a_bar_segmented_is_tab_bar_tall_fills_its_row_and_items_share_the_width(cx: &mut TestAppContext) {
    // Round 3: `variant: bar` (the old TabBar) — `$control.tabBar` tall,
    // full width, each item a column (icon over a caption label).
    let tree = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%"}, "children": [
        {"id": "bar", "component": "Segmented", "props": {"variant": "bar", "value": "inbox", "items": [
            {"label": "Inbox", "value": "inbox", "icon": "nav-inbox"},
            {"label": "Issues", "value": "issues", "icon": "nav-issues"},
            {"label": "Settings", "value": "settings", "icon": "nav-settings"}]},
         "on": {"change": {"event": {"name": "go"}}}}
    ]});
    let (view, log, cx) = surface(cx, tree);
    let (root, bar) = (frame(&view, cx, "root"), frame(&view, cx, "bar"));
    assert_eq!(bar.h, 56.0, "$control.tabBar");
    assert_eq!(bar.w, root.w, "a bar always fills");
    let b = painted(&view, cx, "bar");
    // The third item's centre: the items share the width equally.
    let x = b.origin.x + b.size.width * (5.0 / 6.0);
    cx.simulate_click(gpui::point(x, b.center().y), Modifiers::default());
    draw(cx);
    let actions = log.actions.borrow();
    let go = actions.iter().find(|a| a.name == "go").expect("change fired");
    assert_eq!(go.context["value"], json!("settings"));
}

#[gpui::test]
fn an_accordion_count_is_measured_after_the_title(cx: &mut TestAppContext) {
    let tree = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "flex-start"}, "children": [
        {"id": "with", "component": "Accordion", "props": {"items": [{"title": "History", "value": "h", "count": 4}]}, "children": [{"id": "c1", "component": "Text", "props": {"text": "x"}}]},
        {"id": "without", "component": "Accordion", "props": {"items": [{"title": "History", "value": "h"}]}, "children": [{"id": "c2", "component": "Text", "props": {"text": "x"}}]}
    ]});
    let (view, cx) = layout(cx, tree);
    let (a, b) = (frame(&view, cx, "with.trigger.0"), frame(&view, cx, "without.trigger.0"));
    assert!(a.w > b.w + 8.0, "the count takes its own room: {} vs {}", a.w, b.w);
}

fn themed<'a>(cx: &'a mut TestAppContext, tree: Value, theme: &str, mode: exponential_ui::theme::Mode) -> (Entity<SurfaceView>, &'a mut VisualTestContext) {
    cx.update(gpui_component::init);
    let options = SurfaceViewOptions { theme: exponential_ui::themes::builtin_theme(theme), mode, ..Default::default() };
    let (pane, vcx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| SurfaceView::new(options, window, cx));
        view.update(cx, |v, cx| {
            let out = v.set_nested(serde_json::from_value::<NestedNode>(tree).unwrap(), cx);
            assert!(out.issues.is_empty(), "{:?}", out.issues);
            v.record_bounds(true);
        });
        Pane { view }
    });
    draw(vcx);
    draw(vcx);
    let view = pane.read_with(vcx, |p, _| p.view.clone());
    (view, vcx)
}

#[gpui::test]
fn the_current_step_number_paints_in_its_markers_ink_in_every_theme(cx: &mut TestAppContext) {
    // VAPP-100: the number sat in the Text foreground on the primary fill
    // (white on white in dark mode).
    use exponential_ui::theme::Mode;
    let tree = json!({"id": "s", "component": "Stepper", "props": {"steps": [{"label": "Account"}, {"label": "Team"}, {"label": "Done"}], "current": 1}});
    for theme in ["neutral", "exponential", "playful"] {
        for mode in [Mode::Light, Mode::Dark] {
            let (view, vcx) = themed(cx, tree.clone(), theme, mode);
            let colors = |id: &str| view.read_with(vcx, |v, _| v.painted_colors(id)).unwrap_or_else(|| panic!("{id} not painted"));
            let (fill, marker_ink) = colors("s.step.1.marker");
            let (_, number) = colors("s.step.1.marker.number");
            let fill = fill.expect("the current marker is filled");
            assert_eq!(number, marker_ink, "{theme} {mode:?}");
            assert!((number.l - fill.l).abs() > 0.3, "{theme} {mode:?}: ink {number:?} on fill {fill:?}");
            assert!(painted(&view, vcx, "s.step.1.marker.number").size.width > px(0.0));
        }
    }
}

#[gpui::test]
fn an_untitled_code_block_paints_its_language_beside_the_copy_button(cx: &mut TestAppContext) {
    let tree = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": 8}, "children": [
        {"id": "code", "component": "CodeBlock", "props": {"code": "const answer = 42", "language": "ts"}},
        {"id": "chart", "component": "Chart", "props": {"kind": "bar", "title": "This week", "height": 160, "categories": ["Mon"], "series": [{"name": "Runs", "values": [3]}, {"name": "Merges", "values": [1]}]}}
    ]});
    let (view, cx) = themed(cx, tree, "neutral", exponential_ui::theme::Mode::Dark);
    assert_eq!(prop(&view, cx, "code.title", "text"), json!("ts"));
    let (title, copy, header) = (painted(&view, cx, "code.title"), painted(&view, cx, "code.copy"), painted(&view, cx, "code.header"));
    assert!(title.size.width > px(0.0) && copy.size.width > px(0.0));
    assert!(title.origin.x < copy.origin.x, "the label leads, the copy action trails");
    assert!(header.size.height >= copy.size.height);
    assert_eq!(frame(&view, cx, "chart").h, 160.0, "Chart `height` = the whole box");
}
