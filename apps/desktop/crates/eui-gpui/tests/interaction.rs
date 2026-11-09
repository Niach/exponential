//! Interaction through the painter in a headless gpui window: the keyboard
//! spec of `catalog/a11y.json` (roving tabs and radios, sliders, menus,
//! listboxes, mirrored in RTL), `:focus-visible` from the keyboard only,
//! dismissal rules, hover-opened tooltips, scroll containers, toasts,
//! forms, files, the clipboard, context menus, chips and the first frame's
//! width.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use eui_core::NestedNode;
use eui_gpui::host::{ActionEvent, HostPlugin, UploadEvent};
use eui_gpui::view::{SurfaceView, SurfaceViewOptions};
use gpui::{div, prelude::*, px, App, Entity, Modifiers, MouseButton, ScrollDelta, ScrollEvent, TestAppContext, VisualTestContext, Window};
use serde_json::{json, Value};

#[derive(Default)]
struct Log {
    actions: Vec<ActionEvent>,
    uploads: Vec<UploadEvent>,
    announced: Vec<String>,
}

#[derive(Clone, Default)]
struct Recorder(Rc<RefCell<Log>>);

impl HostPlugin for Recorder {
    fn on_action(&self, event: &ActionEvent, _cx: &mut App) {
        self.0.borrow_mut().actions.push(event.clone());
    }
    fn on_upload(&self, event: &UploadEvent, _cx: &mut App) {
        self.0.borrow_mut().uploads.push(event.clone());
    }
    fn announce(&self, text: &str, _live: &str, _cx: &mut App) {
        self.0.borrow_mut().announced.push(text.to_string());
    }
    fn open_url(&self, _url: &str, _cx: &mut App) {}
}

fn init(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
}

/// A view over `tree` (neutral theme, light), drawn twice.
fn surface(cx: &mut TestAppContext, tree: Value, rtl: bool) -> (Entity<SurfaceView>, Recorder, &mut VisualTestContext) {
    init(cx);
    let log = Recorder::default();
    let host = Rc::new(log.clone());
    let options = SurfaceViewOptions { theme: eui_core::themes::builtin_theme("neutral"), mode: eui_core::theme::Mode::Light, host, ..Default::default() };
    let (view, vcx) = cx.add_window_view(move |window, cx| SurfaceView::new(options, window, cx));
    let mut tree = tree;
    if rtl {
        tree["style"]["direction"] = json!("rtl");
    }
    view.update(vcx, |v, cx| {
        let out = v.set_nested(serde_json::from_value::<NestedNode>(tree).unwrap(), cx);
        assert!(out.issues.is_empty(), "{:?}", out.issues);
    });
    draw(vcx);
    draw(vcx);
    (view, log, vcx)
}

fn key(view: &Entity<SurfaceView>, cx: &mut VisualTestContext, k: &str) -> bool {
    let k = k.to_string();
    let handled = cx.update(|window, cx| view.update(cx, |v, cx| v.handle_key(&k, false, None, window, cx)));
    draw(cx);
    handled
}

fn tab(view: &Entity<SurfaceView>, cx: &mut VisualTestContext) -> Option<String> {
    let at = cx.update(|window, cx| view.update(cx, |v, cx| v.focus_next(false, window, cx)));
    draw(cx);
    view.read_with(cx, |v, _| at.and_then(|i| v.surface().layout_node(i).map(|n| n.id.clone())))
}

fn focused(view: &Entity<SurfaceView>, cx: &mut VisualTestContext) -> Option<String> {
    view.read_with(cx, |v, _| v.focused().and_then(|i| v.surface().layout_node(i).map(|n| n.id.clone())))
}

fn states(view: &Entity<SurfaceView>, cx: &mut VisualTestContext, id: &str) -> Vec<String> {
    view.read_with(cx, |v, _| {
        let i = v.index_of(id).unwrap();
        let mut s: Vec<String> = v.surface().layout_node(i).and_then(|n| n.part_query.as_ref()).map(|q| q.states.clone()).unwrap_or_default();
        s.extend(v.node_states(id));
        s
    })
}

fn hidden(view: &Entity<SurfaceView>, cx: &mut VisualTestContext, id: &str) -> bool {
    view.read_with(cx, |v, _| v.surface().layout_node(v.surface().index_of(id).unwrap()).unwrap().hidden)
}

fn controls() -> Value {
    json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": 12, "padding": 16}, "children": [
        {"id": "tabs", "component": "Tabs", "props": {"tabs": [{"label": "One", "value": "1"}, {"label": "Two", "value": "2"}, {"label": "Three", "value": "3"}], "value": "1"}, "children": [
            {"id": "p1", "component": "Text", "props": {"text": "Panel one"}},
            {"id": "p2", "component": "Text", "props": {"text": "Panel two"}},
            {"id": "p3", "component": "Text", "props": {"text": "Panel three"}}
        ]},
        {"id": "pick", "component": "Radio", "props": {"name": "pick", "options": [{"label": "X", "value": "x"}, {"label": "Y", "value": "y"}, {"label": "Z", "value": "z"}], "value": "x"}},
        {"id": "vol", "component": "Slider", "props": {"label": "Volume", "min": 0, "max": 100, "step": 10, "value": 40}}
    ]})
}

#[gpui::test]
fn tabs_radios_and_sliders_follow_the_keyboard_spec(cx: &mut TestAppContext) {
    let (view, _log, cx) = surface(cx, controls(), false);
    assert_eq!(tab(&view, cx).as_deref(), Some("tabs.tab.0"), "the roving stop is the selected tab");
    assert!(states(&view, cx, "tabs.tab.0").contains(&"focus-visible".to_string()), "keyboard focus is :focus-visible");
    assert!(key(&view, cx, "right"));
    assert_eq!(focused(&view, cx).as_deref(), Some("tabs.tab.1"));
    assert!(!hidden(&view, cx, "p2") && hidden(&view, cx, "p1"), "the arrow activated the tab");
    assert!(key(&view, cx, "end"));
    assert!(!hidden(&view, cx, "p3"));
    assert!(key(&view, cx, "right"), "wraps");
    assert!(!hidden(&view, cx, "p1"));
    // Tab leaves the tab list (one stop) for the panel, then the radio group.
    let next = tab(&view, cx);
    assert_eq!(next.as_deref(), Some("pick.dot.0"), "the checked radio is the group's stop");
    assert!(key(&view, cx, "down"));
    assert_eq!(focused(&view, cx).as_deref(), Some("pick.dot.1"));
    assert!(states(&view, cx, "pick.dot.1").contains(&"checked".to_string()), "arrows select");
    assert_eq!(tab(&view, cx).as_deref(), Some("vol.track"));
    let value = |view: &Entity<SurfaceView>, cx: &mut VisualTestContext| view.read_with(cx, |v, _| v.surface().layout_node(v.index_of("vol.track").unwrap()).unwrap().props.get("value").and_then(Value::as_f64));
    assert!(key(&view, cx, "right"));
    assert_eq!(value(&view, cx), Some(50.0));
    assert!(key(&view, cx, "pagedown"));
    assert_eq!(value(&view, cx), Some(0.0), "PageDown = 10 steps, clamped");
    assert!(key(&view, cx, "end"));
    assert_eq!(value(&view, cx), Some(100.0));
}

#[gpui::test]
fn rtl_mirrors_the_arrow_keys(cx: &mut TestAppContext) {
    let (view, _log, cx) = surface(cx, controls(), true);
    assert!(view.read_with(cx, |v, _| v.is_rtl()));
    assert_eq!(tab(&view, cx).as_deref(), Some("tabs.tab.0"));
    assert!(key(&view, cx, "left"), "Left is forward in RTL");
    assert_eq!(focused(&view, cx).as_deref(), Some("tabs.tab.1"));
    tab(&view, cx);
    tab(&view, cx);
    assert_eq!(focused(&view, cx).as_deref(), Some("vol.track"));
    assert!(key(&view, cx, "left"));
    let v = view.read_with(cx, |v, _| v.surface().layout_node(v.index_of("vol.track").unwrap()).unwrap().props.get("value").and_then(Value::as_f64));
    assert_eq!(v, Some(50.0), "Left raises an RTL slider");
}

#[gpui::test]
fn a_menu_opens_from_the_keyboard_moves_and_selects(cx: &mut TestAppContext) {
    let tree = json!({"id": "root", "component": "Box", "children": [
        {"id": "menu", "component": "Menu", "props": {"label": "More", "items": [{"label": "Rename", "value": "rename"}, {"label": "Archive", "value": "archive"}, {"label": "Delete", "value": "delete"}]}, "on": {"select": {"event": {"name": "menuPick"}}}}
    ]});
    let (view, log, cx) = surface(cx, tree, false);
    let trigger = tab(&view, cx).expect("the trigger takes focus");
    assert!(trigger.starts_with("menu"), "{trigger}");
    assert!(key(&view, cx, "down"), "ArrowDown opens");
    draw(cx);
    assert!(view.read_with(cx, |v, _| v.layers().iter().any(|l| l.owner == "menu")));
    assert_eq!(focused(&view, cx).as_deref(), Some("menu.item.0"), "focus lands on the first item");
    assert!(key(&view, cx, "down"));
    assert_eq!(focused(&view, cx).as_deref(), Some("menu.item.1"));
    assert!(key(&view, cx, "up"));
    assert!(key(&view, cx, "up"), "wraps to the last");
    assert_eq!(focused(&view, cx).as_deref(), Some("menu.item.2"));
    let typed = cx.update(|window, cx| view.update(cx, |v, cx| v.handle_key("a", false, Some("a"), window, cx)));
    draw(cx);
    assert!(typed, "type-ahead");
    assert_eq!(focused(&view, cx).as_deref(), Some("menu.item.1"));
    assert!(key(&view, cx, "enter"));
    draw(cx);
    let picked = log.0.borrow().actions.iter().find(|a| a.name == "menuPick").map(|a| a.context.clone());
    assert_eq!(picked.and_then(|c| c.get("value").cloned()), Some(json!("archive")));
    assert!(!view.read_with(cx, |v, _| v.layers().iter().any(|l| l.owner == "menu")), "selecting closes");
    draw(cx);
    assert_eq!(focused(&view, cx).as_deref(), Some(trigger.as_str()), "focus returns to the trigger");
}

#[gpui::test]
fn a_select_opens_on_its_selection_and_chooses_with_enter(cx: &mut TestAppContext) {
    let tree = json!({"id": "root", "component": "Box", "children": [
        {"id": "board", "component": "Select", "props": {"label": "Board", "name": "board", "options": [{"label": "Backlog", "value": "backlog"}, {"label": "Sprint", "value": "sprint"}, {"label": "Done", "value": "done"}], "value": "sprint"}}
    ]});
    let (view, _log, cx) = surface(cx, tree, false);
    assert_eq!(tab(&view, cx).as_deref(), Some("board.trigger"));
    assert!(key(&view, cx, "enter"));
    draw(cx);
    assert_eq!(focused(&view, cx).as_deref(), Some("board.item.1"), "the selected option takes focus");
    assert!(key(&view, cx, "down"));
    assert!(key(&view, cx, "enter"));
    draw(cx);
    let text = view.read_with(cx, |v, _| v.surface().layout_node(v.index_of("board.trigger").unwrap()).unwrap().props.get("text").cloned());
    assert_eq!(text, Some(json!("Done")));
    assert!(view.read_with(cx, |v, _| v.layers().is_empty()));
}

/// #VAPP-252: a surface narrower than the window with one modal open; where
/// its body painted, and the window's width.
fn modal_in_a_narrow_surface(cx: &mut TestAppContext, modal: Value, lift_layers: bool) -> (f32, f32, f32) {
    init(cx);
    let options = SurfaceViewOptions { theme: eui_core::themes::builtin_theme("neutral"), mode: eui_core::theme::Mode::Light, lift_layers, ..Default::default() };
    let (view, vcx) = cx.add_window_view(move |window, cx| SurfaceView::new(options, window, cx));
    view.update(vcx, |v, cx| {
        v.record_bounds(true);
        v.set_width(400.0, cx);
        v.set_nested(serde_json::from_value::<NestedNode>(json!({"id": "root", "component": "Box", "children": [modal]})).unwrap(), cx);
    });
    draw(vcx);
    draw(vcx);
    let b = view.read_with(vcx, |v, _| v.painted_bounds("body")).expect("the modal painted");
    let window = vcx.update(|window, _| f32::from(window.viewport_size().width));
    (f32::from(b.origin.x), f32::from(b.origin.y), window)
}

#[gpui::test]
fn a_lifted_dialog_centres_in_the_window(cx: &mut TestAppContext) {
    let dialog = json!({"id": "d", "component": "Dialog", "props": {"title": "T", "open": true}, "children": [{"id": "body", "component": "Text", "props": {"text": "x"}}]});
    let (x, _, window) = modal_in_a_narrow_surface(cx, dialog, true);
    assert!(window > 800.0, "the test window is wider than the surface: {window}");
    assert!(x > 400.0 && x < window / 2.0, "centred in the window, not in the 400 px surface: {x} of {window}");
}

