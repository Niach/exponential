//! NEW-67 (voice plan v3): the member's voice settings, server-only like the
//! notification prefs (`fixtures/voice.json`).
//!
//! - `voice.get()` → the settings (query; the fixture's defaults when the
//!   member has none).
//! - `voice.update(partial)` → the stored settings (mutation; the server
//!   refuses a device that is not the member's desktop or an agent outside
//!   the member's teams, and applies clearOnNone).
//! - #EXP-1859 `voice.wakeAgent({agentId, on})` → the stored settings
//!   (mutation; one team agent's wake phrase on/off).
//!
//! The same settings ride every `devices.heartbeat` as `voice`, which is how
//! the IDE learns of a change made on another client.

use serde::{Deserialize, Serialize};

pub use etalk::voice::VoiceSettings;

use crate::{ApiError, TrpcClient};

/// A partial `voice.update` input: only the `Some` fields are sent. A field
/// holding `Some(None)` sends an explicit null (None chosen).
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceUpdate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice_device_id: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<Option<String>>,
    /// S-3: the push-to-talk binding (canonical `shortcutGrammar` text;
    /// empty = no key = off). The legacy `speakReplies` / `pushToTalk`
    /// inputs are never sent by this client.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ptt_shortcut: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wake_word: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wake_phrase: Option<String>,
    /// #NEW-73.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice_transcript: Option<String>,
    /// #NEW-57: `meeting.json` `desktopAudioValues` (never the meeting
    /// itself: [`voice_start_meeting`] / [`voice_stop_meeting`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desktop_audio: Option<etalk::meeting::DesktopAudio>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desktop_audio_apps: Option<Vec<etalk::meeting::AudioApp>>,
    /// #FEED-163: `meeting.json` `autoStop.hoursValues`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meeting_auto_stop_hours: Option<i64>,
}

/// `voice.get` — query.
pub fn voice_get(trpc: &TrpcClient) -> Result<VoiceSettings, ApiError> {
    trpc.query("voice.get")
}

/// `voice.update` — mutation; returns what is stored now.
pub fn voice_update(trpc: &TrpcClient, update: &VoiceUpdate) -> Result<VoiceSettings, ApiError> {
    trpc.mutation("voice.update", update)
}

#[derive(Debug, Clone, Serialize)]
struct StartMeetingInput {
    consent: bool,
}

/// #NEW-57: `voice.startMeeting {consent: true}` — mutation, sent only from
/// the consent dialog's Start; returns what is stored now.
pub fn voice_start_meeting(trpc: &TrpcClient) -> Result<VoiceSettings, ApiError> {
    trpc.mutation("voice.startMeeting", &StartMeetingInput { consent: true })
}

#[derive(Debug, Clone, Serialize)]
struct StopMeetingInput {
    /// None = a manual stop, sent as `{}` (the server's input is an
    /// optional object, so a bare `null` was a 400).
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<etalk::meeting::MeetingStopReason>,
}

/// #NEW-57: `voice.stopMeeting` — mutation; returns what is stored now.
/// #FEED-163: `reason` (`autoStop` / `restart`) = an automatic stop, for
/// which the server posts the conversation note; None = a manual stop.
pub fn voice_stop_meeting(
    trpc: &TrpcClient,
    reason: Option<etalk::meeting::MeetingStopReason>,
) -> Result<VoiceSettings, ApiError> {
    trpc.mutation("voice.stopMeeting", &StopMeetingInput { reason })
}

/// NEW-55: a call ticket (voice-call.json TRANSPORT): `url` is the relay
/// URL with the ticket in it, dialled as-is.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallTicket {
    pub url: String,
    /// #EXP-1941: the call's mode, as the server told the caller too (the
    /// call id vouches for it). Absent on an older server = today's wire.
    #[serde(default)]
    pub seal: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CallHostTicketInput<'a> {
    call_id: &'a str,
    /// EXP-1857: the answering desktop (any of the member's own).
    #[serde(skip_serializing_if = "Option::is_none")]
    device_id: Option<&'a str>,
    /// #EXP-1941: the ring's agent. The call id vouches for it: a host that
    /// names another agent than the caller dialled gets no ticket.
    #[serde(skip_serializing_if = "Option::is_none")]
    agent_id: Option<&'a str>,
}

/// NEW-55: `voice.callHostTicket` — mutation; a desktop the relay rang
/// (`call_ring`) answers with it. EXP-1857: `device_id` = this desktop's
/// own id (any of the member's desktops may be rung; None = the voice device).
pub fn voice_call_host_ticket(trpc: &TrpcClient, call_id: &str, device_id: Option<&str>) -> Result<CallTicket, ApiError> {
    trpc.mutation("voice.callHostTicket", &CallHostTicketInput { call_id, device_id, agent_id: None })
}

