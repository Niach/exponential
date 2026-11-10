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
use crate::settings::board_detail::{BoardDetailPane, BoardShot};
use crate::settings::prompts::{PromptsPane, PromptsShot};
use crate::settings::branch_row_shots::{EuiBranchRow, IdeBranchRow};
use crate::settings::account_row_shots::{EuiAccountRow, IdeAccountRow};
use crate::settings::skills::{ListShot as SkillsListShot, SkillPage, SkillsPane};
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

/// #EXP-1629 (genUI Select option `trailing`): the Branch row's menu opened
/// (the IDE's by its dev open, as a click would; genUI's by a trigger
/// press), today's IDE row vs the genUI row: run each into its own dir, the
/// second with `EXP_SETTINGS_BEFORE=<the first's dir>`.
#[test]
#[ignore]
fn settings_branch_menu_ide() {
    shoot_pane("branch-menu", IdeBranchRow::for_shots, |_, _| {});
}

#[test]
#[ignore]
fn settings_branch_menu_eui() {
    shoot_pane("branch-menu", EuiBranchRow::for_shots, |_, _| {});
}

/// #EXP-1764 (genUI Select `triggerIcon` + the `brand-claude` mark): the
/// Agent detail Account row closed (the brand mark, the email, the
/// chevron), today's IDE row vs the genUI row: run each into its own dir,
/// the second with `EXP_SETTINGS_BEFORE=<the first's dir>`.
#[test]
#[ignore]
fn settings_account_row_ide() {
    shoot_pane("account-row", IdeAccountRow::for_shots, |_, _| {});
}

#[test]
#[ignore]
fn settings_account_row_eui() {
    shoot_pane("account-row", EuiAccountRow::for_shots, |_, _| {});
}

/// #EXP-1629: the IDE's chromeless group fields vs genUI Textarea
/// `row`/`code` and Input `code`/`bare`: the agent instructions (`row`, 4–16
/// rows), the skill description (`row`, empty, 2–6), the SKILL.md / prompt
/// body (`code`, 14–40), the skill name after its glyph (Input `code`), the
/// agent / board / prompt name (Input `bare`) and the team prompt while it
/// loads (`code`, disabled, 10–40). Shoot `settings_fields_ide`, then
/// `settings_fields_eui` with `EXP_SETTINGS_BEFORE` at the first: 0 px.
mod field_variants {
    use super::*;
    use gpui_component::input::{InputState, Textarea, TextareaState};
    use serde_json::json;

    const AREA: &str = "Triage new issues every morning.\nLabel them and ping the owner of the board.";
    const CODE: &str = "---\nname: review\n---\n# Review\nRun the checks, then summarise the diff.";
    const NAME: &str = "review-pr";

    pub(super) struct Ide {
        area: gpui::Entity<TextareaState>,
        empty: gpui::Entity<TextareaState>,
        code: gpui::Entity<TextareaState>,
        name: gpui::Entity<InputState>,
        plain: gpui::Entity<InputState>,
        off: gpui::Entity<TextareaState>,
    }

