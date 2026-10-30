//! NEW-55 (voice V4): caller mode, pure. Byte-locked by
//! `fixtures/voice-call.json` (its `$comment` is the spec); the TS reference
//! is `packages/etalk/src/voice-call.ts`. The same machine runs on
//! the voice device's IDE and (V5/V6) on a caller, which receives the
//! host's events as `call_event` frames.

use std::collections::HashMap;
use std::sync::LazyLock;

use serde::Deserialize;

const VOICE_CALL_JSON: &str = include_str!("../../../../../packages/etalk/fixtures/voice-call.json");

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallReasons {
    pub no_voice_device: String,
    pub offline: String,
    pub not_here: String,
    pub preparing: String,
    pub no_gpu: String,
    pub busy: String,
    /// #NEW-57: a meeting runs on the voice device.
    pub meeting: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallCopy {
    pub call: String,
    pub calling: String,
    pub getting_ready: String,
    pub no_sound: String,
    pub listening: String,
    pub thinking: String,
    pub speaking: String,
    pub muted: String,
    pub mute: String,
    pub unmute: String,
    pub speaker: String,
    pub speaker_off: String,
    pub end_call: String,
    pub cancel: String,
    pub queued: String,
    /// S-3: what the device says once answered, by language.
    pub greeting: HashMap<String, String>,
    pub reasons: CallReasons,
    pub failed: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallIcons {
    pub call: String,
    pub mute: String,
    pub unmute: String,
    pub speaker: String,
    pub speaker_off: String,
    pub end: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CallTestIds {
    pub call: String,
    pub surface: String,
    pub mute: String,
    pub end: String,
    pub speaker: String,
    pub panel: String,
    pub pill: String,
    pub timer: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallConstants {
    /// EXP-1423: a ring's earliest answer is at least this long ([`call_ring_ms`]).
    pub min_ring_ms: u64,
    /// EXP-1423: up to this much more, drawn once per call by the host.
    pub ring_jitter_ms: u64,
    /// EXP-1423: the ring tone's cadence ([`call_ring_tone_on`]): on, then off.
    pub ring_tone_ms: u64,
    pub ring_gap_ms: u64,
    /// EXP-1423: the ring tone's two frequencies, sounding together.
    pub ring_low_hz: f32,
    pub ring_high_hz: f32,
    /// S-3: a voice still not ready after this fails the call `unavailable`.
    pub max_ring_ms: u64,
    /// S-3: the mic check's level floor during the ring.
    pub no_sound_level: f32,
    pub ring_timeout_ms: u64,
    pub max_frames_per_second: u32,
    pub max_call_ms: u64,
    pub idle_timeout_ms: u64,
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

/// S-3: the desktop's floating call pill (fixed size, never resized).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallPill {
    pub width: f32,
    pub height: f32,
    pub margin_px: f32,
}

/// One styleguide row of the call surface (`surfaceSpecimens`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SurfaceSpecimen {
    pub name: String,
    pub agent: String,
    /// None = a status line instead of the timer.
    pub timer: Option<String>,
    pub level: f32,
    pub bands: Vec<f32>,
    pub activity: String,
    pub status: Option<String>,
    pub caption: String,
    pub muted: bool,
    pub speaker: bool,
    pub buttons: Vec<CallButton>,
    pub end: String,
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
    pub pill: CallPill,
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
    pub online: bool,
    /// THIS client is the voice device.
    pub here: bool,
    /// None = no `speech` row yet.
    pub speech: Option<CallSpeech>,
    /// A call this client does not run already runs on the voice device.
    pub busy: bool,
    /// #NEW-57: a meeting runs on the voice device.
    pub meeting: bool,
}

/// None = the telephone is enabled; Some = disabled, with the reason. S-3:
/// a voice that is still downloading rings until it is ready.
pub fn call_unavailable(input: &CallAvailabilityInput<'_>) -> Option<String> {
    let reasons = &VOICE_CALL.copy.reasons;
    let off = |reason: &str| Some(reason.replace("{device}", input.device.unwrap_or_default()));
    if input.device.is_none() {
        return off(&reasons.no_voice_device);
    }
    if !input.online {
        return off(&reasons.offline);
    }
    if input.meeting {
        return off(&reasons.meeting);
    }
    if input.platform == CallPlatform::Desktop && !input.here {
        return off(&reasons.not_here);
    }
    match input.speech {
        None => return off(&reasons.preparing),
        Some(CallSpeech::NoGpu) => return off(&reasons.no_gpu),
        Some(CallSpeech::Ok | CallSpeech::Preparing) => {}
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
    Ringing,
    Listening,
    Thinking,
    Speaking,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallState {
    pub phase: CallPhase,
    pub muted: bool,
    /// Phones: the loudspeaker (false = the earpiece).
    pub speaker: bool,
    pub device: String,
    /// The greeting the device says once answered.
    pub greeting: String,
    /// The ring's earliest answer ([`call_ring_ms`]) passed.
    pub rang: bool,
    /// The worker confirmed it listens (`CallListening`).
    pub ready: bool,
    /// The microphone stayed silent during the ring.
    pub no_sound: bool,
    /// The last utterance, quoted (what Thinking shows).
    pub said: String,
    pub line: String,
    /// Set when the call just ended with a failure.
    pub toast: Option<String>,
}

/// `CALL_IDLE`.
impl Default for CallState {
    fn default() -> Self {
        CallState {
            phase: CallPhase::Hidden,
            muted: false,
            speaker: true,
            device: String::new(),
            greeting: String::new(),
            rang: false,
            ready: false,
            no_sound: false,
            said: String::new(),
            line: String::new(),
            toast: None,
        }
    }
}

/// One flow event (`flowCases` events; a caller gets them as `call_event`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallEvent {
    Start { device: String, language: Option<String> },
    /// The ring's earliest answer ([`call_ring_ms`]) passed.
    Rang,
    /// The worker listens (`CallListening`).
    Ready,
    /// The microphone stayed silent during the ring.
    NoSound,
    Partial(String),
    Said(String),
    SpeechStarted,
    Queued { count: usize, text: String },
    ReplyStarted(String),
    ReplyFinished,
    Mute,
    Unmute,
    Speaker(bool),
    End,
    /// The other side's `call_end`: a reason (`idle`, `max_length`,
    /// `dropped`) fails it with the toast; none ends it silently.
    HungUp(Option<String>),
    Failed(String),
}

/// The greeting in `language` (English for an unknown one).
pub fn call_greeting(language: Option<&str>) -> String {
    let greetings = &VOICE_CALL.copy.greeting;
    greetings.get(language.unwrap_or("en")).or_else(|| greetings.get("en")).cloned().unwrap_or_default()
}

fn quoted(text: &str) -> String {
    format!("“{text}”")
}

fn failed_state(state: &CallState, code: &str) -> CallState {
    let failed = &VOICE_CALL.copy.failed;
    let text = failed.get(code).or_else(|| failed.get("dropped")).cloned().unwrap_or_default();
    CallState { toast: Some(text.replace("{device}", &state.device)), ..CallState::default() }
}

/// Rang and ready: the device speaks its greeting (not an agent turn).
fn answered(state: CallState) -> CallState {
    CallState { phase: CallPhase::Speaking, line: state.greeting.clone(), toast: None, ..state }
}

/// One step of the call (the same machine on the voice device and a caller).
pub fn call_step(state: &CallState, event: &CallEvent) -> CallState {
    let at = |phase: CallPhase, line: String| CallState { phase, line, toast: None, ..state.clone() };
    if let CallEvent::Start { device, language } = event {
        return CallState {
            phase: CallPhase::Ringing,
            device: device.clone(),
            greeting: call_greeting(language.as_deref()),
            ..CallState::default()
        };
    }
    if state.phase == CallPhase::Hidden {
        return state.clone();
    }
    let ringing = state.phase == CallPhase::Ringing;
    match event {
        CallEvent::Start { .. } => unreachable!("handled above"),
        CallEvent::Rang if ringing => {
            let state = CallState { rang: true, ..state.clone() };
            if state.ready { answered(state) } else { state }
        }
        CallEvent::Ready if ringing => {
            let state = CallState { ready: true, ..state.clone() };
            if state.rang { answered(state) } else { state }
        }
        CallEvent::NoSound => CallState { no_sound: true, ..state.clone() },
        CallEvent::Partial(text) if state.phase == CallPhase::Listening && !text.trim().is_empty() => {
            CallState { no_sound: false, ..at(CallPhase::Listening, quoted(text.trim())) }
        }
        CallEvent::Said(text) if !ringing && !text.trim().is_empty() => {
            let said = quoted(text.trim());
            CallState { said: said.clone(), no_sound: false, ..at(CallPhase::Thinking, said) }
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
        CallEvent::ReplyStarted(text) if !ringing => at(CallPhase::Speaking, text.trim().to_string()),
        CallEvent::ReplyFinished if state.phase == CallPhase::Speaking => at(CallPhase::Listening, String::new()),
        CallEvent::Mute => CallState { muted: true, ..state.clone() },
        CallEvent::Unmute => CallState { muted: false, ..state.clone() },
        CallEvent::Speaker(on) => CallState { speaker: *on, ..state.clone() },
        CallEvent::End | CallEvent::HungUp(None) => CallState::default(),
        CallEvent::HungUp(Some(_)) => failed_state(state, &VOICE_CALL.hang_up_rules.failed_code),
        CallEvent::Failed(code) => failed_state(state, code),
        _ => state.clone(),
    }
}

/// One round call button (`call-controls`), in `CallView::buttons` order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CallButton {
    Mute,
    Speaker,
    End,
}

/// What the call surface shows (the desktop's call panel and pill).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallView {
    /// ringing | listening | thinking | speaking | muted.
    pub activity: &'static str,
    /// The line under the name; None = the call's timer (`call_timer`).
    pub status: Option<String>,
    pub caption: String,
    pub muted: bool,
    pub speaker: bool,
    /// The round buttons, in order.
    pub buttons: Vec<CallButton>,
    /// The end button's label (shown under it while ringing).
    pub end: String,
}

/// None = no call surface.
pub fn call_view(state: &CallState, platform: CallPlatform) -> Option<CallView> {
    let copy = &VOICE_CALL.copy;
    let view = |activity: &'static str, status: Option<String>, caption: &str, buttons: Vec<CallButton>, end: &str| CallView {
        activity,
        status,
        caption: caption.to_string(),
        muted: state.muted,
        speaker: state.speaker,
        buttons,
        end: end.to_string(),
    };
    let buttons = || {
        if platform == CallPlatform::Phone {
            vec![CallButton::Mute, CallButton::Speaker, CallButton::End]
        } else {
            vec![CallButton::Mute, CallButton::End]
        }
    };
    let in_call = |activity: &'static str, caption: &str| view(activity, None, caption, buttons(), &copy.end_call);
    Some(match state.phase {
        CallPhase::Hidden => return None,
        CallPhase::Ringing => {
            let status = if state.rang { &copy.getting_ready } else { &copy.calling };
            view("ringing", Some(status.clone()), "", vec![CallButton::End], &copy.cancel)
        }
        CallPhase::Listening if state.muted => in_call("muted", &copy.muted),
        CallPhase::Listening => {
            let caption = if !state.line.is_empty() {
                state.line.as_str()
            } else if state.no_sound {
                copy.no_sound.as_str()
            } else {
                ""
            };
            in_call("listening", caption)
        }
        CallPhase::Thinking => in_call("thinking", &state.line),
        CallPhase::Speaking => in_call("speaking", &state.line),
    })
}

/// EXP-1423: the earliest answer of a ring, ms after it started:
/// `minRingMs` + floor(clamp(random, 0, 1) × `ringJitterMs`). The HOST
/// draws `random` once per call ([`call_ring_random`]); a caller never draws.
pub fn call_ring_ms(random: f64) -> u64 {
    let constants = &VOICE_CALL.constants;
    let unit = if random.is_nan() { 0.0 } else { random.clamp(0.0, 1.0) };
    constants.min_ring_ms + (unit * constants.ring_jitter_ms as f64).floor() as u64
}

/// EXP-1423: when the ring ends (answered), ms after it started: the later
/// of the earliest answer and the warm-up's `ready`; None = never answered
/// (no ready, or one at or past `maxRingMs`: failed `unavailable`).
pub fn call_answer_ms(random: f64, ready_ms: Option<u64>) -> Option<u64> {
    let ready = ready_ms.filter(|ready| *ready < VOICE_CALL.constants.max_ring_ms)?;
    Some(call_ring_ms(random).max(ready))
}

/// EXP-1423: whether the ring tone sounds `ms` after the ring started (the
/// shared cadence: `ringToneMs` on, `ringGapMs` off, looping).
pub fn call_ring_tone_on(ms: i64) -> bool {
    let constants = &VOICE_CALL.constants;
    ms >= 0 && (ms as u64) % (constants.ring_tone_ms + constants.ring_gap_ms) < constants.ring_tone_ms
}

/// EXP-1423: a fresh random in [0, 1) for [`call_ring_ms`] (the standard
/// library's per-process random hasher keys; no dependency).
pub fn call_ring_random() -> f64 {
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_nanos()));
    (hasher.finish() >> 11) as f64 / (1u64 << 53) as f64
}