/// #EXP-1941: [`voice_call_host_ticket`] of a host that names its device AND
/// the ring's agent: the only host a sealed call id is answered for. Asked
/// FIRST, before the agent, its voice or its conversation are read.
pub fn voice_call_host_ticket_for(trpc: &TrpcClient, call_id: &str, device_id: &str, agent_id: &str) -> Result<CallTicket, ApiError> {
    trpc.mutation("voice.callHostTicket", &CallHostTicketInput { call_id, device_id: Some(device_id), agent_id: Some(agent_id) })
}

/// #EXP-1981: `callId` is the ring's server call id (a streamed call) or
/// ABSENT (a call a desktop runs itself): the server takes no empty one.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CallSessionInput<'a> {
    agent_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    call_id: Option<&'a str>,
}

impl<'a> CallSessionInput<'a> {
    fn new(agent_id: &'a str, call_id: Option<&'a str>) -> Self {
        Self { agent_id, call_id: call_id.filter(|id| !id.is_empty()) }
    }
}

/// #EXP-1961: `voice.callSession`'s answer: the call's own session (a child
/// of the chat).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallSession {
    pub session_id: String,
    pub chat_id: String,
    #[serde(default)]
    pub resumed: bool,
}

/// #EXP-1961: `voice.callSession` — mutation; the host asks while it rings
/// (#EXP-1981: `call_id` = the ring's, None = a call run on this desktop).
/// `None` = no call session (no chat yet, no room): the old path.
pub fn voice_call_session(trpc: &TrpcClient, agent_id: &str, call_id: Option<&str>) -> Result<Option<CallSession>, ApiError> {
    trpc.mutation("voice.callSession", &CallSessionInput::new(agent_id, call_id))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CallEndedInput<'a> {
    session_id: &'a str,
}

#[derive(Debug, Clone, Deserialize)]
struct CallEnded {}

/// #EXP-1961: `voice.callEnded` — mutation; the hang-up (its summary follows).
pub fn voice_call_ended(trpc: &TrpcClient, session_id: &str) -> Result<(), ApiError> {
    trpc.mutation::<_, CallEnded>("voice.callEnded", &CallEndedInput { session_id }).map(|_| ())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StartCallInput<'a> {
    agent_id: &'a str,
    platform: &'a str,
    device_id: &'a str,
    /// #EXP-1941: the claim "this caller can seal" (it has a key holder).
    /// Not sent when false: today's input.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    seal: bool,
    /// #EXP-1859: skipped when None, so an older server still accepts it.
    #[serde(skip_serializing_if = "Option::is_none")]
    mode: Option<StartCallMode>,
    /// #EXP-1859: the wake phrase that opened a `turn` (at most 64 chars).
    #[serde(skip_serializing_if = "Option::is_none")]
    phrase: Option<&'a str>,
}

/// #EXP-1859: `voice.startCall`'s `mode`: a full call, or ONE turn opened by
/// an agent's wake phrase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StartCallMode {
    Call,
    Turn,
}

/// EXP-1857: `voice.startCall`'s answer: the caller's ticket URL and the
/// desktop it rings (the server's `callTarget`).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartedCall {
    pub call_id: String,
    pub url: String,
    pub device_id: String,
    pub device: String,
    #[serde(default)]
    pub fallback: bool,
    /// #EXP-1941: the call's mode: the claim AND the target's cap, decided
    /// once by the server and told to the host too. Absent = today's wire.
    #[serde(default)]
    pub seal: bool,
}

/// EXP-1857: `voice.startCall` — mutation; the desktop IDE as a CALLER
/// (platform `desktop`, `device_id` = this desktop, the first fallback).
/// Refused (`PRECONDITION_FAILED`, the availability reason as the message)
/// when no desktop speaks, `BAD_REQUEST` when this desktop is the target.
pub fn voice_start_call(trpc: &TrpcClient, agent_id: &str, device_id: &str) -> Result<StartedCall, ApiError> {
    voice_start_call_claiming(trpc, agent_id, device_id, false)
}

/// #EXP-1941: [`voice_start_call`] with the claim. `seal` = this caller can
/// seal: its key holder is up (one that is still starting is waited for
/// first, `coding::sealing`'s service wait; only a caller that can never
/// hold keys dials without the claim). The answer's `seal` is the mode.
pub fn voice_start_call_claiming(trpc: &TrpcClient, agent_id: &str, device_id: &str, seal: bool) -> Result<StartedCall, ApiError> {
    trpc.mutation("voice.startCall", &StartCallInput { agent_id, platform: "desktop", device_id, seal, mode: None, phrase: None })
}