    impl Ide {
        fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
            let area = cx.new(|cx| crate::controls::web_textarea(4, 16, window, cx).placeholder("Instructions"));
            area.update(cx, |s, cx| s.set_value(AREA, window, cx));
            let empty = cx.new(|cx| crate::controls::web_textarea(2, 6, window, cx).placeholder("What it does"));
            let code = cx.new(|cx| crate::controls::web_textarea(14, 40, window, cx).placeholder("SKILL.md"));
            code.update(cx, |s, cx| s.set_value(CODE, window, cx));
            let name = cx.new(|cx| InputState::new(window, cx).placeholder("name"));
            name.update(cx, |s, cx| s.set_value(NAME, window, cx));
            let plain = cx.new(|cx| InputState::new(window, cx).placeholder("Agent name"));
            plain.update(cx, |s, cx| s.set_value("Triage", window, cx));
            let off = cx.new(|cx| crate::controls::web_textarea(10, 40, window, cx));
            off.update(cx, |s, cx| s.set_value("You are the team's agent.", window, cx));
            Self { area, empty, code, name, plain, off }
        }
    }

    impl Render for Ide {
        fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
            let muted = cx.theme().muted_foreground;
            let name_row = crate::surface::glass_row_shell().child(gpui_component::Icon::from(crate::icons::registry::UI_DOCS).text_color(muted)).child(
                div().flex_1().min_w_0().font_family(theme::terminal::FONT_FAMILY).child(crate::controls::glass_input(&self.name, window, cx).appearance(false).h_auto().px_0().py_0()),
            );
            let plain_row = crate::surface::glass_row_shell().gap_2().child(
                div().flex_1().min_w_0().child(crate::surface::glass_row_input(crate::controls::glass_input(&self.plain, window, cx)).text_left()),
            );
            gpui_component::v_flex().w_full().gap_4()
                .child(crate::surface::glass_group().child(Textarea::new(&self.area).appearance(false).w_full().px_4().py_3()))
                .child(crate::surface::glass_group().child(Textarea::new(&self.empty).appearance(false).w_full().px_4().py_3().text_sm()))
                .child(crate::surface::glass_group().child(Textarea::new(&self.code).appearance(false).w_full().min_h(px(256.)).px_4().py_3().text_xs().font_family(theme::terminal::FONT_FAMILY)))
                .child(crate::surface::glass_group_rows(vec![name_row, plain_row]))
                .child(crate::surface::glass_group().child(Textarea::new(&self.off).appearance(false).w_full().px_4().py_3().font_family(theme::terminal::FONT_FAMILY).text_xs().disabled(true)))
        }
    }

    pub(super) struct Eui {
        view: gpui::Entity<eui_gpui::view::SurfaceView>,
    }

    impl Eui {
        fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
            let ink = crate::settings::doc_ink(cx);
            let state = json!({"area": AREA, "empty": "", "code": CODE, "name": NAME, "plain": "Triage", "off": "You are the team's agent."});
            let binding = cx.new(|_| eui_gpui::binding::Binding::new("/pane", state, |s: &serde_json::Value, _| s.clone()));
            let group = |id: &str, child: serde_json::Value| json!({"id": id, "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "borderRadius": ink.0, "backgroundColor": ink.1, "overflow": "hidden"}, "children": [child]});
            let glyph = ink.2 * 1.0;
            let shell = |id: &str, gap: f32, children: Vec<serde_json::Value>| json!({"id": id, "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": gap, "paddingHorizontal": ink.2, "paddingVertical": ink.2 * 0.75}, "children": children});
            let tree = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.2, "width": "100%"}, "children": [
                group("g1", json!({"id": "area", "component": "Textarea", "props": {"label": "Instructions", "name": "area", "variant": "row", "rows": 4, "autosize": true, "maxRows": 16, "placeholder": "Instructions", "value": {"@path": "/pane/area"}}})),
                group("g2", json!({"id": "empty", "component": "Textarea", "props": {"label": "Description", "name": "empty", "variant": "row", "rows": 2, "autosize": true, "maxRows": 6, "placeholder": "What it does", "value": {"@path": "/pane/empty"}}})),
                group("g3", json!({"id": "code", "component": "Textarea", "props": {"label": "Body", "name": "code", "variant": "code", "rows": 14, "autosize": true, "maxRows": 40, "placeholder": "SKILL.md", "value": {"@path": "/pane/code"}}})),
                {"id": "g4", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "borderRadius": ink.0, "backgroundColor": ink.1, "overflow": "hidden"}, "children": [
                    shell("r1", ink.2 * 0.75, vec![
                        json!({"id": "r1_icon", "component": "Icon", "props": {"name": "ui-docs"}, "style": {"width": glyph, "height": glyph, "color": ink.3, "flexShrink": 0}}),
                        json!({"id": "name", "component": "Input", "props": {"label": "Name", "name": "name", "variant": "code", "placeholder": "name", "value": {"@path": "/pane/name"}}, "style": {"flexGrow": 1, "minWidth": 0}}),
                    ]),
                    {"id": "div", "component": "Box", "style": {"height": 1, "flexShrink": 0, "backgroundColor": ink.4}},
                    shell("r2", ink.2 * 0.5, vec![
                        json!({"id": "plain", "component": "Input", "props": {"label": "Name", "name": "plain", "variant": "bare", "placeholder": "Agent name", "value": {"@path": "/pane/plain"}}, "style": {"flexGrow": 1, "minWidth": 0}}),
                    ]),
                ]},
                group("g5", json!({"id": "off", "component": "Textarea", "props": {"label": "Prompt", "name": "off", "variant": "code", "rows": 10, "autosize": true, "maxRows": 40, "disabled": true, "value": {"@path": "/pane/off"}}})),
            ]});
            let view = crate::settings::doc_mount("bench-fields", tree, &binding, window, cx);
            Self { view }
        }
    }

    impl Render for Eui {
        fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
            div().w_full().child(self.view.clone())
        }
    }

    #[test]
    #[ignore]
    fn settings_fields_ide() {
        shoot_pane("fields", Ide::new, |_, _| {});
    }

    #[test]
    #[ignore]
    fn settings_fields_eui() {
        shoot_pane("fields", Eui::new, |_, _| {});
    }
}

