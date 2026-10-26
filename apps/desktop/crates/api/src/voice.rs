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

use serde::Serialize;

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speak_replies: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub push_to_talk: Option<bool>,
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
}

/// `voice.get` — query.
pub fn voice_get(trpc: &TrpcClient) -> Result<VoiceSettings, ApiError> {
    trpc.query("voice.get")
}

/// `voice.update` — mutation; returns what is stored now.
pub fn voice_update(trpc: &TrpcClient, update: &VoiceUpdate) -> Result<VoiceSettings, ApiError> {
    trpc.mutation("voice.update", update)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_update_sends_only_what_changed_and_explicit_nulls() {
        let update = VoiceUpdate { agent_id: Some(None), speak_replies: Some(true), ..Default::default() };
        assert_eq!(serde_json::to_string(&update).unwrap(), r#"{"agentId":null,"speakReplies":true}"#);
    }

    #[test]
    fn settings_decode_with_defaults_for_missing_fields() {
        let settings: VoiceSettings = serde_json::from_str(r#"{"voiceDeviceId":"d-mint","speakReplies":true}"#).unwrap();
        assert_eq!(settings.voice_device_id.as_deref(), Some("d-mint"));
        assert!(settings.speak_replies);
        assert!(settings.agent_id.is_none());
    }
}
