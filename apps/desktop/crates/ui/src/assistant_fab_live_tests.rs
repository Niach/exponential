//! #EXP-1837 — THE Assistant FAB + widget driven by real input: the FAB
//! press toggles the widget, — minimize closes it and keeps the FAB, ⇱
//! maximize opens the Assistant's own page (where the FAB hides), Esc and
//! an outside click close it.

use gpui::{
    div, px, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, Modifiers, ParentElement as _, Render, Styled as _, VisualTestContext,
    Window,
};
use serde_json::json;
use sync::Store;

use crate::assistant_fab::{AssistantFab, HEADER_TEST_ID, RESIZE_TEST_IDS, TEST_IDS, WIDGET_TEST_ID};
use crate::navigation::{nav_for_window, resolved_screen, Navigation, Screen};

/// The content card the FAB sits in (the Shell's `relative` content area).
struct Scene {
    fab: Entity<AssistantFab>,
    /// #EXP-1872: a right pane pinned to the card's corner holding THE plan
    /// rail (its Back / Next foot), as the plan tab draws it.
    rail: Option<Entity<crate::markdown::ImageCache>>,
    /// #EXP-1872: THE bulk bar's dock over the pane's foot, a bar too wide
    /// to stay clear of the corner by centring alone.
    bulk: bool,
}

/// The test bulk bar's selector.
const BULK_BAR: &str = "test-bulk-bar";

/// The plan the rail shows: three questions, so its foot reads Next.
const PLAN_ID: &str = "plan1";

impl Render for Scene {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "Scene").entered();
        let rail = self.rail.clone().map(|images| {
            let option = |label: &str| domain::session_tree::QuestionOption {
                label: label.into(),
                description: None,
                attachment_id: None,
            };
            let questions: Vec<domain::session_tree::PlanQuestion> = (1..=3)
                .map(|n| domain::session_tree::PlanQuestion {
                    id: format!("q{n}"),
                    prompt: format!("Question {n}?"),
                    multi: false,
                    custom: false,
                    options: vec![option("One"), option("Two")],
                })
                .collect();
            let facts = crate::plan_card::PlanFacts {
                message_id: PLAN_ID,
                session_id: "s1",
                questions: &questions,
                surface: None,
                decision: None,
                fresh: false,
                answers: None,
                feedback: None,
                surface_data: None,
                on_tab: true,
            };
            let body = crate::plan_card::plan_rail(&facts, true, true, &images, window, cx);
            gpui::div()
                .absolute()
                .top_0()
                .right_0()
                .bottom_0()
                .w(px(domain::plan_page::PLAN_PAGE.layout.rail_width))
                .flex()
                .flex_col()
                .p(px(domain::plan_page::PLAN_PAGE.panel.padding))
                .children(body)
        });
        let bulk = self.bulk.then(|| {
            let bar = div().debug_selector(|| BULK_BAR.into()).w(px(700.)).h(px(40.)).into_any_element();
            crate::issue_list::bulk_bar_dock("test-bulk-dock", bar, window, cx)
        });
        div().relative().size_full().children(rail).children(bulk).child(self.fab.clone())
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
    harness_with(cx, false, false)
}

/// [`harness`] with THE plan rail pinned to the card's right edge.
fn harness_with(cx: &mut gpui::TestAppContext, rail: bool, bulk: bool) -> Harness<'_> {
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
            let rail = rail.then(|| cx.new(|_| crate::markdown::ImageCache::new(None)));
            let scene = cx.new(|_| Scene { fab, rail, bulk });
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

