//! #EXP-1629: the settings panes drawn HEADLESS with the real text system:
//! the pictures a pane's move onto the native binding is judged by (the same
//! states before and after, pixel for pixel). Run with
//! `EXP_SETTINGS_SHOTS=<dir> cargo test -p ui --release -- --ignored settings_ --nocapture`.
use std::collections::HashMap;

use gpui::{div, prelude::*, px, size, AnyView, Window};
use gpui_component::ActiveTheme as _;

use api::notifications::EmailPrefs;

use crate::coding_flow::CodingHub;
use crate::settings::about::AboutPane;
use crate::settings::sessions::SessionsPane;
use crate::settings::api_keys::{ApiKeysPane, ShotLoad as KeysLoad};
use crate::settings::notifications_prefs::{NotificationsPrefsPane, ShotLoad};
use crate::settings::storage::{ShotLoad as StorageLoad, StoragePane};
use crate::shell_bench::headless;
use crate::transcript_bench::draw;

/// The settings detail column's width at the default window.
const PANE: (f32, f32) = (760., 1240.);

struct Frame {
    pane: AnyView,
}

impl Render for Frame {
    fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div().size_full().bg(cx.theme().background).p(px(24.)).child(self.pane.clone())
    }
}

fn prefs(transport: bool, email: bool, daily: bool) -> EmailPrefs {
    EmailPrefs {
        email_enabled: email,
        type_prefs: HashMap::from([(domain::contract::NOTIFICATION_TYPE_ISSUE_COMMENT.to_string(), false)]),
        digest: Some(if daily { "daily" } else { "off" }.into()),
        digest_hour: 8,
        allow_agent_messages: true,
        transport_configured: transport,
    }
}

fn shoot(name: &str, load: ShotLoad) {
    shoot_pane(name, NotificationsPrefsPane::for_shots, move |pane, cx| pane.show(load, cx));
}

/// One pane in one state: built, shown, drawn and saved (and diffed).
fn shoot_pane<P: Render + 'static>(name: &str, build: impl FnOnce(&mut Window, &mut gpui::Context<P>) -> P + 'static, show: impl FnOnce(&mut P, &mut gpui::Context<P>) + 'static) {
    shoot_pane_settled(name, build, show, None);
}

/// [`shoot_pane`], drawn again after `settle` of wall time (the
/// scrollbar's fade runs on the real clock, so a frame can only be taken
/// after it, never pinned inside it).
fn shoot_pane_settled<P: Render + 'static>(name: &str, build: impl FnOnce(&mut Window, &mut gpui::Context<P>) -> P + 'static, show: impl FnOnce(&mut P, &mut gpui::Context<P>) + 'static, settle: Option<std::time::Duration>) {
    let Some(dir) = std::env::var_os("EXP_SETTINGS_SHOTS") else { return };
    let mut cx = headless();
    cx.update(|cx| {
        crate::init(cx);
        let store = sync::Store::open(cx, None, None);
        cx.set_global(store);
        let hub = CodingHub::global(cx);
        hub.update(cx, |hub, _| hub.settings.os_notifications = true);
    });
    // Under gpui-component's Root, as in the app: its rem (the theme's font
    // size) and Inter are what the IDE's rows are measured in.
    let mut frame = None;
    let handle = cx
        .open_window(size(px(PANE.0), px(PANE.1)), |window: &mut Window, cx| {
            let pane = cx.new(|cx| build(window, cx));
            let view = cx.new(|_| Frame { pane: pane.clone().into() });
            frame = Some(pane);
            cx.new(|cx| gpui_component::Root::new(view, window, cx))
        })
        .expect("a headless window");
    let window: gpui::AnyWindowHandle = handle.into();
    let frame = frame.expect("the frame");
    cx.update(|cx| frame.update(cx, |pane, cx| show(pane, cx)));
    for _ in 0..6 {
        draw(window, &mut cx);
        cx.run_until_parked();
    }
    if let Some(settle) = settle {
        std::thread::sleep(settle);
        for _ in 0..3 {
            draw(window, &mut cx);
            cx.run_until_parked();
        }
    }
    let shot = cx.capture_screenshot(window).expect("the frame");
    shot.save(std::path::Path::new(&dir).join(format!("{name}.png"))).expect("the PNG");
    // `EXP_SETTINGS_BEFORE=<dir>`: the same state's earlier picture; the
    // differing pixels are counted and painted red over a dimmed copy.
    if let Some(before) = std::env::var_os("EXP_SETTINGS_BEFORE") {
        let before = image::open(std::path::Path::new(&before).join(format!("{name}.png"))).expect("the earlier picture").to_rgba8();
        let after = image::RgbaImage::from_raw(shot.width(), shot.height(), shot.as_raw().to_vec()).expect("the frame's pixels");
        assert_eq!(before.dimensions(), after.dimensions());
        let mut diff = after.clone();
        let (mut count, mut bounds) = (0u32, (u32::MAX, u32::MAX, 0u32, 0u32));
        for (x, y, pixel) in diff.enumerate_pixels_mut() {
            let a = before.get_pixel(x, y);
            if a == pixel {
                pixel.0 = [pixel.0[0] / 3, pixel.0[1] / 3, pixel.0[2] / 3, 255];
                continue;
            }
            count += 1;
            bounds = (bounds.0.min(x), bounds.1.min(y), bounds.2.max(x), bounds.3.max(y));
            pixel.0 = [255, 0, 0, 255];
        }
        eprintln!("{name}: {count} pixels differ, within {bounds:?}");
        diff.save(std::path::Path::new(&dir).join(format!("{name}.diff.png"))).expect("the diff");
    }
    std::mem::forget(cx);
}

#[test]
#[ignore]
fn settings_notifications_no_transport() {
    shoot("notifications-no-transport", ShotLoad::Ready(prefs(false, false, false)));
}

