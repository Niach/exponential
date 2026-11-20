//! #EXP-1725: the IDE's shell regions drawn HEADLESS with the real text
//! system and the real rows: the pictures of a region (no window on any
//! display) and the frame times its move is judged by. Run with
//! `EXP_SHELL_SHOTS=<dir> cargo test -p ui --release -- --ignored shell_ --nocapture`.
use std::{borrow::Cow, path::PathBuf, sync::Arc};

use gpui::{px, size, AppContext as _, AssetSource, Entity, HeadlessAppContext, SharedString, Window};
use serde_json::json;

use uicore_model::ide::{IdeActiveInput, IdeCount, IdeDeviceInput, IdeItemInput, IdeLayoutEntry, IdePrInput, IdeRepoInput, IdeSidebarInput, IdeTeamInput, IdeTone};

use crate::navigation::nav_for_window;
use crate::sidebar::RailView;
use crate::transcript_bench::{draw, percentile};

const SIDEBAR: (f32, f32) = (272., 900.);

#[test]
#[ignore]
fn shell_local_desktop() {
    let Some(dir) = std::env::var_os("EXP_SHELL_SHOTS") else { return };
    let mut cx = headless();
    let root = tempfile::tempdir().unwrap();
    let path = root.path().to_path_buf();
    cx.update(|cx| {
        crate::init(cx);
        let store = sync::Store::open(cx, None, None);
        cx.set_global(store);
        crate::local_desktops::install(path, cx);
        assert!(crate::local_desktops::enter_current(cx));
    });
    let handle = open(&mut cx, size(px(SIDEBAR.0), px(SIDEBAR.1)));
    let window: gpui::AnyWindowHandle = handle.into();
    for _ in 0..4 {
        draw(window, &mut cx);
        cx.run_until_parked();
    }
    let shot = cx.capture_screenshot(window).expect("the frame");
    shot.save(std::path::Path::new(&dir).join("ide-local.png")).expect("the PNG");
    std::mem::forget(cx);
    std::mem::forget(root);
}

#[test]
#[ignore]
fn shell_header() {
    let Some(dir) = std::env::var_os("EXP_SHELL_SHOTS") else { return };
    let mut cx = headless();
    cx.update(seeded);
    let handle = cx
        .open_window(size(px(SIDEBAR.0), px(64.)), |window: &mut Window, cx| {
            let nav = nav_for_window(window, cx);
            let _ = &nav;
            crate::shell_regions::HeaderRegion::new(nav, window, cx)
        })
        .expect("a headless window");
    let window: gpui::AnyWindowHandle = handle.into();
    let region = cx.update(|cx| handle.entity(cx).expect("the header"));
    cx.update_window(window, |_, window, cx| {
        let nav = nav_for_window(window, cx);
        let mut live = std::collections::BTreeMap::new();
        live.insert("t2".to_string(), 3usize);
        region.update(cx, |region, cx| region.refresh(&nav, &live, cx));
    })
    .expect("the window");
    for _ in 0..4 {
        draw(window, &mut cx);
        cx.run_until_parked();
    }
    let shot = cx.capture_screenshot(window).expect("the frame");
    shot.save(std::path::Path::new(&dir).join("ide-header.png")).expect("the PNG");
    std::mem::forget(cx);
}