#[gpui::test]
#[cfg_attr(not(debug_assertions), ignore = "debug_bounds reads gpui debug selectors (debug_assertions only)")]
fn nothing_hides_under_the_fab(cx: &mut gpui::TestAppContext) {
    use domain::agent_settings::ASSISTANT_FAB;
    // #EXP-1860: while the FAB shows, a page's scroll content ends with a
    // spacer the corner tall and THE Dock band keeps a column that runs into
    // the circle `clearance` in; one above the circle (the widget's own
    // chat) keeps its gutter. On the Assistant's own page: nothing.
    let mut h = harness(cx);
    let circle = h.cx.debug_bounds(TEST_IDS.0).expect("the FAB paints");
    let gutter = px(20.);
    // A column that runs to the card's edge (`inset` right of the circle).
    let under = gpui::Bounds::new(
        gpui::point(px(0.), circle.top() - px(40.)),
        gpui::size(circle.right() + px(ASSISTANT_FAB.inset), px(100.)),
    );
    let above = gpui::Bounds::new(gpui::point(px(0.), px(0.)), gpui::size(circle.right(), circle.top() - px(8.)));
    let (end, lane_under, lane_above) = h.cx.update(|window, cx| {
        (
            crate::assistant_fab::page_end_height(window, cx),
            crate::assistant_fab::lane(Some(under), gutter, window, cx),
            crate::assistant_fab::lane(Some(above), gutter, window, cx),
        )
    });
    assert_eq!(end, px(ASSISTANT_FAB.clearance), "the page end reserves the corner");
    assert_eq!(lane_under, px(ASSISTANT_FAB.clearance), "a composer beside the FAB, not under it");
    assert_eq!(lane_above, gutter, "the widget's chat keeps its gutter");

    h.cx.update(|window, cx| crate::navigation::navigate(window, cx, Screen::Agent { agent_id: "as1".into() }));
    h.settle();
    assert!(!h.fab());
    let (end, lane_under) = h.cx.update(|window, cx| {
        (crate::assistant_fab::page_end_height(window, cx), crate::assistant_fab::lane(Some(under), gutter, window, cx))
    });
    assert_eq!(end, px(0.), "no FAB, no spacer");
    assert_eq!(lane_under, gutter, "no FAB, no lane");
}

#[gpui::test]
#[cfg_attr(not(debug_assertions), ignore = "debug_bounds reads gpui debug selectors (debug_assertions only)")]
fn a_pinned_foot_sits_beside_the_fab(cx: &mut gpui::TestAppContext) {
    use domain::agent_settings::ASSISTANT_FAB;
    // #EXP-1872: THE plan rail pinned to the card's right edge: its foot
    // (Back / Next, the decision) is THE `foot`, so Next paints beside the
    // circle, `inset` px clear of it, never under it.
    let mut h = harness_with(cx, true, false);
    let next = format!("{}-{PLAN_ID}", domain::plan_card::test_ids().step_next);
    let next: &'static str = Box::leak(next.into_boxed_str());
    let circle = h.cx.debug_bounds(TEST_IDS.0).expect("the FAB paints");
    let button = h.cx.debug_bounds(next).expect("Next paints");
    assert!(button.bottom() > circle.top(), "the foot sits at the card's foot: {button:?} {circle:?}");
    assert!(
        button.right() + px(ASSISTANT_FAB.inset) <= circle.left(),
        "Next ends `inset` left of the FAB, never under it: {button:?} {circle:?}"
    );
}

#[gpui::test]
#[cfg_attr(not(debug_assertions), ignore = "debug_bounds reads gpui debug selectors (debug_assertions only)")]
fn the_bulk_bar_centres_beside_the_fab(cx: &mut gpui::TestAppContext) {
    use domain::agent_settings::ASSISTANT_FAB;
    // #EXP-1872: THE bulk bar's dock is THE pinned foot: in a pane whose
    // width leaves the centred bar under the circle, it centres in what the
    // corner leaves and ends `inset` px left of the FAB.
    let mut h = harness_with(cx, false, true);
    h.cx.simulate_resize(gpui::size(px(800.), px(600.)));
    h.settle();
    let circle = h.cx.debug_bounds(TEST_IDS.0).expect("the FAB paints");
    let bar = h.cx.debug_bounds(BULK_BAR).expect("the bar paints");
    assert!(bar.bottom() > circle.top(), "the bar floats at the pane's foot: {bar:?} {circle:?}");
    assert!(bar.right() + px(ASSISTANT_FAB.inset) <= circle.left(), "the bar ends beside the FAB: {bar:?} {circle:?}");
}

