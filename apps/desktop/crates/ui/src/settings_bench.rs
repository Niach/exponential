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
use crate::settings::account::{AccountPane, AccountShot};
use crate::settings::sessions::SessionsPane;
use crate::settings::api_keys::{ApiKeysPane, ShotLoad as KeysLoad};
use crate::settings::widget::WidgetPane;
use crate::settings::all_boards::AllBoardsPane;
use crate::settings::sign_in_methods::SignInShot;
use crate::settings::notifications_prefs::{NotificationsPrefsPane, ShotLoad};
use crate::settings::storage::{ShotLoad as StorageLoad, StoragePane};
use crate::settings::voice_prefs::{VoicePrefsPane, VoiceShot};
use crate::settings::prompts::{PromptsPane, PromptsShot};
use crate::settings::branch_row_shots::{EuiBranchRow, IdeBranchRow};
use crate::shell_bench::headless;
use crate::transcript_bench::draw;

/// The settings detail column's width at the default window.
const PANE: (f32, f32) = (760., 1240.);

struct Frame {
    pane: AnyView,
}

impl Render for Frame {
    fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "Frame").entered();
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
    shoot_pane_clicked(name, build, show, settle, &[]);
}

/// [`shoot_pane_settled`] after left clicks at `clicks` (window points),
/// each followed by frames and a short settle (a popup's fade).
fn shoot_pane_clicked<P: Render + 'static>(name: &str, build: impl FnOnce(&mut Window, &mut gpui::Context<P>) -> P + 'static, show: impl FnOnce(&mut P, &mut gpui::Context<P>) + 'static, settle: Option<std::time::Duration>, clicks: &[(f32, f32)]) {
    shoot_pane_acted(name, build, show, settle, clicks, |_: &mut P, _: &mut Window, _: &mut gpui::Context<P>| {});
}