#[test]
#[ignore]
fn shell_local_repositories() {
    let Some(dir) = std::env::var_os("EXP_SHELL_SHOTS") else { return };
    let mut cx = headless();
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("desktops");
    let folder = root.path().join("my-app");
    std::fs::create_dir_all(folder.join(".git")).unwrap();
    std::fs::write(folder.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
    cx.update(|cx| {
        crate::init(cx);
        let store = sync::Store::open(cx, None, None);
        cx.set_global(store);
        crate::local_desktops::install(path, cx);
        assert!(crate::local_desktops::enter_current(cx));
        let trpc = crate::queries::trpc_client(cx).unwrap();
        api::repositories::link_folder(&trpc, folder.to_str().unwrap(), None).unwrap();
    });
    let handle = cx
        .open_window(size(px(720.), px(260.)), |window: &mut Window, cx| {
            let nav = nav_for_window(window, cx);
            cx.new(|cx| crate::settings::repositories::RepositoriesPane::new(nav, window, cx))
        })
        .expect("a headless window");
    let window: gpui::AnyWindowHandle = handle.into();
    for _ in 0..4 {
        draw(window, &mut cx);
        cx.run_until_parked();
    }
    let shot = cx.capture_screenshot(window).expect("the frame");
    shot.save(std::path::Path::new(&dir).join("ide-local-repositories.png")).expect("the PNG");
    std::mem::forget(cx);
    std::mem::forget(root);
}

/// The band the top tab bar sits in: it records the left presses that
/// reach it, as the titlebar's drag latch does.
struct DragBand {
    region: Entity<crate::shell_regions::TabBarRegion>,
    presses: std::rc::Rc<std::cell::Cell<u32>>,
}

impl gpui::Render for DragBand {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl gpui::IntoElement {
        use gpui::{InteractiveElement as _, ParentElement as _, Styled as _};
        let presses = self.presses.clone();
        gpui::div()
            .size_full()
            .on_mouse_down(gpui::MouseButton::Left, move |_, _, _| presses.set(presses.get() + 1))
            .child(gpui::div().w_full().h(px(crate::app_title_bar::WORK_TABS_BAND_H)).child(self.region.clone()))
    }
}

/// The top band drags the window (Danny, desktop 0.14.156: the whole bar
/// was a pointer with an empty tooltip and swallowed the drag). Only the
/// strip's tabs, closes, + and +N are pressable; nothing spans the band
/// (the +N menu's anchor is a zero-width box at its end), and a press on
/// the empty rest reaches the titlebar behind it while one on a tab never
/// does.
#[test]
fn the_tab_band_is_pressable_only_on_its_controls() {
    if !gpu_or_skip("the_tab_band_is_pressable_only_on_its_controls") {
        return;
    }
    let mut cx = headless();
    cx.update(seeded);
    let presses = std::rc::Rc::new(std::cell::Cell::new(0));
    let band_presses = presses.clone();
    let handle = cx
        .open_window(size(px(1100.), px(44.)), move |window: &mut Window, cx| {
            let region = crate::shell_regions::TabBarRegion::new(window, cx);
            cx.new(|_| DragBand { region, presses: band_presses })
        })
        .expect("a headless window");
    let window: gpui::AnyWindowHandle = handle.into();
    let region = cx.update(|cx| handle.update(cx, |band, _, _| band.region.clone()).expect("the band"));
    let press = |cx: &mut HeadlessAppContext, x: f32| {
        cx.update_window(window, |_, window, cx| {
            let position = gpui::point(px(x), px(24.));
            window.dispatch_event(gpui::PlatformInput::MouseMove(gpui::MouseMoveEvent { position, pressed_button: None, modifiers: Default::default() }), cx);
            window.dispatch_event(gpui::PlatformInput::MouseDown(gpui::MouseDownEvent { button: gpui::MouseButton::Left, position, modifiers: Default::default(), click_count: 1, first_mouse: false }), cx);
            window.dispatch_event(gpui::PlatformInput::MouseUp(gpui::MouseUpEvent { button: gpui::MouseButton::Left, position, modifiers: Default::default(), click_count: 1 }), cx);
        })
        .expect("the window");
    };
    for titles in [&["Fix the login redirect", "zsh"][..], &["APP-1 Crash on launch", "Fix the login redirect", "APP-12 Offline queue for issue edits", "zsh", "Mobile App", "Release checklist", "APP-31 Rewrite the image cache", "npm run dev", "One more", "And another"][..]] {
        cx.update(|cx| region.update(cx, |region, cx| region.show(titles, cx)));
        for _ in 0..4 {
            draw(window, &mut cx);
            cx.run_until_parked();
        }
        let (spans, strip_right) = cx.update(|cx| region.read(cx).pressable_spans(cx));
        assert!(strip_right > 0. && strip_right < 1100., "{titles:?}: the strip ends at {strip_right}");
        for (id, left, right) in &spans {
            assert!(id.starts_with("ide_tabs."), "{titles:?}: {id} is pressable");
            assert!(*left >= 0. && *right <= strip_right + 0.5, "{titles:?}: {id} spans {left}..{right} past the strip's end {strip_right}");
        }
        presses.set(0);
        press(&mut cx, 1090.);
        assert_eq!(presses.get(), 1, "{titles:?}: a press on the empty band reaches the titlebar");
        let (_, first_left, first_right) = spans.iter().find(|(id, ..)| id.starts_with("ide_tabs.tab.")).cloned().expect("a tab");
        presses.set(0);
        press(&mut cx, (first_left + first_right) / 2.);
        assert_eq!(presses.get(), 0, "{titles:?}: a press on a tab is the tab's");
    }
    std::mem::forget(cx);
}

/// #EXP-1814: the +N menu opens BELOW the top band and stays inside the
/// window (it used to land above the band, off screen): the strip's popups
/// are placed against the window, not the 44px region.
#[test]
fn the_more_menu_opens_below_the_top_band() {
    if !gpu_or_skip("the_more_menu_opens_below_the_top_band") {
        return;
    }
    let mut cx = headless();
    cx.update(seeded);
    let presses = std::rc::Rc::new(std::cell::Cell::new(0));
    let (window_w, window_h) = (1100., 500.);
    let handle = cx
        .open_window(size(px(window_w), px(window_h)), move |window: &mut Window, cx| {
            let region = crate::shell_regions::TabBarRegion::new(window, cx);
            cx.new(|_| DragBand { region, presses })
        })
        .expect("a headless window");
    let window: gpui::AnyWindowHandle = handle.into();
    let region = cx.update(|cx| handle.update(cx, |band, _, _| band.region.clone()).expect("the band"));
    let titles = ["APP-1 Crash on launch", "Fix the login redirect", "APP-12 Offline queue for issue edits", "zsh", "Mobile App", "Release checklist", "APP-31 Rewrite the image cache", "npm run dev", "One more", "And another"];
    cx.update(|cx| region.update(cx, |region, cx| region.show(&titles, cx)));
    for _ in 0..4 {
        draw(window, &mut cx);
        cx.run_until_parked();
    }
    let (menu, more) = cx.update(|cx| region.read(cx).more_menu_frame(cx));
    assert!(menu.is_none(), "closed until +N is pressed");
    assert!(more.is_some_and(|f| f.w > 0.), "the strip overflows into +N");
    cx.update(|cx| region.update(cx, |region, cx| region.show_more(&titles, 4, cx)));
    for _ in 0..6 {
        draw(window, &mut cx);
        cx.run_until_parked();
    }
    let (menu, _) = cx.update(|cx| region.read(cx).more_menu_frame(cx));
    let menu = menu.expect("+N opens its menu");
    let band = crate::app_title_bar::WORK_TABS_BAND_H;
    assert!(menu.y >= band - 6., "the menu hangs below the band: {menu:?}");
    assert!(menu.h > 0. && menu.y + menu.h <= window_h, "inside the window: {menu:?}");
    assert!(menu.x >= 0. && menu.x + menu.w <= window_w, "inside the window: {menu:?}");
    std::mem::forget(cx);
}

#[test]
#[ignore]
fn shell_tab_bar_frames() {
    let mut cx = headless();
    cx.update(seeded);
    let handle = cx
        .open_window(size(px(1100.), px(44.)), |window: &mut Window, cx| crate::shell_regions::TabBarRegion::new(window, cx))
        .expect("a headless window");
    let window: gpui::AnyWindowHandle = handle.into();
    let region = cx.update(|cx| handle.entity(cx).expect("the tab bar"));
    let titles = ["APP-1 Crash on launch", "Fix the login redirect", "APP-12 Offline queue for issue edits", "zsh", "Mobile App", "Release checklist", "APP-31 Rewrite the image cache", "npm run dev"];
    cx.update(|cx| region.update(cx, |region, cx| region.show(&titles, cx)));
    for _ in 0..4 {
        draw(window, &mut cx);
        cx.run_until_parked();
    }
    if let Some(dir) = std::env::var_os("EXP_SHELL_SHOTS") {
        let shot = cx.capture_screenshot(window).expect("the frame");
        shot.save(std::path::Path::new(&dir).join("ide-tabs.png")).expect("the PNG");
    }
    let mut times: Vec<f64> = (0..120).map(|_| draw(window, &mut cx)).collect();
    times.sort_by(f64::total_cmp);
    eprintln!("shell tab bar: median {:.2} ms, p95 {:.2} ms", percentile(&times, 0.5), percentile(&times, 0.95));
    std::mem::forget(cx);
}

/// The app's assets from the source tree, gpui-component's behind them.
struct Assets;

fn asset_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        match std::fs::read(asset_root().join(path)) {
            Ok(bytes) => Ok(Some(Cow::Owned(bytes))),
            Err(_) => gpui_component_assets::Assets.load(path),
        }
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        gpui_component_assets::Assets.list(path)
    }
}