/// #EXP-1872: every element positioned at a pane's bottom, per file. A new
/// one fails here until it is THE pinned foot (`assistant_fab::foot`, then
/// count it with that reason) or earns an entry saying why it never meets the
/// Assistant FAB. Web twin: packages/ui/src/styles.test.ts.
#[test]
fn nothing_pins_to_a_foot_without_the_fab_foot() {
    const ALLOWED: &[(&str, usize, &str)] = &[
        ("app_csv_import_dialog.rs", 1, "a modal dialog over the FAB"),
        ("app_data_tab.rs", 2, "the table's scrollbar; the record form's modal dialog"),
        ("app_import_dialog.rs", 1, "a modal dialog over the FAB"),
        ("assistant_fab.rs", 1, "the FAB itself"),
        ("chat_list.rs", 1, "a row's selection ring"),
        ("comments.rs", 1, "a clamped comment's fade, inside the scroll"),
        ("controls.rs", 3, "jump-to-bottom (centred above the Dock), a full overlay, a typeahead above its host"),
        ("create_board_dialog.rs", 1, "a modal dialog over the FAB"),
        ("create_team_dialog.rs", 1, "a modal dialog over the FAB"),
        ("dock.rs", 1, "THE Dock band's sheet drop (its column takes `lane`)"),
        ("issue_list.rs", 1, "THE bulk bar's dock = THE pinned foot"),
        ("issue_rail.rs", 1, "a rail guide line"),
        ("join_team.rs", 1, "a modal dialog over the FAB"),
        ("media_tile.rs", 1, "a video tile's duration chip, inside the tile"),
        ("outline.rs", 1, "the active row's bar"),
        ("phone_bar.rs", 2, "the app preview's phone tab bar inside its frame"),
        ("picker/mod.rs", 1, "a picker popover's fade"),
        ("picker/search_list.rs", 1, "a popover's fade"),
        ("plan_page.rs", 2, "the floating panel's overlay (its foot = THE pinned foot), the scrollbar"),
        ("pr_rows.rs", 1, "a tree guide line"),
        ("resize_edge.rs", 1, "a resize handle"),
        ("screens.rs", 1, "a resize handle"),
        ("scroll_pane.rs", 3, "scrollbars"),
        ("session_results.rs", 2, "a tile's scrim + badge, inside the tile"),
        ("shell.rs", 2, "the settings slide strip, a resize handle"),
        ("specimens/entries/phone_board_menu.rs", 2, "a catalogue specimen (no FAB)"),
        ("steer_viewer.rs", 2, "jump-to-bottom (centred), the band's height probe"),
        ("timeline.rs", 1, "a timeline guide line"),
        ("toast.rs", 2, "the toast stack (lifts by `toastLift`)"),
        ("trace.rs", 1, "a trace rail line"),
        ("tree_guides.rs", 1, "a tree guide line"),
    ];
    fn walk(dir: &std::path::Path, root: &std::path::Path, found: &mut std::collections::BTreeMap<String, usize>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, root, found);
                continue;
            }
            let name = path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
            if !name.ends_with(".rs") || name.ends_with("_tests.rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            for line in text.lines() {
                let pinned = line.find(".bottom").is_some_and(|at| {
                    let rest = &line[at + ".bottom".len()..];
                    // `.bottom(x)` / `.bottom_0()`…, never the `bottom()`
                    // getter or a `bottom_left` / `bottom_right` corner.
                    let corner = rest.starts_with("_left") || rest.starts_with("_right");
                    !rest.starts_with("()") && !corner && (rest.starts_with('(') || rest.starts_with('_'))
                });
                if pinned {
                    *found.entry(name.clone()).or_default() += 1;
                }
            }
        }
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut found = std::collections::BTreeMap::new();
    walk(&root, &root, &mut found);
    let allowed: std::collections::BTreeMap<String, usize> = ALLOWED.iter().map(|(file, count, _)| (file.to_string(), *count)).collect();
    assert_eq!(found, allowed, "a new element pinned to a pane's bottom: THE pinned foot (`assistant_fab::foot`) or an entry with its reason");
}