/// #EXP-1859: [`voice_start_call_claiming`] as ONE `Turn` opened by a heard
/// wake phrase (`phrase` = the listener's); the claim holds as for a call.
pub fn voice_start_turn(trpc: &TrpcClient, agent_id: &str, device_id: &str, phrase: &str, seal: bool) -> Result<StartedCall, ApiError> {
    trpc.mutation("voice.startCall", &StartCallInput { agent_id, platform: "desktop", device_id, seal, mode: Some(StartCallMode::Turn), phrase: Some(phrase) })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CallConversationInput<'a> {
    agent_id: &'a str,
    seal: &'a crate::sessions::SendSeal,
    call_id: &'a str,
    device_id: &'a str,
}

/// #EXP-1941: `voice.callConversation`'s answer, what `sendTarget` answers:
/// the root's STORED key (the caller's own seal's only when it wrote it).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallConversation {
    pub root_id: String,
    pub epoch: u32,
    pub public_key: String,
}

/// #EXP-1941: `voice.callConversation` — mutation; a HOST of a sealed call
/// creates the chat of a first call with its key before any audio (a
/// text-less `sessions.send` is refused). It runs only inside a call the
/// server vouched for (`call_id`: a sealed id minted for this member, this
/// device and this agent), posts no message, and writes a key only when the
/// root has none.
pub fn voice_call_conversation(
    trpc: &TrpcClient,
    agent_id: &str,
    call_id: &str,
    device_id: &str,
    seal: &crate::sessions::SendSeal,
) -> Result<CallConversation, ApiError> {
    trpc.mutation("voice.callConversation", &CallConversationInput { agent_id, seal, call_id, device_id })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WakeAgentInput<'a> {
    agent_id: &'a str,
    on: bool,
}