/// #EXP-1629: the IDE's spinner (`feedback::spinner`, muted) vs genUI
/// `Spinner` at the same sizes (xs = the 10 px inline one, sm, default, lg)
/// and phase (Reduce motion freezes both at their start frame). Shoot
/// `settings_spinner_ide`, then `settings_spinner_eui` with
/// `EXP_SETTINGS_BEFORE` at the first: 0 px.
mod spinner_parity {
    use super::*;
    use serde_json::json;

    const SIZES: [(&str, f32); 4] = [("xs", 10.), ("sm", 16.), ("default", 20.), ("lg", 24.)];

    pub(super) struct Ide;

    impl Render for Ide {
        fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
            SIZES.iter().fold(gpui_component::h_flex().items_start().gap(px(16.)), |row, (_, side)| row.child(crate::feedback::spinner(*side, cx)))
        }
    }

    pub(super) struct Eui {
        view: gpui::Entity<eui_gpui::view::SurfaceView>,
    }

    impl Eui {
        fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
            let binding = cx.new(|_| eui_gpui::binding::Binding::new("/pane", json!({}), |s: &serde_json::Value, _| s.clone()));
            let children: Vec<serde_json::Value> = SIZES.iter().map(|(size, _)| json!({"id": format!("spin_{size}"), "component": "Spinner", "props": {"size": size}})).collect();
            let tree = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "flex-start", "gap": 16, "width": "100%"}, "children": children});
            let view = crate::settings::doc_mount("bench-spinner", tree, &binding, window, cx);
            Self { view }
        }
    }

    impl Render for Eui {
        fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
            div().w_full().child(self.view.clone())
        }
    }

    #[test]
    #[ignore]
    fn settings_spinner_ide() {
        shoot_pane("spinner", |_, cx| {
            cx.set_reduce_motion(true);
            Ide
        }, |_, _| {});
    }

    #[test]
    #[ignore]
    fn settings_spinner_eui() {
        shoot_pane("spinner", |window, cx| {
            cx.set_reduce_motion(true);
            Eui::new(window, cx)
        }, |_, _| {});
    }
}

fn skill_row(id: &str, name: &str, description: &str, enabled_by_default: bool, builtin: bool) -> domain::rows::SkillRow {
    serde_json::from_value(serde_json::json!({"id": id, "team_id": "team-shots", "name": name, "description": description, "enabled_by_default": enabled_by_default, "builtin": builtin})).expect("a skill row")
}