#[gpui::test]
fn a_dialog_stays_in_the_surface_by_default(cx: &mut TestAppContext) {
    let dialog = json!({"id": "d", "component": "Dialog", "props": {"title": "T", "open": true}, "children": [{"id": "body", "component": "Text", "props": {"text": "x"}}]});
    let (x, _, _) = modal_in_a_narrow_surface(cx, dialog, false);
    assert!(x < 400.0, "painted inside the surface: {x}");
}

#[gpui::test]
fn a_lifted_side_drawer_hangs_from_the_window_edge(cx: &mut TestAppContext) {
    let drawer = json!({"id": "d", "component": "Drawer", "props": {"title": "T", "open": true, "side": "right"}, "children": [{"id": "body", "component": "Text", "props": {"text": "x"}}]});
    let (x, _, window) = modal_in_a_narrow_surface(cx, drawer, true);
    assert!(x > window - 400.0, "against the window's right edge: {x} of {window}");
}

/// #VAPP-253: where a sheet painted in a 400 px surface, and the window's width.
fn sheet_in_a_narrow_surface(cx: &mut TestAppContext, side: &str, lift_layers: bool) -> (f32, f32, f32) {
    init(cx);
    let options = SurfaceViewOptions { theme: eui_core::themes::builtin_theme("neutral"), mode: eui_core::theme::Mode::Light, lift_layers, ..Default::default() };
    let (view, vcx) = cx.add_window_view(move |window, cx| SurfaceView::new(options, window, cx));
    let tree = json!({"id": "root", "component": "Box", "children": [{"id": "d", "component": "Drawer", "props": {"open": true, "side": side}, "children": [{"id": "body", "component": "Text", "props": {"text": "x"}}]}]});
    view.update(vcx, |v, cx| {
        v.record_bounds(true);
        v.set_width(400.0, cx);
        v.set_nested(serde_json::from_value::<NestedNode>(tree).unwrap(), cx);
    });
    draw(vcx);
    draw(vcx);
    let b = view.read_with(vcx, |v, _| v.painted_bounds("d.content")).expect("the sheet painted");
    let window = vcx.update(|window, _| f32::from(window.viewport_size().width));
    (f32::from(b.origin.x), f32::from(b.size.width), window)
}

#[gpui::test]
fn a_lifted_bottom_sheet_spans_the_window(cx: &mut TestAppContext) {
    let (x, w, window) = sheet_in_a_narrow_surface(cx, "bottom", true);
    assert!(window > 800.0, "the test window is wider than the surface: {window}");
    assert_eq!((x, w), (0.0, window));
}

#[gpui::test]
fn a_lifted_top_sheet_spans_the_window(cx: &mut TestAppContext) {
    let (x, w, window) = sheet_in_a_narrow_surface(cx, "top", true);
    assert_eq!((x, w), (0.0, window));
}

#[gpui::test]
fn a_sheet_keeps_the_surface_width_by_default(cx: &mut TestAppContext) {
    let (x, w, _) = sheet_in_a_narrow_surface(cx, "bottom", false);
    assert_eq!((x, w), (0.0, 400.0));
}

#[gpui::test]
fn escape_and_the_scrim_respect_dismissible(cx: &mut TestAppContext) {
    let tree = json!({"id": "root", "component": "Box", "children": [
        {"id": "keep", "component": "Dialog", "props": {"title": "Stay", "open": true, "dismissible": false}, "children": [{"id": "keep-body", "component": "Text", "props": {"text": "Body"}}]}
    ]});
    let (view, _log, cx) = surface(cx, tree, false);
    assert_eq!(view.read_with(cx, |v, _| v.layers().len()), 1);
    cx.update(|window, cx| view.update(cx, |v, cx| v.escape(window, cx)));
    draw(cx);
    assert_eq!(view.read_with(cx, |v, _| v.layers().len()), 1, "Escape leaves a non-dismissible dialog open");
    // A press on the scrim (top-left corner, outside the centred dialog).
    cx.simulate_mouse_down(gpui::point(px(4.0), px(4.0)), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(gpui::point(px(4.0), px(4.0)), MouseButton::Left, Modifiers::default());
    draw(cx);
    assert_eq!(view.read_with(cx, |v, _| v.layers().len()), 1, "the scrim does not dismiss it either");

    let tree = json!({"id": "root", "component": "Box", "children": [
        {"id": "bye", "component": "Dialog", "props": {"title": "Go", "open": true}, "children": [{"id": "bye-body", "component": "Text", "props": {"text": "Body"}}]}
    ]});
    view.update(cx, |v, cx| {
        v.set_nested(serde_json::from_value::<NestedNode>(tree).unwrap(), cx);
    });
    draw(cx);
    draw(cx);
    assert_eq!(view.read_with(cx, |v, _| v.layers().len()), 1);
    cx.simulate_mouse_down(gpui::point(px(4.0), px(4.0)), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(gpui::point(px(4.0), px(4.0)), MouseButton::Left, Modifiers::default());
    draw(cx);
    assert_eq!(view.read_with(cx, |v, _| v.layers().len()), 0, "a dismissible dialog closes on the scrim");
}

#[gpui::test]
fn a_tooltip_opens_on_keyboard_focus_and_after_the_hover_delay(cx: &mut TestAppContext) {
    let tree = json!({"id": "root", "component": "Box", "style": {"padding": 40}, "children": [
        {"id": "tip", "component": "Tooltip", "props": {"content": "Copy link"}, "children": [{"id": "tip-btn", "component": "Button", "props": {"label": "Copy"}}]}
    ]});
    let (view, _log, cx) = surface(cx, tree, false);
    assert!(view.read_with(cx, |v, _| v.layers().is_empty()));
    // The anchor is the hover target; hovering it opens after the delay.
    view.update(cx, |v, _| v.record_bounds(true));
    draw(cx);
    let b = view.read_with(cx, |v, _| v.painted_bounds("tip-btn")).expect("painted");
    cx.simulate_mouse_move(b.center(), None, Modifiers::default());
    draw(cx);
    assert!(view.read_with(cx, |v, _| v.layers().is_empty()), "not before the delay");
    cx.executor().advance_clock(Duration::from_millis(400));
    cx.run_until_parked();
    draw(cx);
    assert!(view.read_with(cx, |v, _| v.layers().iter().any(|l| l.kind == "Tooltip")), "hover opened the tooltip");
    cx.simulate_mouse_move(gpui::point(px(900.0), px(700.0)), None, Modifiers::default());
    cx.executor().advance_clock(Duration::from_millis(400));
    cx.run_until_parked();
    draw(cx);
    assert!(view.read_with(cx, |v, _| v.layers().is_empty()), "leaving closed it");
    // Keyboard focus opens it at once.
    tab(&view, cx);
    draw(cx);
    assert!(view.read_with(cx, |v, _| v.layers().iter().any(|l| l.kind == "Tooltip")), "keyboard focus opened it");
}

/// #VAPP-277: on the Sidebar rail each Row is ONE tab stop (its tooltip
/// anchor adds none) and the Row's keyboard focus opens its tooltip, on
/// the content's side; hovering the tile opens it too.
#[gpui::test]
fn a_rail_row_is_one_tab_stop_whose_focus_opens_its_tooltip(cx: &mut TestAppContext) {
    let tree = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "height": 300}, "children": [
        {"id": "shell", "component": "Sidebar", "props": {"rail": true, "collapsed": true, "collapseBelow": "never"},
         "slots": {"sidebar": {"id": "nav", "component": "Stack", "props": {"gap": "xs"}, "children": [
            {"id": "inbox", "component": "Row", "props": {"title": "Inbox", "icon": "nav-inbox", "meta": "3", "pressable": true}},
            {"id": "issues", "component": "Row", "props": {"title": "Issues", "icon": "nav-issues", "pressable": true}}
         ]}},
         "children": [{"id": "main", "component": "Text", "props": {"text": "Pick a board."}}]}
    ]});
    let (view, _log, cx) = surface(cx, tree, false);
    assert_eq!(tab(&view, cx).as_deref(), Some("inbox"));
    draw(cx);
    let open = |view: &Entity<SurfaceView>, cx: &mut VisualTestContext| view.read_with(cx, |v, _| v.layers().iter().filter(|l| l.kind == "Tooltip").map(|l| (l.owner.clone(), l.position.clone())).collect::<Vec<_>>());
    assert_eq!(open(&view, cx), [("inbox.tip".to_string(), "right".to_string())], "the Row's keyboard focus opens its tooltip, into the content");
    assert_eq!(tab(&view, cx).as_deref(), Some("issues"), "the anchor is no stop of its own");
    draw(cx);
    assert_eq!(open(&view, cx), [("issues.tip".to_string(), "right".to_string())], "focus moved: the first closed, the second opened");
}

#[gpui::test]
fn hovering_a_rail_tile_opens_its_tooltip(cx: &mut TestAppContext) {
    let (view, _log, cx) = surface(cx, json!({"id": "root", "component": "Box", "style": {"height": 200}, "children": [
        {"id": "shell", "component": "Sidebar", "props": {"rail": true, "collapsed": true, "collapseBelow": "never"},
         "slots": {"sidebar": {"id": "inbox", "component": "Row", "props": {"title": "Inbox", "icon": "nav-inbox", "pressable": true}}}, "children": []}
    ]}), false);
    view.update(cx, |v, _| v.record_bounds(true));
    draw(cx);
    let b = view.read_with(cx, |v, _| v.painted_bounds("inbox.tip.tile")).expect("painted");
    cx.simulate_mouse_move(b.center(), None, Modifiers::default());
    draw(cx);
    cx.executor().advance_clock(Duration::from_millis(400));
    cx.run_until_parked();
    draw(cx);
    assert!(view.read_with(cx, |v, _| v.layers().iter().any(|l| l.kind == "Tooltip" && l.owner == "inbox.tip")), "hovering the tile opened the tooltip");
}

#[gpui::test]
fn scroll_containers_scroll_their_content_and_draw_it_translated(cx: &mut TestAppContext) {
    let rows: Vec<Value> = (0..12).map(|i| json!({"id": format!("row{i}"), "component": "Box", "style": {"height": 40, "flexShrink": 0}, "children": [{"id": format!("label{i}"), "component": "Text", "props": {"text": format!("Row {i}")}}]})).collect();
    let tree = json!({"id": "root", "component": "Box", "style": {"padding": 20}, "children": [
        {"id": "pane", "component": "Box", "style": {"height": 120, "overflowY": "auto", "display": "flex", "flexDirection": "column"}, "children": rows}
    ]});
    let (view, _log, cx) = surface(cx, tree, false);
    view.update(cx, |v, _| v.record_bounds(true));
    draw(cx);
    let y = |view: &Entity<SurfaceView>, cx: &mut VisualTestContext, id: &str| view.read_with(cx, |v, _| f32::from(v.painted_bounds(id).unwrap().origin.y - v.origin().y));
    let before = y(&view, cx, "row3");
    assert_eq!(before, 20.0 + 120.0);
    let pane = view.read_with(cx, |v, _| v.painted_bounds("pane").unwrap());
    cx.simulate_event(ScrollEvent { position: pane.center(), delta: ScrollDelta::Pixels(gpui::point(px(0.0), px(-50.0))), ..Default::default() });
    draw(cx);
    assert_eq!(view.read_with(cx, |v, _| v.surface().scroll_offset("pane")), (0.0, 50.0), "the wheel moved the core's offset");
    assert_eq!(y(&view, cx, "row3"), before - 50.0, "the content is drawn translated");
    for _ in 0..20 {
        cx.simulate_event(ScrollEvent { position: pane.center(), delta: ScrollDelta::Pixels(gpui::point(px(0.0), px(-100.0))), ..Default::default() });
    }
    draw(cx);
    assert_eq!(view.read_with(cx, |v, _| v.surface().scroll_offset("pane").1), 12.0 * 40.0 - 120.0, "clamped at the end");
}

#[gpui::test]
fn toasts_time_out_unless_hovered(cx: &mut TestAppContext) {
    let tree = json!({"id": "root", "component": "Box", "children": [
        {"id": "saved", "component": "Toast", "props": {"title": "Saved", "duration": 1000, "open": true}, "on": {"dismiss": {"event": {"name": "toastGone"}}}}
    ]});
    let (view, log, cx) = surface(cx, tree, false);
    assert!(view.read_with(cx, |v, _| v.layers().iter().any(|l| l.owner == "saved")));
    cx.executor().advance_clock(Duration::from_millis(600));
    cx.run_until_parked();
    draw(cx);
    assert!(view.read_with(cx, |v, _| v.layers().iter().any(|l| l.owner == "saved")), "still up at 600 ms");
    cx.executor().advance_clock(Duration::from_millis(600));
    cx.run_until_parked();
    draw(cx);
    assert!(view.read_with(cx, |v, _| v.layers().is_empty()), "gone after its duration");
    assert!(log.0.borrow().actions.iter().any(|a| a.name == "toastGone"));
}

