//! #EDE-92: the FACTORY start menu's keyboard in a headless gpui window.
//! The shell marks one row `selected` in `/shell/startMenu/nav` (#EDE-258); the
//! document draws it with the Row's selected state and tells the shell when
//! Enter is pressed in the search box (`ede_start_menu {activate}`: the
//! shell opens the selected row). Tab leaves the search box for the controls.

use std::cell::RefCell;
use std::rc::Rc;

use eui_gpui::host::{FunctionCallEvent, HostPlugin, InputEvent};
use gpui::{App, TestAppContext, VisualTestContext};
use serde_json::{json, Value};
use uicore_data::Feed;
use uicore_gpui::{Region, Surfaces, WindowedHost};

#[derive(Clone, Default)]
struct Plugin {
    surfaces: Rc<RefCell<Option<Surfaces>>>,
    /// The host functions the document called, with their args.
    called: Rc<RefCell<Vec<(String, Value)>>>,
    /// The search box's text as the host heard it.
    typed: Rc<RefCell<Vec<String>>>,
}

impl HostPlugin for Plugin {
    fn on_function_call(&self, event: &FunctionCallEvent, _cx: &mut App) {
        self.called.borrow_mut().push((event.name.clone(), event.args.clone()));
    }

    fn on_input(&self, event: &InputEvent, _cx: &mut App) {
        if event.path.as_deref() == Some("/shell/startMenu/query") {
            self.typed.borrow_mut().push(event.value.as_str().unwrap_or_default().to_string());
        }
    }
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
}

/// `/shell` while `term` is searched (#EDE-258: `/shell/startMenu/nav`,
/// uicore shell.json `startMenuNav`): three apps and an issue, the row
/// `selected` marked by the shell.
fn row(id: &str, title: &str, heading: Option<&str>, selected: bool) -> Value {
    json!({"id": id, "kind": "app", "title": title, "subtitle": "A terminal", "icon": "nav-apps", "image": null, "depth": 0, "count": 0, "tone": null, "identifier": null, "heading": heading, "target": {"app": id}, "selected": selected})
}

fn nav_feed(query: &str, rows: Vec<Value>) -> Feed {
    let mut feed = Feed::new();
    feed.set(
        "/shell",
        json!({
            "startMenu": {"open": true, "query": query, "favourites": [], "assistant": null, "nav": {"category": "team:t1", "assistant": false, "categories": [{"id": "team:t1", "kind": "team", "title": "Feedback", "icon": "ui-team", "image": null, "count": 0, "tone": null, "separator": true, "selected": true}], "rows": rows}},
        }),
    );
    feed.set("/user", json!({"name": "Ada", "initials": "A"}));
    feed
}

fn shell_feed(selected: Option<usize>) -> Feed {
    let apps = [("app:gnome-terminal", "Terminal"), ("app:uxterm", "UXTerm"), ("app:xterm", "XTerm")];
    let mut rows: Vec<Value> = apps.iter().enumerate().map(|(i, (id, name))| row(id, name, (i == 0).then_some("Apps"), selected == Some(i))).collect();
    rows.push(json!({"id": "issue:i1", "kind": "issue", "title": "The terminal", "subtitle": "Feedback · Desktop", "icon": "status-backlog", "image": null, "depth": 0, "count": 0, "tone": null, "identifier": "EDE-1", "heading": "Issues", "target": {"issue": "EDE-1", "source": "team", "sourceId": "t1"}, "selected": selected == Some(3)}));
    nav_feed("term", rows)
}

/// The factory start menu in a window of its own (the popup header needs
/// the taskbar; everything under `root` is the factory's).
fn start_menu(feed: &Feed, cx: &mut TestAppContext) -> (Plugin, Region, VisualTestContext) {
    start_menu_with(feed, false, cx)
}

