//! #EXP-1725: the IDE's shell regions drawn HEADLESS with the real text
//! system and the real rows: the pictures of a region (no window on any
//! display) and the frame times its move is judged by. Run with
//! `EXP_SHELL_SHOTS=<dir> cargo test -p ui --release -- --ignored shell_ --nocapture`.
use std::{borrow::Cow, path::PathBuf, sync::Arc};

use gpui::{px, size, AppContext as _, AssetSource, Entity, HeadlessAppContext, SharedString, Window};
use serde_json::json;

use uicore_model::ide::{IdeActiveInput, IdeAgentInput, IdeAppInput, IdeBoardInput, IdeDot, IdePinInput, IdeSidebarInput};

use crate::navigation::nav_for_window;
use crate::sidebar::RailView;
use crate::transcript_bench::{draw, percentile};

const SIDEBAR: (f32, f32) = (272., 900.);

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

fn headless() -> HeadlessAppContext {
    let platform = gpui_platform::current_platform(true);
    let mut cx = HeadlessAppContext::with_platform(platform.text_system(), Arc::new(Assets), gpui_platform::current_headless_renderer);
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
    seed(&c.teams, json!([{"id": "t1", "name": "Exponential", "slug": "exponential"}]), cx);
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

/// Every row kind the sidebar draws: counts, dots, selection, a hover gear.
fn sample() -> IdeSidebarInput {
    let agent = |id: &str, name: &str, needs_you, busy| IdeAgentInput { id: id.into(), name: name.into(), icon: "settings-agents".into(), needs_you, busy };
    let board = |id: &str, name: &str| IdeBoardInput { id: id.into(), name: name.into(), icon: format!("ide:board:{id}") };
    let app = |id: &str, name: &str, icon: &str| IdeAppInput { id: id.into(), slug: id.into(), name: name.into(), icon: icon.into() };
    IdeSidebarInput {
        screen: "board:b1".into(),
        needs_you: 5,
        drafts: 2,
        threads_dot: Some(IdeDot::Amber),
        owner: true,
        pins: vec![IdePinInput { id: "i1".into(), identifier: "APP-1".into(), title: "Crash on launch".into() }],
        agents: vec![agent("a1", "Triage", true, false), agent("a2", "Release", false, true), agent("a3", "Designer", false, false)],
        apps: vec![app("x1", "CRM", "ui-file"), app("x2", "Leave planner", "ui-file")],
        boards: vec![board("b1", "Mobile App"), board("b2", "Website"), board("b3", "Infrastructure")],
        active: vec![IdeActiveInput { id: "r1".into(), title: "Fix the login redirect".into(), icon: "nav-threads".into(), session_ids: vec!["r1".into()], unread: true }],
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
        crate::ui_prefs::set_rail(true);
        cx.update(|cx| cx.refresh_windows());
        cx.update(|cx| {
            let rail = handle.entity(cx).expect("the rail");
            let region = rail.read(cx).region();
            region.update(cx, |region, cx| region.show(sample(), None, cx));
        });
        for _ in 0..3 {
            draw(window, &mut cx);
            cx.run_until_parked();
        }
        let shot = cx.capture_screenshot(window).expect("the frame");
        shot.save(std::path::Path::new(&dir).join("ide-rail.png")).expect("the PNG");
        crate::ui_prefs::set_rail(false);
        let wide = open(&mut cx, size(px(1100.), px(760.)));
        let wide_any: gpui::AnyWindowHandle = wide.into();
        let region = cx.update(|cx| wide.entity(cx).expect("the rail").read(cx).region());
        cx.update(|cx| region.update(cx, |region, cx| region.show(sample(), None, cx)));
        for _ in 0..3 {
            draw(wide_any, &mut cx);
            cx.run_until_parked();
        }
        cx.update_window(wide_any, |_, window, cx| region.update(cx, |region, cx| region.open_menu(window, cx))).expect("the window");
        for _ in 0..4 {
            draw(wide_any, &mut cx);
            cx.run_until_parked();
        }
        let shot = cx.capture_screenshot(wide_any).expect("the frame");
        shot.save(std::path::Path::new(&dir).join("ide-start-menu.png")).expect("the PNG");
    }
    let mut times: Vec<f64> = (0..120).map(|_| draw(window, &mut cx)).collect();
    times.sort_by(f64::total_cmp);
    eprintln!("shell sidebar: median {:.2} ms, p95 {:.2} ms", percentile(&times, 0.5), percentile(&times, 0.95));
    // `Store::open` holds its handles past gpui's exit-time leak check.
    std::mem::forget(cx);
}