/// #EXP-1825: the GPU tests that run in `gate` need a headless adapter, and
/// a GitHub-hosted runner (pick-runner's fallback) has none. Without one the
/// test SKIPS with a message, unless `EXP_REQUIRE_GPU=1` (set on the Mint's
/// self-hosted runners), where a missing adapter is a real failure.
pub(crate) fn gpu_or_skip(test: &str) -> bool {
    let error = match gpui_platform::current_headless_renderer() {
        Ok(Some(_)) => return true,
        Ok(None) => "this platform has no headless renderer".to_string(),
        Err(error) => format!("{error:#}"),
    };
    if std::env::var("EXP_REQUIRE_GPU").is_ok_and(|v| v == "1") {
        panic!("{test}: EXP_REQUIRE_GPU=1 but no headless GPU adapter: {error}");
    }
    eprintln!("SKIPPED {test}: no headless GPU adapter ({error}); set EXP_REQUIRE_GPU=1 to fail instead");
    false
}

pub(crate) fn headless() -> HeadlessAppContext {
        let mut cx = HeadlessAppContext::with_platform(crate::test_text_system(), Arc::new(Assets), gpui_platform::current_headless_renderer);
    cx.update(|cx| {
        let fonts = std::fs::read_dir(asset_root().join("fonts")).into_iter().flatten().flatten().map(|f| f.path()).filter(|p| p.extension().is_some_and(|e| e == "ttf")).filter_map(|p| std::fs::read(p).ok()).map(Cow::Owned).collect();
        cx.text_system().add_fonts(fonts).expect("the fonts");
        gpui_component::init(cx);
        theme::init(cx);
    });
    cx
}

fn seed<T: sync::ShapeRow + serde::de::DeserializeOwned>(collection: &Entity<sync::Collection<T>>, rows: serde_json::Value, cx: &mut gpui::App) {
    collection.update(cx, |c, cx| {
        for row in rows.as_array().unwrap() {
            let row: T = serde_json::from_value(row.clone()).expect("a store row");
            c.seed(row.key(), row);
        }
        cx.notify();
    });
}