fn unscroll(n: &mut Value) {
    if n.get("id").and_then(Value::as_str) == Some("rows_scroll") {
        if let Some(node) = n.as_object_mut() {
            node.insert("style".into(), json!({"overflowX": "hidden", "overflowY": "hidden"}));
        }
    }
    match n {
        Value::Object(m) => m.values_mut().for_each(unscroll),
        Value::Array(a) => a.iter_mut().for_each(unscroll),
        _ => {}
    }
}

/// `unscrolled`: the results' ScrollArea is no scroll container at all (its
/// authored style overflows hidden on both axes), as before #EDE-92 when a
/// missing direction expanded to hidden; since #EXP-1939 a missing direction
/// scrolls vertically, so the case hides the overflow by hand.
fn start_menu_with(feed: &Feed, unscrolled: bool, cx: &mut TestAppContext) -> (Plugin, Region, VisualTestContext) {
    cx.update(gpui_component::init);
    let mut factory = uicore_docs::doc("start-menu").expect("the factory start menu").json();
    if unscrolled {
        unscroll(&mut factory["root"]);
    }
    let source = json!({
        "surface": {"layer": "top", "anchor": ["bottom", "left"], "size": {"width": 860, "height": 620}, "keyboard": "on-demand", "functions": factory["surface"]["functions"]},
        "root": factory["root"],
        "data": factory["data"],
    });
    let doc = ede_ui::validate_document("start-menu", &source).expect("the document validates");
    let plugin = Plugin::default();
    let for_surface = plugin.clone();
    let surfaces = Surfaces::new(Box::new(WindowedHost), eui_core::themes::builtin_theme("exponential").expect("the theme"), Rc::new(move |_| Rc::new(for_surface.clone()) as Rc<dyn HostPlugin>), Rc::new(|_, _| {}), "test");
    *plugin.surfaces.borrow_mut() = Some(surfaces);
    let opened = cx.update(|cx| plugin.surfaces.borrow_mut().as_mut().unwrap().open_layer(&doc, feed, cx));
    assert!(opened, "the start menu opened");
    let region = plugin.surfaces.borrow().as_ref().unwrap().region("start-menu").cloned().expect("the start menu's region");
    let mut vcx = VisualTestContext::from_window(region.handle.into(), cx);
    draw(&mut vcx);
    // As the shell opens it: the keyboard is in the search box.
    cx.update(|cx| plugin.surfaces.borrow().as_ref().unwrap().focus_component("start-menu", "search", cx));
    draw(&mut vcx);
    (plugin, region, vcx)
}

/// The template instance a placed node id belongs to: the row's key.
fn key_of(id: &str) -> Option<String> {
    ["app:", "issue:"].iter().find_map(|kind| id.find(kind).map(|at| id[at..].split(['.', '/']).next().unwrap_or_default().to_string()))
}

fn is_row(id: &str) -> bool {
    id.contains("row_item") && key_of(id).is_some()
}

/// The result rows drawn in the Row's selected state, by key (`app:<id>`, `issue:<id>`).
fn selected_rows(region: &Region, cx: &mut VisualTestContext) -> Vec<String> {
    region.view.read_with(cx, |v, _| {
        let mut out: Vec<String> = v
            .placed_nodes()
            .iter()
            .filter(|n| is_row(&n.id) && n.accessibility.as_ref().and_then(|a| a.get("selected")) == Some(&json!(true)))
            .filter_map(|n| key_of(&n.id))
            .collect();
        out.dedup();
        out
    })
}

fn focused_id(region: &Region, cx: &mut VisualTestContext) -> Option<String> {
    region.view.read_with(cx, |v, _| v.focused().and_then(|i| v.placed_nodes().get(i as usize)).map(|n| n.id.clone()))
}

