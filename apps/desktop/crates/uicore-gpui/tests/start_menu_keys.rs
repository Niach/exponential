//! #EDE-92: the FACTORY start menu's keyboard in a headless gpui window.
//! The shell marks one result row `selected` in `/shell/startMenu`; the
//! document draws it with the Row's selected state and tells the shell when
//! Enter is pressed in the search box (`ede_start_menu {activate}`: the
//! shell runs the selected row). Tab still leaves the search box for the
//! footer, where Enter presses the button it is on.

use std::cell::RefCell;
use std::rc::Rc;

use eui_core::surface::SurfaceCommand;
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

/// `/shell` while `term` is searched: three apps and an issue, the row
/// `selected` (an index over the apps, then the issues) marked by the shell.
fn shell_feed(selected: Option<usize>) -> Feed {
    let apps = [("gnome-terminal", "Terminal"), ("uxterm", "UXTerm"), ("xterm", "XTerm")];
    let apps: Vec<Value> = apps.iter().enumerate().map(|(i, (id, name))| json!({"id": id, "name": name, "comment": "A terminal", "icon": "", "category": "administration", "selected": selected == Some(i)})).collect();
    let issues = json!([{"id": "i1", "identifier": "EDE-1", "title": "The terminal", "status": "backlog", "icon": "status-backlog", "source": "team", "sourceId": "t1", "context": "Feedback · Desktop", "selected": selected == Some(3)}]);
    let mut feed = Feed::new();
    feed.set(
        "/shell",
        json!({
            "switcher": {"current": {"kind": "local", "id": "local", "name": "Local"}, "sections": [], "connection": {"onLocal": true, "connected": false}},
            "startMenu": {"open": true, "query": "term", "category": "all", "categories": [{"id": "all", "label": "All applications", "icon": "nav-apps", "selected": true, "count": 3}], "apps": apps, "issues": issues, "favourites": []},
        }),
    );
    feed.set("/user", json!({"name": "Ada", "initials": "A"}));
    feed
}

/// The factory start menu in a window of its own (the popup header needs
/// the taskbar; everything under `root` is the factory's).
fn start_menu(feed: &Feed, cx: &mut TestAppContext) -> (Plugin, Region, VisualTestContext) {
    start_menu_with(feed, false, cx)
}

fn results_scroll(n: &mut Value) {
    if n.get("id").and_then(Value::as_str) == Some("apps_scroll") {
        n["props"] = json!({"direction": "vertical"});
    }
    match n {
        Value::Object(m) => m.values_mut().for_each(results_scroll),
        Value::Array(a) => a.iter_mut().for_each(results_scroll),
        _ => {}
    }
}

/// `scrolling`: the results' ScrollArea names its direction. The factory's
/// names none, and this renderer then lays it out with `overflow: hidden`
/// (the macro maps a missing `direction` to hidden), so its results do not
/// scroll at all, by wheel or by command: found with #EDE-92, not changed
/// by it.
fn start_menu_with(feed: &Feed, scrolling: bool, cx: &mut TestAppContext) -> (Plugin, Region, VisualTestContext) {
    cx.update(gpui_component::init);
    let mut factory = uicore_docs::doc("start-menu").expect("the factory start menu").json();
    if scrolling {
        results_scroll(&mut factory["root"]);
    }
    let source = json!({
        "surface": {"layer": "top", "anchor": ["bottom", "left"], "size": {"width": 760, "height": 620}, "keyboard": "on-demand", "functions": factory["surface"]["functions"]},
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

/// The result rows drawn in the Row's selected state, by id (`app.<id>`, `issue.<id>`).
fn selected_rows(region: &Region, cx: &mut VisualTestContext) -> Vec<String> {
    region.view.read_with(cx, |v, _| {
        v.placed_nodes()
            .iter()
            .filter(|n| (n.id.starts_with("app.") || n.id.starts_with("issue.")) && n.accessibility.as_ref().and_then(|a| a.get("selected")) == Some(&json!(true)))
            .map(|n| n.id.clone())
            .collect()
    })
}

fn focused_id(region: &Region, cx: &mut VisualTestContext) -> Option<String> {
    region.view.read_with(cx, |v, _| v.focused().and_then(|i| v.placed_nodes().get(i as usize)).map(|n| n.id.clone()))
}

#[gpui::test]
fn the_selected_result_is_drawn_selected(cx: &mut TestAppContext) {
    let (plugin, region, mut vcx) = start_menu(&shell_feed(Some(0)), cx);
    assert_eq!(selected_rows(&region, &mut vcx), ["app.gnome-terminal"]);
    // Down in the shell: the mark moves, the same rows redraw.
    cx.update(|cx| plugin.surfaces.borrow().as_ref().unwrap().feed_all(&shell_feed(Some(1)), cx));
    draw(&mut vcx);
    assert_eq!(selected_rows(&region, &mut vcx), ["app.uxterm"]);
    // The issues follow the apps.
    cx.update(|cx| plugin.surfaces.borrow().as_ref().unwrap().feed_all(&shell_feed(Some(3)), cx));
    draw(&mut vcx);
    assert_eq!(selected_rows(&region, &mut vcx), ["issue.i1"]);
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

#[gpui::test]
fn tab_reaches_the_footer_and_enter_presses_its_button(cx: &mut TestAppContext) {
    let (plugin, region, mut vcx) = start_menu(&shell_feed(Some(0)), cx);
    vcx.simulate_keystrokes("tab");
    draw(&mut vcx);
    assert_eq!(focused_id(&region, &mut vcx).as_deref(), Some("whats_new"));
    vcx.simulate_keystrokes("enter");
    draw(&mut vcx);
    let called = plugin.called.borrow().clone();
    assert_eq!(called.len(), 1, "the button's press alone: {called:?}");
    assert_eq!(called[0].0, "ede_launch");
    assert_eq!(called[0].1["app"], "at.exponential");
}

/// Down past the fold: the shell's command brings the selected row into
/// view, where the results scroll (see [`start_menu_with`]).
#[gpui::test]
fn a_selected_row_below_the_fold_scrolls_into_view(cx: &mut TestAppContext) {
    let mut feed = shell_feed(None);
    let apps: Vec<Value> = (0..40).map(|i| json!({"id": format!("app{i}"), "name": format!("App {i}"), "comment": "An app", "icon": "", "category": "other", "selected": i == 30})).collect();
    let mut shell = feed.get("/shell").cloned().unwrap();
    shell["startMenu"]["apps"] = Value::Array(apps);
    feed.set("/shell", shell);
    let (plugin, region, mut vcx) = start_menu_with(&feed, true, cx);
    let offset = |vcx: &VisualTestContext| region.view.read_with(vcx, |v, _| v.surface().scroll_offset("apps_scroll").1);
    assert_eq!(offset(&vcx), 0.0);
    cx.update(|cx| plugin.surfaces.borrow().as_ref().unwrap().command("start-menu", SurfaceCommand::ScrollToIndex { id: "apps".into(), index: 30, align: None }, cx));
    draw(&mut vcx);
    // Row 30 of 50 px rows ends near 1550; the results' viewport is 525 high.
    let at = offset(&vcx);
    assert!(at > 900.0 && at < 1300.0, "the row is the last one in view: {at}");
}
