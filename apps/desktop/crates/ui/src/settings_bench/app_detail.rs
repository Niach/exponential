//! #EXP-1629: the app editor's Settings tab (`settings::app_detail` and the
//! `app_mcp` groups), in a window tall enough for every group at once.
use super::*;
use crate::settings::app_detail::{AppDetailPane, AppShot};
use serde_json::json;

const WINDOW: (f32, f32) = (760., 2240.);

/// Where the pointer goes before the picture.
#[derive(Clone, Copy)]
enum Pointer {
    Rest,
    Hover((f32, f32)),
    /// A left click (the gpui pane's pictures of the open pickers).
    #[allow(dead_code)]
    Click((f32, f32)),
    /// The document's press on a node (the eUI pane's click).
    Press(&'static str),
}

fn frames(window: gpui::AnyWindowHandle, cx: &mut gpui::HeadlessAppContext, settle: bool) {
    for _ in 0..6 {
        draw(window, cx);
        cx.run_until_parked();
    }
    if settle {
        std::thread::sleep(std::time::Duration::from_millis(600));
        for _ in 0..3 {
            draw(window, cx);
            cx.run_until_parked();
        }
    }
}

fn mouse(window: gpui::AnyWindowHandle, cx: &mut gpui::HeadlessAppContext, event: gpui::PlatformInput) {
    cx.update_window(window, |_, window, cx| {
        window.dispatch_event(event, cx);
    })
    .expect("a pointer event");
}

/// `EXP_APP_POINT=x,y` overrides the state's pointer position.
fn point(default: (f32, f32)) -> gpui::Point<gpui::Pixels> {
    let (x, y) = std::env::var("EXP_APP_POINT").ok().and_then(|v| v.split_once(',').and_then(|(x, y)| Some((x.parse().ok()?, y.parse().ok()?)))).unwrap_or(default);
    gpui::point(px(x), px(y))
}

fn shoot(name: &str, shot: AppShot, pointer: Pointer) {
    let Some(dir) = std::env::var_os("EXP_SETTINGS_SHOTS") else { return };
    let mut cx = headless();
    cx.update(|cx| {
        crate::init(cx);
        let store = sync::Store::open(cx, None, None);
        cx.set_global(store);
    });
    let mut frame = None;
    let handle = cx
        .open_window(size(px(WINDOW.0), px(WINDOW.1)), |window: &mut Window, cx| {
            let pane = cx.new(|cx| AppDetailPane::for_shots(window, cx));
            let view = cx.new(|_| Frame { pane: pane.clone().into() });
            frame = Some(pane);
            cx.new(|cx| gpui_component::Root::new(view, window, cx))
        })
        .expect("a headless window");
    let window: gpui::AnyWindowHandle = handle.into();
    let frame = frame.expect("the frame");
    cx.update(|cx| frame.update(cx, |pane, cx| pane.show(shot, cx)));
    frames(window, &mut cx, true);
    match pointer {
        Pointer::Rest => {}
        Pointer::Hover(at) => {
            let position = point(at);
            mouse(window, &mut cx, gpui::PlatformInput::MouseMove(gpui::MouseMoveEvent { position, pressed_button: None, modifiers: Default::default() }));
            frames(window, &mut cx, true);
        }
        Pointer::Press(id) => {
            cx.update_window(window, |_, window, cx| frame.update(cx, |pane, cx| pane.press_for_shots(id, window, cx))).expect("the press");
            frames(window, &mut cx, true);
            cx.update(|cx| frame.update(cx, |pane, cx| pane.press_for_shots_settle(cx)));
            frames(window, &mut cx, true);
        }
        Pointer::Click(at) => {
            let position = point(at);
            mouse(window, &mut cx, gpui::PlatformInput::MouseMove(gpui::MouseMoveEvent { position, pressed_button: None, modifiers: Default::default() }));
            mouse(window, &mut cx, gpui::PlatformInput::MouseDown(gpui::MouseDownEvent { button: gpui::MouseButton::Left, position, modifiers: Default::default(), click_count: 1, first_mouse: false }));
            frames(window, &mut cx, false);
            mouse(window, &mut cx, gpui::PlatformInput::MouseUp(gpui::MouseUpEvent { button: gpui::MouseButton::Left, position, modifiers: Default::default(), click_count: 1 }));
            frames(window, &mut cx, true);
            // Park the pointer away so no cell paints its hover.
            let away = gpui::point(px(WINDOW.0 - 4.), px(WINDOW.1 - 4.));
            mouse(window, &mut cx, gpui::PlatformInput::MouseMove(gpui::MouseMoveEvent { position: away, pressed_button: None, modifiers: Default::default() }));
            frames(window, &mut cx, true);
        }
    }
    let shot = cx.capture_screenshot(window).expect("the frame");
    shot.save(std::path::Path::new(&dir).join(format!("{name}.png"))).expect("the PNG");
    if let Some(before) = std::env::var_os("EXP_SETTINGS_BEFORE") {
        let before = image::open(std::path::Path::new(&before).join(format!("{name}.png"))).expect("the earlier picture").to_rgba8();
        let after = image::RgbaImage::from_raw(shot.width(), shot.height(), shot.as_raw().to_vec()).expect("the frame's pixels");
        assert_eq!(before.dimensions(), after.dimensions());
        let mut diff = after.clone();
        let (mut count, mut bounds, mut worst) = (0u32, (u32::MAX, u32::MAX, 0u32, 0u32), 0u8);
        for (x, y, pixel) in diff.enumerate_pixels_mut() {
            let a = before.get_pixel(x, y);
            if a == pixel {
                pixel.0 = [pixel.0[0] / 3, pixel.0[1] / 3, pixel.0[2] / 3, 255];
                continue;
            }
            worst = worst.max((0..3).map(|c| a.0[c].abs_diff(pixel.0[c])).max().unwrap_or(0));
            count += 1;
            bounds = (bounds.0.min(x), bounds.1.min(y), bounds.2.max(x), bounds.3.max(y));
            pixel.0 = [255, 0, 0, 255];
        }
        eprintln!("{name}: {count} pixels differ (worst level {worst}), within {bounds:?}");
        diff.save(std::path::Path::new(&dir).join(format!("{name}.diff.png"))).expect("the diff");
    }
    std::mem::forget(cx);
}

fn row(package: bool, head: i64) -> domain::rows::AppRow {
    let package = package.then(|| {
        json!({
            "functions": ["exponential_issues_update_status", "exponential_comments_create", "exponential_issues_delete"],
            "records": {"lead": {"policy": {"read": "author", "update": "author", "delete": "owners"}}, "visit": {}},
        })
    });
    serde_json::from_value(json!({
        "id": "app-crm", "team_id": "team-acme", "slug": "field-crm", "name": "Field CRM", "icon": "rocket", "color": "#3b82f6",
        "description": "Tracks leads and follow-ups for the field team.",
        "package": package, "head_version": head, "published_version": head,
        "granted_tools": ["exponential_comments_create"],
    }))
    .expect("an app row")
}

fn mcp(owner: bool) -> api::app_access::AppToolsOut {
    serde_json::from_value(json!({
        "canToggle": owner,
        "tools": [
            {"name": "list_leads", "title": "List leads", "access": "read", "enabled": true},
            {"name": "log_visit", "title": "Log a visit", "access": "write", "enabled": true},
            {"name": "purge_stale", "title": "Purge stale leads", "access": "write", "enabled": false},
        ],
        "upstreams": [
            {"name": "hubspot", "serverId": null, "auth": "oauth"},
            {"name": "github", "serverId": "s-gh", "serverName": "github", "auth": "oauth", "state": "connected"},
            {"name": "crm", "serverId": "s-sf", "serverName": "salesforce", "auth": "oauth", "state": "disconnected"},
            {"name": "calendar", "serverId": "s-cal", "serverName": "calendar", "auth": "oauth", "state": "reconnect"},
        ],
        "servers": if owner { json!([{"id": "s-cal", "name": "calendar"}, {"id": "s-gh", "name": "github"}, {"id": "s-sf", "name": "salesforce"}]) } else { json!([]) },
        "automations": [
            {"name": "nightly-digest", "on": {"schedule": {"cron": "0 6 * * *", "tz": "Europe/Vienna"}}, "tool": "digest", "sponsorName": "Ada Lovelace", "lastStatus": "ok"},
            {"name": "on-new-lead", "on": {"record": {"kind": "lead", "op": "create"}}, "tool": "welcome", "lastStatus": "error", "droppedRuns": 3},
            {"name": "intake", "on": {"webhook": {"name": "intake"}}, "tool": "intake", "paused": "sponsor_left"},
        ],
        "hooks": [
            {"name": "intake", "url": "https://acme.exponential.at/api/apps/hooks/app-crm/intake", "hasSecret": true},
            {"name": "billing", "url": "https://acme.exponential.at/api/apps/hooks/app-crm/billing", "hasSecret": false},
        ],
        "shares": [
            {"name": "contact-form", "mode": "submit", "kind": "form", "state": "on", "url": "https://acme.exponential.at/s/7f3k2"},
            {"name": "lead-board", "mode": "view", "kind": "view", "state": "off"},
        ],
    }))
    .expect("the app's MCP side")
}

fn owner() -> AppShot {
    AppShot {
        row: row(true, 3),
        manage: true,
        grant: true,
        records: 12,
        records_max: Some(1000),
        mcp: Some(mcp(true)),
        tool_busy: None,
        mcp_busy: None,
        upstream_busy: None,
        hook_busy: None,
        share_busy: None,
        hook_secrets: vec![("intake".into(), "whsec_9a8b7c6d5e4f3a2b1c0d9e8f7a6b5c4d".into())],
    }
}

#[test]
#[ignore]
fn settings_app_detail_owner() {
    shoot("app-detail-owner", owner(), Pointer::Rest);
}

#[test]
#[ignore]
fn settings_app_detail_member() {
    let shot = AppShot { manage: false, grant: false, records_max: None, mcp: Some(mcp(false)), hook_secrets: Vec::new(), ..owner() };
    shoot("app-detail-member", shot, Pointer::Rest);
}

#[test]
#[ignore]
fn settings_app_detail_empty() {
    let shot = AppShot { row: row(false, 0), records: 0, mcp: Some(api::app_access::AppToolsOut::default()), hook_secrets: Vec::new(), ..owner() };
    shoot("app-detail-empty", shot, Pointer::Rest);
}

#[test]
#[ignore]
fn settings_app_detail_busy() {
    let shot = AppShot {
        tool_busy: Some("exponential_issues_update_status".into()),
        mcp_busy: Some("s-sf".into()),
        upstream_busy: Some("crm".into()),
        hook_busy: Some("billing".into()),
        share_busy: Some("contact-form".into()),
        ..owner()
    };
    shoot("app-detail-busy", shot, Pointer::Rest);
}

/// The first share row's ⋯ (window points), read off `app-detail-owner`.
const SHARE_MENU: (f32, f32) = (588., 670.);
/// The identity row's icon and colour pickers.
#[allow(dead_code)]
const ICON_PICKER: (f32, f32) = (56., 52.);
#[allow(dead_code)]
const COLOR_PICKER: (f32, f32) = (96., 52.);

#[test]
#[ignore]
fn settings_app_detail_share_hover() {
    shoot("app-detail-share-hover", owner(), Pointer::Hover(SHARE_MENU));
}

/// #EXP-1629: the first share row's ⋯ open (the genUI Menu).
#[test]
#[ignore]
fn settings_app_detail_share_open() {
    shoot("app-detail-share-open", owner(), Pointer::Press("share_menu_more.contact-form"));
}

#[test]
#[ignore]
fn settings_app_detail_icon_open() {
    // The gpui pane's pictures: `Pointer::Click(ICON_PICKER)`.
    shoot("app-detail-icon-open", owner(), Pointer::Press("icon_picker.trigger"));
}

#[test]
#[ignore]
fn settings_app_detail_color_open() {
    // The gpui pane's pictures: `Pointer::Click(COLOR_PICKER)`.
    shoot("app-detail-color-open", owner(), Pointer::Press("color_picker.trigger"));
}

#[test]
#[ignore]
fn settings_app_detail_account_open() {
    // The IDE pane's picture: a real click on the trigger (its popover never
    // paints headless), so compare the trigger; `app_mcp_twins` judged the popup.
    shoot("app-detail-account-open", owner(), Pointer::Press("account_select.github.trigger"));
}
