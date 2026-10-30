//! #EXP-1422: Exponential as eTalk's host. The voice tray, push to talk, the
//! overlay, the call UI, spoken replies and meeting mode live in
//! `etalk-gpui` (shared with serverless hosts like spezl); this is the
//! one place they reach Exponential: the synced store (messages, agents,
//! sessions, devices), the server (`sessions.send`, `voice.*`), the relay's
//! call host (`steer::call_host`), the device doctor, the toasts and the
//! shell window.

use std::path::PathBuf;

use etalk::meeting::{AudioApp, DesktopAudioCapability};
use etalk::voice::{UtteranceTarget, VoiceMessage, VoiceSettings};
use etalk_gpui::{
    Brand, CallConnection, CallLink, CallLinkEvent, Notice, Outgoing, SendError, SendJob, Sent, SignedIn, Surface, TalkAgent, TalkDevice,
    TalkHost, TalkIcon, TalkMessage, VoiceWrite,
};
use gpui::{App, Task};
use gpui_component::Icon;
use sync::Store;

use crate::icons::{registry, ExpIcon};

pub use etalk_gpui::voice_tray::open_ide;

/// The tray's icon: the app icon `desktop_integration` installs on Linux.
const TRAY_ICON_SVG: &[u8] = include_bytes!("../../../assets/icons/logo-white.svg");

struct ExponentialHost {
    data_dir: PathBuf,
}

/// Install eTalk with Exponential as its host: ONCE, after `Store::open`, the
/// `AuthContext` and the steer subsystem, before the session bootstrap. The
/// store's messages, session and agents feed it from here.
pub fn install(cx: &mut App) {
    let Some(store) = Store::try_global(cx) else { return };
    let Some(data_dir) = cx.try_global::<crate::session::AuthContext>().map(|auth| auth.data_dir.clone()) else { return };
    let collections = store.collections();
    let (messages, agents, state) = (collections.messages.clone(), collections.agents.clone(), store.state());
    etalk_gpui::init(cx, Box::new(ExponentialHost { data_dir }));
    cx.observe(&messages, |_, cx| etalk_gpui::messages_changed(cx)).detach();
    // A sign-out or another account ends the voice (and the worker).
    cx.observe(&state, |_, cx| etalk_gpui::session_changed(cx)).detach();
    // The agents' names label the tray's Call and the wake phrase.
    cx.observe(&agents, |_, cx| etalk_gpui::agents_changed(cx)).detach();
}

/// NEW-67: new voice settings arrived (a heartbeat, or this device's own
/// Settings → Voice save): eTalk adopts them; NEW-78 / NEW-55: when push to
/// talk or Speak replies moved here or away, the doctor's voice rows follow
/// now (the refresh nudges the beat), not a beat later.
pub(crate) fn set_voice_settings(settings: VoiceSettings, cx: &mut App) {
    let rows = crate::device_readiness::voice_rows(cx);
    etalk_gpui::set_settings(settings, cx);
    if crate::device_readiness::voice_rows(cx) != rows {
        refresh_doctor(cx);
    }
}

fn refresh_doctor(cx: &mut App) {
    if let Some(hub) = crate::coding_flow::CodingHub::global_ref(cx) {
        crate::coding_flow::CodingHub::refresh_doctor(&hub, cx);
    }
}

/// THIS device's `speech` doctor row (None = no row).
pub(crate) fn speech_state(cx: &App) -> Option<coding::device_doctor::SpeechState> {
    etalk_gpui::speak_replies::speech_state(cx).map(|state| match state {
        etalk_gpui::SpeechState::Ready => coding::device_doctor::SpeechState::Ready,
        etalk_gpui::SpeechState::Downloading => coding::device_doctor::SpeechState::Downloading,
        etalk_gpui::SpeechState::NoGpu => coding::device_doctor::SpeechState::NoGpu,
        etalk_gpui::SpeechState::Failed => coding::device_doctor::SpeechState::Failed,
    })
}