fn skill_rows() -> Vec<domain::rows::SkillRow> {
    vec![
        skill_row("s1", "exponential-pr", "Open, update and land pull requests the Exponential way.", false, true),
        skill_row("s2", "review", "Run the checks, then summarise the diff for the reviewer.", true, false),
        skill_row("s3", "release-notes", "Draft the changelog entry from the merged pull requests since the last tag, grouped by area, in the house style.", false, false),
        skill_row("s4", "triage", "", false, false),
    ]
}

fn skills_list(name: &str, team: bool, ready: bool, owner: bool, rows: Vec<domain::rows::SkillRow>) {
    shoot_pane(name, SkillsPane::for_shots, move |pane, cx| pane.show(SkillsListShot { team, ready, owner, rows }, cx));
}

#[test]
#[ignore]
fn settings_skills_rows() {
    skills_list("skills-rows", true, true, true, skill_rows());
}

#[test]
#[ignore]
fn settings_skills_member() {
    skills_list("skills-member", true, true, false, skill_rows());
}

#[test]
#[ignore]
fn settings_skills_empty() {
    skills_list("skills-empty", true, true, true, Vec::new());
}

#[test]
#[ignore]
fn settings_skills_loading() {
    skills_list("skills-loading", true, false, true, Vec::new());
}

#[test]
#[ignore]
fn settings_skills_no_team() {
    skills_list("skills-no-team", false, true, true, Vec::new());
}

fn skill(builtin: bool) -> api::skills::Skill {
    serde_json::from_value(serde_json::json!({"id": "s2", "teamId": "team-shots", "name": "review", "description": "Run the checks, then summarise the diff for the reviewer.", "enabledByDefault": true, "builtin": builtin})).expect("a skill")
}

const SKILL_BODY: &str = "---\nname: review\ndescription: Run the checks, then summarise the diff.\n---\n\n# Review\n\n1. Run `bun run typecheck` and the tests.\n2. Read the diff file by file.\n3. Summarise what changed and what to look at.";

fn skill_page(name: &str, skill: Option<api::skills::Skill>, owner: bool, body: Option<Result<String, ()>>, touched: bool, taken: bool) {
    shoot_pane_acted(name, move |window, cx| SkillPage::new("team-shots".into(), skill, window, cx), |_, _| {}, None, &[], move |page, window, cx| page.show(owner, body, touched, taken, window, cx));
}

#[test]
#[ignore]
fn settings_skill_page() {
    skill_page("skill-page", Some(skill(false)), true, Some(Ok(SKILL_BODY.into())), false, false);
}

#[test]
#[ignore]
fn settings_skill_loading() {
    skill_page("skill-loading", Some(skill(false)), true, None, false, false);
}

#[test]
#[ignore]
fn settings_skill_failed() {
    skill_page("skill-failed", Some(skill(false)), true, Some(Err(())), false, false);
}

#[test]
#[ignore]
fn settings_skill_builtin() {
    skill_page("skill-builtin", Some(skill(true)), true, Some(Ok(SKILL_BODY.into())), false, false);
}

#[test]
#[ignore]
fn settings_skill_member() {
    skill_page("skill-member", Some(skill(false)), false, Some(Ok(SKILL_BODY.into())), false, false);
}

#[test]
#[ignore]
fn settings_skill_new() {
    skill_page("skill-new", None, true, Some(Ok(String::new())), false, false);
}

#[test]
#[ignore]
fn settings_skill_new_touched() {
    skill_page("skill-new-touched", None, true, Some(Ok(String::new())), true, false);
}

#[test]
#[ignore]
fn settings_skill_name_taken() {
    skill_page("skill-name-taken", Some(skill(false)), true, Some(Ok(SKILL_BODY.into())), false, true);
}