fn seeded(cx: &mut gpui::App) {
    crate::init(cx);
    let store = sync::Store::open(cx, None, None);
    cx.set_global(store);
    let c = sync::Store::global(cx).collections().clone();
    seed(&c.teams, json!([{"id": "t1", "name": "Exponential", "slug": "exponential"}, {"id": "t2", "name": "Acme", "slug": "acme"}]), cx);
    seed(&c.boards, json!([
        {"id": "b1", "team_id": "t1", "name": "Mobile App", "slug": "mobile-app", "prefix": "APP", "color": "#22c55e", "sort_order": 1},
        {"id": "b2", "team_id": "t1", "name": "Website", "slug": "website", "prefix": "WEB", "color": "#3b82f6", "sort_order": 2},
        {"id": "b3", "team_id": "t1", "name": "Infrastructure", "slug": "infra", "prefix": "INF", "color": "#f97316", "sort_order": 3}
    ]), cx);
    seed(&c.issues, json!([
        {"id": "i1", "board_id": "b1", "number": 1, "identifier": "APP-1", "title": "Crash on launch", "status": "todo"}
    ]), cx);
}

fn open(cx: &mut HeadlessAppContext, bounds: gpui::Size<gpui::Pixels>) -> gpui::WindowHandle<RailView> {
    cx.open_window(bounds, |window: &mut Window, cx| {
        let nav = nav_for_window(window, cx);
        let _ = &nav;
        cx.new(|cx| RailView::new(window, cx))
    })
    .expect("a headless window")
}

/// #FEED-232: chat-list rows wearing THE count badge (`row_badge`):
/// waiting with nothing unread = 1 attention, waiting with replies, plain
/// unread = muted, a busy row = no badge.
struct ChatRowsShot;

impl gpui::Render for ChatRowsShot {
    fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl gpui::IntoElement {
        use domain::chats::{ChatLead, ChatPreview, ChatPreviewKind, ChatRow};
        use gpui::{ParentElement as _, Styled as _};
        use gpui_component::ActiveTheme as _;
        let row = |id: &str, title: &str, lead: ChatLead, kind: ChatPreviewKind, text: &str, unread_count: usize, needs_you: bool, busy: bool| ChatRow {
            session_id: id.into(),
            team_id: "t1".into(),
            title: title.into(),
            device_id: None,
            group: domain::session_tree::ChatsListKey::Recent,
            lead,
            preview: ChatPreview { kind, text: text.into() },
            activity_at: String::new(),
            time: "9:41".into(),
            unread_count,
            needs_you,
            busy,
            word: None,
            waiting: None,
            wall: None,
            dim: false,
        };
        let rows = [
            row("r1", "Fix the login redirect", ChatLead::NeedsYou, ChatPreviewKind::Plan, "Plan: move the redirect into the auth guard", 0, true, false),
            row("r2", "Ship the release notes", ChatLead::NeedsYou, ChatPreviewKind::Question, "Question: which build goes out first?", 3, true, false),
            row("r3", "Dark mode for the settings", ChatLead::Waiting, ChatPreviewKind::Agent, "Done: the settings follow the theme now", 2, false, false),
            row("r4", "Profile the board query", ChatLead::Working, ChatPreviewKind::Working, "Working", 0, false, true),
            row("r5", "Rename the export button", ChatLead::Waiting, ChatPreviewKind::Agent, "Renamed it everywhere", 0, false, false),
        ];
        gpui::div()
            .size_full()
            .bg(cx.theme().background)
            .flex()
            .flex_col()
            .children(rows.iter().map(|row| crate::chat_list::chat_row(row, crate::chat_list::chat_lead_disc(row.lead, None, false, cx), &[], cx)))
    }
}

#[test]
#[ignore]
fn shell_chat_rows() {
    let Some(dir) = std::env::var_os("EXP_SHELL_SHOTS") else { return };
    let mut cx = headless();
    cx.update(seeded);
    let handle = cx.open_window(size(px(560.), px(420.)), |_: &mut Window, cx| cx.new(|_| ChatRowsShot)).expect("a headless window");
    let window: gpui::AnyWindowHandle = handle.into();
    for _ in 0..4 {
        draw(window, &mut cx);
        cx.run_until_parked();
    }
    let shot = cx.capture_screenshot(window).expect("the frame");
    shot.save(std::path::Path::new(&dir).join("chat-rows.png")).expect("the PNG");
    std::mem::forget(cx);
}

/// #EXP-2027: one item of a team's list.
fn item(kind: &str, id: &str, name: &str) -> IdeItemInput {
    IdeItemInput { kind: kind.into(), id: id.into(), name: name.into(), icon: if kind == "agent" { "settings-agents".into() } else if kind == "app" { "ui-file".into() } else if kind == "session" { "nav-chats".into() } else { format!("ide:board:{id}") }, slug: (kind == "app").then(|| id.into()), ..Default::default() }
}