#[gpui::test]
fn the_selected_result_is_drawn_selected(cx: &mut TestAppContext) {
    let (plugin, region, mut vcx) = start_menu(&shell_feed(Some(0)), cx);
    assert_eq!(selected_rows(&region, &mut vcx), ["app:gnome-terminal"]);
    // Down in the shell: the mark moves, the same rows redraw.
    cx.update(|cx| plugin.surfaces.borrow().as_ref().unwrap().feed_all(&shell_feed(Some(1)), cx));
    draw(&mut vcx);
    assert_eq!(selected_rows(&region, &mut vcx), ["app:uxterm"]);
    // The issues follow the apps.
    cx.update(|cx| plugin.surfaces.borrow().as_ref().unwrap().feed_all(&shell_feed(Some(3)), cx));
    draw(&mut vcx);
    assert_eq!(selected_rows(&region, &mut vcx), ["issue:i1"]);
    // No mark (no text in the search): no row is selected, as before.
    cx.update(|cx| plugin.surfaces.borrow().as_ref().unwrap().feed_all(&shell_feed(None), cx));
    draw(&mut vcx);
    assert_eq!(selected_rows(&region, &mut vcx), Vec::<String>::new());
}

#[gpui::test]
fn enter_in_the_search_box_asks_the_shell_to_run_the_selected_row(cx: &mut TestAppContext) {
    let (plugin, region, mut vcx) = start_menu(&shell_feed(Some(0)), cx);
    assert_eq!(focused_id(&region, &mut vcx).as_deref(), Some("search.field"));
    region.view.update_in(&mut vcx, |v, window, cx| assert!(v.type_into("search.field", "i", window, cx)));
    vcx.simulate_keystrokes("enter");
    draw(&mut vcx);
    // The text reaches the shell before the Enter does: it runs the row of this query.
    assert_eq!(plugin.typed.borrow().last().map(String::as_str), Some("termi"));
    assert_eq!(*plugin.called.borrow(), vec![("ede_start_menu".to_string(), json!({"activate": true}))]);
    assert_eq!(focused_id(&region, &mut vcx).as_deref(), Some("search.field"), "the keyboard stays in the search box");
}

/// Tab leaves the search box for the menu's controls, where Enter presses
/// the focused one (never the selection's Enter).
#[gpui::test]
fn tab_leaves_the_search_box_and_enter_presses_the_focused_control(cx: &mut TestAppContext) {
    let (plugin, region, mut vcx) = start_menu(&shell_feed(Some(0)), cx);
    vcx.simulate_keystrokes("tab");
    draw(&mut vcx);
    let focused = focused_id(&region, &mut vcx);
    assert!(focused.as_deref().is_some_and(|id| id != "search.field"), "{focused:?}");
    vcx.simulate_keystrokes("enter");
    draw(&mut vcx);
    let called = plugin.called.borrow().clone();
    assert!(called.iter().all(|c| c.1.get("activate").is_none()), "the control's own press: {called:?}");
}

/// `/shell` while `e` is searched: 40 apps and an issue (the groups get
/// their headings, "Apps" above the first row), row `selected` highlighted.
fn long_feed(selected: usize) -> Feed {
    let rows: Vec<Value> = (0..40).map(|i| row(&format!("app:app{i}"), &format!("App {i}"), (i == 0).then_some("Apps"), i == selected)).collect();
    nav_feed("e", rows)
}

type Span = (f32, f32);

fn span(region: &Region, id: &str, cx: &VisualTestContext) -> Option<Span> {
    region.view.read_with(cx, |v, _| v.painted_bounds(id).map(|b| (f32::from(b.origin.y), f32::from(b.origin.y + b.size.height))))
}

/// What the person sees of the results: the viewport, the app rows drawn
/// inside it (by index) and the highlighted row's place.
fn shown(region: &Region, cx: &VisualTestContext) -> (Span, Vec<usize>, Option<Span>) {
    let viewport = span(region, "rows_scroll", cx).expect("the results are drawn");
    let inside = |r: Span| r.0 >= viewport.0 - 0.5 && r.1 <= viewport.1 + 0.5;
    let ids: Vec<String> = region.view.read_with(cx, |v, _| v.placed_nodes().iter().filter(|n| is_row(&n.id)).map(|n| n.id.clone()).collect());
    let id_of = |i: usize| ids.iter().find(|id| key_of(id).as_deref() == Some(format!("app:app{i}").as_str())).cloned();
    let rows: Vec<usize> = (0..40).filter(|i| id_of(*i).and_then(|id| span(region, &id, cx)).is_some_and(inside)).collect();
    let selected = region.view.read_with(cx, |v, _| v.placed_nodes().iter().find(|n| is_row(&n.id) && n.accessibility.as_ref().and_then(|a| a.get("selected")) == Some(&json!(true))).map(|n| n.id.clone()));
    (viewport, rows, selected.and_then(|id| span(region, &id, cx)))
}