#[gpui::test]
fn a_failed_submit_announces_and_focuses_the_first_invalid_field(cx: &mut TestAppContext) {
    let tree = json!({"id": "root", "component": "Box", "children": [
        {"id": "f", "component": "Form", "props": {"name": "f"}, "on": {"invalid": {"event": {"name": "bad"}}, "submit": {"event": {"name": "ok"}}}, "children": [
            {"id": "agree", "component": "Checkbox", "props": {"label": "Agree", "name": "agree", "checks": [{"condition": {"@call": "required", "args": {"value": {"@path": "/agree"}}}, "message": "Agree first"}]}},
            {"id": "go", "component": "Button", "props": {"label": "Send", "submit": true}}
        ]}
    ]});
    let (view, log, cx) = surface(cx, tree, false);
    cx.update(|window, cx| view.update(cx, |v, cx| {
        let i = v.index_of("go").unwrap();
        v.press(i, window, cx);
    }));
    draw(cx);
    draw(cx);
    assert!(log.0.borrow().actions.iter().any(|a| a.name == "bad"), "invalid fired");
    assert!(!log.0.borrow().announced.is_empty(), "the failure was announced");
    assert!(view.read_with(cx, |v, _| v.announcement().is_some()));
    assert_eq!(focused(&view, cx).as_deref(), Some("agree"), "focus moved to the first invalid field");
}

/// VAPP-208: a press right after typing, inside the field's debounce, runs on
/// what was typed: the Form's check passes and its submit reaches the host.
#[gpui::test]
fn a_press_right_after_typing_submits_what_was_typed(cx: &mut TestAppContext) {
    let tree = json!({"id": "root", "component": "Box", "children": [
        {"id": "f", "component": "Form", "props": {"name": "f"}, "on": {"invalid": {"event": {"name": "bad"}}, "submit": {"event": {"name": "ok"}}}, "children": [
            {"id": "hours", "component": "NumberField", "props": {"label": "Hours", "name": "hours", "value": {"@path": "/hours"}, "checks": [{"condition": {"@call": "lt", "args": {"a": 0, "b": {"@path": "/hours"}}}, "message": "More than 0"}]}},
            {"id": "go", "component": "Button", "props": {"label": "Save", "submit": true}}
        ]}
    ]});
    let (view, log, cx) = surface(cx, tree, false);
    view.update(cx, |v, cx| v.set_data("/hours", Some(json!(0)), cx).unwrap());
    draw(cx);
    cx.update(|window, cx| view.update(cx, |v, cx| assert!(v.type_into("hours.input", "1.5", window, cx))));
    // The edit is in; its debounce has not fired (the clock never advances here).
    cx.update(|window, cx| view.update(cx, |v, cx| {
        let i = v.index_of("go").unwrap();
        v.press(i, window, cx);
    }));
    draw(cx);
    assert!(log.0.borrow().actions.iter().any(|a| a.name == "ok"), "the first press submitted");
    assert!(!log.0.borrow().actions.iter().any(|a| a.name == "bad"), "the check read what was typed");
}

/// VAPP-208: focusing a seeded NumberField selects its text, so a keystroke
/// replaces it: `0.00` + `1.5` = `1.5`, never `0.001.5`; the first Save passes.
#[gpui::test]
fn a_focused_number_field_selects_its_seed_and_typing_replaces_it(cx: &mut TestAppContext) {
    let tree = json!({"id": "root", "component": "Box", "children": [
        {"id": "f", "component": "Form", "props": {"name": "f"}, "on": {"invalid": {"event": {"name": "bad"}}, "submit": {"event": {"name": "ok"}}}, "children": [
            {"id": "hours", "component": "NumberField", "props": {"label": "Hours", "name": "hours", "step": 0.25, "value": {"@path": "/hours"}, "checks": [{"condition": {"@call": "lt", "args": {"a": 0, "b": {"@path": "/hours"}}}, "message": "More than 0"}]}},
            {"id": "note", "component": "Input", "props": {"label": "Note", "name": "note", "value": "seeded"}},
            {"id": "go", "component": "Button", "props": {"label": "Save", "submit": true}}
        ]}
    ]});
    let (view, log, cx) = surface(cx, tree, false);
    view.update(cx, |v, cx| v.set_data("/hours", Some(json!(0)), cx).unwrap());
    draw(cx);
    assert_eq!(view.read_with(cx, |v, cx| v.field_text("hours.input", cx)).as_deref(), Some("0.00"));
    cx.update(|window, cx| view.update(cx, |v, cx| {
        v.field_gained_focus("hours.input", window, cx);
        assert!(v.type_over_selection("hours.input", "1.5", window, cx));
    }));
    assert_eq!(view.read_with(cx, |v, cx| v.field_text("hours.input", cx)).as_deref(), Some("1.5"));
    cx.update(|window, cx| view.update(cx, |v, cx| {
        let i = v.index_of("go").unwrap();
        v.press(i, window, cx);
    }));
    draw(cx);
    assert!(log.0.borrow().actions.iter().any(|a| a.name == "ok"), "the first press submitted");
    assert!(!log.0.borrow().actions.iter().any(|a| a.name == "bad"), "the check read what was typed");
    // Only a NumberField: an Input's focus keeps its caret.
    cx.update(|window, cx| view.update(cx, |v, cx| {
        v.field_gained_focus("note.field", window, cx);
        assert!(v.type_over_selection("note.field", "!", window, cx));
    }));
    let note = view.read_with(cx, |v, cx| v.field_text("note.field", cx)).unwrap();
    assert!(note.contains("seeded"), "an Input keeps its text: {note}");
}

/// eDE's lock screen showed the typed password: an `Input{type: password}`
/// draws bullets and still hands the real text to its binding.
#[gpui::test]
fn a_password_input_is_masked(cx: &mut TestAppContext) {
    let tree = json!({"id": "root", "component": "Box", "children": [
        {"id": "password", "component": "Input", "props": {"label": "Password", "name": "password", "type": "password", "value": {"@path": "/password"}}},
        {"id": "user", "component": "Input", "props": {"label": "User", "name": "user", "value": "danny"}}
    ]});
    let (view, _log, cx) = surface(cx, tree, false);
    view.update(cx, |v, cx| v.set_data("/password", Some(json!("hunter2")), cx).unwrap());
    draw(cx);
    let id = "password.field";
    assert_eq!(view.read_with(cx, |v, cx| v.field_masked(id, cx)), Some(true), "a password is drawn as bullets");
    assert_eq!(view.read_with(cx, |v, cx| v.field_text(id, cx)).as_deref(), Some("hunter2"), "the binding keeps the real text");
    assert_eq!(view.read_with(cx, |v, cx| v.field_masked("user.field", cx)), Some(false), "a plain field is not");
}

#[gpui::test]
fn picked_files_reach_the_host_and_the_upload_event(cx: &mut TestAppContext) {
    let tree = json!({"id": "root", "component": "Box", "children": [
        {"id": "files", "component": "FileUpload", "props": {"label": "Files", "name": "files", "multiple": true}, "on": {"upload": {"event": {"name": "up"}}}}
    ]});
    let (view, log, cx) = surface(cx, tree, false);
    let dir = std::env::temp_dir().join(format!("xui-upload-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("shot.png");
    std::fs::write(&file, [1u8, 2, 3, 4]).unwrap();
    view.update(cx, |v, cx| v.files_picked("files", vec![file.clone()], cx));
    draw(cx);
    let log = log.0.borrow();
    assert_eq!(log.uploads.len(), 1);
    assert_eq!(log.uploads[0].paths, vec![file]);
    let up = log.actions.iter().find(|a| a.name == "up").expect("upload event");
    assert_eq!(up.context["files"][0], json!({"name": "shot.png", "size": 4, "type": "image/png"}));
    drop(log);
    let shows = view.read_with(cx, |v, _| v.index_of("files.file.0").is_some());
    // The picked file only had to exist for the pick: never leak the dir.
    let _ = std::fs::remove_dir_all(&dir);
    assert!(shows, "the file row shows");
}

#[gpui::test]
fn copy_writes_the_clipboard_and_resets(cx: &mut TestAppContext) {
    let tree = json!({"id": "root", "component": "Box", "children": [
        {"id": "code", "component": "CodeBlock", "props": {"code": "let x = 1;", "language": "rust"}}
    ]});
    let (view, _log, cx) = surface(cx, tree, false);
    cx.update(|window, cx| view.update(cx, |v, cx| {
        let i = v.index_of("code.copy").unwrap();
        v.press(i, window, cx);
    }));
    draw(cx);
    assert_eq!(cx.read_from_clipboard().and_then(|c| c.text()), Some("let x = 1;".to_string()));
    let copied = |view: &Entity<SurfaceView>, cx: &mut VisualTestContext| view.read_with(cx, |v, _| v.surface().layout_node(v.index_of("code.copy").unwrap()).unwrap().props.get("copied").cloned());
    assert_eq!(copied(&view, cx), Some(json!(true)));
    cx.executor().advance_clock(Duration::from_millis(2100));
    cx.run_until_parked();
    draw(cx);
    assert_eq!(copied(&view, cx), Some(json!(false)), "the copied state resets");
}

#[gpui::test]
fn copy_takes_the_copy_parts_text_without_ansi_escapes(cx: &mut TestAppContext) {
    let tree = json!({"id": "root", "component": "Box", "children": [
        {"id": "log", "component": "CodeBlock", "props": {"code": "\u{1b}[31merror\u{1b}[0m: boom", "language": "ansi"}}
    ]});
    let (view, _log, cx) = surface(cx, tree, false);
    cx.update(|window, cx| view.update(cx, |v, cx| {
        let i = v.index_of("log.copy").unwrap();
        v.press(i, window, cx);
    }));
    draw(cx);
    assert_eq!(cx.read_from_clipboard().and_then(|c| c.text()), Some("error: boom".to_string()));
}

#[gpui::test]
fn a_right_click_opens_the_context_menu_at_the_pointer(cx: &mut TestAppContext) {
    let tree = json!({"id": "root", "component": "Box", "style": {"padding": 20}, "children": [
        {"id": "ctx", "component": "Menu", "props": {"openOn": "contextmenu", "items": [{"label": "Copy", "value": "copy"}, {"label": "Delete", "value": "delete"}]}, "children": [
            {"id": "area", "component": "Box", "style": {"height": 200, "width": 300}}
        ]}
    ]});
    let (view, _log, cx) = surface(cx, tree, false);
    let at = gpui::point(px(120.0), px(90.0));
    cx.simulate_mouse_down(at, MouseButton::Right, Modifiers::default());
    cx.simulate_mouse_up(at, MouseButton::Right, Modifiers::default());
    draw(cx);
    draw(cx);
    let layer = view.read_with(cx, |v, _| v.layers().iter().find(|l| l.owner == "ctx").cloned()).expect("the context menu opened");
    assert_eq!(layer.position, "point");
    let root = layer.frames.first().unwrap();
    assert!((root.x - 120.0).abs() < 1.0 && root.y >= 90.0 && root.y - 90.0 <= 8.0, "at the pointer (plus the menu offset): {},{}", root.x, root.y);
}

#[gpui::test]
fn typing_a_comma_adds_a_chip_and_backspace_takes_one_away(cx: &mut TestAppContext) {
    let tree = json!({"id": "root", "component": "Box", "children": [
        {"id": "tags", "component": "ChipInput", "props": {"label": "Tags", "name": "tags", "values": ["bug"]}}
    ]});
    let (view, _log, cx) = surface(cx, tree, false);
    cx.update(|window, cx| view.update(cx, |v, cx| {
        v.type_into("tags.input", "ios,", window, cx);
    }));
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();
    draw(cx);
    let chips = |view: &Entity<SurfaceView>, cx: &mut VisualTestContext| view.read_with(cx, |v, _| (0..4).filter(|i| v.index_of(&format!("tags.chip.{i}")).is_some()).count());
    assert_eq!(chips(&view, cx), 2, "the comma added a chip");
    assert_eq!(view.read_with(cx, |v, cx| v.field_text("tags.input", cx)), Some(String::new()), "the field emptied");
}

/// A host pane 500 px wide: the very first frame lays out at 500.
struct Pane {
    view: Entity<SurfaceView>,
}

impl Render for Pane {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        div().w(px(500.0)).child(self.view.clone())
    }
}

#[gpui::test]
fn the_first_frame_lays_out_at_the_element_width(cx: &mut TestAppContext) {
    init(cx);
    let (pane, vcx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| SurfaceView::new(SurfaceViewOptions::default(), window, cx));
        view.update(cx, |v, cx| {
            v.set_nested(serde_json::from_value::<NestedNode>(json!({"id": "root", "component": "Box", "children": [{"id": "t", "component": "Text", "props": {"text": "Hello"}}]})).unwrap(), cx);
        });
        Pane { view }
    });
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    let view = pane.read_with(vcx, |p, _| p.view.clone());
    let (width, passes, calls) = view.read_with(vcx, |v, _| (v.stats().width, v.stats().passes, v.stats().measure_calls));
    assert_eq!(width, 500.0, "no 900 px first frame");
    // gpui's layout asks the measured node twice per frame; the second
    // answer comes from the core's memo.
    assert!(passes <= 2 && calls == 0, "{passes} passes, last measured {calls}");
}

