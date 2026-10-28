//! NEW-67 — `voice-settings` (3 Special components): Settings → Personal →
//! Voice (`fixtures/voice.json`), drawn by the pane's own
//! [`crate::settings::voice_prefs::voice_rows`] over static values: mint as
//! the voice device, Studio as the agent, Speak replies on, the tray group
//! with Push to talk and Wake word off, Ctrl + Space and an empty wake phrase
//! (placeholder "Hey Studio"), as the web entry. The rows
//! are inert here (no `on_change`); the pane wires the same rows to
//! `voice.update`.
//!
//! NEW-68: the voice device's IDE also shows a system tray item
//! (`crate::voice_tray`) whose menu is OS-native, not a styleguide
//! component: Open Exponential first (Call {agent} after it once calls
//! land, V4), then Speak replies, Push to talk · {shortcut} and Wake word ·
//! {phrase} as check items (disabled while the device or the agent is None,
//! as on this page; #NEW-58: Screen memory ("Screen memory · Paused" while
//! a private window is focused; device-local, always enabled) and Mute
//! microphone follow them, Mute while the microphone listens: Push to talk
//! on and its worker up), then
//! Voice settings… (opens this page) and Quit.
//!
//! #NEW-57 (`fixtures/meeting.json`): after the wake phrase the tray group
//! gains Meeting mode (on here, "You, Zoom and Slack" on its second line),
//! Desktop audio (Pick apps) and Apps ("Zoom, Slack"), as the web entry;
//! turning Meeting mode on opens THE consent dialog (`meeting-consent`).
//!
//! #NEW-97: while no tray host shows that item (GNOME without AppIndicator)
//! and the wake word is on, the rail's footer carries a voice button beside
//! Computer instead (`domain::voice::rail_voice`): the same ghost square
//! with the settings-voice glyph, the rail's destructive dot while the wake
//! word holds the microphone open, the tooltip "Listening for “{phrase}”"
//! (or "Microphone muted"), and a pointer menu with the tray's rows minus
//! Open Exponential and Quit. The specimen under the page pictures that
//! button listening (the dot on) with its menu open: the rows
//! `rail_voice` derives with Push to talk and Wake word on, the microphone
//! up and Screen memory paused by a private window, as the web entry.

use gpui::{div, prelude::FluentBuilder as _, px, App, IntoElement, ParentElement as _, Styled as _, Window};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    input::InputState,
    v_flex, ActiveTheme as _, Sizable as _,
};

use domain::voice::{
    agent_options, rail_voice, voice_device_options, ScreenMemoryState, TrayContext, TrayItem, VoiceAgent, VoiceDevice,
    VoiceSettings, VOICE,
};

use super::composer_dialog::OpenMenu;
use crate::icons::registry;

use crate::controls::glass_input;
use crate::settings::voice_prefs::{apps_control, voice_rows, MeetingRows, VoiceInputs};
use crate::surface;

pub(crate) const ID: &str = "voice-settings";
pub(crate) const OWNER: &str = "NEW-67";

pub(crate) fn render(window: &mut Window, cx: &mut App) -> gpui::Div {
    let devices = voice_device_options(&[
        VoiceDevice { device_id: "d-mint".into(), name: "mint".into(), kind: "desktop".into() },
        VoiceDevice { device_id: "d-mac".into(), name: "macbook".into(), kind: "desktop".into() },
        VoiceDevice { device_id: "d-phone".into(), name: "iPhone".into(), kind: "phone".into() },
    ]);
    let agents = agent_options(&[
        VoiceAgent { id: "a1".into(), name: "Studio".into(), team: "Acme".into() },
        VoiceAgent { id: "a2".into(), name: "Helper".into(), team: "Acme".into() },
    ]);
    let settings = VoiceSettings {
        voice_device_id: Some("d-mint".into()),
        agent_id: Some("a1".into()),
        speak_replies: true,
        push_to_talk: false,
        ptt_shortcut: "ctrl+space".into(),
        wake_word: false,
        wake_phrase: String::new(),
        language: "en".into(),
        voice_file: String::new(),
        voice_transcript: String::new(),
        // #NEW-57: a meeting runs for Zoom and Slack.
        desktop_audio: domain::meeting::DesktopAudio::Include,
        desktop_audio_apps: vec![
            domain::meeting::AudioApp { id: "us.zoom.xos".into(), name: "Zoom".into() },
            domain::meeting::AudioApp { id: "com.tinyspeck.slackmacgap".into(), name: "Slack".into() },
        ],
        meeting_started_at: Some("2026-10-28T10:00:00.000Z".into()),
    };
    let mut field = |key: &'static str, placeholder: String, value: &'static str| {
        let input = window.use_keyed_state(key, cx, |window, cx| {
            let mut state = InputState::new(window, cx).placeholder(placeholder);
            state.set_value(value, window, cx);
            state
        });
        surface::glass_row_input(glass_input(&input, window, cx)).into_any_element()
    };
    let inputs = VoiceInputs {
        wake_phrase: field("sg-voice-settings-wake", domain::voice::wake_phrase_placeholder("Studio"), ""),
        voice_file: field("sg-voice-settings-file", VOICE.copy.voice_file_placeholder.clone(), ""),
        voice_transcript: field("sg-voice-settings-transcript", VOICE.copy.voice_transcript_placeholder.clone(), ""),
    };
    let mut body = v_flex().w_full().gap_2();
    let meeting = MeetingRows {
        capability: domain::meeting::DesktopAudioCapability::PerApp,
        apps: apps_control(&settings, &[], None, window, cx),
        on_meeting: None,
    };
    for row in voice_rows(&settings, &devices, &agents, inputs, meeting, None, cx) {
        body = body.child(row);
    }
    div().w_full().max_w(px(560.)).child(body).child(div().mt_8().child(rail_voice_specimen(window, cx)))
}

