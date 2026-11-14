//! #EXP-1837 — THE Assistant FAB + widget driven by real input: the FAB
//! press toggles the widget, — minimize closes it and keeps the FAB, ⇱
//! maximize opens the Assistant's own page (where the FAB hides), Esc and
//! an outside click close it.

use gpui::{
    div, px, AppContext as _, Context, Entity, IntoElement, Modifiers, ParentElement as _, Render, Styled as _, VisualTestContext,
    Window,
};
use serde_json::json;
use sync::Store;

use crate::assistant_fab::{AssistantFab, TEST_IDS, WIDGET_TEST_ID};
use crate::navigation::{nav_for_window, resolved_screen, Navigation, Screen};

/// The content card the FAB sits in (the Shell's `relative` content area).
struct Scene {
    fab: Entity<AssistantFab>,
}

impl Render for Scene {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "Scene").entered();
        div().relative().size_full().child(self.fab.clone())
    }
}

fn seed<T: sync::ShapeRow + serde::de::DeserializeOwned>(collection: &Entity<sync::Collection<T>>, rows: serde_json::Value, cx: &mut gpui::App) {
    collection.update(cx, |c, cx| {
        for row in rows.as_array().unwrap() {
            let row: T = serde_json::from_value(row.clone()).unwrap();
            c.seed(row.key(), row);
        }
        cx.notify();
    });
}

struct Harness<'a> {
    nav: Entity<Navigation>,
    cx: &'a mut VisualTestContext,
    _dir: tempfile::TempDir,
}

/// A signed-in viewer (`u1`) in team `t1` whose Assistant is `as1`.
fn harness(cx: &mut gpui::TestAppContext) -> Harness<'_> {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("accounts.json"),
        json!({"accounts": [{"id": "acc", "instanceUrl": "https://example.test", "userId": "u1", "email": "me@example.test", "name": "Me"}]}).to_string(),
    )
    .unwrap();
    let db = dir.path().join("local.sqlite");
    let (_changes, rx) = flume::unbounded::<(String, Vec<String>)>();
    let data = dir.path().to_path_buf();
    cx.update(|cx| {
        gpui_component::init(cx);
        theme::init(cx);
        crate::init(cx);
        cx.set_global(crate::AuthContext { auth: api::AuthStore::load(data.clone()), client: std::sync::Arc::new(api::AuthClient::new()), data_dir: data });
        let store = Store::open(cx, None, None);
        cx.set_global(store);
        Store::global(cx).clone().connect_local("acc", &db, rx, cx).unwrap();
        let c = Store::global(cx).collections().clone();
        seed(&c.teams, json!([{"id": "t1", "name": "One", "slug": "one"}]), cx);
        seed(&c.boards, json!([{"id": "b1", "team_id": "t1", "name": "App", "slug": "app", "prefix": "APP", "sort_order": 1}]), cx);
        seed(&c.agents, json!([{"id": "as1", "team_id": "t1", "name": "Assistant", "role": "assistant", "owner_id": "u1"}]), cx);
    });
    cx.run_until_parked();
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_root, vcx) = cx.add_window_view({
        let slot = slot.clone();
        move |window, cx| {
            let nav = nav_for_window(window, cx);
            nav.update(cx, |nav, _| nav.team_id = Some("t1".into()));
            let fab = cx.new(|cx| AssistantFab::new(nav.clone(), cx));
            *slot.borrow_mut() = Some(nav);
            let scene = cx.new(|_| Scene { fab });
            gpui_component::Root::new(scene, window, cx)
        }
    });
    let nav = slot.borrow().clone().expect("the nav was built");
    let mut h = Harness { nav, cx: vcx, _dir: dir };
    h.settle();
    h
}

impl Harness<'_> {
    fn settle(&mut self) {
        for _ in 0..3 {
            self.cx.update(|window, _| window.refresh());
            self.cx.run_until_parked();
        }
    }

    fn paints(&mut self, selector: &'static str) -> bool {
        self.cx.debug_bounds(selector).is_some()
    }

    fn fab(&mut self) -> bool {
        self.paints(TEST_IDS.0)
    }

    fn widget(&mut self) -> bool {
        self.paints(WIDGET_TEST_ID)
    }

    fn click(&mut self, selector: &'static str) {
        let at = self.cx.debug_bounds(selector).unwrap_or_else(|| panic!("{selector} paints")).center();
        self.cx.simulate_click(at, Modifiers::default());
        self.settle();
    }

    fn screen(&mut self) -> Option<Screen> {
        let nav = self.nav.clone();
        self.cx.update(|_, cx| resolved_screen(&nav, cx))
    }
}

#[gpui::test]
#[cfg_attr(not(debug_assertions), ignore = "debug_bounds reads gpui debug selectors (debug_assertions only)")]
fn the_fab_press_toggles_the_widget(cx: &mut gpui::TestAppContext) {
    let mut h = harness(cx);
    assert!(h.fab(), "the FAB shows on Home");
    assert!(!h.widget());
    h.click(TEST_IDS.0);
    assert!(h.widget(), "a press opens the widget");
    h.click(TEST_IDS.0);
    assert!(!h.widget(), "a second press closes it");
    assert!(h.fab());
}

#[gpui::test]
#[cfg_attr(not(debug_assertions), ignore = "debug_bounds reads gpui debug selectors (debug_assertions only)")]
fn minimize_closes_the_widget_and_keeps_the_fab(cx: &mut gpui::TestAppContext) {
    let mut h = harness(cx);
    h.click(TEST_IDS.0);
    assert!(h.widget());
    h.click(TEST_IDS.2);
    assert!(!h.widget(), "— closes the widget");
    assert!(h.fab(), "the FAB stays");
    assert_eq!(h.screen(), Some(Screen::Home), "no navigation");
}

#[gpui::test]
#[cfg_attr(not(debug_assertions), ignore = "debug_bounds reads gpui debug selectors (debug_assertions only)")]
fn maximize_opens_the_assistant_page_where_the_fab_hides(cx: &mut gpui::TestAppContext) {
    let mut h = harness(cx);
    h.click(TEST_IDS.0);
    assert!(h.widget());
    h.click(TEST_IDS.1);
    assert_eq!(h.screen(), Some(Screen::Agent { agent_id: "as1".into() }), "⇱ opens the Assistant's chat page");
    assert!(!h.widget(), "and closes the widget");
    assert!(!h.fab(), "the FAB hides on the Assistant's own page");
}

#[gpui::test]
#[cfg_attr(not(debug_assertions), ignore = "debug_bounds reads gpui debug selectors (debug_assertions only)")]
fn escape_closes_the_widget(cx: &mut gpui::TestAppContext) {
    let mut h = harness(cx);
    h.click(TEST_IDS.0);
    assert!(h.widget());
    h.cx.simulate_keystrokes("escape");
    h.settle();
    assert!(!h.widget(), "Esc closes the widget");
    assert!(h.fab());
}

#[gpui::test]
#[cfg_attr(not(debug_assertions), ignore = "debug_bounds reads gpui debug selectors (debug_assertions only)")]
fn an_outside_click_closes_the_widget(cx: &mut gpui::TestAppContext) {
    let mut h = harness(cx);
    h.click(TEST_IDS.0);
    assert!(h.widget());
    h.cx.simulate_click(gpui::point(px(10.), px(10.)), Modifiers::default());
    h.settle();
    assert!(!h.widget(), "an outside click closes the widget");
    assert!(h.fab());
}