/// #EXP-1629: Settings → Repositories in every state (owner rows with the
/// branch picker, chips, sharer and both access banners; a member's
/// read-only rows; busy; loading; failed; empty; no team; each GitHub
/// status line; the Local account's folders; the branch picker open).
mod repositories {
    use super::*;
    use crate::settings::repositories::{RepoShot, RepositoriesPane};
    use serde_json::{json, Value};

    const INSTALL: &str = "https://github.com/apps/exponential/installations/new";

    fn installation(id: i64, login: &str, org: bool, suspended: bool) -> Value {
        json!({"installationId": id, "accountLogin": login, "accountType": if org { "Organization" } else { "User" }, "manageUrl": format!("https://github.com/settings/installations/{id}"), "suspended": suspended})
    }

    fn linked(installations: Vec<Value>) -> Value {
        json!({"configured": true, "connectConfigured": true, "linked": true, "login": "dana-scully", "installed": !installations.is_empty(), "installUrl": INSTALL, "installations": installations})
    }

    fn suspended() -> Value {
        linked(vec![installation(1, "acme", true, false), installation(2, "dana-scully", false, true)])
    }

    fn rows() -> Value {
        json!([
            {"id": "r1", "fullName": "acme/web", "defaultBranch": "main", "githubDefaultBranch": "main", "private": true, "installationId": 1,
                "sharedBy": {"id": "u-me", "name": "Dana Scully"}, "boards": [{"id": "b1", "name": "Web"}, {"id": "b2", "name": "Design"}, {"id": "b3", "name": "Ops"}]},
            {"id": "r2", "fullName": "dana-scully/dotfiles", "defaultBranch": "release/2026-11", "githubDefaultBranch": "main", "private": false, "installationId": 2,
                "sharedBy": {"id": "u-sam", "email": "sam@acme.dev"}, "boards": []},
            {"id": "r3", "fullName": "acme/legacy-monolith-with-a-rather-long-repository-name", "defaultBranch": "develop", "private": true, "installationId": 1,
                "inaccessibleAt": "2026-11-01T10:00:00Z", "boards": [{"id": "b1", "name": "Web"}]},
        ])
    }

    fn boards() -> Vec<domain::rows::Board> {
        let board = |id: &str, name: &str, color: Option<&str>, icon: Option<&str>| {
            let mut board = domain::rows::Board::seeded(id, "t", name);
            board.color = color.map(str::to_string);
            board.icon = icon.map(str::to_string);
            board
        };
        vec![board("b1", "Web", Some("#3b82f6"), Some("rocket")), board("b2", "Design", None, Some("bug"))]
    }

    fn shot(team: bool, owner: bool, busy: bool) -> RepoShot {
        RepoShot { team: team.then(|| "t".to_string()), owner, me: Some("u-me".into()), local: None, boards: boards(), busy, branches: Vec::new() }
    }

    fn repos(name: &str, shot: RepoShot, status: Option<Value>, list: Option<Result<Value, String>>, clicks: &'static [(f32, f32)]) {
        shoot_pane_clicked(name, RepositoriesPane::for_shots, move |pane, cx| pane.show(shot, status, list, cx), None, clicks);
    }

    #[test]
    #[ignore]
    fn settings_repositories_owner() {
        repos("repositories-owner", shot(true, true, false), Some(suspended()), Some(Ok(rows())), &[]);
    }

    #[test]
    #[ignore]
    fn settings_repositories_member() {
        repos("repositories-member", shot(true, false, false), Some(suspended()), Some(Ok(rows())), &[]);
    }

    #[test]
    #[ignore]
    fn settings_repositories_busy() {
        repos("repositories-busy", shot(true, true, true), Some(suspended()), Some(Ok(rows())), &[]);
    }

    #[test]
    #[ignore]
    fn settings_repositories_loading() {
        repos("repositories-loading", shot(true, true, false), None, None, &[]);
    }