/// #NEW-97 / #NEW-58: the rail's voice button (`sidebar::rail_voice_entry`)
/// listening, its pointer menu drawn open above it: the rows `rail_voice`
/// derives for Studio with every switch on, the microphone up and Screen
/// memory on but paused ("Screen memory · Paused").
fn rail_voice_specimen(window: &mut Window, cx: &mut App) -> gpui::Div {
    let settings = VoiceSettings {
        voice_device_id: Some("d-mint".into()),
        agent_id: Some("a1".into()),
        speak_replies: true,
        push_to_talk: true,
        ptt_shortcut: "ctrl+space".into(),
        wake_word: true,
        wake_phrase: String::new(),
        language: "en".into(),
        voice_file: String::new(),
        voice_transcript: String::new(),
        ..Default::default()
    };
    let context = TrayContext {
        agent: Some("Studio"),
        muted: false,
        call_available: false,
        mic_available: true,
        device_is_voice_device: true,
        shortcut_unavailable: false,
        shortcut_label: None,
        screen_memory: Some(ScreenMemoryState { on: true, paused: true }),
        meeting: None,
        pause: None,
    };
    let Some(rail) = rail_voice(&settings, &context, true, false) else { return div() };
    let rows = rail.menu.clone();
    let menu = window.use_keyed_state("sg-voice-settings-rail-menu", cx, move |window, cx| {
        OpenMenu::new(window, cx, move |mut menu, _, _| {
            menu = menu.min_w(px(240.));
            for item in rows {
                let row = match item {
                    TrayItem::Check { id, label, checked, .. } => crate::controls::pointer_toggle_item(
                        format!("sg-rail-voice-{id:?}"),
                        None,
                        label,
                        checked,
                        |_, _, _| {},
                    ),
                    TrayItem::Action { label, .. } => {
                        crate::controls::pointer_menu_item(None, label, |_| None, |_, _| {})
                    }
                    TrayItem::Separator => continue,
                };
                menu = menu.item(row);
            }
            menu
        })
    });
    let button = div()
        .relative()
        .flex_shrink_0()
        .child(Button::new("sg-rail-voice").ghost().small().icon(registry::SETTINGS_VOICE).tooltip(rail.tooltip.clone()))
        .when(rail.badge, |this| {
            // The rail's destructive dot (`RailBadge::Dot`), placed as the rail places it.
            this.child(
                div()
                    .absolute()
                    .top(px(3.))
                    .right(px(3.))
                    .child(div().size_1p5().rounded_full().bg(theme::tokens::DESTRUCTIVE.to_hsla())),
            )
        });
    let caption = format!(
        "The rail's voice button while no tray host shows the tray: listening (the destructive dot), its menu open (rail_voice), Screen memory paused by a private window. Tooltip: \"{}\".",
        rail.tooltip
    );
    v_flex()
        .w_full()
        .gap_2()
        .child(div().flex().child(menu))
        .child(div().flex().child(button))
        .child(div().text_xs().text_color(cx.theme().muted_foreground).child(caption))
}