#[gpui::test]
fn a_tall_dialog_scrolls_its_body(cx: &mut TestAppContext) {
    let lines: Vec<Value> = (0..90).map(|i| json!({"id": format!("line{i}"), "component": "Text", "props": {"text": format!("Line {i}")}})).collect();
    let tree = json!({"id": "root", "component": "Box", "children": [
        {"id": "long", "component": "Dialog", "props": {"title": "Terms", "open": true}, "children": lines}
    ]});
    let (view, _log, cx) = surface(cx, tree, false);
    view.update(cx, |v, _| v.record_bounds(true));
    draw(cx);
    let (content, window_h) = view.read_with(cx, |v, _| (v.painted_bounds("long.content").expect("the dialog painted"), v.visible_region().height));
    assert!(f32::from(content.size.height) <= window_h, "the dialog fits the viewport ({} > {window_h})", f32::from(content.size.height));
    cx.simulate_event(ScrollEvent { position: content.center(), delta: ScrollDelta::Pixels(gpui::point(px(0.0), px(-200.0))), ..Default::default() });
    draw(cx);
    assert_eq!(view.read_with(cx, |v, _| v.surface().scroll_offset("long.body").1), 200.0, "its body scrolled");
}

/// A host that scrolls the surface inside a 400 px pane.
struct Scroller {
    view: Entity<SurfaceView>,
    handle: gpui::ScrollHandle,
}

impl Render for Scroller {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        div().id("scroller").w(px(600.0)).h(px(400.0)).overflow_y_scroll().track_scroll(&self.handle).child(div().w_full().child(self.view.clone()))
    }
}

#[gpui::test]
fn a_dialog_centres_in_the_region_the_host_shows(cx: &mut TestAppContext) {
    init(cx);
    let rows: Vec<Value> = (0..60).map(|i| json!({"id": format!("r{i}"), "component": "Box", "style": {"height": 40}})).collect();
    let mut children = rows;
    children.push(json!({"id": "dlg", "component": "Dialog", "props": {"title": "Hi", "open": false}, "children": [{"id": "dlg-body", "component": "Text", "props": {"text": "Body"}}]}));
    let tree = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column"}, "children": children});
    let handle = gpui::ScrollHandle::new();
    let h2 = handle.clone();
    let (host, vcx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| SurfaceView::new(SurfaceViewOptions::default(), window, cx));
        view.update(cx, |v, cx| {
            v.set_nested(serde_json::from_value::<NestedNode>(tree).unwrap(), cx);
            v.record_bounds(true);
        });
        Scroller { view, handle: h2 }
    });
    draw(vcx);
    handle.set_offset(gpui::point(px(0.0), px(-1200.0)));
    draw(vcx);
    let view = host.read_with(vcx, |h, _| h.view.clone());
    view.update(vcx, |v, cx| {
        v.surface_mut().set_open("dlg", true);
        cx.notify();
    });
    draw(vcx);
    draw(vcx);
    let (region, content) = view.read_with(vcx, |v, _| (v.visible_region(), v.painted_bounds("dlg.content")));
    assert!((region.top - 1200.0).abs() < 1.0 && (region.height - 400.0).abs() < 1.0, "{region:?}");
    let b = content.expect("the dialog painted");
    let (top, bottom) = (f32::from(b.origin.y), f32::from(b.origin.y + b.size.height));
    assert!(top >= 0.0 && bottom <= 400.0, "the dialog sits in the visible pane: {top}..{bottom}");
}

#[gpui::test]
fn hovering_a_toast_pauses_its_timer(cx: &mut TestAppContext) {
    let tree = json!({"id": "root", "component": "Box", "children": [
        {"id": "note", "component": "Toast", "props": {"title": "Copied", "duration": 1000, "open": true}}
    ]});
    let (view, _log, cx) = surface(cx, tree, false);
    view.update(cx, |v, _| v.record_bounds(true));
    draw(cx);
    let b = view.read_with(cx, |v, _| v.painted_bounds("note.root")).expect("the toast painted");
    cx.simulate_mouse_move(b.center(), None, Modifiers::default());
    draw(cx);
    cx.executor().advance_clock(Duration::from_millis(2000));
    cx.run_until_parked();
    draw(cx);
    assert!(view.read_with(cx, |v, _| v.layers().iter().any(|l| l.owner == "note")), "paused while hovered");
    cx.simulate_mouse_move(gpui::point(px(5.0), px(5.0)), None, Modifiers::default());
    draw(cx);
    cx.executor().advance_clock(Duration::from_millis(1100));
    cx.run_until_parked();
    draw(cx);
    assert!(view.read_with(cx, |v, _| v.layers().is_empty()), "resumed and expired");
}

#[gpui::test]
fn dragging_a_drawer_toward_its_edge_dismisses_it(cx: &mut TestAppContext) {
    let tree = json!({"id": "root", "component": "Box", "children": [
        {"id": "sheet", "component": "Drawer", "props": {"title": "Filters", "open": true, "side": "bottom"}, "children": [{"id": "sheet_body", "component": "Text", "props": {"text": "Body"}}]}
    ]});
    let (view, _log, cx) = surface(cx, tree, false);
    view.update(cx, |v, _| v.record_bounds(true));
    draw(cx);
    // Finish the enter animation (a short rise on the motion clock) first, so
    // the frames read below never land mid-rise on a loaded test runner.
    view.update(cx, |v, cx| v.advance_clock(Duration::from_secs(1), cx));
    draw(cx);
    let b = view.read_with(cx, |v, _| v.painted_bounds("sheet.title").or_else(|| v.painted_bounds("sheet.content"))).expect("the sheet painted");
    let start = b.center();
    let top = view.read_with(cx, |v, _| f32::from(v.painted_bounds("sheet.content").unwrap().origin.y));
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    draw(cx);
    cx.simulate_mouse_move(gpui::point(start.x, start.y + px(40.0)), Some(MouseButton::Left), Modifiers::default());
    draw(cx);
    let moved = view.read_with(cx, |v, _| v.painted_bounds("sheet.content").map(|c| f32::from(c.origin.y))).unwrap();
    assert!((moved - (top + 40.0)).abs() <= 1.0, "the sheet follows the drag: {moved} vs {}", top + 40.0);
    cx.simulate_mouse_move(gpui::point(start.x, start.y + px(120.0)), Some(MouseButton::Left), Modifiers::default());
    draw(cx);
    cx.simulate_mouse_up(gpui::point(start.x, start.y + px(120.0)), MouseButton::Left, Modifiers::default());
    draw(cx);
    assert!(view.read_with(cx, |v, _| v.layers().is_empty()), "the drag dismissed the drawer");
}

#[gpui::test]
fn a_windowed_list_follows_the_host_scroller(cx: &mut TestAppContext) {
    init(cx);
    let tree = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column"}, "children": [
        {"id": "feed", "component": "List", "props": {"direction": "vertical"}, "template": {"component": "row-tpl", "path": "/rows"}, "children": []},
        {"id": "row-tpl", "component": "Box", "style": {"height": 40}, "children": [{"id": "row-label", "component": "Text", "props": {"text": {"@path": "label"}}}]}
    ]});
    let rows: Vec<Value> = (0..300).map(|i| json!({"label": format!("Row {i}")})).collect();
    let handle = gpui::ScrollHandle::new();
    let h2 = handle.clone();
    let (host, vcx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| SurfaceView::new(SurfaceViewOptions::default(), window, cx));
        view.update(cx, |v, cx| {
            v.set_nested(serde_json::from_value::<NestedNode>(tree).unwrap(), cx);
            v.set_data("/rows", Some(Value::Array(rows)), cx).unwrap();
        });
        Scroller { view, handle: h2 }
    });
    draw(vcx);
    draw(vcx);
    let view = host.read_with(vcx, |h, _| h.view.clone());
    let live = |view: &Entity<SurfaceView>, cx: &mut VisualTestContext, id: &str| view.read_with(cx, |v, _| v.index_of(id).is_some_and(|i| v.surface().layout_node(i).is_some()));
    assert!(live(&view, vcx, "row-tpl.0"), "the first rows are built");
    assert!(!live(&view, vcx, "row-tpl.150"), "far rows are windowed out");
    handle.set_offset(gpui::point(px(0.0), px(-6000.0)));
    draw(vcx);
    draw(vcx);
    assert!(live(&view, vcx, "row-tpl.150"), "the rows under the scroller were built");
    assert!(!live(&view, vcx, "row-tpl.0"), "the top ones left the window");
}