/// [`shoot_pane_clicked`], then `act` on the pane (a press by node id).
fn shoot_pane_acted<P: Render + 'static>(name: &str, build: impl FnOnce(&mut Window, &mut gpui::Context<P>) -> P + 'static, show: impl FnOnce(&mut P, &mut gpui::Context<P>) + 'static, settle: Option<std::time::Duration>, clicks: &[(f32, f32)], act: impl FnOnce(&mut P, &mut Window, &mut gpui::Context<P>) + 'static) {
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
    for &(x, y) in clicks {
        let position = gpui::point(px(x), px(y));
        cx.update_window(window, |_, window, cx| {
            let moved = gpui::MouseMoveEvent { position, pressed_button: None, modifiers: Default::default() };
            window.dispatch_event(gpui::PlatformInput::MouseMove(moved), cx);
            let down = gpui::MouseDownEvent { button: gpui::MouseButton::Left, position, modifiers: Default::default(), click_count: 1, first_mouse: false };
            window.dispatch_event(gpui::PlatformInput::MouseDown(down), cx);
        })
        .expect("a press");
        draw(window, &mut cx);
        cx.run_until_parked();
        cx.update_window(window, |_, window, cx| {
            let up = gpui::MouseUpEvent { button: gpui::MouseButton::Left, position, modifiers: Default::default(), click_count: 1 };
            window.dispatch_event(gpui::PlatformInput::MouseUp(up), cx);
        })
        .expect("a release");
        for _ in 0..6 {
            draw(window, &mut cx);
            cx.run_until_parked();
        }
        std::thread::sleep(std::time::Duration::from_millis(600));
        for _ in 0..3 {
            draw(window, &mut cx);
            cx.run_until_parked();
        }
    }
    cx.update_window(window, |_, window, cx| frame.update(cx, |pane, cx| act(pane, window, cx))).expect("the act");
    for _ in 0..6 {
        draw(window, &mut cx);
        cx.run_until_parked();
    }
    std::thread::sleep(std::time::Duration::from_millis(600));
    for _ in 0..3 {
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

fn provider(id: &str, name: &str, linked: bool, available: bool, linked_at: Option<&str>) -> api::users::SignInProvider {
    api::users::SignInProvider { id: id.into(), name: name.into(), kind: id.into(), available, linked, linked_at: linked_at.map(Into::into) }
}

fn passkey(id: &str, name: Option<&str>, backed_up: bool) -> api::users::SignInPasskey {
    api::users::SignInPasskey { id: id.into(), name: name.map(Into::into), created_at: Some("2026-09-02T09:00:00.000Z".into()), backed_up }
}

/// Every row kind: the email code, linked and unlinked providers, one no
/// longer offered, the password, and two passkeys.
fn many_ways_in() -> api::users::SignInMethods {
    api::users::SignInMethods {
        email: "dana@example.com".into(),
        email_verified: true,
        email_otp_enabled: true,
        password_enabled: true,
        passkey_enabled: true,
        providers: vec![
            provider("google", "Google", true, true, Some("2026-09-01T10:00:00.000Z")),
            provider("apple", "Apple", false, true, None),
            provider("github", "GitHub", false, false, None),
            provider("acme-sso", "Acme SSO", true, false, None),
            provider("credential", "Password", true, true, Some("2026-08-12T10:00:00.000Z")),
        ],
        passkeys: vec![passkey("pk1", Some("MacBook"), false), passkey("pk2", None, true)],
        ways_in: 5,
    }
}

/// The last way in: the code is off on this server, Google is all there is.
fn one_way_in() -> api::users::SignInMethods {
    api::users::SignInMethods {
        email: "dana@example.com".into(),
        email_verified: true,
        email_otp_enabled: false,
        password_enabled: false,
        passkey_enabled: true,
        providers: vec![provider("google", "Google", true, true, None), provider("apple", "Apple", false, true, None)],
        passkeys: vec![],
        ways_in: 1,
    }
}

fn sign_in_shot(methods: Option<api::users::SignInMethods>) -> SignInShot {
    SignInShot { methods, pending_link: None, copy_url: None, manage_url: Some("https://app.exponential.at/t/acme/settings/account".into()), loading: false }
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

fn widget_rows() -> Vec<api::widgets::WidgetConfigSummary> {
    serde_json::from_value(serde_json::json!([
        {"id": "w1", "name": "Docs site", "enabled": true, "submissionCount": 12, "boardName": "Feedback", "allowedDomains": ["docs.example.com", "example.com"], "publicKey": "pk_1"},
        {"id": "w2", "name": "", "enabled": false, "submissionCount": 1, "allowedDomains": []},
    ]))
    .expect("widget rows")
}

#[test]
#[ignore]
fn settings_widget_rows() {
    shoot_pane("widget-rows", WidgetPane::for_shots, |pane, cx| pane.show(Some(widget_rows()), cx));
}

#[test]
#[ignore]
fn settings_widget_empty() {
    shoot_pane("widget-empty", WidgetPane::for_shots, |pane, cx| pane.show(Some(Vec::new()), cx));
}

#[test]
#[ignore]
fn settings_widget_failed() {
    shoot_pane("widget-failed", WidgetPane::for_shots, |pane, cx| pane.show(None, cx));
}

#[test]
#[ignore]
fn settings_all_boards_put_away() {
    shoot_pane("all-boards-put-away", AllBoardsPane::for_shots, |pane, cx| {
        let archived = serde_json::from_value(serde_json::json!([
            {"id": "b1", "name": "Old roadmap", "color": "#3b82f6", "icon": "rocket", "archivedAt": "2026-06-30"},
            {"id": "b2", "name": "Experiments", "icon": "flask-conical"},
        ]))
        .expect("archived boards");
        let trash = serde_json::from_value(serde_json::json!([
            {"id": "b3", "name": "Scratch", "color": "#ef4444", "icon": "code", "purgeAt": "2020-01-01T00:00:00Z"},
            {"id": "b4", "name": "Duplicates"},
        ]))
        .expect("trashed boards");
        pane.show(archived, trash, cx)
    });
}

#[test]
#[ignore]
fn settings_all_boards_empty() {
    shoot_pane("all-boards-empty", AllBoardsPane::for_shots, |pane, cx| pane.show(Vec::new(), Vec::new(), cx));
}

/// A 96 px picture (a two-colour gradient), PNG-encoded.
fn picture() -> Vec<u8> {
    let image = image::RgbaImage::from_fn(96, 96, |x, y| image::Rgba([(x * 255 / 95) as u8, 96, (y * 255 / 95) as u8, 255]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    image.write_to(&mut bytes, image::ImageFormat::Png).expect("the PNG");
    bytes.into_inner()
}

fn account_shot() -> AccountShot {
    AccountShot {
        user_id: "user-dana".into(),
        name: Some("Dana Scully".into()),
        email: "dana@example.com".into(),
        picture: None,
        timezone: Some(Some("Europe/Vienna".into())),
        timezone_open: false,
        send_diagnostics: false,
        manage_url: Some("https://app.exponential.at/t/acme/settings/account".into()),
    }
}

fn account(name: &str, shot: AccountShot, sign_in: SignInShot) {
    // Settled, as the earlier pictures of this pane were taken.
    let diagnostics = shot.send_diagnostics;
    shoot_pane_settled(name, move |window, cx| AccountPane::for_shots(diagnostics, window, cx), move |pane, cx| pane.show(shot, sign_in, cx), Some(std::time::Duration::from_millis(1200)));
}

#[test]
#[ignore]
fn settings_account_initials() {
    account("account-initials", account_shot(), sign_in_shot(Some(many_ways_in())));
}

#[test]
#[ignore]
fn settings_account_picture() {
    let shot = AccountShot { picture: Some(("https://bench.invalid/avatar.png".into(), picture())), send_diagnostics: true, ..account_shot() };
    account("account-picture", shot, sign_in_shot(Some(one_way_in())));
}

#[test]
#[ignore]
fn settings_account_email_only() {
    let shot = AccountShot { name: None, timezone: Some(None), ..account_shot() };
    account("account-email-only", shot, sign_in_shot(None));
}

#[test]
#[ignore]
fn settings_account_loading() {
    let shot = AccountShot { timezone: None, manage_url: None, ..account_shot() };
    account("account-loading", shot, SignInShot { loading: true, ..sign_in_shot(None) });
}

#[test]
#[ignore]
fn settings_account_timezone_open() {
    let shot = AccountShot { timezone_open: true, ..account_shot() };
    account("account-timezone-open", shot, sign_in_shot(Some(many_ways_in())));
}

#[test]
#[ignore]
fn settings_account_linking() {
    let sign_in = SignInShot {
        pending_link: Some("apple".into()),
        copy_url: Some("https://app.exponential.at/api/auth/link/start?ticket=t-123&provider=apple&challenge=abc".into()),
        ..sign_in_shot(Some(many_ways_in()))
    };
    account("account-linking", AccountShot { send_diagnostics: true, ..account_shot() }, sign_in);
}

fn voice_settings(ptt: &str) -> etalk::voice::VoiceSettings {
    etalk::voice::VoiceSettings {
        voice_device_id: Some("d-mint".into()),
        agent_id: Some("a1".into()),
        ptt_shortcut: ptt.into(),
        language: "en".into(),
        ..Default::default()
    }
}

fn voice(name: &str, settings: etalk::voice::VoiceSettings, capture: Option<etalk::voice::CaptureState>) {
    shoot_pane(name, VoicePrefsPane::for_shots, move |pane, cx| pane.show(VoiceShot { settings, capture }, cx));
}

#[test]
#[ignore]
fn settings_voice_unbound() {
    voice("voice-unbound", voice_settings(""), None);
}

#[test]
#[ignore]
fn settings_voice_bound() {
    let settings = etalk::voice::VoiceSettings {
        wake_word: true,
        wake_phrase: "Hey Studio".into(),
        voice_file: "~/voices/me.wav".into(),
        ..voice_settings("ctrl+shift+space")
    };
    voice("voice-bound", settings, None);
}

#[test]
#[ignore]
fn settings_voice_recording_pending() {
    use etalk::voice::{capture_start, capture_step, CaptureEvent};
    let capture = [CaptureEvent::Down("ctrl".into()), CaptureEvent::Down("shift".into())].iter().fold(capture_start("ctrl+space"), |c, e| capture_step(&c, e));
    voice("voice-recording-pending", voice_settings("ctrl+space"), Some(capture));
}

#[test]
#[ignore]
fn settings_voice_error() {
    let capture = etalk::voice::CaptureState {
        status: etalk::voice::CaptureStatus::Recording,
        value: "ctrl+space".into(),
        held: Vec::new(),
        error: Some(etalk::voice::VOICE.ptt_copy.shortcut_needs_modifier.clone()),
    };
    voice("voice-error", voice_settings("ctrl+space"), Some(capture));
}

/// #NEW-57: Desktop audio picks apps on a per-app device: the Apps row.
fn voice_apps_settings() -> etalk::voice::VoiceSettings {
    etalk::voice::VoiceSettings {
        desktop_audio: etalk::meeting::DesktopAudio::Include,
        desktop_audio_apps: vec![etalk::meeting::AudioApp { id: "us.zoom.xos".into(), name: "Zoom".into() }],
        ..voice_settings("ctrl+space")
    }
}

fn voice_clicked(name: &str, settings: etalk::voice::VoiceSettings, clicks: &'static [(f32, f32)]) {
    crate::meeting_mode::SHOT_AUDIO.with(|shot| {
        *shot.borrow_mut() = Some((
            etalk::meeting::DesktopAudioCapability::PerApp,
            vec![
                etalk::meeting::AudioApp { id: "us.zoom.xos".into(), name: "Zoom".into() },
                etalk::meeting::AudioApp { id: "com.tinyspeck.slackmacgap".into(), name: "Slack".into() },
            ],
        ))
    });
    let points: Vec<(f32, f32)> = std::env::var("EXP_VOICE_CLICKS").ok().map(|v| v.split(';').filter_map(|p| p.split_once(',')).filter_map(|(x, y)| Some((x.parse().ok()?, y.parse().ok()?))).collect()).unwrap_or_else(|| clicks.to_vec());
    // The eUI pane presses node ids (a headless click doesn't reach its popups);
    // the pointer still moves to the first point, as a click would leave it.
    let presses: Vec<String> = std::env::var("EXP_VOICE_PRESS").ok().map(|v| v.split(';').map(str::to_string).collect()).unwrap_or_default();
    let points = if presses.is_empty() { points } else { Vec::new() };
    let hover = std::env::var("EXP_VOICE_HOVER").ok().and_then(|v| v.split_once(',').and_then(|(x, y)| Some((x.parse::<f32>().ok()?, y.parse::<f32>().ok()?))));
    shoot_pane_acted(name, VoicePrefsPane::for_shots, move |pane, cx| pane.show(VoiceShot { settings, capture: None }, cx), None, &points, move |pane, window, cx| {
        if let Some((x, y)) = hover {
            let moved = gpui::MouseMoveEvent { position: gpui::point(px(x), px(y)), pressed_button: None, modifiers: Default::default() };
            window.dispatch_event(gpui::PlatformInput::MouseMove(moved), cx);
        }
        pane.press_for_shots(&presses, window, cx)
    });
}

#[test]
#[ignore]
fn settings_voice_apps() {
    voice_clicked("voice-apps", voice_apps_settings(), &[]);
}

#[test]
#[ignore]
fn settings_voice_apps_open() {
    voice_clicked("voice-apps-open", voice_apps_settings(), &[]);
}

fn prompts_rows() -> Vec<api::prompts::Prompt> {
    serde_json::from_value(serde_json::json!([
        {"id": "p1", "name": "Release train", "icon": "rocket"},
        {"id": "p2", "name": "Triage the inbox and file every bug against its board with a reproduction", "icon": "bug"},
        {"id": "p3", "name": "Weekly digest"},
        {"id": "p4", "name": "Unknown glyph", "icon": "not-an-icon"},
    ]))
    .expect("prompt rows")
}

fn prompts(name: &str, shot: PromptsShot) {
    shoot_pane(name, PromptsPane::for_shots, move |pane, cx| pane.show(shot, cx));
}

#[test]
#[ignore]
fn settings_prompts_owner() {
    prompts("prompts-owner", PromptsShot { team: Some("t".into()), owner: true, prompts: prompts_rows(), ready: true });
}

#[test]
#[ignore]
fn settings_prompts_member() {
    prompts("prompts-member", PromptsShot { team: Some("t".into()), owner: false, prompts: prompts_rows(), ready: true });
}

#[test]
#[ignore]
fn settings_prompts_empty() {
    prompts("prompts-empty", PromptsShot { team: Some("t".into()), owner: true, prompts: Vec::new(), ready: true });
}

#[test]
#[ignore]
fn settings_prompts_loading() {
    prompts("prompts-loading", PromptsShot { team: Some("t".into()), owner: false, prompts: Vec::new(), ready: false });
}

#[test]
#[ignore]
fn settings_prompts_no_team() {
    prompts("prompts-no-team", PromptsShot { team: None, owner: true, prompts: Vec::new(), ready: true });
}

/// #EXP-1629 (genUI Select option `trailing`): the Branch row's menu opened,
/// today's IDE row vs the genUI row: run each into its own dir, the second
/// with `EXP_SETTINGS_BEFORE=<the first's dir>`. `EXP_BRANCH_CLICK=x,y`
/// moves the IDE row's click (its value button).
#[test]
#[ignore]
fn settings_branch_menu_ide() {
    let click = std::env::var("EXP_BRANCH_CLICK").ok().and_then(|v| v.split_once(',').and_then(|(x, y)| Some((x.parse::<f32>().ok()?, y.parse::<f32>().ok()?)))).unwrap_or((PANE.0 - 24. - 14. - 30., 24. + 20.));
    shoot_pane_clicked("branch-menu", IdeBranchRow::for_shots, |_, _| {}, None, &[click]);
}

#[test]
#[ignore]
fn settings_branch_menu_eui() {
    shoot_pane("branch-menu", EuiBranchRow::for_shots, |_, _| {});
}