    #[test]
    #[ignore]
    fn settings_repositories_failed() {
        repos("repositories-failed", shot(true, true, false), Some(linked(vec![installation(1, "acme", true, false)])), Some(Err("offline".into())), &[]);
    }

    #[test]
    #[ignore]
    fn settings_repositories_empty() {
        repos("repositories-empty", shot(true, true, false), Some(linked(vec![installation(1, "acme", true, false)])), Some(Ok(json!([]))), &[]);
    }

    #[test]
    #[ignore]
    fn settings_repositories_no_team() {
        repos("repositories-no-team", shot(false, true, false), None, None, &[]);
    }

    #[test]
    #[ignore]
    fn settings_repositories_status_failed() {
        repos("repositories-status-failed", shot(true, true, false), None, Some(Ok(json!([]))), &[]);
    }

    #[test]
    #[ignore]
    fn settings_repositories_status_not_configured() {
        repos("repositories-status-not-configured", shot(true, true, false), Some(json!({"configured": false})), Some(Ok(json!([]))), &[]);
    }

    #[test]
    #[ignore]
    fn settings_repositories_status_not_linked() {
        repos("repositories-status-not-linked", shot(true, true, false), Some(json!({"configured": true, "connectConfigured": true})), Some(Ok(json!([]))), &[]);
    }

    #[test]
    #[ignore]
    fn settings_repositories_status_expired() {
        repos("repositories-status-expired", shot(true, true, false), Some(json!({"configured": true, "linked": true, "needsReconnect": true, "login": "dana-scully"})), Some(Ok(json!([]))), &[]);
    }

    #[test]
    #[ignore]
    fn settings_repositories_status_linked() {
        repos("repositories-status-linked", shot(true, true, false), Some(linked(vec![installation(1, "acme", true, false), installation(3, "dana-scully", false, false)])), Some(Ok(json!([]))), &[]);
    }

    #[test]
    #[ignore]
    fn settings_repositories_status_not_installed() {
        repos("repositories-status-not-installed", shot(true, true, false), Some(linked(vec![])), Some(Ok(json!([]))), &[]);
    }

    #[test]
    #[ignore]
    fn settings_repositories_status_suspended() {
        repos("repositories-status-suspended", shot(true, true, false), Some(suspended()), Some(Ok(json!([]))), &[]);
    }

    #[test]
    #[ignore]
    fn settings_repositories_status_busy() {
        repos("repositories-status-busy", shot(true, true, true), Some(json!({"configured": true, "linked": true, "needsReconnect": true})), Some(Ok(json!([]))), &[]);
    }

    fn local(rows: Vec<api::repositories::LocalRepository>) -> RepoShot {
        RepoShot { local: Some(rows), ..shot(false, true, false) }
    }

    #[test]
    #[ignore]
    fn settings_repositories_local() {
        let folder = |id: &str, name: &str, branch: &str, path: &str| api::repositories::LocalRepository { id: id.into(), full_name: name.into(), default_branch: branch.into(), path: path.into() };
        let rows = vec![folder("l1", "web", "main", "/home/dana/code/web"), folder("l2", "a-folder-with-a-rather-long-name-to-see-it-truncate", "develop", "/home/dana/code/clients/acme/a-folder-with-a-rather-long-name-to-see-it-truncate")];
        repos("repositories-local", local(rows), None, None, &[]);
    }

    #[test]
    #[ignore]
    fn settings_repositories_local_empty() {
        repos("repositories-local-empty", local(Vec::new()), None, None, &[]);
    }