/// Down `steps` times as the shell does it: the mark moves, then the reveal.
fn press_down(plugin: &Plugin, steps: usize, vcx: &mut VisualTestContext, cx: &mut TestAppContext) -> Vec<bool> {
    let mut revealed = Vec::new();
    for i in 1..=steps {
        cx.update(|cx| plugin.surfaces.borrow().as_ref().unwrap().feed_all(&long_feed(i), cx));
        draw(vcx);
        revealed.push(cx.update(|cx| plugin.surfaces.borrow().as_ref().unwrap().reveal_row("start-menu", "rows_scroll", "rows", i, cx)));
        draw(vcx);
    }
    revealed
}

/// The VM's long list (#EDE-92 B5): 40 results, 24 Downs. The highlighted
/// row is inside the results' viewport, every row there is drawn (none
/// blank), and Enter runs the row drawn highlighted.
#[gpui::test]
fn forty_results_and_twenty_four_downs_keep_the_highlight_in_view(cx: &mut TestAppContext) {
    let (plugin, region, mut vcx) = start_menu(&long_feed(0), cx);
    region.view.update(&mut vcx, |v, _| v.record_bounds(true));
    draw(&mut vcx);
    assert!(press_down(&plugin, 24, &mut vcx, cx).iter().all(|r| *r), "the results scroll: every reveal ran");
    let (viewport, rows, selected) = shown(&region, &vcx);
    let selected = selected.expect("the highlighted row is drawn");
    assert!(selected.0 >= viewport.0 - 0.5 && selected.1 <= viewport.1 + 0.5, "the highlight is in view: {selected:?} in {viewport:?}");
    assert!(rows.contains(&24), "row 24 is the one shown: {rows:?}");
    let full = ((viewport.1 - viewport.0) / 50.0).floor() as usize;
    assert!(rows.len() + 1 >= full, "the viewport is full of drawn rows, none blank: {rows:?} in {viewport:?}");
    assert!(rows.windows(2).all(|w| w[1] == w[0] + 1), "no gap between the drawn rows: {rows:?}");
    // Enter: the shell runs the row the feed marks, the one drawn highlighted.
    vcx.simulate_keystrokes("enter");
    draw(&mut vcx);
    assert_eq!(plugin.called.borrow().last().map(|c| c.0.as_str()), Some("ede_start_menu"));
}

/// A reveal on results that cannot scroll does nothing: the rows stay
/// drawn (the VM drew the first rows blank, then all of them).
#[gpui::test]
fn a_reveal_on_results_that_cannot_scroll_does_nothing(cx: &mut TestAppContext) {
    let (plugin, region, mut vcx) = start_menu_with(&long_feed(0), true, cx);
    region.view.update(&mut vcx, |v, _| v.record_bounds(true));
    draw(&mut vcx);
    let (_, before, _) = shown(&region, &vcx);
    assert!(before.starts_with(&[0, 1, 2, 3, 4, 5, 6]), "{before:?}");
    assert!(press_down(&plugin, 24, &mut vcx, cx).iter().all(|r| !*r), "no reveal ran");
    let (_, after, _) = shown(&region, &vcx);
    assert_eq!(after, before, "every row is still drawn where it was");
}

/// The "Apps" heading above the first row.
fn heading_span(region: &Region, cx: &VisualTestContext) -> Option<Span> {
    let id = region.view.read_with(cx, |v, _| v.placed_nodes().iter().find(|n| n.id.contains("row_heading") && n.id.contains("app:app0")).map(|n| n.id.clone()))?;
    span(region, &id, cx)
}