#[gpui::test]
#[cfg_attr(not(debug_assertions), ignore = "debug_bounds reads gpui debug selectors (debug_assertions only)")]
fn the_widget_sizes_off_the_card_and_resizes_from_its_edges(cx: &mut gpui::TestAppContext) {
    use domain::agent_settings::{assistant_widget_size, AssistantWidgetSize};
    // #EXP-1876 (`assistant.fab.widget`): the widget takes the responsive
    // size of the card it floats in; a drag of its top edge grows it upward,
    // of its left edge leftward, of the corner both (anchored to the FAB),
    // the size is remembered, and a double-click on the header resets it.
    let mut h = harness(cx);
    let card = h.cx.update(|window, _| window.viewport_size());
    let card = AssistantWidgetSize { width: f32::from(card.width), height: f32::from(card.height) };
    h.click(TEST_IDS.0);
    let size = |h: &mut Harness<'_>| {
        let b = h.cx.debug_bounds(WIDGET_TEST_ID).expect("the widget paints");
        (f32::from(b.size.width).round(), f32::from(b.size.height).round(), f32::from(b.right()), f32::from(b.bottom()))
    };
    let default = assistant_widget_size(card, None);
    let (w0, h0, right, bottom) = size(&mut h);
    assert_eq!((w0, h0), (default.width, default.height), "the responsive default");
    let mut drag = |h: &mut Harness<'_>, id: &'static str, dx: f32, dy: f32| {
        let b = h.cx.debug_bounds(id).unwrap_or_else(|| panic!("{id} paints"));
        let from = b.center();
        let to = gpui::point(from.x + px(dx), from.y + px(dy));
        h.cx.simulate_mouse_down(from, gpui::MouseButton::Left, Modifiers::default());
        h.settle();
        h.cx.simulate_mouse_move(to, Some(gpui::MouseButton::Left), Modifiers::default());
        h.settle();
        h.cx.simulate_mouse_up(to, gpui::MouseButton::Left, Modifiers::default());
        h.settle();
    };
    drag(&mut h, RESIZE_TEST_IDS.0, 0., -40.);
    let (w1, h1, r1, b1) = size(&mut h);
    let grown = assistant_widget_size(card, Some(AssistantWidgetSize { width: w0, height: h0 + 40. }));
    assert_eq!((w1, h1), (grown.width, grown.height), "the top edge drags the height");
    assert_eq!((r1, b1), (right, bottom), "anchored to the FAB");
    drag(&mut h, RESIZE_TEST_IDS.1, -30., 0.);
    let (w2, h2, _, _) = size(&mut h);
    let wider = assistant_widget_size(card, Some(AssistantWidgetSize { width: w1 + 30., height: h1 }));
    assert_eq!((w2, h2), (wider.width, wider.height), "the left edge drags the width");
    assert_eq!(
        crate::ui_prefs::assistant_widget_size("u1"),
        Some((w2, h2)),
        "the size is remembered for the member on this device"
    );
    let header = h.cx.debug_bounds(HEADER_TEST_ID).expect("the header paints").center();
    h.cx.simulate_event(gpui::MouseDownEvent {
        position: header,
        button: gpui::MouseButton::Left,
        modifiers: Modifiers::default(),
        click_count: 2,
        first_mouse: false,
    });
    h.cx.simulate_event(gpui::MouseUpEvent { position: header, button: gpui::MouseButton::Left, modifiers: Modifiers::default(), click_count: 2 });
    h.settle();
    let (w3, h3, _, _) = size(&mut h);
    assert_eq!((w3, h3), (default.width, default.height), "a header double-click resets the size");
    assert_eq!(crate::ui_prefs::assistant_widget_size("u1"), None);
}