    /// A click at the first row's branch pill: `EXP_REPOS_CLICK=x,y`
    /// overrides [`BRANCH`].
    fn branch_click() -> &'static [(f32, f32)] {
        let point = std::env::var("EXP_REPOS_CLICK").ok().and_then(|v| v.split_once(',').and_then(|(x, y)| Some((x.parse().ok()?, y.parse().ok()?)))).unwrap_or(BRANCH);
        Box::leak(Box::new([point]))
    }

    #[test]
    #[ignore]
    fn settings_repositories_branch_open() {
        let shot = RepoShot { branches: vec![("r1".into(), vec!["main".into(), "develop".into(), "feature/branch-picker".into(), "release/2026-11".into()])], ..shot(true, true, false) };
        repos("repositories-branch-open", shot, Some(suspended()), Some(Ok(rows())), branch_click());
    }

    #[test]
    #[ignore]
    fn settings_repositories_branch_loading() {
        repos("repositories-branch-loading", shot(true, true, false), Some(suspended()), Some(Ok(rows())), branch_click());
    }

    /// The first row's branch pill (window points), read off `repositories-owner`.
    const BRANCH: (f32, f32) = (580., 291.);
}

/// #EXP-1629: a pending pill (`Button::loading(true)`: the spinner in the
/// icon slot) at 2× — the settings panes' Reconnect / Connect / Set key
/// while their write is in flight.
mod pending_pill {
    use super::*;

    pub(super) struct Pills;

    impl Render for Pills {
        fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
            use gpui_component::Disableable as _;
            use crate::surface::{glass_pill_button, glass_pill_button_primary, PillSize};
            gpui_component::h_flex().gap_2()
                .child(glass_pill_button("reconnect", PillSize::Sm, cx).icon(crate::icons::registry::UI_WARNING).label("Reconnect").loading(true).disabled(true))
                .child(glass_pill_button_primary("connect", PillSize::Sm).icon(crate::icons::registry::UI_SIGN_IN).label("Connect").loading(true).disabled(true))
                .child(glass_pill_button("add", PillSize::Md, cx).icon(crate::icons::registry::UI_ADD).label("Add server").loading(true))
        }
    }

    #[test]
    #[ignore]
    fn settings_pending_pill() {
        shoot_pane("pending-pill", |_, _| Pills, |_, _| {});
    }
}

fn board(repository: Option<&str>, branch: Option<&str>) -> domain::rows::Board {
    serde_json::from_value(serde_json::json!({
        "id": "board-web", "team_id": "team-acme", "name": "Web app", "prefix": "WEB", "color": "#3b82f6", "icon": "rocket",
        "repository_id": repository, "default_branch": branch,
    }))
    .expect("a board")
}

fn board_repos() -> serde_json::Value {
    serde_json::json!([
        {"id": "r1", "fullName": "acme/web", "defaultBranch": "main", "githubDefaultBranch": "main", "private": true},
        {"id": "r2", "fullName": "acme/api", "defaultBranch": "develop", "private": false},
    ])
}

fn board_detail(name: &str, shot: BoardShot) {
    shoot_pane(name, BoardDetailPane::for_shots, move |pane, cx| pane.show(shot, cx));
}

#[test]
#[ignore]
fn settings_board_unlinked() {
    board_detail("board-unlinked", BoardShot { board: board(None, None), repos: Some(board_repos()), failed: false });
}

#[test]
#[ignore]
fn settings_board_linked() {
    board_detail("board-linked", BoardShot { board: board(Some("r1"), None), repos: Some(board_repos()), failed: false });
}

#[test]
#[ignore]
fn settings_board_pinned() {
    board_detail("board-pinned", BoardShot { board: board(Some("r2"), Some("release/1.2")), repos: Some(board_repos()), failed: false });
}

#[test]
#[ignore]
fn settings_board_loading() {
    board_detail("board-loading", BoardShot { board: board(Some("r1"), None), repos: None, failed: false });
}

#[test]
#[ignore]
fn settings_board_unavailable() {
    board_detail("board-unavailable", BoardShot { board: board(Some("gone"), None), repos: Some(board_repos()), failed: false });
}

#[test]
#[ignore]
fn settings_board_failed_plain() {
    let plain = domain::rows::Board { icon: None, color: None, prefix: None, ..board(None, None) };
    board_detail("board-failed-plain", BoardShot { board: plain, repos: None, failed: true });
}