/// A failed push-to-talk send's text for the chat of session `session_id`
/// (the member's conversation with that agent), taken once by its composer.
pub(crate) fn take_unsent(session_id: &str, cx: &mut App) -> Option<String> {
    if !etalk_gpui::push_to_talk::has_unsent(cx) {
        return None;
    }
    let me = crate::queries::active_account(cx)?.user_id;
    let agent_id = Store::try_global(cx)?
        .collections()
        .sessions
        .read(cx)
        .iter()
        .find(|row| row.id == session_id && row.is_root() && row.user_id.as_deref() == Some(me.as_str()))?
        .agent_id
        .clone()?;
    etalk_gpui::push_to_talk::take_unsent(&agent_id, cx)
}

/// A send's answer the batch can never pass (4xx: the server refused it).
/// A 401 (an expired token, `Unauthorized` or a bare `Http 401`) is a failed
/// send, never a refusal: the batch waits for the refreshed token.
fn refused(err: &api::ApiError) -> bool {
    matches!(err, api::ApiError::Http { status, .. } if (400..500).contains(status) && !matches!(*status, 401 | 408 | 429))
}

fn send_error(err: api::ApiError) -> SendError {
    SendError { refused: refused(&err), message: err.to_string() }
}

/// `outgoing` as `sessions.send`'s input.
fn send_input(outgoing: Outgoing) -> api::sessions::SendInput {
    let mut input = match outgoing.target {
        UtteranceTarget::Session { session_id, text } => api::sessions::SendInput::message(&session_id, &text, Vec::new()),
        UtteranceTarget::Agent { team_id, agent_id, text } => crate::chat_launch::agent_send_input(&team_id, &agent_id, &text, Vec::new()),
    };
    if outgoing.meeting {
        input = input.meeting();
    }
    if outgoing.client_message_id.is_some() {
        input.client_message_id = outgoing.client_message_id;
    }
    input
}

/// The relay's call host socket as eTalk's link.
struct SteerLink(steer::call_host::CallHost);

impl CallLink for SteerLink {
    fn send_event(&self, event: serde_json::Value) {
        self.0.send_event(event);
    }

    fn send_speech(&self, pcm: Vec<i16>, rate: u32) {
        self.0.send_speech(pcm, rate);
    }

    fn flush(&self) {
        self.0.flush();
    }

    fn end(&self) {
        self.0.end();
    }
}

fn link_event(event: steer::call_host::CallHostEvent) -> CallLinkEvent {
    use steer::call_host::CallHostEvent;
    match event {
        CallHostEvent::Ready => CallLinkEvent::Ready,
        CallHostEvent::Audio(pcm) => CallLinkEvent::Audio(pcm),
        CallHostEvent::Mute(muted) => CallLinkEvent::Mute(muted),
        CallHostEvent::Ended { reason } => CallLinkEvent::Ended { reason },
        CallHostEvent::Dropped(reason) => CallLinkEvent::Dropped(reason),
    }
}

/// The voice device's desktop audio as its synced doctor reports it
/// (`doctor.desktopAudio`; absent = `none` and `[]`).
pub(crate) fn desktop_audio(device_id: &str, cx: &App) -> (DesktopAudioCapability, Vec<AudioApp>) {
    let Some(store) = Store::try_global(cx) else { return Default::default() };
    let devices = store.collections().devices.read(cx);
    let audio = devices
        .iter()
        .find(|row| row.device_id.as_deref() == Some(device_id))
        .and_then(|row| row.doctor.as_ref())
        .and_then(|doctor| doctor.get("desktopAudio"))
        .and_then(|audio| serde_json::from_value::<coding::device_doctor::DoctorDesktopAudio>(audio.clone()).ok());
    audio.map(|audio| (audio.capability, audio.apps)).unwrap_or_default()
}

impl TalkHost for ExponentialHost {
    fn brand(&self) -> Brand {
        Brand { name: "Exponential", tray_icon_svg: TRAY_ICON_SVG, app_id: crate::undock::CHANNEL_APP_ID, tray_id: "exponential-voice" }
    }

    fn data_dir(&self) -> PathBuf {
        self.data_dir.clone()
    }

    fn device_id(&self) -> String {
        steer::persistent_device_id(&self.data_dir)
    }

