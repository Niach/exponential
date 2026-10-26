//! NEW-67 — `voice-settings` (3 Special components): Settings → Personal →
//! Voice (`fixtures/voice.json`), drawn by the pane's own
//! [`crate::settings::voice_prefs::voice_rows`] over static values: mint as
//! the voice device, Ops as the agent, Speak replies and Push to talk on,
//! the tray group with Ctrl + Space and the wake phrase "Hey Ops". The rows
//! are inert here (no `on_change`); the pane wires the same rows to
//! `voice.update`.
//!
//! NEW-68: the voice device's IDE also shows a system tray item
//! (`crate::voice_tray`) whose menu is OS-native, not a styleguide
//! component: Speak replies, Push to talk · {shortcut}, Wake word ·
//! {phrase} and Mute microphone as check items (disabled while the device
//! or the agent is None, as on this page), then Voice settings… (opens this
//! page) and Quit; Call {agent} leads once calls land (V4).

use gpui::{div, px, App, IntoElement, ParentElement as _, Styled as _, Window};
use gpui_component::{input::InputState, v_flex};

use domain::voice::{agent_options, voice_device_options, VoiceAgent, VoiceDevice, VoiceSettings, VOICE};

use crate::controls::glass_input;
use crate::settings::voice_prefs::{voice_rows, VoiceInputs};
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
        VoiceAgent { id: "ops".into(), name: "Ops".into(), team: "Exponential".into() },
        VoiceAgent { id: "scout".into(), name: "Scout".into(), team: "Exponential".into() },
    ]);
    let settings = VoiceSettings {
        voice_device_id: Some("d-mint".into()),
        agent_id: Some("ops".into()),
        speak_replies: true,
        push_to_talk: true,
        ptt_shortcut: "ctrl+space".into(),
        wake_word: false,
        wake_phrase: "Hey Ops".into(),
        language: "en".into(),
        voice_file: String::new(),
        voice_transcript: String::new(),
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
        wake_phrase: field("sg-voice-settings-wake", domain::voice::wake_phrase_placeholder("Ops"), "Hey Ops"),
        voice_file: field("sg-voice-settings-file", VOICE.copy.voice_file_placeholder.clone(), ""),
        voice_transcript: field("sg-voice-settings-transcript", VOICE.copy.voice_transcript_placeholder.clone(), ""),
    };
    let mut body = v_flex().w_full().gap_2();
    for row in voice_rows(&settings, &devices, &agents, inputs, None, cx) {
        body = body.child(row);
    }
    div().w_full().max_w(px(560.)).child(body)
}