#[gpui::test]
fn a_closed_dialog_fades_out_unless_motion_is_reduced(cx: &mut TestAppContext) {
    let tree = json!({"id": "root", "component": "Box", "style": {"padding": 8, "alignItems": "flex-start"}, "children": [
        {"id": "under", "component": "Button", "props": {"label": "Under"}, "on": {"press": {"event": {"name": "under"}}}},
        {"id": "dlg", "component": "Dialog", "props": {"title": "Bye", "open": true}, "children": [{"id": "dlg-body", "component": "Text", "props": {"text": "Body"}}]}
    ]});
    let (view, log, cx) = surface(cx, tree.clone(), false);
    view.update(cx, |v, _| v.record_bounds(true));
    draw(cx);
    let under = view.read_with(cx, |v, _| v.painted_bounds("under")).expect("painted");
    assert_eq!(view.read_with(cx, |v, _| v.layers().len()), 1);
    cx.update(|window, cx| view.update(cx, |v, cx| v.escape(window, cx)));
    draw(cx);
    assert_eq!(view.read_with(cx, |v, _| v.layers().len()), 0, "Escape closes it");
    assert_eq!(view.read_with(cx, |v, _| v.exiting_layers()), vec!["dlg".to_string()], "the closed dialog paints fading out");
    let ghost = view.read_with(cx, |v, _| v.exiting_nodes("dlg"));
    assert!(ghost.iter().any(|(id, _)| id == "dlg-body") && ghost.iter().any(|(_, bg)| *bg), "the ghost is the dialog as painted: {ghost:?}");
    // The ghost (scrim included) is not hit-tested: a press on the button
    // under it reaches the button.
    cx.simulate_mouse_down(under.center(), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(under.center(), MouseButton::Left, Modifiers::default());
    draw(cx);
    assert!(log.0.borrow().actions.iter().any(|a| a.name == "under"), "the press went through the fading scrim");
    view.update(cx, |v, cx| v.advance_clock(Duration::from_millis(60), cx));
    draw(cx);
    assert_eq!(view.read_with(cx, |v, _| v.exiting_layers()).len(), 1, "still fading half-way through motion.fast");
    view.update(cx, |v, cx| v.advance_clock(Duration::from_millis(400), cx));
    draw(cx);
    assert!(view.read_with(cx, |v, _| v.exiting_layers()).is_empty(), "the fade ends after motion.fast");

    // Reduced motion: it closes at once.
    let settings = eui_core::surface::SurfaceSettings { reduced_motion: true, mode: eui_core::theme::ModeSetting::Light, ..Default::default() };
    cx.update(|window, cx| view.update(cx, |v, cx| v.set_settings(settings, window, cx)));
    let again = serde_json::to_string(&tree).unwrap().replace("dlg", "dlg2");
    view.update(cx, |v, cx| {
        v.set_nested(serde_json::from_str::<NestedNode>(&again).unwrap(), cx);
    });
    draw(cx);
    draw(cx);
    assert_eq!(view.read_with(cx, |v, _| v.layers().len()), 1);
    cx.update(|window, cx| view.update(cx, |v, cx| v.escape(window, cx)));
    draw(cx);
    assert_eq!(view.read_with(cx, |v, _| v.layers().len()), 0);
    assert!(view.read_with(cx, |v, _| v.exiting_layers()).is_empty(), "no exit animation under reduced motion");
}

/// The dialog's nodes as its ghost paints them: every one OF the dialog,
/// the content box styled.
fn assert_ghost_is_the_dialog(ghost: &[(String, bool)], body_rows: usize) {
    assert!(!ghost.is_empty(), "a ghost paints");
    assert!(ghost.iter().all(|(id, _)| id.starts_with("dlg") || id.starts_with("row")), "only the dialog's nodes: {ghost:?}");
    let rows = ghost.iter().filter(|(id, _)| id.starts_with("row")).count();
    assert_eq!(rows, body_rows, "every row of the dialog");
    assert!(ghost.iter().any(|(_, bg)| *bg), "the content keeps its background");
}

#[gpui::test]
fn a_dialog_closed_through_a_compaction_fades_out_as_it_painted(cx: &mut TestAppContext) {
    // 80 dialog rows over a 3-node page: closing frees more dead slots than
    // live ones, and the core compacts (every slot renumbers) that pass.
    let rows: Vec<Value> = (0..80).map(|i| json!({"id": format!("row{i}"), "component": "Text", "props": {"text": format!("Setting {i}")}})).collect();
    let tree = json!({"id": "root", "component": "Box", "children": [
        {"id": "dlg", "component": "Dialog", "props": {"title": "Settings", "open": true}, "children": rows},
        {"id": "after", "component": "Text", "props": {"text": "After"}}
    ]});
    let (view, _log, cx) = surface(cx, tree, false);
    let before = view.read_with(cx, |v, _| v.index_of("after"));
    cx.update(|window, cx| view.update(cx, |v, cx| v.escape(window, cx)));
    draw(cx);
    assert_eq!(view.read_with(cx, |v, _| v.layers().len()), 0);
    assert_ne!(view.read_with(cx, |v, _| v.index_of("after")), before, "the close compacted the slots");
    assert_ghost_is_the_dialog(&view.read_with(cx, |v, _| v.exiting_nodes("dlg")), 80);
}

#[gpui::test]
fn a_dialog_closed_through_surface_mut_fades_out_as_it_painted(cx: &mut TestAppContext) {
    let rows: Vec<Value> = (0..3).map(|i| json!({"id": format!("row{i}"), "component": "Text", "props": {"text": format!("Row {i}")}})).collect();
    let tree = json!({"id": "root", "component": "Box", "children": [
        {"id": "dlg", "component": "Dialog", "props": {"title": "Host", "open": true}, "children": rows}
    ]});
    let (view, _log, cx) = surface(cx, tree, false);
    view.update(cx, |v, cx| {
        v.surface_mut().set_open("dlg", false);
        cx.notify();
    });
    draw(cx);
    assert_eq!(view.read_with(cx, |v, _| v.layers().len()), 0);
    assert_ghost_is_the_dialog(&view.read_with(cx, |v, _| v.exiting_nodes("dlg")), 3);
}

#[gpui::test]
fn letter_spacing_widens_the_measured_and_painted_text(cx: &mut TestAppContext) {
    let tree = json!({"id": "root", "component": "Box", "style": {"flexDirection": "row", "alignItems": "flex-start", "gap": 8}, "children": [
        {"id": "plain", "component": "Text", "props": {"text": "Hello"}},
        {"id": "tracked", "component": "Text", "props": {"text": "Hello"}, "style": {"letterSpacing": 3}},
        {"id": "caps", "component": "Text", "props": {"text": "Hello", "lines": 1}, "style": {"letterSpacing": 2, "textTransform": "uppercase", "textDecoration": "underline"}},
        {"id": "btn", "component": "Button", "props": {"label": "Go"}, "style": {"letterSpacing": 4}},
        {"id": "docs", "component": "Link", "props": {"label": "Docs", "href": "https://example.com"}, "style": {"letterSpacing": 4}}
    ]});
    eui_gpui::paint::natives::record_tracked(true);
    let (view, _log, cx) = surface(cx, tree, false);
    view.update(cx, |v, _| v.record_bounds(true));
    eui_gpui::paint::natives::take_tracked();
    draw(cx);
    let painted = eui_gpui::paint::natives::take_tracked();
    eui_gpui::paint::natives::record_tracked(false);
    let bounds = |id: &str| view.read_with(cx, |v, _| v.painted_bounds(id)).unwrap_or_else(|| panic!("{id} painted"));
    let w = |id: &str| f32::from(bounds(id).size.width);
    let (plain, tracked) = (w("plain"), w("tracked"));
    assert!((tracked - plain - 15.0).abs() < 0.6, "5 characters × 3 px: {plain} → {tracked}");
    let line = |text: &str| painted.iter().find(|l| l.text == text).cloned().unwrap_or_else(|| panic!("{text:?} painted tracked: {painted:?}"));
    // Each painted glyph moves by `ls` per character before it; the painted
    // extent is the measured frame (nothing cut, nothing overhanging).
    let t = line("Hello");
    assert!((t.last_shift - 12.0).abs() < 0.01, "the 'o' sits 4 × 3 px right: {t:?}");
    assert!(!t.cut && (t.end - t.start - tracked).abs() < 0.6, "painted {} vs measured {tracked}", t.end - t.start);
    let caps = line("HELLO");
    assert!((caps.last_shift - 8.0).abs() < 0.01 && !caps.cut, "{caps:?}");
    let (u0, u1) = caps.underline.expect("underlined");
    assert!((u1 - u0 - (caps.end - caps.start - 2.0)).abs() < 0.6, "the underline spans the text without the trailing tracking: {caps:?}");
    let go = line("Go");
    assert!((go.last_shift - 4.0).abs() < 0.01 && !go.cut, "{go:?}");
    // The Link paints tracked too, centred in its measured frame, underlined.
    let docs = line("Docs");
    assert!((docs.last_shift - 12.0).abs() < 0.01 && !docs.cut && docs.underline.is_some(), "{docs:?}");
    let frame = bounds("docs");
    assert!((docs.end - docs.start - f32::from(frame.size.width)).abs() < 0.6, "painted {:?} vs frame {frame:?}", (docs.start, docs.end));
    assert!((docs.start - f32::from(frame.origin.x)).abs() < 0.6, "no off-centre slack");
}

#[gpui::test]
fn a_focused_chart_moves_its_tooltip_with_the_arrows(cx: &mut TestAppContext) {
    let tree = json!({"id": "root", "component": "Box", "children": [
        {"id": "sales", "component": "Chart", "props": {"kind": "bar", "categories": ["Q1", "Q2", "Q3"], "series": [{"name": "Sales", "values": [3, 5, 4]}]}}
    ]});
    for rtl in [false, true] {
        let (view, _log, cx) = surface(cx, tree.clone(), rtl);
        assert_eq!(tab(&view, cx).as_deref(), Some("sales"), "a chart is a tab stop");
        let point = |view: &Entity<SurfaceView>, cx: &mut VisualTestContext| view.read_with(cx, |v, _| v.chart_point("sales"));
        let (fwd, back) = if rtl { ("left", "right") } else { ("right", "left") };
        assert!(key(&view, cx, fwd));
        assert_eq!(point(&view, cx), Some(0));
        assert!(key(&view, cx, fwd));
        assert_eq!(point(&view, cx), Some(1));
        assert!(key(&view, cx, back));
        assert!(key(&view, cx, back), "wraps");
        assert_eq!(point(&view, cx), Some(2));
        assert!(key(&view, cx, "home"));
        assert_eq!(point(&view, cx), Some(0));
        assert!(key(&view, cx, "end"));
        assert_eq!(point(&view, cx), Some(2));
        assert!(key(&view, cx, "escape"));
        assert_eq!(point(&view, cx), None, "Escape hides the tooltip");
    }
}

#[gpui::test]
fn a_chart_tooltip_closes_on_blur_and_sparklines_are_no_tab_stops(cx: &mut TestAppContext) {
    let tree = json!({"id": "root", "component": "Box", "children": [
        {"id": "trend", "component": "Chart", "props": {"kind": "sparkline", "series": [{"name": "Visits", "values": [1, 4, 2]}]}},
        {"id": "sales", "component": "Chart", "props": {"kind": "bar", "categories": ["Q1", "Q2"], "series": [{"name": "Sales", "values": [3, 5]}]}},
        {"id": "next", "component": "Button", "props": {"label": "Next"}}
    ]});
    let (view, _log, cx) = surface(cx, tree, false);
    assert_eq!(tab(&view, cx).as_deref(), Some("sales"), "the sparkline is skipped (no tabIndex on the web)");
    assert!(key(&view, cx, "right"));
    assert_eq!(view.read_with(cx, |v, _| v.chart_point("sales")), Some(0));
    assert_eq!(tab(&view, cx).as_deref(), Some("next"));
    assert_eq!(view.read_with(cx, |v, _| v.chart_point("sales")), None, "the keyboard tooltip closes when focus leaves the chart");
    assert!(!view.read_with(cx, |v, _| v.node_states("sales")).iter().any(|s| s.starts_with("focus")), "the chart's focus states clear");
    assert_eq!(tab(&view, cx).as_deref(), Some("sales"), "wraps past the sparkline again");
}

#[gpui::test]
fn macro_roots_carry_their_a11y_roles(cx: &mut TestAppContext) {
    use gpui::Role;
    let tree = json!({"id": "root", "component": "Box", "children": [
        {"id": "card", "component": "Card", "props": {"title": "Plan"}, "children": [{"id": "x", "component": "Text", "props": {"text": "x"}}]},
        {"id": "grp", "component": "Group", "props": {"title": "Account", "footer": "Shown to admins"}},
        {"id": "err", "component": "Alert", "props": {"type": "error", "title": "Payment failed"}},
        {"id": "warn", "component": "Alert", "props": {"type": "warning", "title": "Low balance"}},
        {"id": "note", "component": "Alert", "props": {"type": "info", "title": "Heads up"}},
        {"id": "named", "component": "Card", "props": {"title": "Ignored"}, "accessibility": {"label": "Billing"}}
    ]});
    let (view, _log, cx) = surface(cx, tree, false);
    let info = |id: &str| view.read_with(cx, |v, _| v.accessible_info(id)).unwrap_or_else(|| panic!("{id} has an a11y node"));
    let card = info("card");
    assert_eq!((card.role, card.label.as_deref()), (Role::Group, Some("Plan")), "a Card = a group labelled by its title");
    let grp = info("grp");
    assert_eq!((grp.role, grp.label.as_deref(), grp.description.as_deref()), (Role::Group, Some("Account"), Some("Shown to admins")));
    assert_eq!((info("err").role, info("err").label.as_deref()), (Role::Alert, Some("Payment failed")));
    assert_eq!(info("warn").role, Role::Alert);
    assert_eq!(info("note").role, Role::Status, "info / success = status");
    assert_eq!(info("named").label.as_deref(), Some("Billing"), "the author's label wins");
    assert!(view.read_with(cx, |v, _| v.accessible_info("card.body")).is_none(), "macro parts stay plain");
}

#[gpui::test]
fn built_in_labels_come_from_the_strings_table(cx: &mut TestAppContext) {
    let tree = json!({"id": "root", "component": "Box", "children": [
        {"id": "chat", "component": "Composer", "props": {"attachments": true}},
        {"id": "busy", "component": "Composer", "props": {"busy": true}},
        {"id": "spin", "component": "Spinner"},
        {"id": "deck", "component": "Carousel", "children": [
            {"id": "p1", "component": "Text", "props": {"text": "One"}},
            {"id": "p2", "component": "Text", "props": {"text": "Two"}}
        ]}
    ]});
    let (view, _log, cx) = surface(cx, tree, false);
    let label = |view: &Entity<SurfaceView>, cx: &mut VisualTestContext, id: &str| view.read_with(cx, |v, _| v.accessible_info(id).and_then(|i| i.label).map(|l| l.to_string()));
    let dot = view.update(cx, |v, _| v.surface_mut().nodes().into_iter().find(|n| n.part.as_deref() == Some("indicator")).map(|n| n.id)).expect("the carousel has dots");
    assert_eq!(label(&view, cx, "chat.send").as_deref(), Some("Send"));
    assert_eq!(label(&view, cx, "chat.attach").as_deref(), Some("Browse"));
    assert_eq!(label(&view, cx, "busy.send").as_deref(), Some("Stop"));
    assert_eq!(label(&view, cx, "spin").as_deref(), Some("Loading"));
    assert_eq!(label(&view, cx, &format!("{dot}.dot.1")).as_deref(), Some("Page 2 of 2"));
    let strings = serde_json::from_value(json!({"send": "Senden", "stop": "Stopp", "loading": "Lädt", "browse": "Durchsuchen", "pageOf": "Seite {page} von {total}"})).unwrap();
    let settings = eui_core::surface::SurfaceSettings { strings, mode: eui_core::theme::ModeSetting::Light, ..Default::default() };
    cx.update(|window, cx| view.update(cx, |v, cx| v.set_settings(settings, window, cx)));
    draw(cx);
    assert_eq!(label(&view, cx, "chat.send").as_deref(), Some("Senden"));
    assert_eq!(label(&view, cx, "chat.attach").as_deref(), Some("Durchsuchen"));
    assert_eq!(label(&view, cx, "busy.send").as_deref(), Some("Stopp"));
    assert_eq!(label(&view, cx, "spin").as_deref(), Some("Lädt"));
    assert_eq!(label(&view, cx, &format!("{dot}.dot.0")).as_deref(), Some("Seite 1 von 2"));
}

#[gpui::test]
fn a_markdown_selection_drops_when_its_text_changes(cx: &mut TestAppContext) {
    let tree = json!({"id": "root", "component": "Box", "children": [
        {"id": "md", "component": "Markdown", "props": {"text": {"@path": "/reply"}}}
    ]});
    init(cx);
    let options = SurfaceViewOptions { theme: eui_core::themes::builtin_theme("neutral"), mode: eui_core::theme::Mode::Light, ..Default::default() };
    let (view, cx) = cx.add_window_view(move |window, cx| SurfaceView::new(options, window, cx));
    view.update(cx, |v, cx| {
        v.set_nested(serde_json::from_value::<NestedNode>(tree).unwrap(), cx);
        v.set_data("/reply", Some(json!("The first sentence.")), cx).unwrap();
    });
    draw(cx);
    draw(cx);
    assert!(view.update(cx, |v, cx| v.select_all_markdown("md", cx)));
    draw(cx);
    assert_eq!(view.read_with(cx, |v, _| v.selected_text()), "The first sentence.");
    // A streamed chunk lands: the old byte range means nothing now.
    view.update(cx, |v, cx| v.set_data("/reply", Some(json!("The first sentence. And a second one.")), cx)).unwrap();
    draw(cx);
    assert_eq!(view.read_with(cx, |v, _| v.selected_text()), "", "the selection does not jump onto other text");
    // An unrelated change keeps it.
    assert!(view.update(cx, |v, cx| v.select_all_markdown("md", cx)));
    view.update(cx, |v, cx| v.set_data("/other", Some(json!(1)), cx)).unwrap();
    draw(cx);
    assert_eq!(view.read_with(cx, |v, _| v.selected_text()), "The first sentence. And a second one.");
}

#[gpui::test]
fn markdown_text_selects_with_a_drag_and_copies(cx: &mut TestAppContext) {
    let tree = json!({"id": "root", "component": "Box", "style": {"padding": 10}, "children": [
        {"id": "md", "component": "Markdown", "props": {"text": "Hello **world**\n\n- one item\n\n```\nlet x = 1;\n```"}}
    ]});
    let (view, _log, cx) = surface(cx, tree, false);
    view.update(cx, |v, _| v.record_bounds(true));
    draw(cx);
    let b = view.read_with(cx, |v, _| v.painted_bounds("md")).expect("painted");
    // Press at the top-left of the text, drag past its bottom: everything.
    cx.simulate_mouse_down(b.origin + gpui::point(px(1.0), px(1.0)), MouseButton::Left, Modifiers::default());
    draw(cx);
    let end = b.origin + gpui::point(b.size.width - px(1.0), b.size.height + px(40.0));
    cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
    draw(cx);
    assert_eq!(view.read_with(cx, |v, _| v.selected_text()), "Hello world\none item\nlet x = 1;");
    cx.simulate_keystrokes("ctrl-c");
    assert_eq!(cx.read_from_clipboard().and_then(|c| c.text()), Some("Hello world\none item\nlet x = 1;".to_string()));
    // A press elsewhere drops it; select-all takes it back.
    cx.simulate_mouse_down(gpui::point(px(1.0), px(1.0)), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(gpui::point(px(1.0), px(1.0)), MouseButton::Left, Modifiers::default());
    draw(cx);
    assert_eq!(view.read_with(cx, |v, _| v.selected_text()), "");
    assert!(view.update(cx, |v, cx| v.select_all_markdown("md", cx)));
    assert!(view.read_with(cx, |v, _| v.selected_text()).starts_with("Hello"));
}

/// EXP-1259: the search list keeps the focus in its field: typing filters,
/// the arrows move the highlighted row, Enter (and Tab) pick it.
#[gpui::test]
fn a_search_list_filters_and_picks_from_its_field(cx: &mut TestAppContext) {
    let tree = json!({"id": "root", "component": "Box", "children": [
        {"id": "sl", "component": "SearchList", "props": {"items": [
            {"label": "Ship the search list", "value": "a", "identifier": "EXP-12"},
            {"label": "Spinner sizes", "value": "b", "identifier": "EXP-14"},
            {"label": "Spin the mark", "value": "c", "identifier": "EXP-15"}
        ]}, "on": {"select": {"event": {"name": "pick"}}}}
    ]});
    let (view, log, cx) = surface(cx, tree, false);
    // (The field is never focused here: a focused gpui-component Input asks
    // the test window for its native view on macOS.)
    cx.update(|window, cx| view.update(cx, |v, cx| {
        v.type_into("sl.search", "spin", window, cx);
    }));
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();
    draw(cx);
    assert!(view.read_with(cx, |v, _| v.index_of("sl.item.0").is_none()), "filtered");
    assert!(states(&view, cx, "sl.item.1").iter().any(|s| s == "selected"), "the top row is highlighted");
    let press = |view: &Entity<SurfaceView>, cx: &mut VisualTestContext, k: &str| {
        let k = k.to_string();
        cx.update(|window, cx| view.update(cx, |v, cx| v.search_key("sl.search", &k, window, cx)));
        draw(cx);
    };
    press(&view, cx, "ArrowDown");
    assert!(states(&view, cx, "sl.item.2").iter().any(|s| s == "selected"));
    press(&view, cx, "Enter");
    let picked = log.0.borrow().actions.iter().find(|a| a.name == "pick").map(|a| a.context["value"].clone());
    assert_eq!(picked, Some(json!("c")));
}

/// EXP-1259: a loading Button keeps (at least) its idle width and grows to
/// fit its `pendingLabel`.
#[gpui::test]
fn a_pending_button_never_narrows(cx: &mut TestAppContext) {
    let tree = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "alignItems": "flex-start", "gap": 8}, "children": [
        {"id": "idle", "component": "Button", "props": {"label": "Start run", "icon": "ui-add"}},
        {"id": "busy", "component": "Button", "props": {"label": "Start run", "icon": "ui-add", "loading": true, "pendingLabel": "Starting"}},
        {"id": "long", "component": "Button", "props": {"label": "Go", "loading": true, "pendingLabel": "Starting the run"}}
    ]});
    let (view, _log, cx) = surface(cx, tree, false);
    let w = |view: &Entity<SurfaceView>, cx: &mut VisualTestContext, id: &str| view.read_with(cx, |v, _| v.surface().last_frame(v.index_of(id).unwrap()).unwrap().w);
    let (idle, busy, long) = (w(&view, cx, "idle"), w(&view, cx, "busy"), w(&view, cx, "long"));
    assert!(busy >= idle - 0.5, "{busy} < {idle}");
    let go = view.read_with(cx, |v, _| v.surface().last_frame(v.index_of("long").unwrap()).unwrap().h);
    assert!(long > go * 2.0, "the pending word fits: {long}");
}