/// The results' scroll offset (px from the top of the "Apps" heading).
fn offset(region: &Region, cx: &VisualTestContext) -> f32 {
    region.view.read_with(cx, |v, _| v.scroll_extent("rows_scroll").expect("the results scroll").0)
}

/// Up `steps` times from row `from`, as the shell does it: the mark moves,
/// then the reveal (the first row = the results' top, heading included).
fn press_up(plugin: &Plugin, from: usize, steps: usize, vcx: &mut VisualTestContext, cx: &mut TestAppContext) {
    for i in (from - steps..from).rev() {
        cx.update(|cx| plugin.surfaces.borrow().as_ref().unwrap().feed_all(&long_feed(i), cx));
        draw(vcx);
        cx.update(|cx| {
            let surfaces = plugin.surfaces.borrow();
            let surfaces = surfaces.as_ref().unwrap();
            if i == 0 {
                surfaces.reveal_top("start-menu", "rows_scroll", cx);
            } else {
                surfaces.reveal_row("start-menu", "rows_scroll", "rows", i, cx);
            }
        });
        draw(vcx);
    }
}

/// #EDE-133: Down to a row already in full view scrolls nothing: the
/// "Apps" heading stays above the first row.
#[gpui::test]
fn down_to_a_row_in_view_keeps_the_heading(cx: &mut TestAppContext) {
    let (plugin, region, mut vcx) = start_menu(&long_feed(0), cx);
    region.view.update(&mut vcx, |v, _| v.record_bounds(true));
    draw(&mut vcx);
    assert_eq!(offset(&region, &vcx), 0.0);
    press_down(&plugin, 1, &mut vcx, cx);
    assert_eq!(offset(&region, &vcx), 0.0, "no scroll for a row in view");
    let viewport = span(&region, "rows_scroll", &vcx).unwrap();
    let heading = heading_span(&region, &vcx).expect("the heading is drawn");
    assert!(heading.0 >= viewport.0 - 0.5, "the heading is in view: {heading:?} in {viewport:?}");
}

/// #EDE-133: Down ×24 then Up ×24: back on the first row, the results are
/// at their very top, heading included.
#[gpui::test]
fn back_up_to_the_first_row_shows_the_top(cx: &mut TestAppContext) {
    let (plugin, region, mut vcx) = start_menu(&long_feed(0), cx);
    region.view.update(&mut vcx, |v, _| v.record_bounds(true));
    draw(&mut vcx);
    press_down(&plugin, 24, &mut vcx, cx);
    assert!(offset(&region, &vcx) > 0.0, "the list scrolled down");
    press_up(&plugin, 24, 24, &mut vcx, cx);
    assert_eq!(offset(&region, &vcx), 0.0, "back at the top");
    let viewport = span(&region, "rows_scroll", &vcx).unwrap();
    let heading = heading_span(&region, &vcx).expect("the heading is drawn");
    assert!(heading.0 >= viewport.0 - 0.5, "the heading is in view: {heading:?} in {viewport:?}");
    let (_, rows, selected) = shown(&region, &vcx);
    assert_eq!(rows.first(), Some(&0), "{rows:?}");
    assert!(selected.is_some());
}

/// #EDE-133: Up from row 24 to row 10 scrolls only as far as the
/// highlighted row needs (its top at the viewport's top).
#[gpui::test]
fn up_scrolls_only_as_far_as_the_row_needs(cx: &mut TestAppContext) {
    let (plugin, region, mut vcx) = start_menu(&long_feed(0), cx);
    region.view.update(&mut vcx, |v, _| v.record_bounds(true));
    draw(&mut vcx);
    press_down(&plugin, 24, &mut vcx, cx);
    press_up(&plugin, 24, 14, &mut vcx, cx);
    let (viewport, _, selected) = shown(&region, &vcx);
    let selected = selected.expect("the highlighted row is drawn");
    assert!((selected.0 - viewport.0).abs() < 0.5, "row 10 at the top: {selected:?} in {viewport:?}");
}