/// #EXP-2027: the owner-approved draft's tree: Devices with three rows,
/// Acme open (a Mobile folder of a board, an agent, an app and a placed
/// session; boards, an agent and an app at its top level), Side project
/// folded with its needs-you count.
fn sample() -> IdeSidebarInput {
    let pr = |id: &str, n: u32, title: &str, subtitle: &str, depth| IdePrInput { id: id.into(), identifier: format!("#{n}"), title: title.into(), subtitle: subtitle.into(), depth, session: None };
    let device = |id: &str, name: &str, online: bool, team_mark: Option<&str>| IdeDeviceInput { id: id.into(), name: name.into(), icon: if team_mark.is_some() { "ui-server".into() } else { "nav-devices".into() }, online, team_mark: team_mark.map(Into::into) };
    IdeSidebarInput {
        screen: "home".into(),
        needs_you: 0,
        drafts: 0,
        chats: IdeCount { count: 1, tone: IdeTone::Attention },
        devices: vec![device("d1", "Alex's MacBook Pro", true, None), device("d2", "Mint workstation", true, None), device("d3", "Acme build server", false, Some("ide:team:t1"))],
        devices_open: true,
        teams: vec![
            IdeTeamInput {
                id: "t1".into(),
                name: "Acme".into(),
                mark: "ui-team".into(),
                open: true,
                owner: true,
                needs_you: 0,
                items: vec![item("board", "b1", "Mobile App"), item("board", "b2", "Launch Marketing"), item("board", "b3", "Product Feedback"), item("agent", "a1", "Triage"), item("agent", "a2", "Release"), item("app", "x1", "CRM"), item("app", "x2", "Release checklist"), item("session", "s1", "Store screenshots")],
                layout: vec![entry("f1", "folder", None, None, Some("Mobile"), 1.), entry("e1", "item", Some("b1"), Some("f1"), None, 1.), entry("e2", "item", Some("a2"), Some("f1"), None, 2.), entry("e3", "item", Some("x2"), Some("f1"), None, 3.), entry("e4", "item", Some("s1"), Some("f1"), None, 4.)],
            },
            IdeTeamInput { id: "t2".into(), name: "Side project".into(), mark: "ui-team".into(), open: false, owner: true, needs_you: 2, items: vec![item("board", "b9", "Site")], layout: Vec::new() },
        ],
        active: vec![IdeActiveInput { id: "r1".into(), title: "Fix the login redirect".into(), icon: "nav-chats".into(), session_ids: vec!["r1".into()], unread: true, team_mark: Some("ui-team".into()) }],
        repos: vec![IdeRepoInput { id: "repo1".into(), name: "engine".into(), prs: vec![pr("p2", 2, "Faster sync", "sync → main", 0), pr("p3", 3, "Sync tests", "sync-tests → sync", 1)] }],
        closed: Vec::new(),
        editing_folder: None,
    }
}

fn entry(id: &str, kind: &str, target: Option<&str>, parent: Option<&str>, name: Option<&str>, sort_order: f64) -> IdeLayoutEntry {
    IdeLayoutEntry { id: id.into(), kind: kind.into(), target_id: target.map(Into::into), parent_id: parent.map(Into::into), name: name.map(Into::into), sort_order }
}

#[test]
#[ignore]
fn shell_sidebar_frames() {
    let mut cx = headless();
    cx.update(seeded);
    let handle = open(&mut cx, size(px(SIDEBAR.0), px(SIDEBAR.1)));
    let window: gpui::AnyWindowHandle = handle.into();
    for _ in 0..3 {
        draw(window, &mut cx);
        cx.run_until_parked();
    }
    if let Some(dir) = std::env::var_os("EXP_SHELL_SHOTS") {
        cx.update(|cx| {
            let rail = handle.entity(cx).expect("the rail");
            let region = rail.read(cx).region();
            region.update(cx, |region, cx| region.show(sample(), Some("b2"), cx));
        });
        for _ in 0..3 {
            draw(window, &mut cx);
            cx.run_until_parked();
        }
        let shot = cx.capture_screenshot(window).expect("the frame");
        shot.save(std::path::Path::new(&dir).join("ide-sidebar.png")).expect("the PNG");
    }
    let mut times: Vec<f64> = (0..120).map(|_| draw(window, &mut cx)).collect();
    times.sort_by(f64::total_cmp);
    eprintln!("shell sidebar: median {:.2} ms, p95 {:.2} ms", percentile(&times, 0.5), percentile(&times, 0.95));
    // `Store::open` holds its handles past gpui's exit-time leak check.
    std::mem::forget(cx);
}