/// EXP-1280: ChoiceTiles (a11y.json): one roving stop on the picked tile,
/// arrows move it (a single pick follows), Space toggles a multiple's
/// focused tile; every tile wears a picture once any option has one.
#[gpui::test]
fn choice_tiles_rove_pick_and_toggle(cx: &mut TestAppContext) {
    let tree = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": 12, "padding": 16, "width": 420}, "children": [
        {"id": "one", "component": "ChoiceTiles", "props": {"name": "one", "options": [
            {"label": "Two columns", "value": "two", "description": "Pictures beside", "image": "https://example.com/two.png"},
            {"label": "One column", "value": "one"}
        ], "value": "two"}},
        {"id": "many", "component": "ChoiceTiles", "props": {"name": "many", "type": "multiple", "options": [
            {"label": "iOS", "value": "ios"}, {"label": "Android", "value": "android"}, {"label": "Web", "value": "web"}
        ], "value": "ios"}}
    ]});
    let (view, _log, cx) = surface(cx, tree, false);
    let exists = |view: &Entity<SurfaceView>, cx: &mut VisualTestContext, id: &str| view.read_with(cx, |v, _| v.index_of(id).is_some());
    assert!(exists(&view, cx, "one.picture.0") && exists(&view, cx, "one.picture.1"), "a picture row keeps one height");
    assert!(!exists(&view, cx, "many.picture.0"), "text-only tiles carry no picture");
    let frame = |view: &Entity<SurfaceView>, cx: &mut VisualTestContext, id: &str| view.read_with(cx, |v, _| v.frame(v.index_of(id).unwrap()).unwrap());
    let (a, b) = (frame(&view, cx, "one.picture.0"), frame(&view, cx, "one.picture.1"));
    assert!(a.w > 0.0 && (a.w / a.h - 1.6).abs() < 0.05, "picture aspect 1.6: {a:?}");
    assert_eq!((a.h, a.y), (b.h, b.y), "the tiles of a row line up");
    assert_eq!(tab(&view, cx).as_deref(), Some("one.tile.0"), "the picked tile is the stop");
    assert!(states(&view, cx, "one.tile.0").contains(&"selected".to_string()));
    assert!(key(&view, cx, "right"));
    assert_eq!(focused(&view, cx).as_deref(), Some("one.tile.1"));
    assert!(states(&view, cx, "one.tile.1").contains(&"selected".to_string()), "a single pick follows the focus");
    assert!(!states(&view, cx, "one.tile.0").contains(&"selected".to_string()));
    assert_eq!(tab(&view, cx).as_deref(), Some("many.tile.0"));
    assert!(key(&view, cx, "right"));
    assert_eq!(focused(&view, cx).as_deref(), Some("many.tile.1"));
    assert!(!states(&view, cx, "many.tile.1").contains(&"selected".to_string()), "a multiple only moves");
    assert!(key(&view, cx, "space"));
    assert!(states(&view, cx, "many.tile.1").contains(&"selected".to_string()), "Space toggles");
    assert!(states(&view, cx, "many.tile.0").contains(&"selected".to_string()), "the other pick stays");
}

#[gpui::test]
/// VAPP-279: the strip chip's spacing is the IDE work tab's: the title
/// `$spacing.xs` before the ×, the × `$spacing.xs` (+ the hairline) from
/// the end border, the title 10 from the start border.
fn a_strip_chip_spaces_its_close_like_the_ide(cx: &mut TestAppContext) {
    let tree = json!({"id": "root", "component": "Box", "style": {"width": 900}, "children": [
        {"id": "s", "component": "Tabs", "props": {"variant": "strip", "value": "a", "closable": "always", "tabs": [{"label": "ExponentialOS", "value": "a", "icon": "status-in-progress"}, {"label": "google keywords", "value": "b", "icon": "action-chat"}]}}
    ]});
    let (view, _log, cx) = surface(cx, tree, false);
    view.read_with(cx, |v, _| {
        let s = v.surface();
        let f = |id: &str| v.frame(s.index_of(id).unwrap()).unwrap();
        for i in 0..2 {
            let (item, tab, close) = (f(&format!("s.item.{i}")), f(&format!("s.tab.{i}")), f(&format!("s.close.{i}")));
            let pad = s.visual(s.index_of(&format!("s.tab.{i}")).unwrap()).and_then(|x| x.padding).unwrap();
            assert_eq!(pad[1], 4.0, "title → × = $spacing.xs");
            assert!((close.x - (tab.x + tab.w)).abs() < 0.01, "the × follows the tab box");
            assert!(((item.x + item.w) - (close.x + close.w) - 5.0).abs() < 0.01, "× → border = 4 + hairline");
            assert!(((tab.x + pad[3]) - item.x - 11.0).abs() < 0.01, "title starts 10 + hairline in");
        }
    });
}

