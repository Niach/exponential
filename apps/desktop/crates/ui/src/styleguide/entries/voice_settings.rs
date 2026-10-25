//! NEW-67 — `voice-settings` (3 Special components): Settings → Personal →
//! Voice (`fixtures/voice.json`), drawn by the pane's own
//! [`crate::settings::voice_prefs::voice_rows`] over static values: mint as
//! the voice device, Ops as the agent, Speak replies and Push to talk on,
//! the tray group with Ctrl + Space and the wake phrase "Hey Ops". The rows
//! are inert here (no `on_change`); the pane wires the same rows to
//! `voice.update`.

use gpui::{div, px, App, IntoElement, ParentElement as _, Styled as _, Window};
use gpui_component::{input::InputState, v_flex};

use domain::voice::{agent_options, voice_device_options, VoiceAgent, VoiceDevice, VoiceSettings};

use crate::controls::glass_input;
use crate::settings::voice_prefs::voice_rows;
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
    };
    let input = window.use_keyed_state("sg-voice-settings-wake", cx, |window, cx| {
        let mut state = InputState::new(window, cx).placeholder(domain::voice::wake_phrase_placeholder("Ops"));
        state.set_value("Hey Ops", window, cx);
        state
    });
    let wake = surface::glass_row_input(glass_input(&input, window, cx)).into_any_element();
    let mut body = v_flex().w_full().gap_2().child(surface::glass_section_header(domain::voice::VOICE.copy.group.clone(), None, cx));
    for row in voice_rows(&settings, &devices, &agents, wake, None, cx) {
        body = body.child(row);
    }
    div().w_full().max_w(px(560.)).child(body)
}