/// The whole IDE window with three tabs open, for the tab placement
/// pictures: run once plain (tabs on top, the standalone IDE) and once with
/// `XDG_CURRENT_DESKTOP=Exponential` (tabs at the bottom, eDE).
/// `EXP_SKIP_ONBOARDING=1` keeps the first-run wizard away.
#[test]
#[ignore]
fn shell_tab_placement() {
    let Some(dir) = std::env::var_os("EXP_SHELL_SHOTS") else { return };
    let mut cx = headless();
    let root = tempfile::tempdir().unwrap();
    let data = root.path().to_path_buf();
    std::fs::write(data.join("accounts.json"), json!({"accounts": [{"id": "acc", "instanceUrl": "https://app.exponential.at", "userId": "u1", "email": "demo@exponential.at", "name": "Demo", "onboardingCompletedAt": "2026-01-01T00:00:00Z"}]}).to_string()).unwrap();
    let (_changes, rx) = flume::unbounded::<(String, Vec<String>)>();
    cx.update(|cx| {
        crate::init(cx);
        let auth = api::AuthStore::load_with_token_store(data.clone(), api::TokenStore::file_only(data.clone()));
        cx.set_global(crate::AuthContext { auth, client: Arc::new(api::AuthClient::new()), data_dir: data.clone() });
        let store = sync::Store::open(cx, None, None);
        cx.set_global(store);
        sync::Store::global(cx).clone().connect_local("acc", &data.join("sync.sqlite"), rx, cx).expect("the store");
        let c = sync::Store::global(cx).collections().clone();
        seed(&c.teams, json!([{"id": "t1", "name": "Exponential", "slug": "exponential"}]), cx);
        seed(&c.boards, json!([
            {"id": "b1", "team_id": "t1", "name": "Mobile App", "slug": "mobile-app", "prefix": "APP", "color": "#22c55e", "sort_order": 1},
            {"id": "b2", "team_id": "t1", "name": "Website", "slug": "website", "prefix": "WEB", "color": "#3b82f6", "sort_order": 2}
        ]), cx);
        seed(&c.issues, json!([
            {"id": "i1", "board_id": "b1", "number": 1, "identifier": "APP-1", "title": "Crash on launch", "status": "todo", "description": "The app closes right after the splash screen on a cold start."},
            {"id": "i2", "board_id": "b1", "number": 12, "identifier": "APP-12", "title": "Offline queue for issue edits", "status": "in_progress"},
            {"id": "i3", "board_id": "b2", "number": 4, "identifier": "WEB-4", "title": "Fix the login redirect", "status": "todo"}
        ]), cx);
    });
    let handle = cx
        .open_window(size(px(1400.), px(860.)), |window: &mut Window, cx| {
            let nav = nav_for_window(window, cx);
            let _ = &nav;
            let shell = cx.new(|cx| crate::Shell::new(0, window, cx));
            cx.new(|cx| gpui_component::Root::new(shell, window, cx))
        })
        .expect("a headless window");
    let window: gpui::AnyWindowHandle = handle.into();
    for issue in ["i2", "i3", "i1"] {
        cx.update_window(window, |_, window, cx| crate::navigation::navigate(window, cx, crate::navigation::Screen::IssueDetail { issue_id: issue.into() })).unwrap();
        for _ in 0..3 {
            draw(window, &mut cx);
            cx.run_until_parked();
        }
    }
    for _ in 0..4 {
        draw(window, &mut cx);
        cx.run_until_parked();
    }
    let name = if crate::local_desktops::available() { "ide-tabs-ede.png" } else { "ide-tabs-top.png" };
    let shot = cx.capture_screenshot(window).expect("the frame");
    shot.save(std::path::Path::new(&dir).join(name)).expect("the PNG");
    std::mem::forget(cx);
    std::mem::forget(root);
}

/// #EXP-1837: the whole IDE window with the viewer's Assistant FAB and a
/// toast: the stack rides `toastLift` above the FAB. `EXP_SKIP_ONBOARDING=1`.
#[test]
#[ignore]
fn shell_toast_above_fab() {
    let Some(dir) = std::env::var_os("EXP_SHELL_SHOTS") else { return };
    let mut cx = headless();
    let root = tempfile::tempdir().unwrap();
    let data = root.path().to_path_buf();
    std::fs::write(data.join("accounts.json"), json!({"accounts": [{"id": "acc", "instanceUrl": "https://app.exponential.at", "userId": "u1", "email": "demo@exponential.at", "name": "Demo", "onboardingCompletedAt": "2026-01-01T00:00:00Z"}]}).to_string()).unwrap();
    let (_changes, rx) = flume::unbounded::<(String, Vec<String>)>();
    cx.update(|cx| {
        crate::init(cx);
        let auth = api::AuthStore::load_with_token_store(data.clone(), api::TokenStore::file_only(data.clone()));
        cx.set_global(crate::AuthContext { auth, client: Arc::new(api::AuthClient::new()), data_dir: data.clone() });
        let store = sync::Store::open(cx, None, None);
        cx.set_global(store);
        sync::Store::global(cx).clone().connect_local("acc", &data.join("sync.sqlite"), rx, cx).expect("the store");
        let c = sync::Store::global(cx).collections().clone();
        seed(&c.teams, json!([{"id": "t1", "name": "Exponential", "slug": "exponential"}]), cx);
        seed(&c.boards, json!([
            {"id": "b1", "team_id": "t1", "name": "Mobile App", "slug": "mobile-app", "prefix": "APP", "color": "#22c55e", "sort_order": 1}
        ]), cx);
        seed(&c.issues, json!([
            {"id": "i1", "board_id": "b1", "number": 1, "identifier": "APP-1", "title": "Crash on launch", "status": "todo", "description": "The app closes right after the splash screen on a cold start."}
        ]), cx);
        seed(&c.agents, json!([{"id": "as1", "team_id": "t1", "name": "Assistant", "role": "assistant", "owner_id": "u1"}]), cx);
    });
    let handle = cx
        .open_window(size(px(1440.), px(900.)), |window: &mut Window, cx| {
            let nav = nav_for_window(window, cx);
            let _ = &nav;
            let shell = cx.new(|cx| crate::Shell::new(0, window, cx));
            cx.new(|cx| gpui_component::Root::new(shell, window, cx))
        })
        .expect("a headless window");
    let window: gpui::AnyWindowHandle = handle.into();
    cx.update_window(window, |_, window, cx| crate::navigation::navigate(window, cx, crate::navigation::Screen::IssueDetail { issue_id: "i1".into() })).unwrap();
    for _ in 0..4 {
        draw(window, &mut cx);
        cx.run_until_parked();
    }
    cx.update_window(window, |_, window, cx| crate::toast::success("Link copied", window, cx)).unwrap();
    // The toast's enter motion: the executor's (simulated) clock and the
    // transitions' wall clock.
    for _ in 0..12 {
        std::thread::sleep(std::time::Duration::from_millis(60));
        cx.advance_clock(std::time::Duration::from_millis(60));
        draw(window, &mut cx);
        cx.run_until_parked();
    }
    let shot = cx.capture_screenshot(window).expect("the frame");
    shot.save(std::path::Path::new(&dir).join("ide-toast-above-fab.png")).expect("the PNG");
    std::mem::forget(cx);
    std::mem::forget(root);
}

