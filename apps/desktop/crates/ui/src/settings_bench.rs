//! #EXP-1629: the settings panes drawn HEADLESS with the real text system:
//! the pictures a pane's move onto the native binding is judged by (the same
//! states before and after, pixel for pixel). Run with
//! `EXP_SETTINGS_SHOTS=<dir> cargo test -p ui --release -- --ignored settings_ --nocapture`.
use std::collections::HashMap;

use gpui::{div, prelude::*, px, size, AnyView, Window};
use gpui_component::ActiveTheme as _;

use api::notifications::EmailPrefs;

use crate::coding_flow::CodingHub;
use crate::settings::local_repos::{LocalReposPane, LocalShot};
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
use crate::settings::tools::{ToolsPane, ToolsShot};
use crate::settings::team_general::{GeneralPane, GeneralShot, PromptShot};
use crate::settings::board_detail::{BoardDetailPane, BoardShot};
use crate::settings::prompts::{PageShot as PromptPageShot, PromptPage, PromptsPane, PromptsShot};
use crate::settings::branch_row_shots::{EuiBranchRow, IdeBranchRow};
use crate::settings::account_row_shots::{EuiAccountRow, IdeAccountRow};
use crate::settings::branch_picker_shots::{EuiBranchPicker, IdeBranchPicker, PickerState};
use crate::settings::statuses::{DeleteStatusContent, StatusesPane, StatusesShot};
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

/// #EXP-1765: Repositories' per-repo branch picker: the IDE's glass pill
/// (`repositories::branch_trigger` + its `Popover`) vs genUI's `pill`
/// Select, closed (normal, disabled) and open (loading, empty, ready with
/// `main` the default and checked). Shoot `settings_repo_branch_ide`, then
/// `settings_repo_branch_eui` with `EXP_SETTINGS_BEFORE` at the first.
const REPO_BRANCH_STATES: [(&str, PickerState); 5] = [
    ("repo-branch-trigger", PickerState::Trigger),
    ("repo-branch-disabled", PickerState::Disabled),
    ("repo-branch-loading", PickerState::Loading),
    ("repo-branch-empty", PickerState::Empty),
    ("repo-branch-ready", PickerState::Ready),
];

#[test]
#[ignore]
fn settings_repo_branch_ide() {
    for (name, state) in REPO_BRANCH_STATES {
        shoot_pane(name, IdeBranchPicker::for_shots(state), |_, _| {});
    }
}