/// VAPP-279: a strip tab drags past its neighbour's centre (the press is
/// spent, `reorder` fires on the release), a click without movement
/// presses, and the strip folds into `+N` on a narrow surface.
#[gpui::test]
fn a_strip_tab_drags_into_place_and_the_strip_folds(cx: &mut TestAppContext) {
    let tabs: Vec<Value> = ["Inbox", "Release notes", "Calendar", "Settings", "Search", "Drafts"].iter().enumerate().map(|(i, l)| json!({"label": l, "value": format!("t{i}")})).collect();
    let tree = json!({"id": "root", "component": "Box", "style": {"width": 900}, "children": [
        {"id": "tabs", "component": "Tabs", "props": {"variant": "strip", "tabs": tabs, "value": "t0"}, "on": {"reorder": {"event": {"name": "reorder"}}, "change": {"event": {"name": "change"}}}}
    ]});
    let (view, log, cx) = surface(cx, tree, false);
    view.update(cx, |v, _| v.record_bounds(true));
    draw(cx);
    let a = view.read_with(cx, |v, _| v.painted_bounds("tabs.tab.0")).expect("painted");
    let b = view.read_with(cx, |v, _| v.painted_bounds("tabs.item.1")).expect("painted");
    let start = a.center();
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    let past = b.center().x - start.x + px(4.0);
    for step in [px(2.0), px(10.0), past] {
        cx.simulate_mouse_move(gpui::point(start.x + step, start.y), Some(MouseButton::Left), Modifiers::default());
        draw(cx);
    }
    cx.simulate_mouse_up(gpui::point(start.x + past, start.y), MouseButton::Left, Modifiers::default());
    draw(cx);
    let fired: Vec<Value> = log.0.borrow().actions.iter().filter(|e| e.name == "reorder").map(|e| json!({"from": e.context["from"], "to": e.context["to"]})).collect();
    assert_eq!(fired, vec![json!({"from": 0, "to": 1})], "one reorder on the release");
    // A click without movement presses (`change` to t2).
    let c = view.read_with(cx, |v, _| v.painted_bounds("tabs.tab.2")).expect("painted");
    cx.simulate_mouse_down(c.center(), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(c.center(), MouseButton::Left, Modifiers::default());
    draw(cx);
    assert!(log.0.borrow().actions.iter().any(|e| e.name == "change"), "a click presses");
    assert_eq!(log.0.borrow().actions.iter().filter(|e| e.name == "reorder").count(), 1, "no reorder from a click");
}

/// #VAPP-284: the foldable Row through the painter: interactions-round1.json
/// `foldRow` (a press on the head or its fold button, ArrowRight / ArrowLeft
/// on the focused one, mirrored in rtl), the events the host sees and the
/// head's `expanded` after.
#[gpui::test]
fn a_foldable_row_presses_folds_and_takes_the_arrows(cx: &mut TestAppContext) {
    let f: Value = serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/fixtures/interactions-round1.json")).unwrap()).unwrap();
    let fold = &f["foldRow"];
    let c = fold["cases"].as_array().unwrap();
    init(cx);
    for c in c {
        let name = c["name"].as_str().unwrap();
        let mut row = fold["surface"].clone();
        if c["pressable"] == json!(false) {
            row["on"].as_object_mut().unwrap().remove("press");
        }
        let tree = json!({"id": "root", "component": "Box", "children": [row]});
        let log = Recorder::default();
        let host = Rc::new(log.clone());
        let options = SurfaceViewOptions { theme: eui_core::themes::builtin_theme("neutral"), mode: eui_core::theme::Mode::Light, host, ..Default::default() };
        let (view, vcx) = cx.add_window_view(move |window, cx| SurfaceView::new(options, window, cx));
        let mut tree = tree;
        if c["rtl"] == json!(true) {
            tree["style"]["direction"] = json!("rtl");
        }
        let open = c["open"].clone();
        view.update(vcx, |v, cx| {
            v.set_nested(serde_json::from_value::<NestedNode>(tree).unwrap(), cx);
            if !open.is_null() {
                v.set_data("/open", Some(open), cx).unwrap();
            }
        });
        draw(vcx);
        draw(vcx);
        let step = &c["step"];
        let part = step.get("press").or_else(|| step.get("on")).and_then(Value::as_str).unwrap();
        let target = if part == "head" { "issues.head" } else { "issues.head.fold" };
        let index = view.read_with(vcx, |v, _| v.index_of(target)).unwrap_or_else(|| panic!("{name}: no {target}"));
        if step.get("press").is_some() {
            vcx.update(|window, cx| view.update(cx, |v, cx| v.press(index, window, cx)));
            draw(vcx);
        } else {
            while focused(&view, vcx).as_deref() != Some(target) {
                assert!(tab(&view, vcx).is_some(), "{name}: {target} is not focusable");
            }
            let k = if step["key"] == json!("ArrowLeft") { "left" } else { "right" };
            assert!(key(&view, vcx, k), "{name}: the arrow is the fold's");
        }
        let names: Vec<String> = log.0.borrow().actions.iter().map(|a| a.name.clone()).collect();
        assert_eq!(json!(names), c["events"], "{name}: events");
        let expanded = view.read_with(vcx, |v, _| v.surface().layout_node(v.index_of("issues.head").unwrap()).and_then(|n| n.accessibility.as_ref()).and_then(|a| a.get("expanded")).cloned());
        assert_eq!(expanded, Some(c["after"].clone()), "{name}: expanded after");
    }
}

/// #VAPP-285: the Tree through the painter: interactions-round1.json `tree`
/// (a press, or an arrow / Home / End on the focused row), the events the
/// host sees, where the focus lands and /open after.
#[gpui::test]
fn a_tree_selects_folds_fetches_and_moves_the_focus(cx: &mut TestAppContext) {
    let f: Value = serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/fixtures/interactions-round1.json")).unwrap()).unwrap();
    let t = &f["tree"];
    let row_ids = t["rowIds"].as_object().unwrap().clone();
    let item_of = |node: Option<String>| node.and_then(|node| row_ids.iter().find(|(_, id)| id.as_str() == Some(node.as_str())).map(|(item, _)| item.clone()));
    init(cx);
    for c in t["cases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let mut tree = json!({"id": "root", "component": "Box", "children": [t["surface"].clone()]});
        if c["rtl"] == json!(true) {
            tree["style"]["direction"] = json!("rtl");
        }
        let log = Recorder::default();
        let host = Rc::new(log.clone());
        let options = SurfaceViewOptions { theme: eui_core::themes::builtin_theme("neutral"), mode: eui_core::theme::Mode::Light, host, ..Default::default() };
        let (view, vcx) = cx.add_window_view(move |window, cx| SurfaceView::new(options, window, cx));
        let open = c["open"].clone();
        view.update(vcx, |v, cx| {
            v.set_nested(serde_json::from_value::<NestedNode>(tree).unwrap(), cx);
            v.set_data("/open", Some(open), cx).unwrap();
        });
        draw(vcx);
        draw(vcx);
        let step = &c["step"];
        let item = step.get("press").or_else(|| step.get("on")).and_then(Value::as_str).unwrap();
        let target = row_ids[item].as_str().unwrap().to_string();
        let index = view.read_with(vcx, |v, _| v.index_of(&target)).unwrap_or_else(|| panic!("{name}: no {target}"));
        let mut moved = None;
        if step.get("press").is_some() {
            vcx.update(|window, cx| view.update(cx, |v, cx| v.press(index, window, cx)));
            draw(vcx);
        } else {
            while focused(&view, vcx).as_deref() != Some(target.as_str()) {
                assert!(tab(&view, vcx).is_some(), "{name}: {target} is not focusable");
            }
            let k = match step["key"].as_str().unwrap() {
                "ArrowUp" => "up",
                "ArrowDown" => "down",
                "ArrowLeft" => "left",
                "ArrowRight" => "right",
                "Home" => "home",
                _=> "end",
            };
            key(&view, vcx, k);
            let now = focused(&view, vcx);
            if now.as_deref() != Some(target.as_str()) {
                moved = item_of(now);
            }
        }
        let names: Vec<String> = log.0.borrow().actions.iter().map(|a| a.name.clone()).collect();
        assert_eq!(json!(names), c["events"], "{name}: events");
        assert_eq!(json!(moved), c["after"]["focus"], "{name}: focus");
        let data = view.read_with(vcx, |v, _| v.surface().get_data("/open").cloned());
        assert_eq!(data, Some(c["after"]["open"].clone()), "{name}: /open");
    }
}

/// Round 8: the mixed Checkbox through the painter: interactions-round1.json
/// `checkbox` (a press on a mixed box checks it and clears
/// `indeterminate`, a plain press toggles; `change` carries both), the
/// toggled state a reader hears (`Mixed` wins over checked) and the bound
/// paths after each press.
#[gpui::test]
fn a_mixed_checkbox_presses_to_checked_and_reads_mixed(cx: &mut TestAppContext) {
    let f: Value = serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/fixtures/interactions-round1.json")).unwrap()).unwrap();
    let fx = &f["checkbox"];
    init(cx);
    let state_of = |t: Option<gpui::Toggled>| match t {
        Some(gpui::Toggled::Mixed) => "mixed",
        Some(gpui::Toggled::True) => "true",
        Some(gpui::Toggled::False) => "false",
        None => "none",
    };
    for c in fx["cases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let mut node = fx["surface"].clone();
        if let Some(Value::Object(unbound)) = c.get("unbound") {
            for (k, v) in unbound {
                node["props"][k] = v.clone();
            }
        }
        let tree = json!({"id": "root", "component": "Box", "children": [node]});
        let log = Recorder::default();
        let host = Rc::new(log.clone());
        let options = SurfaceViewOptions { theme: eui_core::themes::builtin_theme("neutral"), mode: eui_core::theme::Mode::Light, host, ..Default::default() };
        let (view, vcx) = cx.add_window_view(move |window, cx| SurfaceView::new(options, window, cx));
        let data = c.get("data").cloned();
        view.update(vcx, |v, cx| {
            for (k, val) in data.as_ref().and_then(Value::as_object).into_iter().flatten() {
                v.set_data(&format!("/{k}"), Some(val.clone()), cx).unwrap();
            }
            v.set_nested(serde_json::from_value::<NestedNode>(tree).unwrap(), cx);
        });
        draw(vcx);
        draw(vcx);
        let toggled = |view: &Entity<SurfaceView>, vcx: &mut VisualTestContext| view.read_with(vcx, |v, _| v.toggled_state("c"));
        assert_eq!(state_of(toggled(&view, vcx)), c["before"].as_str().unwrap(), "{name}: before");
        let presses = c["presses"].as_u64().unwrap() as usize;
        let steps = c["steps"].as_array().unwrap();
        assert_eq!(presses, steps.len());
        for (k, step) in steps.iter().enumerate() {
            log.0.borrow_mut().actions.clear();
            let index = view.read_with(vcx, |v, _| v.index_of("c")).unwrap();
            vcx.update(|window, cx| view.update(cx, |v, cx| v.press(index, window, cx)));
            draw(vcx);
            let changes: Vec<Value> = log.0.borrow().actions.iter().filter(|a| a.name == "change").map(|a| a.payload.clone().unwrap_or_else(|| a.context.clone())).collect();
            assert_eq!(changes.len(), 1, "{name} #{k}: one change");
            assert_eq!(changes[0]["checked"], step["change"]["checked"], "{name} #{k}: change.checked");
            assert_eq!(changes[0]["indeterminate"], step["change"]["indeterminate"], "{name} #{k}: change.indeterminate");
            assert_eq!(state_of(toggled(&view, vcx)), step["state"].as_str().unwrap(), "{name} #{k}: state");
            if c.get("unbound").is_none() {
                let (checked, mixed) = view.read_with(vcx, |v, _| (v.surface().get_data("/checked").cloned(), v.surface().get_data("/mixed").cloned()));
                assert_eq!(checked.unwrap_or(Value::Null), step["checked"], "{name} #{k}: /checked");
                assert_eq!(mixed.unwrap_or(Value::Null), step["mixed"], "{name} #{k}: /mixed");
            }
        }
    }
}

/// #EXP-1739 overlayAnchor on gpui: a wheel over the container holding a
/// Popover's anchor scrolls it, and the Popover closes (the core raises it,
/// the view takes it with the scroll); a frame that moves nothing keeps it.
#[gpui::test]
fn a_wheel_over_the_anchors_container_closes_its_popover(cx: &mut TestAppContext) {
    let fixture: Value = serde_json::from_str(include_str!("../../../../../packages/exponential-ui/fixtures/interactions-round1.json")).unwrap();
    let tree = json!({"id": "root", "component": "Box", "children": [fixture["overlayAnchor"]["surface"].clone()]});
    let (view, _, cx) = surface(cx, tree, false);
    view.update(cx, |v, _| v.record_bounds(true));
    draw(cx);
    let open = |view: &Entity<SurfaceView>, cx: &mut VisualTestContext| view.read_with(cx, |v, _| v.layers().iter().any(|l| l.owner == "pop"));
    assert!(open(&view, cx), "opens against its anchor");
    draw(cx);
    assert!(open(&view, cx), "a frame that moves nothing keeps it");
    let list = view.read_with(cx, |v, _| v.painted_bounds("list").expect("the container painted"));
    // Low in the container, clear of the Popover hanging under the anchor.
    let at = list.bottom_left() + gpui::point(px(10.0), px(-10.0));
    cx.simulate_event(ScrollEvent { position: at, delta: ScrollDelta::Pixels(gpui::point(px(0.0), px(-40.0))), ..Default::default() });
    draw(cx);
    assert!(view.read_with(cx, |v, _| v.surface().scroll_offset("list").1) > 0.0, "the wheel scrolled the container");
    assert!(!open(&view, cx), "the Popover closed with its anchor's scroll");
}

/// #EDE-65: a Box's `on.hover` through the painter: interactions-round1.json
/// `boxHover` (real pointer moves enter / leave a tile, a click presses it),
/// the actions the host sees, in order.
#[gpui::test]
fn a_box_fires_hover_on_entering(cx: &mut TestAppContext) {
    let f: Value = serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/fixtures/interactions-round1.json")).unwrap()).unwrap();
    // #FEED-201: `tilePress` (StatTile and Card presses) has the same shape.
    for key in ["boxHover", "tilePress"] {
    let fx = &f[key];
    for c in fx["cases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let (view, log, vcx) = surface(cx, json!({"id": "root", "component": "Box", "style": {"padding": 40}, "children": [fx["surface"].clone()]}), false);
        view.update(vcx, |v, _| v.record_bounds(true));
        draw(vcx);
        let outside = view.read_with(vcx, |v, _| v.painted_bounds("tiles")).map(|b| b.bottom_right() + gpui::point(px(20.0), px(20.0))).unwrap();
        for step in c["steps"].as_array().unwrap() {
            let (kind, id) = step.as_object().unwrap().iter().next().unwrap();
            let b = view.read_with(vcx, |v, _| v.painted_bounds(id.as_str().unwrap())).unwrap_or_else(|| panic!("{name}: {id} painted"));
            match kind.as_str() {
                "enter" => vcx.simulate_mouse_move(b.center(), None, Modifiers::default()),
                "leave" => vcx.simulate_mouse_move(outside, None, Modifiers::default()),
                _ => {
                    vcx.simulate_mouse_down(b.center(), MouseButton::Left, Modifiers::default());
                    vcx.simulate_mouse_up(b.center(), MouseButton::Left, Modifiers::default());
                }
            }
            draw(vcx);
        }
        let fired: Vec<Value> = log.0.borrow().actions.iter().map(|a| json!({"name": a.name, "context": a.context})).collect();
        assert_eq!(fired, c["events"].as_array().unwrap().clone(), "{name}");
    }
    }
}

/// #EXP-1629 (interactions-round1.json keyRecorder): every case with real
/// input: a click on the field, key downs/ups through the window (the
/// interceptor beats Tab and Escape), modifier changes, mouse buttons over
/// the field, the window's focus taken away, and host writes.
#[gpui::test]
fn a_key_recorder_captures_like_the_fixture(cx: &mut TestAppContext) {
    let f: Value = serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/fixtures/interactions-round1.json")).unwrap()).unwrap();
    let k = &f["keyRecorder"];
    let mods = |m: &Value| {
        let held = |x: &str| m.as_array().is_some_and(|a| a.iter().any(|v| v == x));
        Modifiers { control: held("ctrl"), alt: held("alt"), shift: held("shift"), platform: held("meta"), function: false }
    };
    let keystroke = |key: &str, m: &Value| {
        let mut s = String::new();
        for (flag, prefix) in [("ctrl", "ctrl-"), ("alt", "alt-"), ("shift", "shift-"), ("meta", "cmd-")] {
            if m.as_array().is_some_and(|a| a.iter().any(|v| v == flag)) {
                s.push_str(prefix);
            }
        }
        s.push_str(key);
        gpui::Keystroke::parse(Box::leak(s.into_boxed_str())).unwrap()
    };
    for c in k["cases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let (view, log, vcx) = surface(cx, json!({"id": "root", "component": "Box", "style": {"padding": 40}, "children": [k["surface"].clone()]}), false);
        view.update(vcx, |v, _| v.record_bounds(true));
        draw(vcx);
        let border = |view: &Entity<SurfaceView>, vcx: &mut VisualTestContext| view.read_with(vcx, |v, _| v.surface().visual(v.index_of("ptt.field").unwrap()).and_then(|s| s.border_color.clone()));
        let idle_border = border(&view, vcx);
        for step in c["steps"].as_array().unwrap() {
            let (kind, value) = step.as_object().unwrap().iter().next().unwrap();
            let b = view.read_with(vcx, |v, _| v.painted_bounds("ptt.field")).unwrap_or_else(|| panic!("{name}: the field painted"));
            match kind.as_str() {
                "press" => {
                    vcx.simulate_mouse_down(b.center(), MouseButton::Left, Modifiers::default());
                    vcx.simulate_mouse_up(b.center(), MouseButton::Left, Modifiers::default());
                }
                "down" => vcx.simulate_event(gpui::KeyDownEvent { keystroke: keystroke(value.as_str().unwrap(), &step["mods"]), is_held: false, prefer_character_input: false }),
                "up" => vcx.simulate_event(gpui::KeyUpEvent { keystroke: keystroke(value.as_str().unwrap(), &step["mods"]) }),
                "modifiers" => vcx.simulate_modifiers_change(mods(value)),
                "mouse" => {
                    let button = match value.as_u64().unwrap() {
                        1 => MouseButton::Left,
                        2 => MouseButton::Right,
                        3 => MouseButton::Middle,
                        4 => MouseButton::Navigate(gpui::NavigationDirection::Back),
                        _ => MouseButton::Navigate(gpui::NavigationDirection::Forward),
                    };
                    vcx.simulate_mouse_down(b.center(), button, Modifiers::default());
                    vcx.simulate_mouse_up(b.center(), button, Modifiers::default());
                }
                "blur" => vcx.update(|window, cx| window.blur(cx)),
                "hostWrite" => view.update(vcx, |v, cx| v.set_data(value["path"].as_str().unwrap(), Some(value["value"].clone()), cx).unwrap()),
                other => panic!("{name}: step {other}"),
            }
            draw(vcx);
            if kind == "press" {
                assert_eq!(focused(&view, vcx).as_deref(), Some("ptt.field"), "{name}: the field takes the focus");
            }
        }
        let fired: Vec<Value> = log.0.borrow().actions.iter().map(|a| json!({"name": a.name, "context": a.context})).collect();
        assert_eq!(fired, c["events"].as_array().unwrap().clone(), "{name}: events");
        let recording = view.read_with(vcx, |v, _| v.surface().get_data("/recording").cloned()).unwrap_or(Value::Null);
        assert_eq!(recording, c["recording"], "{name}: /recording");
        if recording == json!(true) {
            assert_ne!(border(&view, vcx), idle_border, "{name}: the recording ring paints");
        }
    }
}

/// #EXP-1629: a key repeat while recording sends nothing; a shifted
/// character sends its physical key with shift.
#[gpui::test]
fn a_key_recorder_drops_repeats_and_unshifts(cx: &mut TestAppContext) {
    let f: Value = serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/fixtures/interactions-round1.json")).unwrap()).unwrap();
    let (view, log, vcx) = surface(cx, json!({"id": "root", "component": "Box", "style": {"padding": 40}, "children": [f["keyRecorder"]["surface"].clone()]}), false);
    view.update(vcx, |v, _| v.record_bounds(true));
    draw(vcx);
    let b = view.read_with(vcx, |v, _| v.painted_bounds("ptt.field")).unwrap();
    vcx.simulate_mouse_down(b.center(), MouseButton::Left, Modifiers::default());
    vcx.simulate_mouse_up(b.center(), MouseButton::Left, Modifiers::default());
    draw(vcx);
    let down = |key: &'static str, held: bool| gpui::KeyDownEvent { keystroke: gpui::Keystroke::parse(key).unwrap(), is_held: held, prefer_character_input: false };
    vcx.simulate_event(down("k", false));
    vcx.simulate_event(down("k", true));
    vcx.simulate_event(down("shift-!", false));
    vcx.simulate_event(down("home", false));
    draw(vcx);
    let keys: Vec<Value> = log.0.borrow().actions.iter().filter(|a| a.name == "key").map(|a| json!([a.context["key"], a.context["shift"]])).collect();
    assert_eq!(keys, vec![json!(["k", false]), json!(["1", true]), json!(["home", false])], "the repeat is dropped, `!` is shift+1");
}

/// #EDE-65 (interactions-round1.json composerKeys): with menuKeys on, Up,
/// Down, Enter, Tab and Escape (no modifier, no IME composition) fire
/// `key` {key} only; off, Enter submits. A key the Composer leaves goes on
/// to the textarea, whose own Enter (no modifier) is `submit_composer`.
#[gpui::test]
fn a_composer_hands_menu_keys_to_the_host_like_the_fixture(cx: &mut TestAppContext) {
    let f: Value = serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/fixtures/interactions-round1.json")).unwrap()).unwrap();
    let k = &f["composerKeys"];
    for c in k["cases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let (view, log, vcx) = surface(cx, json!({"id": "root", "component": "Box", "style": {"padding": 40}, "children": [k["surface"].clone()]}), false);
        view.update(vcx, |v, cx| {
            for (path, value) in c["data"].as_object().unwrap() {
                v.set_data(&format!("/{path}"), Some(value.clone()), cx).unwrap();
            }
        });
        draw(vcx);
        for step in c["steps"].as_array().unwrap() {
            if let Some(text) = step["type"].as_str() {
                let text = text.to_string();
                cx_type(&view, vcx, &text);
            } else if let Some(name_key) = step["key"].as_str() {
                let held = |x: &str| step["mods"].as_array().is_some_and(|a| a.iter().any(|v| v == x));
                let m = Modifiers { control: held("ctrl"), alt: held("alt"), shift: held("shift"), platform: held("meta"), function: false };
                let composing = step["composing"].as_bool().unwrap_or(false);
                let key = gpui_key(name_key);
                let handled = vcx.update(|window, cx| {
                    view.update(cx, |v, cx| {
                        if v.composer_key("c", key, &m, composing, window, cx) {
                            return true;
                        }
                        // The textarea's own Enter (an IME's Enter never reaches it).
                        if name_key == "Enter" && m == Modifiers::default() && !composing {
                            v.submit_composer("c", window, cx);
                            return true;
                        }
                        false
                    })
                });
                assert_eq!(handled, step["handled"].as_bool().unwrap(), "{name}: {name_key} handled");
            } else if let Some(w) = step.get("hostWrite") {
                view.update(vcx, |v, cx| v.set_data(w["path"].as_str().unwrap(), Some(w["value"].clone()), cx).unwrap());
            }
            vcx.executor().advance_clock(Duration::from_millis(20));
            draw(vcx);
        }
        vcx.executor().advance_clock(Duration::from_millis(400));
        draw(vcx);
        let fired: Vec<Value> = log.0.borrow().actions.iter().map(|a| json!({"name": a.name, "context": a.context})).collect();
        assert_eq!(fired, c["events"].as_array().unwrap().clone(), "{name}: events");
        let draft = view.read_with(vcx, |v, _| v.surface().get_data("/draft").cloned()).unwrap_or(Value::Null);
        assert_eq!(draft, c["draft"], "{name}: /draft");
    }
}

/// The gpui key of a fixture key name (SearchList's DOM-style names).
fn gpui_key(name: &str) -> &'static str {
    match name {
        "ArrowUp" => "up",
        "ArrowDown" => "down",
        "Enter" => "enter",
        "Tab" => "tab",
        "Escape" => "escape",
        other => panic!("composerKeys: unknown key {other}"),
    }
}