/// #EXP-1832: the sidebar folders of the seeded demo (seed-screenshots.ts:
/// board folder Product, app folder Ops tools), pictured at rest, folded,
/// mid-drag (the line = reorder, the highlighted row = drop into) and with a
/// fresh folder's name in place. The drags go through the real List drag core.
#[test]
#[ignore]
fn shell_sidebar_folders() {
    use gpui::{KeyDownEvent, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PlatformInput};
    let Some(dir) = std::env::var_os("EXP_SHELL_SHOTS") else { return };
    let dir = PathBuf::from(dir);
    let mut cx = headless();
    cx.update(seeded);
    let handle = open(&mut cx, size(px(SIDEBAR.0), px(SIDEBAR.1)));
    let window: gpui::AnyWindowHandle = handle.into();
    let region = cx.update(|cx| handle.entity(cx).expect("the rail").read(cx).region());
    let surface = cx.update(|cx| region.read(cx).surface());
    cx.update(|cx| surface.update(cx, |v, _| v.record_bounds(true)));
    let settle = |cx: &mut HeadlessAppContext| {
        for _ in 0..3 {
            draw(window, cx);
            cx.run_until_parked();
        }
    };
    let shoot = |cx: &mut HeadlessAppContext, name: &str| {
        let shot = cx.capture_screenshot(window).expect("the frame");
        shot.save(dir.join(name)).expect("the PNG");
    };
    let show = |cx: &mut HeadlessAppContext, input: IdeSidebarInput| {
        cx.update(|cx| region.update(cx, |region, cx| region.show(input, None, cx)));
        settle(cx);
    };
    let bounds = |cx: &mut HeadlessAppContext, id: &str| cx.update(|cx| surface.read(cx).painted_bounds(id)).unwrap_or_else(|| panic!("{id} painted"));
    let input = |cx: &mut HeadlessAppContext, event: PlatformInput| {
        cx.update_window(window, |_, window, cx| {
            window.dispatch_event(event, cx);
        })
        .expect("the window");
        settle(cx);
    };
    let press = |cx: &mut HeadlessAppContext, at: gpui::Point<gpui::Pixels>| input(cx, PlatformInput::MouseDown(MouseDownEvent { button: MouseButton::Left, position: at, modifiers: Modifiers::default(), click_count: 1, first_mouse: false }));
    let move_to = |cx: &mut HeadlessAppContext, at: gpui::Point<gpui::Pixels>| input(cx, PlatformInput::MouseMove(MouseMoveEvent { position: at, pressed_button: Some(MouseButton::Left), modifiers: Modifiers::default() }));

    show(&mut cx, demo(Vec::new(), None));
    shoot(&mut cx, "folders-rest.png");
    show(&mut cx, demo(vec!["fa".into()], None));
    shoot(&mut cx, "folders-folded.png");

    // CRM lifted to Release checklist's top edge: a line, before it.
    show(&mut cx, demo(Vec::new(), None));
    let (from, to) = (bounds(&mut cx, "ide_item_row.x4"), bounds(&mut cx, "ide_item_row.x3"));
    // Held by its bottom edge: the lifted copy rides above the pointer.
    let grab = gpui::point(from.center().x, from.origin.y + from.size.height - px(2.));
    press(&mut cx, grab);
    move_to(&mut cx, gpui::point(grab.x, grab.y - px(6.)));
    move_to(&mut cx, gpui::point(to.center().x, to.origin.y + px(3.)));
    shoot(&mut cx, "folders-drag-line.png");
    cx.update(|cx| surface.update(cx, |v, cx| v.list_drag_cancel(cx)));
    settle(&mut cx);

    // Leave planner held over Ops desk's middle: the row lights, drop INTO.
    let (from, to) = (bounds(&mut cx, "ide_item_row.x5"), bounds(&mut cx, "ide_item_row.x6"));
    let grab = gpui::point(from.center().x, from.origin.y + from.size.height - px(2.));
    press(&mut cx, grab);
    move_to(&mut cx, gpui::point(grab.x, grab.y + px(6.)));
    move_to(&mut cx, gpui::point(to.center().x, to.origin.y + to.size.height * 0.3));
    shoot(&mut cx, "folders-drag-into.png");
    cx.update(|cx| surface.update(cx, |v, cx| v.list_drag_cancel(cx)));
    settle(&mut cx);

    // A folder's right-click menu: Rename puts its name in place, Esc keeps it.
    let folder = bounds(&mut cx, "ide_folder_row.fa");
    input(&mut cx, PlatformInput::MouseDown(MouseDownEvent { button: MouseButton::Right, position: folder.center(), modifiers: Modifiers::default(), click_count: 1, first_mouse: false }));
    input(&mut cx, PlatformInput::MouseUp(MouseUpEvent { button: MouseButton::Right, position: folder.center(), modifiers: Modifiers::default(), click_count: 1 }));
    assert!(cx.update(|cx| surface.read(cx).layers().iter().any(|l| l.owner == "ide_folder_menu.fa")), "the folder menu opened");
    let rename = bounds(&mut cx, "ide_folder_menu.fa.item.0").center();
    press(&mut cx, rename);
    input(&mut cx, PlatformInput::MouseUp(MouseUpEvent { button: MouseButton::Left, position: rename, modifiers: Modifiers::default(), click_count: 1 }));
    let name = "ide_folder_name.fa.field";
    assert_eq!(cx.update(|cx| surface.read(cx).field_text(name, cx)).as_deref(), Some("Ops tools"), "Rename edits the name in place");
    cx.update_window(window, |_, window, cx| {
        assert!(surface.update(cx, |v, cx| v.focus_field(name, window, cx)));
    })
    .expect("the window");
    settle(&mut cx);
    input(&mut cx, PlatformInput::KeyDown(KeyDownEvent { keystroke: gpui::Keystroke::parse("escape").unwrap(), is_held: false, prefer_character_input: false }));
    assert_eq!(cx.update(|cx| surface.read(cx).field_text(name, cx)), None, "Esc ends the edit");

    // That drop: both rows in a new folder, its name an Input in place.
    let mut after = demo(Vec::new(), None);
    after.teams[0].layout.extend([entry("fn", "folder", None, None, None, 4.), entry("e9", "item", Some("x6"), Some("fn"), None, 1.), entry("e10", "item", Some("x5"), Some("fn"), None, 2.)]);
    show(&mut cx, after);
    cx.update(|cx| region.update(cx, |region, cx| region.show_editing(Some("fn"), cx)));
    settle(&mut cx);
    // The region focuses it on the next frame, which a headless window never
    // requests: focus the same field by hand.
    cx.update_window(window, |_, window, cx| {
        assert!(surface.update(cx, |v, cx| v.focus_field("ide_folder_name.fn.field", window, cx)), "the name Input");
    })
    .expect("the window");
    settle(&mut cx);
    assert_eq!(cx.update(|cx| surface.read(cx).field_text("ide_folder_name.fn.field", cx)).as_deref(), Some("New folder"));
    shoot(&mut cx, "folders-rename.png");
    std::mem::forget(cx);
}

