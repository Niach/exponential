//! NEW-67 (voice plan v3): the member's voice settings, server-only like the
//! notification prefs (`fixtures/voice.json`).
//!
//! - `voice.get()` → the settings (query; the fixture's defaults when the
//!   member has none).
//! - `voice.update(partial)` → the stored settings (mutation; the server
//!   refuses a device that is not the member's desktop or an agent outside
//!   the member's teams, and applies clearOnNone).
//!
//! The same settings ride every `devices.heartbeat` as `voice`, which is how
//! the IDE learns of a change made on another client.

use serde::{Deserialize, Serialize};

pub use domain::voice::VoiceSettings;

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
    pub desktop_audio: Option<domain::meeting::DesktopAudio>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desktop_audio_apps: Option<Vec<domain::meeting::AudioApp>>,
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
    reason: Option<domain::meeting::MeetingStopReason>,
}

/// #NEW-57: `voice.stopMeeting` — mutation; returns what is stored now.
/// #FEED-163: `reason` (`autoStop` / `restart`) = an automatic stop, for
/// which the server posts the conversation note; None = a manual stop.
pub fn voice_stop_meeting(
    trpc: &TrpcClient,
    reason: Option<domain::meeting::MeetingStopReason>,
) -> Result<VoiceSettings, ApiError> {
    trpc.mutation("voice.stopMeeting", &StopMeetingInput { reason })
}

/// NEW-55: a call ticket (voice-call.json TRANSPORT): `url` is the relay
/// URL with the ticket in it, dialled as-is.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallTicket {
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CallHostTicketInput<'a> {
    call_id: &'a str,
}

/// NEW-55: `voice.callHostTicket` — mutation; the voice device answers the
/// relay's `call_ring` with it.
pub fn voice_call_host_ticket(trpc: &TrpcClient, call_id: &str) -> Result<CallTicket, ApiError> {
    trpc.mutation("voice.callHostTicket", &CallHostTicketInput { call_id })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_update_sends_only_what_changed_and_explicit_nulls() {
        let update = VoiceUpdate { agent_id: Some(None), ptt_shortcut: Some(String::new()), ..Default::default() };
        assert_eq!(serde_json::to_string(&update).unwrap(), r#"{"agentId":null,"pttShortcut":""}"#);
        let update = VoiceUpdate {
            desktop_audio: Some(domain::meeting::DesktopAudio::Include),
            desktop_audio_apps: Some(vec![domain::meeting::AudioApp { id: "z".into(), name: "Zoom".into() }]),
            ..Default::default()
        };
        assert_eq!(serde_json::to_string(&update).unwrap(), r#"{"desktopAudio":"include","desktopAudioApps":[{"id":"z","name":"Zoom"}]}"#);
    }

    #[test]
    fn an_automatic_stop_names_its_reason() {
        let input = StopMeetingInput { reason: Some(domain::meeting::MeetingStopReason::AutoStop) };
        assert_eq!(serde_json::to_string(&input).unwrap(), r#"{"reason":"autoStop"}"#);
        let input = StopMeetingInput { reason: Some(domain::meeting::MeetingStopReason::Restart) };
        assert_eq!(serde_json::to_string(&input).unwrap(), r#"{"reason":"restart"}"#);
        let update = VoiceUpdate { meeting_auto_stop_hours: Some(8), ..Default::default() };
        assert_eq!(serde_json::to_string(&update).unwrap(), r#"{"meetingAutoStopHours":8}"#);
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