#[test]
#[ignore]
fn settings_repo_branch_eui() {
    for (name, state) in REPO_BRANCH_STATES {
        shoot_pane(name, EuiBranchPicker::for_shots(state), |_, _| {});
    }
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

/// #EXP-1629: the IDE's boxed field (`glass_input` at `.web_input_sm()` and
/// `.web_input()`, a 176 px one beside a flex one, one empty with its
/// placeholder, one with a value) against genUI's Input `variant: "box"`
/// at `size` `sm` and `default`: the same picture.
mod box_fields {
    use super::*;
    use crate::controls::WebControl as _;
    use gpui_component::input::InputState;
    use serde_json::json;

    const VALUE: &str = "ada@example.com";

    pub(super) struct Ide {
        fields: Vec<gpui::Entity<InputState>>,
    }

    impl Ide {
        fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
            let mut fields = Vec::new();
            for _ in 0..2 {
                fields.push(cx.new(|cx| InputState::new(window, cx).placeholder("Name")));
                let value = cx.new(|cx| InputState::new(window, cx).placeholder("teammate@example.com"));
                value.update(cx, |s, cx| s.set_value(VALUE, window, cx));
                fields.push(value);
            }
            Self { fields }
        }
    }

    impl Render for Ide {
        fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
            let f = &self.fields;
            let sm = gpui_component::h_flex().w_full().gap_2().items_center()
                .child(div().w(px(176.)).flex_shrink_0().child(crate::controls::glass_input(&f[0], window, cx).web_input_sm()))
                .child(div().flex_1().min_w_0().child(crate::controls::glass_input(&f[1], window, cx).web_input_sm()));
            let md = gpui_component::h_flex().w_full().gap_2().items_center()
                .child(div().w(px(176.)).flex_shrink_0().child(crate::controls::glass_input(&f[2], window, cx).web_input()))
                .child(div().flex_1().min_w_0().child(crate::controls::glass_input(&f[3], window, cx).web_input()));
            gpui_component::v_flex().w_full().gap_4().child(sm).child(md)
        }
    }

    pub(super) struct Eui {
        view: gpui::Entity<eui_gpui::view::SurfaceView>,
    }

    impl Eui {
        fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
            // The IDE's `gap_2` / `gap_4`: half a rem, one rem.
            let rem = crate::settings::doc_ink(cx).2;
            let state = json!({"a": "", "b": VALUE, "c": "", "d": VALUE});
            let binding = cx.new(|_| eui_gpui::binding::Binding::new("/pane", state, |s: &serde_json::Value, _| s.clone()));
            let pair = |id: &str, size: &str, a: &str, b: &str| json!({"id": id, "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": rem * 0.5, "width": "100%"}, "children": [
                {"id": a, "component": "Input", "props": {"label": "Name", "name": a, "variant": "box", "size": size, "placeholder": "Name", "value": {"@path": format!("/pane/{a}")}}, "style": {"width": 176, "flexShrink": 0}},
                {"id": b, "component": "Input", "props": {"label": "Email", "name": b, "variant": "box", "size": size, "placeholder": "teammate@example.com", "value": {"@path": format!("/pane/{b}")}}, "style": {"flexGrow": 1, "minWidth": 0}},
            ]});
            let tree = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": rem, "width": "100%"}, "children": [
                pair("sm", "sm", "a", "b"),
                pair("md", "default", "c", "d"),
            ]});
            let view = crate::settings::doc_mount("bench-box-fields", tree, &binding, window, cx);
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
    fn settings_box_fields_ide() {
        shoot_pane("box-fields", Ide::new, |_, _| {});
    }

    #[test]
    #[ignore]
    fn settings_box_fields_eui() {
        shoot_pane("box-fields", Eui::new, |_, _| {});
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

/// #EXP-1768: a 30k-char body (no hard break) grows the body to its 40
/// rows, as the gpui pane did, instead of scrolling at 14.
#[test]
#[ignore]
fn settings_skill_long_body() {
    skill_page("skill-long-body", Some(skill(false)), true, Some(Ok("word ".repeat(6_000))), false, false);
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
    board_detail("board-unlinked", BoardShot { board: board(None, None), repos: Some(board_repos()), failed: false, press: Vec::new() });
}

#[test]
#[ignore]
fn settings_board_linked() {
    board_detail("board-linked", BoardShot { board: board(Some("r1"), None), repos: Some(board_repos()), failed: false, press: Vec::new() });
}

#[test]
#[ignore]
fn settings_board_pinned() {
    board_detail("board-pinned", BoardShot { board: board(Some("r2"), Some("release/1.2")), repos: Some(board_repos()), failed: false, press: Vec::new() });
}

/// The identity row's ColorPicker open: the label palette's 28 px cells.
#[test]
#[ignore]
fn settings_board_color_open() {
    board_detail("board-color-open", BoardShot { board: board(Some("r1"), None), repos: Some(board_repos()), failed: false, press: vec!["color_picker.trigger".into()] });
}

#[test]
#[ignore]
fn settings_board_loading() {
    board_detail("board-loading", BoardShot { board: board(Some("r1"), None), repos: None, failed: false, press: Vec::new() });
}

#[test]
#[ignore]
fn settings_board_unavailable() {
    board_detail("board-unavailable", BoardShot { board: board(Some("gone"), None), repos: Some(board_repos()), failed: false, press: Vec::new() });
}

#[test]
#[ignore]
fn settings_board_failed_plain() {
    let plain = domain::rows::Board { icon: None, color: None, prefix: None, ..board(None, None) };
    board_detail("board-failed-plain", BoardShot { board: plain, repos: None, failed: true, press: Vec::new() });
}

const PROMPT_BODY: &str = "Review the open PRs on this board.\n\n- Summarise each diff in two lines.\n- Flag anything that touches `apps/web/src/db`.\n";

fn prompt_row() -> api::prompts::Prompt {
    serde_json::from_value(serde_json::json!({"id": "p1", "teamId": "team-shots", "name": "Review train", "icon": "rocket"})).expect("a prompt")
}

fn prompt_page(name: &str, prompt: Option<api::prompts::Prompt>, shot: PromptPageShot) {
    shoot_pane_acted(name, move |window, cx| PromptPage::new("team-shots".into(), prompt, window, cx), |_, _| {}, None, &[], move |page, window, cx| page.show(shot, window, cx));
}

fn page_shot(owner: bool, body: Option<Result<String, ()>>) -> PromptPageShot {
    PromptPageShot { owner, body, name: None, touched: false, name_error: None }
}

#[test]
#[ignore]
fn settings_prompt_page() {
    prompt_page("prompt-page", Some(prompt_row()), page_shot(true, Some(Ok(PROMPT_BODY.into()))));
}

#[test]
#[ignore]
fn settings_prompt_dirty() {
    prompt_page("prompt-dirty", Some(prompt_row()), PromptPageShot { name: Some("Review train (weekly)".into()), ..page_shot(true, Some(Ok(PROMPT_BODY.into()))) });
}

#[test]
#[ignore]
fn settings_prompt_loading() {
    prompt_page("prompt-loading", Some(prompt_row()), page_shot(true, None));
}

#[test]
#[ignore]
fn settings_prompt_failed() {
    prompt_page("prompt-failed", Some(prompt_row()), page_shot(true, Some(Err(()))));
}

#[test]
#[ignore]
fn settings_prompt_member() {
    prompt_page("prompt-member", Some(prompt_row()), page_shot(false, Some(Ok(PROMPT_BODY.into()))));
}

#[test]
#[ignore]
fn settings_prompt_new_touched() {
    prompt_page("prompt-new-touched", None, PromptPageShot { touched: true, ..page_shot(true, Some(Ok(String::new()))) });
}

#[test]
#[ignore]
fn settings_prompt_taken() {
    prompt_page("prompt-taken", Some(prompt_row()), PromptPageShot { name: Some("Release train".into()), name_error: Some("A prompt with this name already exists".into()), ..page_shot(true, Some(Ok(PROMPT_BODY.into()))) });
}

#[test]
#[ignore]
fn settings_prompt_too_long() {
    let body = "x ".repeat(crate::settings::prompts::PROMPT_BODY_MAX_CHARS / 2 + 1);
    prompt_page("prompt-too-long", Some(prompt_row()), page_shot(true, Some(Ok(body))));
}



fn team(icon: bool, zone: Option<&str>) -> domain::rows::Team {
    serde_json::from_value(serde_json::json!({
        "id": "team-acme", "name": "Acme", "slug": "acme",
        "icon_url": if icon { serde_json::json!("https://bench.invalid/team.png") } else { serde_json::Value::Null },
        "time_zone": zone, "merge_mode": "auto",
    }))
    .expect("a team")
}

fn team_plan(plan: &str) -> api::billing::TeamPlanOut {
    serde_json::from_value(serde_json::json!({"plan": plan, "limits": {"seats": 10.0, "storageMb": 10240.0, "widgetConfigs": null}, "usage": {"members": 4.0, "storageMb": 2355.2, "widgetConfigs": 2.0}})).expect("a plan")
}

const TEAM_PROMPT: &str = "# Rules\n\n- Keep PRs small: one concern each.\n- Run `bun run typecheck` before you push.\n- Never touch `apps/web/src/db/out` by hand.\n";

fn general_shot() -> GeneralShot {
    GeneralShot {
        team: Some(team(true, Some("Europe/Vienna"))),
        owner: true,
        picture: Some(picture()),
        name_draft: None,
        saving: false,
        prompt: PromptShot::Ready(TEAM_PROMPT.into(), None, false),
        billing: Some(team_plan("team")),
    }
}

fn general(name: &str, shot: GeneralShot) {
    shoot_pane_acted(name, GeneralPane::for_shots, |_, _| {}, Some(std::time::Duration::from_millis(1200)), &[], move |pane, window, cx| pane.show(shot, window, cx));
}

#[test]
#[ignore]
fn settings_general_owner() {
    general("general-owner", general_shot());
}

#[test]
#[ignore]
fn settings_general_plain() {
    let shot = GeneralShot {
        team: Some(team(false, None)),
        picture: None,
        name_draft: Some("Acme Labs".into()),
        prompt: PromptShot::Ready(TEAM_PROMPT.into(), Some("2020-01-01T00:00:00.000Z".into()), true),
        billing: None,
        ..general_shot()
    };
    general("general-plain", shot);
}

#[test]
#[ignore]
fn settings_general_edited() {
    let shot = GeneralShot { prompt: PromptShot::Ready(String::new(), Some("2020-01-01T00:00:00.000Z".into()), false), billing: Some(team_plan("free")), ..general_shot() };
    general("general-edited", shot);
}

#[test]
#[ignore]
fn settings_general_loading() {
    general("general-loading", GeneralShot { saving: true, name_draft: Some("Acme Labs".into()), prompt: PromptShot::Loading, ..general_shot() });
}

#[test]
#[ignore]
fn settings_general_failed() {
    general("general-failed", GeneralShot { prompt: PromptShot::Failed, billing: Some(team_plan("unlimited")), ..general_shot() });
}

#[test]
#[ignore]
fn settings_general_over_cap() {
    let text = "x".repeat(domain::contract::TEAM_AGENT_PROMPT_MAX_BYTES + 5);
    general("general-over-cap", GeneralShot { prompt: PromptShot::Ready(text, None, false), billing: None, ..general_shot() });
}

#[test]
#[ignore]
fn settings_general_member() {
    general("general-member", GeneralShot { owner: false, ..general_shot() });
}

#[test]
#[ignore]
fn settings_general_no_team() {
    general("general-no-team", GeneralShot { team: None, ..general_shot() });
}

/// #EXP-1629: the IDE's device picker (`picker::device_picker` behind a
/// `glass_picker_row`) vs genUI Select `triggerVariant: row` with option
/// `icon` + `description`, closed and open. Shoot `settings_select_ide*`,
/// then `settings_select_eui*` with `EXP_SETTINGS_BEFORE` at the first: 0 px.
mod select_variants {
    use super::*;
    use serde_json::json;
    use std::rc::Rc;

    fn rows(long: bool) -> Vec<crate::picker::device_picker::DevicePickerDevice> {
        let device = |id: &str, name: &str, icon: &str, server: bool, offline: bool| crate::picker::device_picker::DevicePickerDevice {
            id: id.into(),
            name: name.into(),
            icon: Some(icon.into()),
            server,
            description: offline.then(|| "Offline".to_string()),
            disabled: false,
        };
        let server = if long { "build-server-eu-central-hetzner-2026" } else { "build-server" };
        vec![device("d1", "mint", "monitor", false, false), device("d2", server, "server", true, false), device("d3", "macbook", "laptop", false, true)]
    }

    pub(super) struct Ide(bool, bool);

    impl Render for Ide {
        fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
            let rows = rows(self.1);
            let trigger = crate::trigger_editor::picker_trigger("bench-device".into(), "mint", cx).into_any_element();
            let open = self.0;
            let control = crate::picker::deferred(move |window, cx| {
                let picker = crate::picker::device_picker::device_picker(&rows, Some("d1".to_string()), trigger, Rc::new(|_: Vec<String>, _: &mut Window, _: &mut gpui::App| {}))
                    .id(gpui::SharedString::from("bench-device-picker"));
                let picker = if open { picker.open(Rc::new(|_: &mut Window, _: &mut gpui::App| {})) } else { picker };
                picker.render(window, cx)
            })
            .into_any_element();
            crate::surface::glass_group_rows(vec![crate::surface::glass_picker_row("Device", None, control, cx)])
        }
    }

    pub(super) struct Eui {
        view: gpui::Entity<eui_gpui::view::SurfaceView>,
        open: Option<u8>,
    }

    impl Eui {
        fn new(open: bool, long: bool, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
            let ink = crate::settings::doc_ink(cx);
            let binding = cx.new(|_| eui_gpui::binding::Binding::new("/pane", json!({}), |s: &serde_json::Value, _| s.clone()));
            let options: Vec<serde_json::Value> = rows(long).iter().map(|d| {
                let mut o = json!({"label": d.name, "value": d.id, "icon": crate::icons::device_icon_name(d.icon.as_deref(), d.server)});
                if let Some(description) = &d.description {
                    o["description"] = json!(description);
                }
                o
            }).collect();
            let tree = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "borderRadius": ink.0, "backgroundColor": ink.1, "overflow": "hidden"}, "children": [
                {"id": "device", "component": "Select", "props": {"label": "Device", "name": "device", "triggerVariant": "row", "value": "d1", "popupMinWidth": 170, "options": options}},
            ]});
            let view = crate::settings::doc_mount_lifted("bench-select", tree, &binding, window, cx);
            Self { view, open: open.then_some(2) }
        }
    }

    impl Render for Eui {
        fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
            match self.open {
                Some(0) => {
                    self.open = None;
                    self.view.update(cx, |view, cx| {
                        if let Some(trigger) = view.index_of("device.trigger") {
                            view.fire(trigger, "press", None, cx);
                        }
                    });
                }
                Some(n) => {
                    self.open = Some(n - 1);
                    cx.notify();
                }
                // The headless clock stands still: the popup's enter fade
                // runs out on the frames after the press.
                None if self.view.read(cx).layers().iter().any(|layer| layer.owner == "device") => {
                    self.view.update(cx, |view, cx| view.advance_clock(std::time::Duration::from_millis(500), cx));
                }
                None => {}
            }
            div().w_full().child(self.view.clone())
        }
    }

    #[test]
    #[ignore]
    fn settings_select_ide() {
        shoot_pane("select", |_, _| Ide(false, false), |_, _| {});
    }

    #[test]
    #[ignore]
    fn settings_select_ide_open() {
        shoot_pane_settled("select-open", |_, _| Ide(true, false), |_, _| {}, Some(std::time::Duration::from_millis(1200)));
    }

    #[test]
    #[ignore]
    fn settings_select_eui() {
        shoot_pane("select", |window, cx| Eui::new(false, false, window, cx), |_, _| {});
    }

    #[test]
    #[ignore]
    fn settings_select_eui_open() {
        shoot_pane_settled("select-open", |window, cx| Eui::new(true, false, window, cx), |_, _| {}, Some(std::time::Duration::from_millis(1200)));
    }

    #[test]
    #[ignore]
    fn settings_select_ide_open_long() {
        shoot_pane_settled("select-open-long", |_, _| Ide(true, true), |_, _| {}, Some(std::time::Duration::from_millis(1200)));
    }

    #[test]
    #[ignore]
    fn settings_select_eui_open_long() {
        shoot_pane_settled("select-open-long", |window, cx| Eui::new(true, true, window, cx), |_, _| {}, Some(std::time::Duration::from_millis(1200)));
    }
}

