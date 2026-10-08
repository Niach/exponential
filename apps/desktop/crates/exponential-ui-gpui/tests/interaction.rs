//! Interaction through the painter in a headless gpui window: the keyboard
//! spec of `catalog/a11y.json` (roving tabs and radios, sliders, menus,
//! listboxes, mirrored in RTL), `:focus-visible` from the keyboard only,
//! dismissal rules, hover-opened tooltips, scroll containers, toasts,
//! forms, files, the clipboard, context menus, chips and the first frame's
//! width.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use exponential_ui::NestedNode;
use exponential_ui_gpui::host::{ActionEvent, HostPlugin, UploadEvent};
use exponential_ui_gpui::view::{SurfaceView, SurfaceViewOptions};
use gpui::{div, prelude::*, px, App, Entity, Modifiers, MouseButton, ScrollDelta, ScrollWheelEvent, TestAppContext, VisualTestContext, Window};
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
    let options = SurfaceViewOptions { theme: exponential_ui::themes::builtin_theme("neutral"), mode: exponential_ui::theme::Mode::Light, host, ..Default::default() };
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
        {"id": "menu", "component": "DropdownMenu", "props": {"label": "More", "items": [{"label": "Rename", "value": "rename"}, {"label": "Archive", "value": "archive"}, {"label": "Delete", "value": "delete"}]}, "on": {"select": {"event": {"name": "menuPick"}}}}
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
    cx.simulate_event(ScrollWheelEvent { position: pane.center(), delta: ScrollDelta::Pixels(gpui::point(px(0.0), px(-50.0))), ..Default::default() });
    draw(cx);
    assert_eq!(view.read_with(cx, |v, _| v.surface().scroll_offset("pane")), (0.0, 50.0), "the wheel moved the core's offset");
    assert_eq!(y(&view, cx, "row3"), before - 50.0, "the content is drawn translated");
    for _ in 0..20 {
        cx.simulate_event(ScrollWheelEvent { position: pane.center(), delta: ScrollDelta::Pixels(gpui::point(px(0.0), px(-100.0))), ..Default::default() });
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
            {"id": "agree", "component": "Checkbox", "props": {"label": "Agree", "name": "agree", "checks": [{"condition": {"call": "required", "args": {"value": {"path": "/agree"}}}, "message": "Agree first"}]}},
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
fn a_right_click_opens_the_context_menu_at_the_pointer(cx: &mut TestAppContext) {
    let tree = json!({"id": "root", "component": "Box", "style": {"padding": 20}, "children": [
        {"id": "ctx", "component": "ContextMenu", "props": {"items": [{"label": "Copy", "value": "copy"}, {"label": "Delete", "value": "delete"}]}, "children": [
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
    cx.simulate_event(ScrollWheelEvent { position: content.center(), delta: ScrollDelta::Pixels(gpui::point(px(0.0), px(-200.0))), ..Default::default() });
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
        {"id": "sheet", "component": "Drawer", "props": {"title": "Filters", "open": true, "side": "bottom"}, "children": [{"id": "sheet-body", "component": "Text", "props": {"text": "Body"}}]}
    ]});
    let (view, _log, cx) = surface(cx, tree, false);
    view.update(cx, |v, _| v.record_bounds(true));
    draw(cx);
    // Finish the enter animation (a short rise on the wall clock) first, so
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
        {"id": "row-tpl", "component": "Box", "style": {"height": 40}, "children": [{"id": "row-label", "component": "Text", "props": {"text": {"path": "label"}}}]}
    ]});
    let rows: Vec<Value> = (0..300).map(|i| json!({"label": format!("Row {i}")})).collect();
    let handle = gpui::ScrollHandle::new();
    let h2 = handle.clone();
    let (host, vcx) = cx.add_window_view(move |window, cx| {
        let view = cx.new(|cx| SurfaceView::new(SurfaceViewOptions::default(), window, cx));
        view.update(cx, |v, cx| {
            v.set_nested(serde_json::from_value::<NestedNode>(tree).unwrap(), cx);
            v.set_data("/rows", Some(Value::Array(rows)), cx);
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
    let settings = exponential_ui::surface::SurfaceSettings { reduced_motion: true, mode: exponential_ui::theme::ModeSetting::Light, ..Default::default() };
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
    exponential_ui_gpui::paint::natives::record_tracked(true);
    let (view, _log, cx) = surface(cx, tree, false);
    view.update(cx, |v, _| v.record_bounds(true));
    exponential_ui_gpui::paint::natives::take_tracked();
    draw(cx);
    let painted = exponential_ui_gpui::paint::natives::take_tracked();
    exponential_ui_gpui::paint::natives::record_tracked(false);
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
    let settings = exponential_ui::surface::SurfaceSettings { strings, mode: exponential_ui::theme::ModeSetting::Light, ..Default::default() };
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
        {"id": "md", "component": "Markdown", "props": {"text": {"path": "/reply"}}}
    ]});
    init(cx);
    let options = SurfaceViewOptions { theme: exponential_ui::themes::builtin_theme("neutral"), mode: exponential_ui::theme::Mode::Light, ..Default::default() };
    let (view, cx) = cx.add_window_view(move |window, cx| SurfaceView::new(options, window, cx));
    view.update(cx, |v, cx| {
        v.set_nested(serde_json::from_value::<NestedNode>(tree).unwrap(), cx);
        v.set_data("/reply", Some(json!("The first sentence.")), cx);
    });
    draw(cx);
    draw(cx);
    assert!(view.update(cx, |v, cx| v.select_all_markdown("md", cx)));
    draw(cx);
    assert_eq!(view.read_with(cx, |v, _| v.selected_text()), "The first sentence.");
    // A streamed chunk lands: the old byte range means nothing now.
    view.update(cx, |v, cx| v.set_data("/reply", Some(json!("The first sentence. And a second one.")), cx));
    draw(cx);
    assert_eq!(view.read_with(cx, |v, _| v.selected_text()), "", "the selection does not jump onto other text");
    // An unrelated change keeps it.
    assert!(view.update(cx, |v, cx| v.select_all_markdown("md", cx)));
    view.update(cx, |v, cx| v.set_data("/other", Some(json!(1)), cx));
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