fn cx_type(view: &Entity<SurfaceView>, vcx: &mut VisualTestContext, text: &str) {
    vcx.update(|window, cx| view.update(cx, |v, cx| assert!(v.type_into("c", text, window, cx))));
}

/// #EDE-65: through the window, the interceptor beats the textarea's own
/// bindings on a focused Composer: Down and Enter fire `key`, never a
/// caret move or a submit; menuKeys off, Enter submits again.
#[gpui::test]
fn a_focused_composer_with_menu_keys_intercepts_the_window_keys(cx: &mut TestAppContext) {
    let f: Value = serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/fixtures/interactions-round1.json")).unwrap()).unwrap();
    let (view, log, vcx) = surface(cx, json!({"id": "root", "component": "Box", "style": {"padding": 40}, "children": [f["composerKeys"]["surface"].clone()]}), false);
    view.update(vcx, |v, cx| v.set_data("/menu", Some(json!(true)), cx).unwrap());
    draw(vcx);
    vcx.update(|window, cx| view.update(cx, |v, cx| assert!(v.focus_field("c", window, cx))));
    cx_type(&view, vcx, "/he");
    draw(vcx);
    let down = |key: &'static str| gpui::KeyDownEvent { keystroke: gpui::Keystroke::parse(key).unwrap(), is_held: false, prefer_character_input: false };
    vcx.simulate_event(down("down"));
    vcx.simulate_event(down("enter"));
    draw(vcx);
    let names = |log: &Recorder| log.0.borrow().actions.iter().map(|a| json!({"name": a.name, "context": a.context})).collect::<Vec<_>>();
    assert_eq!(names(&log), vec![json!({"name": "key", "context": {"key": "ArrowDown"}}), json!({"name": "key", "context": {"key": "Enter"}})]);
    assert_eq!(view.read_with(vcx, |v, cx| v.field_text("c", cx)).as_deref(), Some("/he"), "the field kept its text");
    view.update(vcx, |v, cx| v.set_data("/menu", Some(json!(false)), cx).unwrap());
    draw(vcx);
    vcx.simulate_event(down("enter"));
    draw(vcx);
    assert_eq!(names(&log).last(), Some(&json!({"name": "send", "context": {"value": "/he"}})), "menuKeys off: Enter submits");
}
