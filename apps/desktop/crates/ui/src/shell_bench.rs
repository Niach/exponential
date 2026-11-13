//! #EXP-1725: the IDE's shell regions drawn HEADLESS with the real text
//! system and the real rows: the pictures of a region (no window on any
//! display) and the frame times its move is judged by. Run with
//! `EXP_SHELL_SHOTS=<dir> cargo test -p ui --release -- --ignored shell_ --nocapture`.
use std::{borrow::Cow, path::PathBuf, sync::Arc};

use gpui::{px, size, AppContext as _, AssetSource, Entity, HeadlessAppContext, SharedString, Window};
use serde_json::json;

use uicore_model::ide::{IdeActiveInput, IdeAgentInput, IdeAppInput, IdeBoardInput, IdeCount, IdePinInput, IdePrInput, IdeRepoInput, IdeSidebarInput, IdeTone};

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
            nav.update(cx, |nav, _| nav.team_id = Some("t1".into()));
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
    seed(&c.pins, json!([
        {"id": "p1", "team_id": "t1", "kind": "issue", "issue_id": "i1", "sort_order": 1}
    ]), cx);
}

fn open(cx: &mut HeadlessAppContext, bounds: gpui::Size<gpui::Pixels>) -> gpui::WindowHandle<RailView> {
    cx.open_window(bounds, |window: &mut Window, cx| {
        let nav = nav_for_window(window, cx);
        nav.update(cx, |nav, _| nav.team_id = Some("t1".into()));
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
            .children(rows.iter().map(|row| crate::chat_list::chat_row(row, crate::chat_list::chat_lead_disc(row.lead, None, false, cx), cx)))
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

/// Every row kind the sidebar draws: counts, selection, a hover gear.
fn sample() -> IdeSidebarInput {
    let agent = |id: &str, name: &str, needs_you, unread| IdeAgentInput { id: id.into(), name: name.into(), icon: "settings-agents".into(), needs_you, unread };
    let board = |id: &str, name: &str| IdeBoardInput { id: id.into(), name: name.into(), icon: format!("ide:board:{id}") };
    let app = |id: &str, name: &str, icon: &str| IdeAppInput { id: id.into(), slug: id.into(), name: name.into(), icon: icon.into() };
    let pr = |id: &str, n: u32, title: &str, subtitle: &str, depth| IdePrInput { id: id.into(), identifier: format!("#{n}"), title: title.into(), subtitle: subtitle.into(), depth };
    IdeSidebarInput {
        screen: "board:b1".into(),
        needs_you: 5,
        drafts: 2,
        chats: IdeCount { count: 3, tone: IdeTone::Attention },
        owner: true,
        pins: vec![IdePinInput { id: "i1".into(), identifier: "APP-1".into(), title: "Crash on launch".into() }],
        agents: vec![agent("a1", "Triage", true, 0), agent("a2", "Release", false, 2), agent("a3", "Designer", false, 0)],
        apps: vec![app("x1", "CRM", "ui-file"), app("x2", "Leave planner", "ui-file")],
        boards: vec![board("b1", "Mobile App"), board("b2", "Website"), board("b3", "Infrastructure")],
        active: vec![IdeActiveInput { id: "r1".into(), title: "Fix the login redirect".into(), icon: "nav-chats".into(), session_ids: vec!["r1".into()], unread: true }],
        repos: vec![IdeRepoInput { id: "repo1".into(), name: "engine".into(), prs: vec![pr("p2", 2, "Faster sync", "sync → main", 0), pr("p3", 3, "Sync tests", "sync-tests → sync", 1)] }],
    }
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
            nav.update(cx, |nav, _| nav.team_id = Some("t1".into()));
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
