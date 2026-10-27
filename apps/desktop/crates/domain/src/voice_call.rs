//! NEW-55 (voice V4): caller mode, pure. Byte-locked by
//! `fixtures/voice-call.json` (its `$comment` is the spec); the TS reference
//! is `packages/domain-contract/src/voice-call.ts`. The same machine runs on
//! the voice device's IDE and (V5/V6) on a caller, which receives the
//! host's events as `call_event` frames.

use std::collections::HashMap;
use std::sync::LazyLock;

use serde::Deserialize;

const VOICE_CALL_JSON: &str = include_str!("../../../../../packages/domain-contract/fixtures/voice-call.json");

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallReasons {
    pub no_voice_device: String,
    pub speak_off: String,
    pub offline: String,
    pub not_here: String,
    pub preparing: String,
    pub no_gpu: String,
    pub busy: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallCopy {
    pub call: String,
    pub connecting: String,
    pub listening: String,
    pub thinking: String,
    pub speaking: String,
    pub muted: String,
    pub muted_line: String,
    pub mute: String,
    pub unmute: String,
    pub end_call: String,
    pub queued: String,
    pub reasons: CallReasons,
    pub failed: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CallIcons {
    pub call: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CallTestIds {
    pub call: String,
    pub surface: String,
    pub mute: String,
    pub end: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallConstants {
    pub ring_timeout_ms: u64,
    pub min_silence_ms: u64,
    pub barge_in_ms: u64,
    pub frame_ms: u32,
    pub up_rate: u32,
    pub down_rate: u32,
    pub up_kbps: u32,
    pub down_kbps: u32,
    pub jitter_ms: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CallKinds {
    pub up: u8,
    pub down: u8,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallWire {
    pub kinds: CallKinds,
    pub max_frame_bytes: usize,
}

/// One styleguide row of the call surface (`surfaceSpecimens`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SurfaceSpecimen {
    pub activity: String,
    pub level: f32,
    pub bands: Vec<f32>,
    pub state: String,
    pub line: String,
    pub mute: String,
}

/// What the other side's `call_end` does (`hangUpRules`).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HangUpRules {
    /// The failure code a `call_end` WITH a reason ends as.
    pub failed_code: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceCallSpec {
    pub copy: CallCopy,
    pub surface_specimens: Vec<SurfaceSpecimen>,
    pub icons: CallIcons,
    pub test_ids: CallTestIds,
    pub constants: CallConstants,
    pub wire: CallWire,
    pub hang_up_rules: HangUpRules,
}

pub static VOICE_CALL: LazyLock<VoiceCallSpec> =
    LazyLock::new(|| serde_json::from_str(VOICE_CALL_JSON).expect("voice-call.json parses"));

/// Where the telephone is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallPlatform {
    Desktop,
    Phone,
    Web,
}

/// The voice device's doctor row `speech` as a caller reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallSpeech {
    Ok,
    Preparing,
    NoGpu,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallAvailabilityInput<'a> {
    pub platform: CallPlatform,
    /// The voice device's name; None = none picked.
    pub device: Option<&'a str>,
    pub speak_replies: bool,
    pub online: bool,
    /// THIS client is the voice device.
    pub here: bool,
    /// None = no `speech` row yet.
    pub speech: Option<CallSpeech>,
    /// A call already runs on the voice device.
    pub busy: bool,
}

/// None = the telephone is enabled; Some = disabled, with the reason.
pub fn call_unavailable(input: &CallAvailabilityInput<'_>) -> Option<String> {
    let reasons = &VOICE_CALL.copy.reasons;
    let off = |reason: &str| Some(reason.replace("{device}", input.device.unwrap_or_default()));
    if input.device.is_none() {
        return off(&reasons.no_voice_device);
    }
    if !input.speak_replies {
        return off(&reasons.speak_off);
    }
    if !input.online {
        return off(&reasons.offline);
    }
    if input.platform == CallPlatform::Desktop && !input.here {
        return off(&reasons.not_here);
    }
    match input.speech {
        None | Some(CallSpeech::Preparing) => return off(&reasons.preparing),
        Some(CallSpeech::NoGpu) => return off(&reasons.no_gpu),
        Some(CallSpeech::Ok) => {}
    }
    if input.busy {
        return off(&reasons.busy);
    }
    None
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CallPhase {
    #[default]
    Hidden,
    Connecting,
    Listening,
    Thinking,
    Speaking,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CallState {
    pub phase: CallPhase,
    pub muted: bool,
    pub device: String,
    /// The last utterance, quoted (what Thinking shows).
    pub said: String,
    pub line: String,
    /// Set when the call just ended with a failure.
    pub toast: Option<String>,
}

/// One flow event (`flowCases` events; a caller gets them as `call_event`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallEvent {
    Start { device: String },
    Connected,
    Partial(String),
    Said(String),
    SpeechStarted,
    Queued { count: usize, text: String },
    ReplyStarted(String),
    ReplyFinished,
    Mute,
    Unmute,
    End,
    /// The other side's `call_end`: a reason (`idle`, `max_length`,
    /// `dropped`) fails it with the toast; none ends it silently.
    HungUp(Option<String>),
    Failed(String),
}

fn quoted(text: &str) -> String {
    format!("“{text}”")
}

fn failed_state(state: &CallState, code: &str) -> CallState {
    let failed = &VOICE_CALL.copy.failed;
    let text = failed.get(code).or_else(|| failed.get("dropped")).cloned().unwrap_or_default();
    CallState { toast: Some(text.replace("{device}", &state.device)), ..CallState::default() }
}

/// One step of the call.
pub fn call_step(state: &CallState, event: &CallEvent) -> CallState {
    let at = |phase: CallPhase, line: String| CallState { phase, line, toast: None, ..state.clone() };
    if let CallEvent::Start { device } = event {
        return CallState { phase: CallPhase::Connecting, device: device.clone(), ..CallState::default() };
    }
    if state.phase == CallPhase::Hidden {
        return state.clone();
    }
    match event {
        CallEvent::Start { .. } => unreachable!("handled above"),
        CallEvent::Connected if state.phase == CallPhase::Connecting => at(CallPhase::Listening, String::new()),
        CallEvent::Partial(text) if state.phase == CallPhase::Listening && !text.trim().is_empty() => {
            at(CallPhase::Listening, quoted(text.trim()))
        }
        CallEvent::Said(text) if !text.trim().is_empty() => {
            let said = quoted(text.trim());
            CallState { said: said.clone(), ..at(CallPhase::Thinking, said) }
        }
        CallEvent::SpeechStarted if state.phase == CallPhase::Speaking => at(CallPhase::Listening, String::new()),
        CallEvent::Queued { count, text } if matches!(state.phase, CallPhase::Thinking | CallPhase::Speaking) => {
            if *count == 0 {
                let line = if state.phase == CallPhase::Thinking { state.said.clone() } else { state.line.clone() };
                at(state.phase, line)
            } else {
                let line = VOICE_CALL.copy.queued.replace("{count}", &count.to_string()).replace("{text}", &quoted(text.trim()));
                at(state.phase, line)
            }
        }
        CallEvent::ReplyStarted(text) => at(CallPhase::Speaking, text.trim().to_string()),
        CallEvent::ReplyFinished if state.phase == CallPhase::Speaking => at(CallPhase::Listening, String::new()),
        CallEvent::Mute => CallState { muted: true, ..state.clone() },
        CallEvent::Unmute => CallState { muted: false, ..state.clone() },
        CallEvent::End | CallEvent::HungUp(None) => CallState::default(),
        CallEvent::HungUp(Some(_)) => failed_state(state, &VOICE_CALL.hang_up_rules.failed_code),
        CallEvent::Failed(code) => failed_state(state, code),
        _ => state.clone(),
    }
}

/// What the call surface shows (THE voice capsule on the desktop).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallView {
    /// VoiceWave activity: listening | thinking | speaking | muted.
    pub activity: &'static str,
    pub state: String,
    pub line: String,
    /// The mute pill's label; End call is always beside it.
    pub mute: String,
}

/// None = no call surface.
pub fn call_view(state: &CallState) -> Option<CallView> {
    let copy = &VOICE_CALL.copy;
    let mute = if state.muted { copy.unmute.clone() } else { copy.mute.clone() };
    let view = |activity: &'static str, label: &str, line: &str| CallView {
        activity,
        state: label.to_string(),
        line: line.to_string(),
        mute: mute.clone(),
    };
    Some(match state.phase {
        CallPhase::Hidden => return None,
        CallPhase::Connecting => view("thinking", &copy.connecting, &state.line),
        CallPhase::Listening if state.muted => view("muted", &copy.muted, &copy.muted_line),
        CallPhase::Listening => view("listening", &copy.listening, &state.line),
        CallPhase::Thinking => view("thinking", &copy.thinking, &state.line),
        CallPhase::Speaking => view("speaking", &copy.speaking, &state.line),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn fixture() -> Value {
        serde_json::from_str(VOICE_CALL_JSON).unwrap()
    }

    #[test]
    fn the_spec_parses() {
        assert_eq!(VOICE_CALL.copy.end_call, "End call");
        assert_eq!(VOICE_CALL.icons.call, "voice-call");
        assert_eq!(VOICE_CALL.wire.kinds.up, 1);
        assert_eq!(VOICE_CALL.constants.min_silence_ms, 250);
    }

    #[test]
    fn the_surface_specimens_read_the_call_copy() {
        let copy = &VOICE_CALL.copy;
        let states = [&copy.listening, &copy.thinking, &copy.speaking, &copy.muted, &copy.connecting];
        for specimen in &VOICE_CALL.surface_specimens {
            assert!(states.contains(&&specimen.state), "{}", specimen.state);
            assert!(specimen.mute == copy.mute || specimen.mute == copy.unmute, "{}", specimen.mute);
        }
    }

    #[test]
    fn availability_matches_the_fixture() {
        for case in fixture()["availabilityCases"].as_array().unwrap() {
            let input = &case["input"];
            let platform = match input["platform"].as_str().unwrap() {
                "desktop" => CallPlatform::Desktop,
                "phone" => CallPlatform::Phone,
                _ => CallPlatform::Web,
            };
            let speech = input["speech"].as_str().map(|speech| match speech {
                "ok" => CallSpeech::Ok,
                "noGpu" => CallSpeech::NoGpu,
                _ => CallSpeech::Preparing,
            });
            let reason = call_unavailable(&CallAvailabilityInput {
                platform,
                device: input["device"].as_str(),
                speak_replies: input["speakReplies"].as_bool().unwrap(),
                online: input["online"].as_bool().unwrap(),
                here: input["here"].as_bool().unwrap(),
                speech,
                busy: input["busy"].as_bool().unwrap(),
            });
            let expected = &case["expected"];
            assert_eq!(reason.is_none(), expected["enabled"].as_bool().unwrap(), "{}", case["name"]);
            assert_eq!(reason.as_deref(), expected["reason"].as_str(), "{}", case["name"]);
        }
    }

    fn event(value: &Value) -> CallEvent {
        let text = || value["text"].as_str().unwrap_or_default().to_string();
        match value["t"].as_str().unwrap() {
            "start" => CallEvent::Start { device: value["device"].as_str().unwrap().into() },
            "connected" => CallEvent::Connected,
            "partial" => CallEvent::Partial(text()),
            "said" => CallEvent::Said(text()),
            "speechStarted" => CallEvent::SpeechStarted,
            "queued" => CallEvent::Queued { count: value["count"].as_u64().unwrap() as usize, text: text() },
            "replyStarted" => CallEvent::ReplyStarted(text()),
            "replyFinished" => CallEvent::ReplyFinished,
            "mute" => CallEvent::Mute,
            "unmute" => CallEvent::Unmute,
            "end" => CallEvent::End,
            "hungUp" => CallEvent::HungUp(value["reason"].as_str().map(str::to_string)),
            "failed" => CallEvent::Failed(value["code"].as_str().unwrap().into()),
            other => panic!("unknown event {other}"),
        }
    }

    #[test]
    fn flow_matches_the_fixture() {
        for case in fixture()["flowCases"].as_array().unwrap() {
            let state = case["events"].as_array().unwrap().iter().fold(CallState::default(), |state, value| call_step(&state, &event(value)));
            let expected = &case["view"];
            match call_view(&state) {
                None => assert!(expected.is_null(), "{}: hidden, expected {expected}", case["name"]),
                Some(view) => {
                    assert_eq!(view.activity, expected["activity"].as_str().unwrap(), "{}", case["name"]);
                    assert_eq!(view.state, expected["state"].as_str().unwrap(), "{}", case["name"]);
                    assert_eq!(view.line, expected["line"].as_str().unwrap(), "{}", case["name"]);
                    assert_eq!(view.mute, expected["mute"].as_str().unwrap(), "{}", case["name"]);
                }
            }
            assert_eq!(state.toast.as_deref(), case["toast"].as_str(), "{}", case["name"]);
        }
    }
}