    fn account(&self, cx: &App) -> Option<String> {
        match Store::try_global(cx)?.session(cx) {
            sync::SessionPhase::Synced { account_id } => Some(account_id),
            _ => None,
        }
    }

    fn accounts(&self, cx: &App) -> SignedIn {
        let auth = crate::session::AuthContext::global(cx);
        let all = auth.auth.signed_in_accounts().into_iter().map(|account| account.id).collect();
        SignedIn { active: crate::queries::active_account(cx).map(|account| account.id), all }
    }

    fn agent(&self, agent_id: &str, cx: &App) -> Option<TalkAgent> {
        let agent = crate::session_data::agent_def(agent_id, cx)?;
        Some(TalkAgent {
            name: agent.name.clone().unwrap_or_default(),
            title: crate::agent_screen::agent_def_name(&agent),
            team_id: agent.team_id.clone().unwrap_or_default(),
            icon: agent.icon.clone(),
            color: agent.color.clone(),
        })
    }

    fn conversation(&self, agent_id: &str, cx: &App) -> Option<String> {
        crate::session_data::my_conversation(agent_id, cx).map(|row| row.id)
    }

    fn messages(&self, cx: &App, visit: &mut dyn FnMut(&TalkMessage<'_>)) -> bool {
        let Some(store) = Store::try_global(cx) else { return false };
        let collections = store.collections();
        let sessions = domain::message_conversation::conversation_sessions(collections.conversations.read(cx).iter());
        let rows = collections.messages.read(cx);
        for row in rows.iter() {
            visit(&TalkMessage {
                id: &row.id,
                message: VoiceMessage {
                    author_kind: row.author_kind.as_deref(),
                    body: row.body.as_deref(),
                    has_card: row.card.as_ref().is_some_and(|card| !card.is_null()),
                    author_session_id: row.author_session_id.as_deref(),
                    session_id: domain::message_conversation::session_of(row, &sessions),
                    created_at: row.created_at.as_deref(),
                },
            });
        }
        rows.is_ready()
    }

    fn send(&self, outgoing: Outgoing, cx: &App) -> Option<SendJob> {
        let trpc = crate::queries::trpc_client(cx)?;
        let input = send_input(outgoing);
        Some(Box::new(move || {
            api::sessions::send(&trpc, &input)
                .map(|output| Sent { session_id: output.session_id, message_id: output.message_id })
                .map_err(send_error)
        }))
    }

    fn write(&self, write: VoiceWrite, done: Box<dyn FnOnce(&mut App)>, cx: &mut App) -> bool {
        let Some(trpc) = crate::queries::trpc_client(cx) else { return false };
        let (verb, noun) = match write {
            VoiceWrite::WakeWord(_) => ("save", "the voice settings"),
            VoiceWrite::StartMeeting => ("start", "meeting mode"),
            VoiceWrite::StopMeeting(_) => ("stop", "meeting mode"),
        };
        cx.spawn(async move |cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    match write {
                        VoiceWrite::WakeWord(on) => {
                            api::voice::voice_update(&trpc, &api::voice::VoiceUpdate { wake_word: Some(on), ..Default::default() })
                        }
                        VoiceWrite::StartMeeting => api::voice::voice_start_meeting(&trpc),
                        VoiceWrite::StopMeeting(reason) => api::voice::voice_stop_meeting(&trpc, reason),
                    }
                })
                .await;
            cx.update(|cx| {
                match result {
                    // The speaker adopts it (the tray and the meeting host follow).
                    Ok(stored) => set_voice_settings(stored, cx),
                    Err(err) => crate::action_error::failed_action_err(verb, noun, &err, cx),
                }
                done(cx);
            });
        })
        .detach();
        true
    }

    fn connect_call(&self, call_id: &str, cx: &mut App) -> Option<Task<Result<CallConnection, String>>> {
        let trpc = crate::queries::trpc_client(cx)?;
        let runtime = crate::steer_wiring::runtime(cx)?;
        let call_id = call_id.to_string();
        Some(cx.spawn(async move |cx| {
            let ticket = cx
                .background_executor()
                .spawn(async move { api::voice::voice_call_host_ticket(&trpc, &call_id) })
                .await
                .map_err(|err| err.to_string())?;
            let (tx, events) = flume::unbounded::<CallLinkEvent>();
            let host = steer::call_host::connect(&runtime, ticket.url, move |event| {
                let _ = tx.send(link_event(event));
            });
            Ok(CallConnection { link: Box::new(SteerLink(host)), events })
        }))
    }

    fn device(&self, device_id: &str, cx: &App) -> Option<TalkDevice> {
        let store = Store::try_global(cx)?;
        let devices = store.collections().devices.read(cx);
        let row = devices.iter().find(|row| row.device_id.as_deref() == Some(device_id))?;
        Some(TalkDevice {
            label: row.label.clone(),
            online: crate::device_settings::row_is_online(row.last_seen_at.as_deref(), chrono::Utc::now().timestamp_millis()),
        })
    }

    fn desktop_audio(&self, device_id: &str, cx: &App) -> (DesktopAudioCapability, Vec<AudioApp>) {
        desktop_audio(device_id, cx)
    }

    fn icon(&self, icon: TalkIcon<'_>) -> Icon {
        match icon {
            TalkIcon::Brand => Icon::from(ExpIcon::Logo),
            TalkIcon::Concept(name) => Icon::new(registry::concept_by_name(name).unwrap_or(registry::VOICE_CALL)),
            TalkIcon::Agent { icon, color } => crate::icons::agent_icon(Some(icon), color),
        }
    }

    fn open(&self, surface: Surface, cx: &mut App) {
        match surface {
            Surface::Window => {
                if let Some(window) = crate::os_notifications::shell_window(cx) {
                    let _ = window.update(cx, |_, window, _| window.activate_window());
                    return;
                }
                crate::window_hooks::request_shell_window(cx);
            }
            Surface::VoiceSettings => crate::os_notifications::open_settings(crate::settings::SettingsSection::Voice, cx),
            Surface::MeetingConsent => crate::meeting_mode::request_consent(cx),
        }
    }

    fn notify(&self, notice: Notice, cx: &mut App) {
        match notice {
            Notice::Error(text) => crate::toast::show_in_active_window(crate::toast::Toast::error(text), cx),
            Notice::Failed { verb, noun, reason } => crate::action_error::failed_action(verb, noun, reason, cx),
            Notice::MeetingAutoStopped { hours } => crate::meeting_mode::notify_auto_stopped(hours, cx),
        }
    }

    fn readiness_changed(&self, cx: &mut App) {
        refresh_doctor(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// meeting.json FLUSH: a refusal (4xx) drops the batch, never requeued;
    /// a transport error or 5xx retries.
    #[test]
    fn a_4xx_is_a_refusal_a_401_or_transport_error_retries() {
        let http = |status| api::ApiError::Http { status, message: "x".into() };
        assert!(refused(&http(400)) && refused(&http(403)) && refused(&http(413)));
        assert!(!refused(&http(408)) && !refused(&http(429)) && !refused(&http(500)) && !refused(&http(503)));
        assert!(!refused(&http(401)) && !refused(&api::ApiError::Unauthorized), "an expired token retries");
        assert!(!refused(&api::ApiError::Transport { message: "down".into(), offline: true }));
        assert!(send_error(http(413)).refused && !send_error(http(503)).refused);
    }

    /// F21: every meeting part names its `clientMessageId`.
    #[test]
    fn a_meeting_part_sends_its_client_message_id() {
        let target = UtteranceTarget::Session { session_id: "s1".into(), text: "hi".into() };
        let input = send_input(Outgoing { target: target.clone(), meeting: true, client_message_id: Some("0b7c".into()) });
        let json = serde_json::to_value(&input).unwrap();
        assert_eq!(json["clientMessageId"], "0b7c");
        assert_eq!(json["meeting"], true);
        let plain = serde_json::to_value(send_input(Outgoing { target, meeting: false, client_message_id: None })).unwrap();
        assert!(plain.get("clientMessageId").is_none());
        assert_eq!(plain, serde_json::to_value(api::sessions::SendInput::message("s1", "hi", Vec::new())).unwrap());
    }
}