/// #EXP-1859: `voice.wakeAgent({agentId, on})` — mutation; the `wake-word`
/// row's flip (agent-settings.json `voice.wake` `wakeToggle`), answering the
/// new settings. Refused with `BAD_REQUEST` (the copy as the message: too
/// short, clash, cap) or `NOT_FOUND` (not a team agent of the member's teams).
pub fn voice_wake_agent(trpc: &TrpcClient, agent_id: &str, on: bool) -> Result<VoiceSettings, ApiError> {
    trpc.mutation("voice.wakeAgent", &WakeAgentInput { agent_id, on })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #EXP-1941: the chat of a first call.
    #[test]
    fn the_call_conversation_names_the_vouched_call() {
        let seal = crate::sessions::SendSeal { install_id: "i1".into(), public_key: "k".into(), epoch: 1, wraps: Vec::new() };
        let input = CallConversationInput { agent_id: "a1", seal: &seal, call_id: "c1", device_id: "d-mac" };
        assert_eq!(
            serde_json::to_string(&input).unwrap(),
            r#"{"agentId":"a1","seal":{"installId":"i1","publicKey":"k","epoch":1,"wraps":[]},"callId":"c1","deviceId":"d-mac"}"#
        );
        let answer: CallConversation = serde_json::from_str(r#"{"rootId":"r1","epoch":1,"publicKey":"k"}"#).unwrap();
        assert_eq!(answer, CallConversation { root_id: "r1".into(), epoch: 1, public_key: "k".into() });
    }

    /// EXP-1857: the host ticket names the answering desktop; the caller
    /// is a desktop with its own id.
    #[test]
    fn call_inputs_name_this_desktop() {
        let host = CallHostTicketInput { call_id: "c1", device_id: Some("d-mac"), agent_id: None };
        assert_eq!(serde_json::to_string(&host).unwrap(), r#"{"callId":"c1","deviceId":"d-mac"}"#);
        let old = CallHostTicketInput { call_id: "c1", device_id: None, agent_id: None };
        assert_eq!(serde_json::to_string(&old).unwrap(), r#"{"callId":"c1"}"#);
        let start = StartCallInput { agent_id: "a1", platform: "desktop", device_id: "d-mac", seal: false, mode: None, phrase: None };
        assert_eq!(serde_json::to_string(&start).unwrap(), r#"{"agentId":"a1","platform":"desktop","deviceId":"d-mac"}"#);
        // #EXP-1941: the claim and the ring's agent are new fields; without
        // them the inputs above are today's, byte for byte.
        let claiming = StartCallInput { agent_id: "a1", platform: "desktop", device_id: "d-mac", seal: true, mode: None, phrase: None };
        assert_eq!(serde_json::to_string(&claiming).unwrap(), r#"{"agentId":"a1","platform":"desktop","deviceId":"d-mac","seal":true}"#);
        let naming = CallHostTicketInput { call_id: "c1", device_id: Some("d-mac"), agent_id: Some("a1") };
        assert_eq!(serde_json::to_string(&naming).unwrap(), r#"{"callId":"c1","deviceId":"d-mac","agentId":"a1"}"#);
        let ticket: CallTicket = serde_json::from_str(r#"{"url":"wss://r/x?ticket=t","seal":true}"#).unwrap();
        assert!(ticket.seal && !serde_json::from_str::<CallTicket>(r#"{"url":"u"}"#).unwrap().seal, "an older server names no mode");
        let turn = StartCallInput { agent_id: "a1", platform: "desktop", device_id: "d-mac", seal: false, mode: Some(StartCallMode::Turn), phrase: Some("Hey Ops") };
        assert_eq!(serde_json::to_string(&turn).unwrap(), r#"{"agentId":"a1","platform":"desktop","deviceId":"d-mac","mode":"turn","phrase":"Hey Ops"}"#);
        assert_eq!(serde_json::to_string(&StartCallMode::Call).unwrap(), r#""call""#);
        let started: StartedCall = serde_json::from_str(
            r#"{"callId":"c1","ticket":"t","url":"wss://r/x?ticket=t","relayUrl":"wss://r","deviceId":"d-mint","device":"mint","fallback":true}"#,
        )
        .unwrap();
        assert_eq!((started.device.as_str(), started.fallback, started.url.as_str()), ("mint", true, "wss://r/x?ticket=t"));
        assert!(!started.seal, "an older server names no mode: today's wire");
    }

    /// #EXP-1859: the `wake-word` row's flip.
    #[test]
    fn wake_agent_input_names_the_agent_and_the_flip() {
        let input = WakeAgentInput { agent_id: "a-ops", on: true };
        assert_eq!(serde_json::to_string(&input).unwrap(), r#"{"agentId":"a-ops","on":true}"#);
    }

    #[test]
    fn an_update_sends_only_what_changed_and_explicit_nulls() {
        let update = VoiceUpdate { agent_id: Some(None), ptt_shortcut: Some(String::new()), ..Default::default() };
        assert_eq!(serde_json::to_string(&update).unwrap(), r#"{"agentId":null,"pttShortcut":""}"#);
        let update = VoiceUpdate {
            desktop_audio: Some(etalk::meeting::DesktopAudio::Include),
            desktop_audio_apps: Some(vec![etalk::meeting::AudioApp { id: "z".into(), name: "Zoom".into() }]),
            ..Default::default()
        };
        assert_eq!(serde_json::to_string(&update).unwrap(), r#"{"desktopAudio":"include","desktopAudioApps":[{"id":"z","name":"Zoom"}]}"#);
    }

    #[test]
    fn an_automatic_stop_names_its_reason() {
        let input = StopMeetingInput { reason: Some(etalk::meeting::MeetingStopReason::AutoStop) };
        assert_eq!(serde_json::to_string(&input).unwrap(), r#"{"reason":"autoStop"}"#);
        let input = StopMeetingInput { reason: Some(etalk::meeting::MeetingStopReason::Restart) };
        assert_eq!(serde_json::to_string(&input).unwrap(), r#"{"reason":"restart"}"#);
        let update = VoiceUpdate { meeting_auto_stop_hours: Some(8), ..Default::default() };
        assert_eq!(serde_json::to_string(&update).unwrap(), r#"{"meetingAutoStopHours":8}"#);
    }

    /// #EXP-1981: the server takes `callId` absent or non-empty: a streamed
    /// call names the ring's id, a call this desktop runs itself none, and
    /// an empty id never goes out (it was a 400 on every IDE call).
    #[test]
    fn a_call_session_never_sends_an_empty_call_id() {
        let json = |call_id| serde_json::to_string(&CallSessionInput::new("a-1", call_id)).unwrap();
        assert_eq!(json(Some("c-1")), r#"{"agentId":"a-1","callId":"c-1"}"#);
        assert_eq!(json(None), r#"{"agentId":"a-1"}"#);
        assert_eq!(json(Some("")), r#"{"agentId":"a-1"}"#);
    }

    #[test]
    fn a_manual_stop_sends_an_empty_object_not_null() {
        let input = StopMeetingInput { reason: None };
        assert_eq!(serde_json::to_string(&input).unwrap(), "{}");
    }

    #[test]
    fn settings_decode_with_defaults_for_missing_fields() {
        // S-3: the legacy speakReplies / pushToTalk the server still answers
        // old clients are tolerated and ignored.
        let settings: VoiceSettings =
            serde_json::from_str(r#"{"voiceDeviceId":"d-mint","speakReplies":true,"pushToTalk":true,"pttShortcut":"f13"}"#).unwrap();
        assert_eq!(settings.voice_device_id.as_deref(), Some("d-mint"));
        assert_eq!(settings.ptt_shortcut, "f13");
        assert!(settings.agent_id.is_none());
    }
}