fn tools_shot(doctor: Option<&'static str>) -> ToolsShot {
    ToolsShot { repos: "~/code/exp", prefix: "exp/", shell: None, typed: None, doctor, path_open: false }
}

fn tools(name: &str, shot: ToolsShot) {
    // Pinned before the first frame (the hub's own doctor run is superseded).
    shoot_pane(name, move |window, cx| {
        let mut pane = ToolsPane::for_shots(window, cx);
        pane.show(shot, window, cx);
        pane
    }, |_, _| {});
}

#[test]
#[ignore]
fn settings_tools_ready() {
    tools("tools-ready", tools_shot(Some("ready: git + claude, codex missing, computer use off")));
}

#[test]
#[ignore]
fn settings_tools_dirty() {
    tools("tools-dirty", ToolsShot { shell: Some("/bin/zsh"), typed: Some("feature/"), ..tools_shot(Some("git missing, claude signed out, codex ok, computer use on (no OS permissions here)")) });
}

#[test]
#[ignore]
fn settings_tools_doctor_running() {
    tools("tools-doctor-running", tools_shot(None));
}

// ---------------------------------------------------------------------------
// #EXP-1770: the device readiness block in every host
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn settings_tools_path_open() {
    tools("tools-path-open", ToolsShot { path_open: true, ..tools_shot(Some("ready: git + claude, codex missing, computer use off")) });
}