/// The seeded demo's rows (seed-screenshots.ts) as ONE team list.
fn demo(closed: Vec<String>, editing: Option<&str>) -> IdeSidebarInput {
    IdeSidebarInput {
        screen: "board:b1".into(),
        chats: IdeCount { count: 0, tone: IdeTone::Muted },
        teams: vec![IdeTeamInput {
            id: "t1".into(),
            name: "Acme".into(),
            mark: "ui-team".into(),
            open: true,
            owner: true,
            needs_you: 0,
            items: vec![item("board", "b1", "Mobile App"), item("board", "b2", "Launch Marketing"), item("board", "b3", "Product Feedback"), item("agent", "a1", "Triage"), item("app", "x1", "Working hours"), item("app", "x2", "Team dashboard"), item("app", "x3", "Release checklist"), item("app", "x4", "CRM"), item("app", "x5", "Leave planner"), item("app", "x6", "Ops desk")],
            layout: vec![entry("e1", "item", Some("b1"), None, None, 1.), entry("fb", "folder", None, None, Some("Product"), 2.), entry("e2", "item", Some("b2"), Some("fb"), None, 1.), entry("e3", "item", Some("b3"), Some("fb"), None, 2.), entry("fa", "folder", None, None, Some("Ops tools"), 3.), entry("e4", "item", Some("x2"), Some("fa"), None, 1.), entry("e5", "item", Some("x1"), Some("fa"), None, 2.)],
        }],
        closed,
        editing_folder: editing.map(Into::into),
        ..Default::default()
    }
}