/// The call's timer since it was answered: mm:ss, h:mm:ss from an hour.
pub fn call_timer(ms: u64) -> String {
    let total = ms / 1000;
    let (hours, minutes, seconds) = (total / 3600, (total % 3600) / 60, total % 60);
    if hours > 0 { format!("{hours}:{minutes:02}:{seconds:02}") } else { format!("{minutes:02}:{seconds:02}") }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn fixture() -> Value {
        serde_json::from_str(VOICE_CALL_JSON).unwrap()
    }

    fn platform(value: &Value) -> CallPlatform {
        match value.as_str().unwrap() {
            "desktop" => CallPlatform::Desktop,
            "phone" => CallPlatform::Phone,
            _ => CallPlatform::Web,
        }
    }

    #[test]
    fn the_spec_parses() {
        assert_eq!(VOICE_CALL.copy.end_call, "End call");
        assert_eq!(VOICE_CALL.icons.call, "voice-call");
        assert_eq!(VOICE_CALL.icons.end, "call-end");
        assert_eq!(VOICE_CALL.wire.kinds.up, 1);
        assert_eq!(VOICE_CALL.constants.min_silence_ms, 700);
        assert_eq!(VOICE_CALL.constants.min_ring_ms, 3000);
        assert_eq!(VOICE_CALL.constants.ring_jitter_ms, 6000);
        assert_eq!(VOICE_CALL.pill.width, 320.);
    }

    #[test]
    fn the_ring_matches_the_fixture() {
        let fixture = fixture();
        for case in fixture["ringCases"].as_array().unwrap() {
            assert_eq!(call_ring_ms(case["random"].as_f64().unwrap()), case["ringMs"].as_u64().unwrap(), "{case}");
        }
        for case in fixture["answerCases"].as_array().unwrap() {
            let answered = call_answer_ms(case["random"].as_f64().unwrap(), case["readyMs"].as_u64());
            assert_eq!(answered, case["answeredMs"].as_u64(), "{case}");
        }
        for case in fixture["ringToneCases"].as_array().unwrap() {
            assert_eq!(call_ring_tone_on(case["ms"].as_i64().unwrap()), case["on"].as_bool().unwrap(), "{case}");
        }
    }

    #[test]
    fn the_ring_random_is_a_unit() {
        for _ in 0..100 {
            let random = call_ring_random();
            assert!((0.0..1.0).contains(&random), "{random}");
        }
        assert_ne!(call_ring_random(), call_ring_random());
    }

    #[test]
    fn the_greeting_falls_back_to_english() {
        assert_eq!(call_greeting(Some("de")), "Hallo?");
        assert_eq!(call_greeting(Some("xx")), "Hello?");
        assert_eq!(call_greeting(None), "Hello?");
    }

    #[test]
    fn timer_matches_the_fixture() {
        for case in fixture()["timerCases"].as_array().unwrap() {
            assert_eq!(call_timer(case["ms"].as_u64().unwrap()), case["text"].as_str().unwrap(), "{case}");
        }
    }

    #[test]
    fn the_surface_specimens_read_the_call_copy() {
        let copy = &VOICE_CALL.copy;
        for specimen in &VOICE_CALL.surface_specimens {
            assert!(["ringing", "listening", "thinking", "speaking", "muted"].contains(&specimen.activity.as_str()), "{}", specimen.name);
            assert!(specimen.end == copy.end_call || specimen.end == copy.cancel, "{}", specimen.name);
            assert_eq!(specimen.status.is_none(), specimen.timer.is_some(), "{}", specimen.name);
        }
    }

    #[test]
    fn availability_matches_the_fixture() {
        for case in fixture()["availabilityCases"].as_array().unwrap() {
            let input = &case["input"];
            let speech = input["speech"].as_str().map(|speech| match speech {
                "ok" => CallSpeech::Ok,
                "noGpu" => CallSpeech::NoGpu,
                _ => CallSpeech::Preparing,
            });
            let reason = call_unavailable(&CallAvailabilityInput {
                platform: platform(&input["platform"]),
                device: input["device"].as_str(),
                online: input["online"].as_bool().unwrap(),
                here: input["here"].as_bool().unwrap(),
                speech,
                busy: input["busy"].as_bool().unwrap(),
                meeting: input["meeting"].as_bool().unwrap_or(false),
            });
            let expected = &case["expected"];
            assert_eq!(reason.is_none(), expected["enabled"].as_bool().unwrap(), "{}", case["name"]);
            assert_eq!(reason.as_deref(), expected["reason"].as_str(), "{}", case["name"]);
        }
    }

    fn event(value: &Value) -> CallEvent {
        let text = || value["text"].as_str().unwrap_or_default().to_string();
        match value["t"].as_str().unwrap() {
            "start" => CallEvent::Start {
                device: value["device"].as_str().unwrap().into(),
                language: value["language"].as_str().map(str::to_string),
            },
            "rang" => CallEvent::Rang,
            "ready" => CallEvent::Ready,
            "noSound" => CallEvent::NoSound,
            "partial" => CallEvent::Partial(text()),
            "said" => CallEvent::Said(text()),
            "speechStarted" => CallEvent::SpeechStarted,
            "queued" => CallEvent::Queued { count: value["count"].as_u64().unwrap() as usize, text: text() },
            "replyStarted" => CallEvent::ReplyStarted(text()),
            "replyFinished" => CallEvent::ReplyFinished,
            "mute" => CallEvent::Mute,
            "unmute" => CallEvent::Unmute,
            "speaker" => CallEvent::Speaker(value["on"].as_bool().unwrap()),
            "end" => CallEvent::End,
            "hungUp" => CallEvent::HungUp(value["reason"].as_str().map(str::to_string)),
            "failed" => CallEvent::Failed(value["code"].as_str().unwrap().into()),
            other => panic!("unknown event {other}"),
        }
    }

    #[test]
    fn flow_matches_the_fixture() {
        let cases = fixture()["flowCases"].as_array().unwrap().clone();
        assert!(cases.len() >= 41);
        for case in &cases {
            let name = &case["name"];
            let state = case["events"].as_array().unwrap().iter().fold(CallState::default(), |state, value| call_step(&state, &event(value)));
            let expected = &case["view"];
            match call_view(&state, platform(&case["platform"])) {
                None => assert!(expected.is_null(), "{name}: hidden, expected {expected}"),
                Some(view) => {
                    assert!(!expected.is_null(), "{name}: expected hidden, got {view:?}");
                    assert_eq!(view.activity, expected["activity"].as_str().unwrap(), "{name}");
                    assert_eq!(view.status.as_deref(), expected["status"].as_str(), "{name}");
                    assert_eq!(view.caption, expected["caption"].as_str().unwrap(), "{name}");
                    assert_eq!(view.muted, expected["muted"].as_bool().unwrap(), "{name}");
                    assert_eq!(view.speaker, expected["speaker"].as_bool().unwrap(), "{name}");
                    let buttons: Vec<CallButton> = serde_json::from_value(expected["buttons"].clone()).unwrap();
                    assert_eq!(view.buttons, buttons, "{name}");
                    assert_eq!(view.end, expected["end"].as_str().unwrap(), "{name}");
                }
            }
            assert_eq!(state.toast.as_deref(), case["toast"].as_str(), "{name}");
        }
    }
}