#[test]
#[ignore]
fn settings_notifications_daily() {
    shoot("notifications-daily", ShotLoad::Ready(prefs(true, true, true)));
}

#[test]
#[ignore]
fn settings_notifications_hourly_email_off() {
    shoot("notifications-email-off", ShotLoad::Ready(prefs(true, false, false)));
}

#[test]
#[ignore]
fn settings_notifications_failed() {
    shoot("notifications-failed", ShotLoad::Failed);
}

/// A Storage list from JSON (the rows the pictures show).
fn storage_list(rows: serde_json::Value, total: i64) -> api::attachments::AttachmentsListForTeamOutput {
    serde_json::from_value(serde_json::json!({"attachments": rows, "totalBytes": total, "draftBytes": 0})).expect("a storage list")
}

fn storage_rows() -> serde_json::Value {
    let row = |id: &str, name: &str, kind: &str, bytes: i64, image: bool, referenced: bool, uploader: Option<&str>| {
        serde_json::json!({"id": id, "issueId": format!("issue-{id}"), "boardId": "board", "uploaderId": uploader, "filename": name, "contentType": kind, "sizeBytes": bytes, "createdAt": "2026-07-01", "isImage": image, "referenced": referenced})
    };
    serde_json::json!([
        row("a1", "screenshot-of-the-settings-pane-after-the-move.png", "image/png", 412_877, true, false, None),
        row("a2", "design-notes.pdf", "application/pdf", 1_048_576, false, true, Some("user-x")),
        row("a3", "walkthrough.mp4", "video/mp4", 23_456_789, false, true, None),
        row("a4", "archive.zip", "application/zip", 2_048, false, false, None),
    ])
}

fn storage(name: &str, load: impl FnOnce() -> StorageLoad + 'static) {
    shoot_pane(name, StoragePane::for_shots, move |pane, cx| pane.show(load(), cx));
}

#[test]
#[ignore]
fn settings_storage_rows() {
    storage("storage-rows", || StorageLoad::Ready(storage_list(storage_rows(), 24_920_290), None));
}

#[test]
#[ignore]
fn settings_storage_meter() {
    storage("storage-meter", || {
        let plan = serde_json::from_value(serde_json::json!({"plan": "pro", "limits": {"storageMb": 1024.0}, "usage": {"storageMb": 23.8}})).expect("a plan");
        StorageLoad::Ready(storage_list(storage_rows(), 24_920_290), Some(plan))
    });
}

#[test]
#[ignore]
fn settings_storage_empty() {
    storage("storage-empty", || StorageLoad::Ready(storage_list(serde_json::json!([]), 0), None));
}

#[test]
#[ignore]
fn settings_storage_failed() {
    storage("storage-failed", || StorageLoad::Failed);
}

#[test]
#[ignore]
fn settings_about() {
    shoot_pane("about", AboutPane::for_shots, |_, _| {});
}

#[test]
#[ignore]
fn settings_about_end() {
    shoot_pane("about-end", AboutPane::for_shots, |pane, cx| pane.show_end(cx));
}

/// The thumb faded out (gpui-component: 2 s idle, then a 1 s fade).
#[test]
#[ignore]
fn settings_about_end_idle() {
    shoot_pane_settled("about-end-idle", AboutPane::for_shots, |pane, cx| pane.show_end(cx), Some(std::time::Duration::from_millis(3500)));
}

#[test]
#[ignore]
fn settings_sessions_unlimited() {
    shoot_pane("sessions-unlimited", SessionsPane::for_shots, |pane, cx| pane.show(None, cx));
}

#[test]
#[ignore]
fn settings_sessions_30_days() {
    shoot_pane("sessions-30-days", SessionsPane::for_shots, |pane, cx| pane.show(Some(30), cx));
}

fn api_keys_rows() -> Vec<api::users::PersonalKeyMeta> {
    serde_json::from_value(serde_json::json!([
        {"id": "k1", "name": "Device: studio-mac", "start": "expu_d1", "createdAt": "2026-07-01", "lastRequest": "2026-07-03"},
        {"id": "k2", "name": "Device: build-box", "start": "expu_d2", "createdAt": "2026-06-12"},
        {"id": "k3", "name": "CI deploys", "start": "expu_ab", "createdAt": "2026-05-20", "lastRequest": "2026-07-02"},
        {"id": "k4", "name": "MCP client", "start": "expu_cd", "createdAt": "2026-06-30", "scope": {"teams": [{"id": "t", "name": "Acme"}], "boards": []}},
    ]))
    .expect("api key rows")
}

fn api_keys(name: &str, load: impl FnOnce() -> KeysLoad + 'static, minted: bool) {
    shoot_pane(name, ApiKeysPane::for_shots, move |pane, cx| {
        let minted = minted.then(|| serde_json::from_value(serde_json::json!({"key": "expu_ab12cd34ef56gh78ij90kl12mn34op56qr78st90", "id": "k5"})).expect("a minted key"));
        pane.show(load(), minted, cx)
    });
}

#[test]
#[ignore]
fn settings_api_keys_rows() {
    api_keys("api-keys-rows", || KeysLoad::Ready(api_keys_rows(), Some("k1".into())), false);
}

#[test]
#[ignore]
fn settings_api_keys_minted() {
    api_keys("api-keys-minted", || KeysLoad::Ready(api_keys_rows(), Some("k1".into())), true);
}

#[test]
#[ignore]
fn settings_api_keys_empty() {
    api_keys("api-keys-empty", || KeysLoad::Ready(Vec::new(), None), false);
}

#[test]
#[ignore]
fn settings_api_keys_failed() {
    api_keys("api-keys-failed", || KeysLoad::Failed, false);
}