/// A `device-doctor.json` case's block.
fn doctor_case(name: &str) -> coding::device_doctor::DeviceDoctor {
    let fixture: serde_json::Value = serde_json::from_str(include_str!("../../../../../packages/domain-contract/fixtures/device-doctor.json")).expect("the fixture");
    let case = fixture["cases"].as_array().expect("cases").iter().find(|case| case["name"].as_str() == Some(name)).expect("the case");
    crate::device_readiness::parse(Some(&case["doctor"])).expect("a block")
}

const CASE_READY: &str = "ready: git + claude, codex missing, computer use off";
const CASE_TOO_OLD: &str = "claude too old, computer use on with one macOS permission missing";
const CASE_GIT_MISSING: &str = "git missing, claude signed out, codex ok, computer use on (no OS permissions here)";
const CASE_LOW_DISK: &str = "EXP-1375: low disk with two kept worktrees: a warning, never a blocker";

/// What a [`BlockShot`] draws.
#[derive(Clone, Copy)]
enum BlockKind {
    /// A fixture case's block (`local` = this device), its switch wired or
    /// not, `busy` rows spinning.
    Case { case: &'static str, local: bool, toggle: bool, busy: &'static [&'static str] },
    /// The device settings dialog: another device's block with the switch
    /// on, the Computer use model + Work screen rows under it and a
    /// finished update's note under Codex.
    Device,
    /// The device settings dialog for an older build (no doctor): the bare
    /// switch row and its tail.
    DeviceBare,
    /// The chat composer's row: the agent's blocker or the low-disk warning.
    Composer { case: &'static str, warning: bool },
    /// No report yet.
    Loading,
}

struct BlockShot {
    kind: BlockKind,
    model: crate::coding_selects::ChoiceSelect,
    work_screen: crate::coding_selects::ChoiceSelect,
    block: gpui::Entity<crate::settings::readiness::ReadinessBlock>,
}

impl BlockShot {
    fn new(kind: BlockKind, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let settings = coding::Settings::default();
        Self {
            kind,
            model: crate::coding_selects::model_select(coding::CodingAgent::Claude, &settings.computer_use_model, window, cx),
            work_screen: crate::coding_selects::work_screen_select(&crate::styleguide::entries::work_screen::demo_displays(), None, window, cx),
            block: cx.new(|cx| crate::settings::readiness::ReadinessBlock::new("shot", window, cx)),
        }
    }
}

fn block(name: &str, kind: BlockKind) {
    shoot_pane(name, move |window, cx| BlockShot::new(kind, window, cx), |_, _| {});
}

#[test]
#[ignore]
fn settings_readiness_primary() {
    block("readiness-primary", BlockKind::Case { case: CASE_GIT_MISSING, local: true, toggle: true, busy: &[] });
}

#[test]
#[ignore]
fn settings_readiness_busy() {
    block("readiness-busy", BlockKind::Case { case: CASE_TOO_OLD, local: true, toggle: true, busy: &["claude"] });
}

#[test]
#[ignore]
fn settings_readiness_switch_read_only() {
    block("readiness-switch-read-only", BlockKind::Case { case: CASE_READY, local: false, toggle: false, busy: &[] });
}

#[test]
#[ignore]
fn settings_readiness_warning() {
    block("readiness-warning", BlockKind::Case { case: CASE_LOW_DISK, local: false, toggle: false, busy: &[] });
}

#[test]
#[ignore]
fn settings_readiness_loading() {
    block("readiness-loading", BlockKind::Loading);
}

#[test]
#[ignore]
fn settings_readiness_device() {
    block("readiness-device", BlockKind::Device);
}

#[test]
#[ignore]
fn settings_readiness_device_bare() {
    block("readiness-device-bare", BlockKind::DeviceBare);
}

#[test]
#[ignore]
fn settings_readiness_composer_blocker() {
    block("readiness-composer-blocker", BlockKind::Composer { case: CASE_TOO_OLD, warning: false });
}

#[test]
#[ignore]
fn settings_readiness_composer_warning() {
    block("readiness-composer-warning", BlockKind::Composer { case: CASE_LOW_DISK, warning: true });
}

impl Render for BlockShot {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        use crate::device_readiness::{self as readiness, BlockProps};
        use crate::settings::readiness::Shape;
        let noop_action: readiness::ActionHandler = std::rc::Rc::new(|_, _, _, _| {});
        let noop_toggle: readiness::ToggleHandler = std::rc::Rc::new(|_, _, _| {});
        let block = self.block.clone();
        let mut show = |shape: Shape, props: BlockProps, cx: &mut gpui::Context<Self>| block.update(cx, |block, cx| block.show(shape, props, cx));
        match self.kind {
            BlockKind::Case { case, local, toggle, busy } => {
                let props = BlockProps {
                    on_action: Some(noop_action),
                    on_toggle: toggle.then_some(noop_toggle),
                    busy: busy.iter().map(|key| key.to_string()).collect(),
                    ..BlockProps::default()
                };
                show(Shape::Sections(&readiness::sections(&doctor_case(case), local, None)), props, cx);
            }
            BlockKind::Device => {
                let mut props = BlockProps {
                    on_action: Some(noop_action),
                    on_toggle: Some(noop_toggle),
                    computer_use_tail: crate::device_settings::computer_use_rows(&self.model, &self.work_screen),
                    ..BlockProps::default()
                };
                props.notes.insert("codex".into(), "Updated to 0.156.1".into());
                show(Shape::Sections(&readiness::sections(&doctor_case(CASE_GIT_MISSING), false, Some(true))), props, cx);
            }
            BlockKind::DeviceBare => {
                let props = BlockProps {
                    on_toggle: Some(noop_toggle),
                    computer_use_tail: crate::device_settings::computer_use_rows(&self.model, &self.work_screen),
                    ..BlockProps::default()
                };
                show(Shape::Single(&readiness::bare_switch_row(true)), props, cx);
            }
            BlockKind::Composer { case, warning } => {
                let doctor = doctor_case(case);
                let row = if warning { readiness::warning_row(&doctor, true) } else { readiness::blocking_row(&doctor, coding::CodingAgent::Claude, true) }.expect("a row");
                show(Shape::Single(&row), crate::chat_screen::ChatScreenView::readiness_row_props(None, cx), cx);
                // The chat's notes column under the composer.
                return div().w_full().child(gpui_component::v_flex().w_full().min_w_0().gap_0p5().px_1().text_xs().child(self.block.clone()));
            }
            BlockKind::Loading => show(Shape::Loading, BlockProps::default(), cx),
        }
        div().w_full().child(self.block.clone())
    }
}

/// #EXP-1629: the app editor's Settings tab (`settings::app_detail` and the
/// `app_mcp` groups), in a window tall enough for every group at once.
mod app_detail {
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
}

fn status(id: &str, category: &str, name: &str, color: Option<&str>, order: f64, builtin: Option<&str>) -> domain::rows::IssueStatusRow {
    domain::rows::IssueStatusRow { id: id.into(), team_id: "team-shots".into(), category: category.into(), name: name.into(), color: color.map(str::to_string), sort_order: Some(order), builtin_key: builtin.map(str::to_string), created_at: None, updated_at: None }
}

/// The builtins plus two custom statuses (Unstarted stays empty).
fn status_rows() -> Vec<domain::rows::IssueStatusRow> {
    vec![
        status("s-backlog", "backlog", "Backlog", None, 0., Some("backlog")),
        status("s-progress", "started", "In Progress", None, 0., Some("in_progress")),
        status("s-blocked", "started", "Blocked", Some("#ef4444"), 1., None),
        status("s-review", "started", "In Review", None, 2., Some("in_review")),
        status("s-done", "completed", "Done", None, 0., Some("done")),
        status("s-verified", "completed", "Verified", Some("#8b5cf6"), 1., None),
        status("s-cancelled", "cancelled", "Cancelled", None, 0., Some("cancelled")),
        status("s-duplicate", "duplicate", "Duplicate", None, 0., Some("duplicate")),
    ]
}

fn status_counts() -> HashMap<String, usize> {
    HashMap::from([("s-backlog".to_string(), 12), ("s-progress".to_string(), 3), ("s-blocked".to_string(), 1), ("s-done".to_string(), 48)])
}

fn statuses_shot(rows: Vec<domain::rows::IssueStatusRow>) -> StatusesShot {
    StatusesShot { counts: status_counts(), ready: true, creating: None, row_error: None, press: Vec::new(), rows }
}

fn statuses(name: &str, shot: StatusesShot, clicks: &'static [(f32, f32)]) {
    shoot_pane_clicked(name, StatusesPane::for_shots, move |pane, cx| pane.show(shot, cx), None, clicks);
}


#[test]
#[ignore]
fn settings_statuses_rows() {
    statuses("statuses-rows", statuses_shot(status_rows()), &[]);
}

#[test]
#[ignore]
fn settings_statuses_color_open() {
    // Before: a click at the Blocked row's glyph (46, 415) opened today's popover.
    let press = vec!["started_row_color.s-blocked.customTrigger|started_row_color.customTrigger.s-blocked".into()];
    statuses("statuses-color-open", StatusesShot { press, ..statuses_shot(status_rows()) }, &[]);
}

#[test]
#[ignore]
fn settings_statuses_menu_open() {
    // Before: a click at the Blocked row's ⋯ (709, 415) only hovered it (the
    // pointer menu does not open headless).
    let press = vec!["started_row_more.s-blocked".into()];
    statuses("statuses-menu-open", StatusesShot { press, ..statuses_shot(status_rows()) }, &[]);
}

#[test]
#[ignore]
fn settings_statuses_create() {
    let shot = StatusesShot { creating: Some((domain::statuses::IssueStatusCategory::Unstarted, String::new())), ..statuses_shot(status_rows()) };
    statuses("statuses-create", shot, &[]);
}

#[test]
#[ignore]
fn settings_statuses_create_duplicate() {
    let shot = StatusesShot { creating: Some((domain::statuses::IssueStatusCategory::Completed, "done".into())), ..statuses_shot(status_rows()) };
    statuses("statuses-create-duplicate", shot, &[]);
}

#[test]
#[ignore]
fn settings_statuses_row_error() {
    let shot = StatusesShot { row_error: Some(("s-blocked".into(), "A status with this name already exists.".into())), ..statuses_shot(status_rows()) };
    statuses("statuses-row-error", shot, &[]);
}

/// Started at its cap: the band's "+" disabled and the cap line.
#[test]
#[ignore]
fn settings_statuses_capped() {
    let mut rows = status_rows();
    rows.push(status("s-qa", "started", "QA", Some("#eab308"), 3., None));
    rows.push(status("s-ready", "unstarted", "Ready", Some("#3b82f6"), 0., None));
    statuses("statuses-capped", statuses_shot(rows), &[]);
}

#[test]
#[ignore]
fn settings_statuses_loading() {
    statuses("statuses-loading", StatusesShot { ready: false, ..statuses_shot(Vec::new()) }, &[]);
}

#[test]
#[ignore]
fn settings_statuses_empty() {
    statuses("statuses-empty", statuses_shot(Vec::new()), &[]);
}

/// The delete dialog's body (native, unchanged by the document): the
/// server count, the PR-pin notice and the target picker.
#[test]
#[ignore]
fn settings_statuses_delete_pin() {
    let notice = "This status is where issues move when a pull request merges. That automation will fall back to Done.";
    shoot_pane("statuses-delete-pin", move |_, _| DeleteStatusContent::for_shots(&status_rows(), "s-verified", Some(3), Some(notice)), |_, _| {});
}

/// #EXP-1629: Settings → MCP › Servers in every state (owner, member, a
/// Connect waiting + a Reconnect in flight, reloading, loading, failed,
/// empty, no team, a row's ⋯ menu open).
mod mcp_servers {
    use super::*;
    use crate::settings::mcp_servers::{McpServersPane, McpShot};
    use api::mcp_servers::{McpConnection, McpServerConfig, McpServerDevice, McpServerListEntry};

    fn entry(id: &str, name: &str, url: Option<&str>, auth: &str, status: &str) -> McpServerListEntry {
        McpServerListEntry {
            config: McpServerConfig { id: id.into(), team_id: "t".into(), name: name.into(), url: url.map(str::to_string), auth: auth.into(), ..Default::default() },
            connection: McpConnection { status: status.into(), expires_at: None, error: None },
            ..Default::default()
        }
    }

    fn rows() -> Vec<McpServerListEntry> {
        let mut linear = entry("s1", "Linear", Some("https://mcp.linear.app/mcp"), "oauth", "connected");
        linear.config.enabled_by_default = true;
        linear.shared = true;
        linear.used_by = vec!["Triage".into()];
        let notion = entry("s2", "Notion", Some("https://mcp.notion.com/mcp"), "oauth", "expired");
        let docs = entry("s3", "Internal docs", Some("https://mcp.docs.acme.dev/sse"), "oauth", "not_connected");
        let mut keys = entry("s4", "Stripe", Some("https://mcp.stripe.com"), "secret", "not_connected");
        keys.config.header_names = vec!["Authorization".into()];
        let mut files = entry("s5", "Filesystem", None, "none", "not_needed");
        files.config.transport = "stdio".into();
        files.config.command = Some("npx".into());
        files.config.args = vec!["-y".into(), "@modelcontextprotocol/server-filesystem".into(), "/srv/shared".into()];
        files.device = Some(McpServerDevice { id: "d1".into(), label: "studio-mac".into(), owner_id: "u".into(), kind: "desktop".into(), icon: None });
        vec![linear, notion, docs, keys, files]
    }

    fn mcp(name: &str, team: bool, owner: bool, load: Option<Result<Vec<McpServerListEntry>, String>>, reloading: bool, pending: Option<&'static str>, connecting: Option<&'static str>, clicks: &'static [(f32, f32)]) {
        let shot = McpShot { team: team.then(|| "t".to_string()), owner };
        shoot_pane_clicked(name, McpServersPane::for_shots, move |pane, cx| pane.show(shot, load, reloading, pending, connecting, cx), None, clicks);
    }

    #[test]
    #[ignore]
    fn settings_mcp_servers_owner() {
        mcp("mcp-servers-owner", true, true, Some(Ok(rows())), false, None, None, &[]);
    }

    #[test]
    #[ignore]
    fn settings_mcp_servers_member() {
        mcp("mcp-servers-member", true, false, Some(Ok(rows())), false, None, None, &[]);
    }

    #[test]
    #[ignore]
    fn settings_mcp_servers_busy() {
        mcp("mcp-servers-busy", true, true, Some(Ok(rows())), false, Some("s2"), Some("s3"), &[]);
    }

    #[test]
    #[ignore]
    fn settings_mcp_servers_reloading() {
        mcp("mcp-servers-reloading", true, true, Some(Ok(rows())), true, None, None, &[]);
    }

    #[test]
    #[ignore]
    fn settings_mcp_servers_loading() {
        mcp("mcp-servers-loading", true, true, None, false, None, None, &[]);
    }

    #[test]
    #[ignore]
    fn settings_mcp_servers_failed() {
        mcp("mcp-servers-failed", true, true, Some(Err("offline".into())), false, None, None, &[]);
    }

    #[test]
    #[ignore]
    fn settings_mcp_servers_empty() {
        mcp("mcp-servers-empty", true, true, Some(Ok(vec![])), false, None, None, &[]);
    }

    #[test]
    #[ignore]
    fn settings_mcp_servers_no_team() {
        mcp("mcp-servers-no-team", false, true, None, false, None, None, &[]);
    }

    #[test]
    #[ignore]
    fn settings_mcp_servers_menu_open() {
        mcp("mcp-servers-menu-open", true, true, Some(Ok(rows())), false, None, None, &[(MENU_X, MENU_Y)]);
    }

    /// The first row's ⋯ (window points), read off `mcp-servers-owner`.
    const MENU_X: f32 = 709.;
    const MENU_Y: f32 = 97.5;
}

/// #EXP-1629: Settings → Members in every state (an owner's roster with
/// owners, members, the three placeholder pills, a name-less email row and
/// a row with no synced user, plus the pending invites; a member's view;
/// the invite form typed, sending, adding, with its own notices; the link
/// path's seat-cap notice, generated link and generating; no team; a row's
/// ⋯ under the pointer).
mod members {
    use super::*;
    use crate::settings::members::{MembersPane, MembersShot};
    use serde_json::json;

    fn members() -> Vec<domain::rows::TeamMember> {
        let m = |id: &str, user: &str, role: &str, at: &str| json!({"id": id, "team_id": "t", "user_id": user, "role": role, "created_at": at});
        serde_json::from_value(json!([
            m("m1", "u-me", "owner", "2026-01-01T00:00:00Z"),
            m("m2", "u-fox", "owner", "2026-01-02T00:00:00Z"),
            m("m3", "u-sam", "member", "2026-01-03T00:00:00Z"),
            m("m4", "u-ada", "member", "2026-11-01T00:00:00Z"),
            m("m5", "u-bob", "member", "2026-01-04T00:00:00Z"),
            m("m6", "u-cy", "member", "2026-02-01T00:00:00Z"),
            m("m7", "u-apple", "member", "2026-03-01T00:00:00Z"),
            m("m8", "u-gone-7f3a", "member", "2026-04-01T00:00:00Z"),
        ]))
        .expect("members")
    }

    fn users() -> Vec<domain::rows::User> {
        let u = |id: &str, name: &str, email: &str| json!({"id": id, "name": name, "email": email});
        serde_json::from_value(json!([
            u("u-me", "Dana Scully", "dana@acme.dev"),
            u("u-fox", "Fox Mulder", "fox@acme.dev"),
            u("u-sam", "Sam Rivera", "sam@acme.dev"),
            u("u-ada", "Ada Lovelace", "ada@example.com"),
            u("u-bob", "Bob Ross", "bob@example.com"),
            u("u-cy", "Cy Linear", "cy@example.com"),
            u("u-apple", "", "x7k2@privaterelay.appleid.com"),
        ]))
        .expect("users")
    }

    fn invites() -> Vec<domain::rows::TeamInvite> {
        serde_json::from_value(json!([
            {"id": "i1", "team_id": "t", "role": "member", "email": "ada@example.com", "placeholder_user_id": "u-ada", "sent_at": "2026-11-01T00:00:00Z", "expires_at": "2099-11-08T00:00:00Z", "created_at": "2026-11-01T00:00:00Z"},
            {"id": "i2", "team_id": "t", "role": "member", "email": "bob@example.com", "placeholder_user_id": "u-bob", "sent_at": "2026-01-04T00:00:00Z", "expires_at": "2026-01-11T00:00:00Z", "created_at": "2026-01-04T00:00:00Z"},
            {"id": "i3", "team_id": "t", "role": "member", "email": "cy@example.com", "placeholder_user_id": "u-cy", "expires_at": "2026-02-01T00:00:00Z", "created_at": "2026-02-01T00:00:00Z"},
            {"id": "i4", "team_id": "t", "role": "member", "sent_at": "2026-11-05T00:00:00Z", "expires_at": "2099-12-01T00:00:00Z", "created_at": "2026-11-05T00:00:00Z"},
            {"id": "i5", "team_id": "t", "role": "owner", "email": "eve@other.dev", "sent_at": "2026-11-03T00:00:00Z", "expires_at": "2099-11-20T00:00:00Z", "created_at": "2026-11-03T00:00:00Z"},
        ]))
        .expect("invites")
    }

    const LINK: &str = "https://exponential.at/invite/9f2c4e7a1b3d5f6e8a0c2e4b6d8f0a1c3e5b7d9f";

    fn shot() -> MembersShot {
        MembersShot {
            team: Some("t".into()),
            owner: true,
            me: "u-me".into(),
            members: members(),
            users: users(),
            invites: invites(),
            name: String::new(),
            email: String::new(),
            sending: false,
            adding: false,
            form_out_of_seats: false,
            form_link: None,
            email_error: None,
            link: None,
            out_of_seats: false,
            generating: false,
        }
    }

    fn typed() -> MembersShot {
        MembersShot { name: "Grace Hopper".into(), email: "grace@example.com".into(), ..shot() }
    }

    fn members_shot(name: &str, shot: MembersShot) {
        members_hover(name, shot, None);
    }

    /// The pane, then the form filled; `hover` = the pointer moved there
    /// (window points) after it.
    fn members_hover(name: &str, shot: MembersShot, hover: Option<(f32, f32)>) {
        shoot_pane_acted(name, MembersPane::for_shots, move |pane, cx| pane.show(shot, cx), None, &[], move |pane, window, cx| {
            pane.shot_act(window, cx);
            if let Some((x, y)) = hover {
                // After the pane's update: the move repaints it.
                window.defer(cx, move |window, cx| {
                    let moved = gpui::MouseMoveEvent { position: gpui::point(px(x), px(y)), pressed_button: None, modifiers: Default::default() };
                    window.dispatch_event(gpui::PlatformInput::MouseMove(moved), cx);
                });
            }
        });
    }

    #[test]
    #[ignore]
    fn settings_members_owner() {
        members_shot("members-owner", shot());
    }

    #[test]
    #[ignore]
    fn settings_members_member() {
        members_shot("members-member", MembersShot { owner: false, me: "u-sam".into(), ..shot() });
    }

    #[test]
    #[ignore]
    fn settings_members_typed() {
        members_shot("members-typed", typed());
    }

    #[test]
    #[ignore]
    fn settings_members_sending() {
        members_shot("members-sending", MembersShot { sending: true, ..typed() });
    }

    #[test]
    #[ignore]
    fn settings_members_adding() {
        members_shot("members-adding", MembersShot { adding: true, ..typed() });
    }

    #[test]
    #[ignore]
    fn settings_members_out_of_seats() {
        members_shot("members-out-of-seats", MembersShot { out_of_seats: true, ..shot() });
    }

    #[test]
    #[ignore]
    fn settings_members_link() {
        members_shot("members-link", MembersShot { link: Some(LINK.into()), ..shot() });
    }

    #[test]
    #[ignore]
    fn settings_members_generating() {
        members_shot("members-generating", MembersShot { generating: true, ..typed() });
    }

    #[test]
    #[ignore]
    fn settings_members_form_notices() {
        members_shot("members-form-notices", MembersShot { form_out_of_seats: true, form_link: Some(LINK.into()), email_error: Some("Enter a full email address".into()), ..typed() });
    }

    #[test]
    #[ignore]
    fn settings_members_no_pending() {
        members_shot("members-no-pending", MembersShot { invites: Vec::new(), ..shot() });
    }

    #[test]
    #[ignore]
    fn settings_members_no_team() {
        members_shot("members-no-team", MembersShot { team: None, ..shot() });
    }

    /// The pointer over the second row's ⋯ (`EXP_MEMBERS_HOVER=x,y`
    /// overrides [`MENU`]).
    #[test]
    #[ignore]
    fn settings_members_menu_hover() {
        let point = std::env::var("EXP_MEMBERS_HOVER").ok().and_then(|v| v.split_once(',').and_then(|(x, y)| Some((x.parse().ok()?, y.parse().ok()?)))).unwrap_or(MENU);
        members_hover("members-menu-hover", shot(), Some(point));
    }

    /// The second row's ⋯ (window points), read off `members-owner`.
    const MENU: (f32, f32) = (700., 140.);
}

fn local_repos_rows() -> Vec<(String, Option<u64>, Vec<(String, Option<String>)>)> {
    vec![
        ("acme/web".into(), Some(1_288_490_189), vec![("exp-session-1a2b".into(), Some("exp/session-1a2b".into())), ("detached".into(), None)]),
        ("acme/api".into(), Some(314_572_800), vec![("fix-login".into(), Some("exp/fix-login".into()))]),
        ("acme/docs".into(), Some(52_428_800), vec![]),
    ]
}

fn local_shot(repos: Option<Vec<(String, Option<u64>, Vec<(String, Option<String>)>)>>) -> LocalShot {
    LocalShot { repos, disk: Some((400_000_000_000, 500_000_000_000)), in_use: Vec::new(), busy: None, message: None }
}

fn local_repos(name: &str, shot: LocalShot) {
    shoot_pane(name, LocalReposPane::for_shots, move |pane, cx| pane.show(shot, cx));
}

#[test]
#[ignore]
fn settings_local_repos_rows() {
    local_repos("local-repos-rows", local_shot(Some(local_repos_rows())));
}

#[test]
#[ignore]
fn settings_local_repos_busy() {
    let shot = LocalShot {
        disk: Some((4_000_000_000, 500_000_000_000)),
        in_use: vec!["acme/api".into()],
        busy: Some("acme/web".into()),
        message: Some(("acme/docs".into(), "Removed 2 worktrees · deleted 3 branches".into())),
        ..local_shot(Some(local_repos_rows()))
    };
    local_repos("local-repos-busy", shot);
}

#[test]
#[ignore]
fn settings_local_repos_measuring() {
    let rows = local_repos_rows().into_iter().map(|(name, _, worktrees)| (name, None, worktrees)).collect();
    local_repos("local-repos-measuring", local_shot(Some(rows)));
}

#[test]
#[ignore]
fn settings_local_repos_no_worktrees() {
    local_repos("local-repos-no-worktrees", local_shot(Some(vec![("acme/docs".into(), Some(52_428_800), vec![])])));
}

#[test]
#[ignore]
fn settings_local_repos_empty() {
    local_repos("local-repos-empty", local_shot(Some(Vec::new())));
}

#[test]
#[ignore]
fn settings_local_repos_loading() {
    local_repos("local-repos-loading", local_shot(None));
}
