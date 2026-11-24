//! NEW-55 (voice V4): caller mode, pure. Byte-locked by
//! `fixtures/voice-call.json` (its `$comment` is the spec); the TS reference
//! is `packages/etalk/src/voice-call.ts`. The same machine runs on
//! the voice device's IDE and (V5/V6) on a caller, which receives the
//! host's events as `call_event` frames.

use std::collections::HashMap;
use std::sync::LazyLock;

use serde::Deserialize;

/// #EXP-1992: the host side of a call (`hostCases`): the one machine the
/// desktop app and the CLI daemon run their calls on.
pub mod host;

pub const VOICE_CALL_JSON: &str = include_str!("../../../../../packages/etalk/fixtures/voice-call.json");

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallReasons {
    pub offline: String,
    pub preparing: String,
    pub no_gpu: String,
    /// NEW-99: the voice failed to load on the voice device.
    pub voice_failed: String,
    pub busy: String,
    /// #NEW-57: a meeting runs on the voice device.
    pub meeting: String,
    /// EXP-1857: the member has no desktop at all.
    pub no_desktop: String,
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
    /// EXP-1681: the phones' minimize button (the pill's way in).
    pub minimize: String,
    /// EXP-1857: the device that speaks while a call falls back.
    pub via: String,
    pub queued: String,
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
    pub minimize: String,
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
    pub minimize: String,
    /// EXP-1857: the fallback's second status line.
    pub via: String,
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
    /// EXP-1929: the relay's beat (`call_alive` + a ping) once paired.
    pub alive_ms: u64,
    pub min_silence_ms: u64,
    pub barge_in_ms: u64,
    pub frame_ms: u32,
    pub up_rate: u32,
    pub down_rate: u32,
    pub up_kbps: u32,
    pub down_kbps: u32,
    pub jitter_ms: u32,
    /// EXP-1858: a server's speech worker exits after this long without a
    /// call (its GPU memory is free while idle; a ring loads it again).
    pub server_idle_ms: u64,
    /// EXP-1859: a turn call not connected (`call_ready`) after this long
    /// becomes the local turn on the caller.
    pub turn_dial_ms: u64,
    /// EXP-1859: a turn call hangs up when no reply started this long after its send.
    pub turn_reply_wait_ms: u64,
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

/// S-3: the floating call pill (fixed size, never resized); EXP-1681 the
/// phones' minimized call too.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallPill {
    pub width: f32,
    pub height: f32,
    pub margin_px: f32,
    /// EXP-1681: a phone pill's default bottom edge above the safe area's.
    pub phone_bottom_px: f32,
}

/// FEED-272: one tab of the call surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CallTab {
    /// The agent's main chat with the call docked as the pill's row.
    Chat,
    /// The call surface as before.
    Voice,
}

/// FEED-272 (`tabs`): the Chat / Voice tabs on top of the call surface.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallTabs {
    pub order: Vec<CallTab>,
    /// The tab a call opens on.
    pub initial: CallTab,
    pub copy: CallTabsText,
    pub test_ids: CallTabsTestIds,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CallTabsText {
    pub chat: String,
    pub voice: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CallTabsTestIds {
    pub tabs: String,
    pub chat: String,
    pub voice: String,
}

impl CallTabs {
    pub fn label(&self, tab: CallTab) -> &str {
        match tab {
            CallTab::Chat => &self.copy.chat,
            CallTab::Voice => &self.copy.voice,
        }
    }

    pub fn test_id(&self, tab: CallTab) -> &str {
        match tab {
            CallTab::Chat => &self.test_ids.chat,
            CallTab::Voice => &self.test_ids.voice,
        }
    }
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
    /// EXP-1857: the fallback's device line (None = the agent's own device).
    #[serde(default)]
    pub via: Option<String>,
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
pub struct CallSealPadStep {
    pub up: usize,
    pub down: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallSealEndReasons {
    /// No key within `handshake_ms`, or a `finish` that failed.
    pub no_key: String,
    /// Clear content, a deleted event or too many frames that do not open.
    pub seal: String,
}

/// #EXP-1941 (`seal`): the call seal's rules outside the bytes (those are
/// `enet_core::call_seal`'s, which replays `seal.vectors`).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallSealSpec {
    /// The device cap a host advertises while it has a key holder.
    pub cap: String,
    pub audio_pad_step: CallSealPadStep,
    pub control_pad_bytes: usize,
    pub max_plain_bytes: usize,
    /// A caller sends its hello again this often until a host hello is taken.
    pub hello_every_ms: u64,
    /// Not sealed this long after an end's own start: the call ends `no_key`.
    pub handshake_ms: u64,
    pub pull_every_ms: u64,
    /// Once sealed: this many audio frames in a row that do not open end the call.
    pub max_open_failures: u32,
    /// Question 9: an utterance that lost more than this on the way is not
    /// sent (`etalk::CALL_MAX_LOST_MS`). A starting value, to be set from a
    /// measurement on real calls before sealing is switched on.
    pub max_lost_ms: u32,
    /// The codes about the caller's OWN device: never taken from the wire.
    pub own_device_codes: Vec<String>,
    pub end_reasons: CallSealEndReasons,
    pub required: bool,
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
    /// FEED-272: the Chat / Voice tabs on top of the call surface.
    pub tabs: CallTabs,
    pub host_voice: HostVoice,
    pub call_session: CallSessionSpec,
    pub seal: CallSealSpec,
}

/// #EXP-1961 (`callSession`): the call's own session's host-side numbers.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallSessionSpec {
    pub resume_ms: u64,
    pub opening_wait_ms: u64,
}

/// #EXP-1961: what [`call_opening`] answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallOpening {
    /// Keep ringing.
    Wait,
    /// Pick up and say the agent's opening line.
    Speak,
    /// Pick up silently on Listening; that opening is never spoken.
    Listen,
}

/// #EXP-1961 (`callSession.openingCases`): the agent's own opening line
/// (its call session's first reply), against the host's `ready` (the voice
/// is in): before ready = wait; in by `ready + openingWaitMs` = speak;
/// none by then = listen.
pub fn call_opening(ready_at_ms: Option<u64>, opening_at_ms: Option<u64>, now_ms: u64) -> CallOpening {
    let Some(ready) = ready_at_ms else { return CallOpening::Wait };
    let deadline = ready + VOICE_CALL.call_session.opening_wait_ms;
    if opening_at_ms.is_some_and(|at| at < deadline) {
        return CallOpening::Speak;
    }
    if now_ms >= deadline { CallOpening::Listen } else { CallOpening::Wait }
}

/// FEED-276: how long a host's `ready` waits for the agent's own voice.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostVoice {
    pub wait_ms: u64,
}

/// FEED-276 (`hostVoice`): the agent's own voice on the call's host.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CallVoice {
    /// The agent has no clip: the built-in voice IS its voice.
    #[default]
    None,
    /// Its clip is on disk and the worker was prepared with it.
    Installed,
    /// Its clip landed during the ring; that Prepare is not confirmed yet.
    Installing,
    /// Its clip still downloads.
    Fetching,
    /// Its clip could not be fetched.
    Failed,
}

/// FEED-276: what [`call_voice_ready`] answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallVoiceReady {
    /// The agent's own voice speaks: `ready` may go.
    Ready,
    /// Keep ringing.
    Wait,
    /// `ready` goes with the built-in voice (the host logs it).
    BuiltIn,
}

/// FEED-276 (`hostVoice.cases`): whether a host's `ready` may go
/// `waited_ms` after the ring started, given the agent's voice: none or
/// installed = ready; failed = the built-in voice at once; fetching waits
/// until `hostVoice.waitMs`, then the built-in voice; installing waits
/// until then, then ready (the opening queues behind its Prepare).
pub fn call_voice_ready(voice: CallVoice, waited_ms: u64) -> CallVoiceReady {
    let bound = waited_ms >= VOICE_CALL.host_voice.wait_ms;
    match voice {
        CallVoice::None | CallVoice::Installed => CallVoiceReady::Ready,
        CallVoice::Failed => CallVoiceReady::BuiltIn,
        CallVoice::Fetching if bound => CallVoiceReady::BuiltIn,
        CallVoice::Installing if bound => CallVoiceReady::Ready,
        CallVoice::Fetching | CallVoice::Installing => CallVoiceReady::Wait,
    }
}

pub static VOICE_CALL: LazyLock<VoiceCallSpec> =
    LazyLock::new(|| serde_json::from_str(VOICE_CALL_JSON).expect("voice-call.json parses"));

// --- #EXP-1941: the call seal's handshake steps (`seal.modeCases`) ----------
//
// One pure step for both ends of a call the server marked sealed (TS:
// `callSealStep`). It decides WHAT happens; the end does it: the key source,
// `finish` and the frames live in `steer::call_host` and `steer::call_caller`
// (the sealer is `enet_core::call_seal`). The order is Noise NNpsk2's: the
// caller's hello, the host's hello with its tag (opened inside the caller's
// `finish`), then the caller's first frame that opens.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CallSealRole {
    Caller,
    Host,
}

/// `Clear` = a call the server did not mark sealed: today's wire, not stepped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CallSealPhase {
    Idle,
    Clear,
    Told,
    Offered,
    Keying,
    Sealed,
    Over,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CallSealState {
    pub phase: CallSealPhase,
    /// Sealed audio frames in a row that did not open.
    pub failures: u32,
}

pub const CALL_SEAL_IDLE: CallSealState = CallSealState { phase: CallSealPhase::Idle, failures: 0 };

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CallSealTimer {
    Handshake,
    MaxCall,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "t", rename_all = "camelCase")]
pub enum CallSealEvent {
    /// What the server told this end; the host's socket opens here.
    Start {
        seal: bool,
        #[serde(default)]
        required: Option<bool>,
    },
    Ready,
    /// A hello of the other end; `ok` = well-formed, checked before any `finish`.
    Hello { ok: bool },
    /// The key source gave up.
    NoKey,
    Key,
    Finished { ok: bool },
    /// A binary frame; `opens` = the sealer opened it.
    Frame { opens: bool },
    /// A `sealed` event; `opens` = the sealer opened it.
    Sealed { opens: bool },
    /// Clear content: a `call_event` that is neither `hello`, `sealed` nor
    /// `failed`, or a `call_mute`.
    Clear,
    /// A clear `failed {code}`, or the code of the relay's `error` / `bye`.
    Failed { code: String },
    /// The socket closed, or `call_end`, `bye`, `error`.
    Closed,
    Timeout { timer: CallSealTimer },
}

/// A wire code about the caller's OWN device reads `dropped`.
pub fn call_seal_wire_code(code: &str) -> String {
    if VOICE_CALL.seal.own_device_codes.iter().any(|own| own == code) { VOICE_CALL.hang_up_rules.failed_code.clone() } else { code.to_string() }
}

/// The next state and what the end does now, in order: `timer`, `hello`,
/// `helloStop`, `key`, `finish`, `go`, `count`, `end` / `end:<reason>`,
/// `fail:<code>`, `abandon` (voice-call.ts names each).
pub fn call_seal_step(role: CallSealRole, state: CallSealState, event: &CallSealEvent) -> (CallSealState, Vec<String>) {
    use CallSealEvent as E;
    use CallSealPhase as P;
    let spec = &VOICE_CALL.seal;
    let (no_key, seal) = (spec.end_reasons.no_key.as_str(), spec.end_reasons.seal.as_str());
    let dropped = VOICE_CALL.hang_up_rules.failed_code.as_str();
    let caller = role == CallSealRole::Caller;
    let phase = state.phase;
    let stay = || (state, Vec::new());
    let to = |phase: P, actions: &[&str]| (CallSealState { phase, failures: 0 }, actions.iter().map(|action| action.to_string()).collect::<Vec<_>>());
    // Every seal failure reads `dropped` on the caller; the host just ends.
    let over = |reason: &str| {
        let fail = format!("fail:{dropped}");
        let end = format!("end:{reason}");
        if caller { to(P::Over, &[&end, &fail, "abandon"]) } else { to(P::Over, &[&end, "abandon"]) }
    };
    let count = || (state, vec!["count".to_string()]);

    if let E::Start { seal, required } = event {
        if phase != P::Idle {
            return stay();
        }
        if *seal {
            return to(P::Told, if caller { &[] } else { &["timer"] });
        }
        if !required.unwrap_or(spec.required) {
            return to(P::Clear, &[]);
        }
        return if caller { to(P::Over, &[&format!("fail:{dropped}")]) } else { to(P::Over, &["end"]) };
    }
    // The host ends a call it answered at `maxCallMs` itself, sealed or clear.
    if matches!(event, E::Timeout { timer: CallSealTimer::MaxCall }) {
        return if !caller && !matches!(phase, P::Idle | P::Over) { to(P::Over, &["end", "abandon"]) } else { stay() };
    }
    if matches!(phase, P::Idle | P::Clear | P::Over) {
        return stay();
    }
    let sealed = phase == P::Sealed;
    match event {
        E::Start { .. } => unreachable!("handled above"),
        E::Failed { code } => {
            let code = if caller { call_seal_wire_code(code) } else { code.clone() };
            to(P::Over, &[&format!("fail:{code}"), "abandon"])
        }
        E::Closed => to(P::Over, &["abandon"]),
        E::Timeout { .. } => if sealed { stay() } else { over(no_key) },
        E::Ready => if caller && phase == P::Told { to(P::Offered, &["timer", "hello"]) } else { stay() },
        E::Hello { ok } => {
            // The first well-formed hello is taken on arrival, once; another changes nothing.
            let waiting = phase == if caller { P::Offered } else { P::Told };
            if waiting && *ok {
                return if caller { to(P::Keying, &["helloStop", "key"]) } else { to(P::Keying, &["key"]) };
            }
            if !*ok && !sealed && (waiting || !caller) { count() } else { stay() }
        }
        E::NoKey => if phase == P::Keying { over(no_key) } else { stay() },
        E::Key => if phase == P::Keying { (state, vec!["finish".to_string()]) } else { stay() },
        E::Finished { ok } => {
            if phase != P::Keying {
                return stay();
            }
            if !*ok {
                return over(if caller { seal } else { no_key });
            }
            if caller { to(P::Sealed, &["go"]) } else { to(P::Offered, &["hello"]) }
        }
        E::Frame { opens } => {
            if sealed {
                if *opens {
                    return if state.failures == 0 { stay() } else { to(P::Sealed, &[]) };
                }
                let failures = state.failures + 1;
                return if failures >= spec.max_open_failures { over(seal) } else { (CallSealState { phase: P::Sealed, failures }, vec!["count".to_string()]) };
            }
            // A caller gets no binary frame before it is sealed; a host's
            // first frame that opens is the caller's proof.
            if caller {
                return if phase == P::Told { stay() } else { over(seal) };
            }
            if phase == P::Offered && *opens { to(P::Sealed, &["go"]) } else { count() }
        }
        E::Sealed { opens } => {
            if !caller {
                return stay();
            }
            if sealed {
                return if *opens { stay() } else { over(seal) };
            }
            if phase == P::Told { stay() } else { over(seal) }
        }
        E::Clear => {
            if sealed {
                return over(seal);
            }
            if caller {
                return if phase == P::Told { stay() } else { over(seal) };
            }
            count()
        }
    }
}

/// #EXP-1941: a host answers one call id ONCE. The server's check of a call
/// id keeps no state, so a vouched id could be rung again for as long as it
/// lives (two to three minutes): by the relay alone after a clear call, and
/// after a sealed one by the relay with anyone who holds the chat's key. A
/// host remembers the ids it got a ticket for longer than an id lives and
/// drops a ring that names one. An honest relay never rings an id twice and
/// a redial mints a new id, so this refuses nothing real. It is in the host
/// process: a restart forgets it.
pub const CALL_ANSWERED_ONCE_MS: u64 = 5 * 60 * 1000;

/// The call ids a host process answered, by its own clock in ms.
#[derive(Debug, Clone, Default)]
pub struct AnsweredCalls(Vec<(String, u64)>);

impl AnsweredCalls {
    /// `call_id` got its ticket at `now_ms`.
    pub fn note(&mut self, call_id: &str, now_ms: u64) {
        self.0.retain(|(_, at)| now_ms.saturating_sub(*at) < CALL_ANSWERED_ONCE_MS);
        self.0.push((call_id.to_string(), now_ms));
    }

    /// A ring for `call_id` at `now_ms` names a call answered before.
    pub fn answered(&self, call_id: &str, now_ms: u64) -> bool {
        self.0.iter().any(|(id, at)| id == call_id && now_ms.saturating_sub(*at) < CALL_ANSWERED_ONCE_MS)
    }
}

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
    /// NEW-99: the doctor row reads `error` (the voice failed to load).
    Failed,
}

/// NEW-99 (`speechCases`): the voice device's doctor row `speech` state as a
/// caller reads it; None = no row yet.
pub fn call_speech_of(state: Option<&str>) -> Option<CallSpeech> {
    Some(match state? {
        "ok" => CallSpeech::Ok,
        "missing" => CallSpeech::NoGpu,
        "error" => CallSpeech::Failed,
        _ => CallSpeech::Preparing,
    })
}

/// EXP-1991: ONE desktop, as [`call_target`] reads it for its reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallAvailabilityInput<'a> {
    /// The desktop's name.
    pub device: &'a str,
    pub online: bool,
    /// None = no `speech` row yet.
    pub speech: Option<CallSpeech>,
    /// #NEW-57: a meeting runs on it.
    pub meeting: bool,
}

/// None = the desktop can take the call; Some = it cannot, with the reason
/// (`availabilityCases`, first match wins). S-3: a voice that is still
/// downloading rings until it is ready.
pub fn call_unavailable(input: &CallAvailabilityInput<'_>) -> Option<String> {
    let reasons = &VOICE_CALL.copy.reasons;
    let off = |reason: &str| Some(reason.replace("{device}", input.device));
    if !input.online {
        return off(&reasons.offline);
    }
    if input.meeting {
        return off(&reasons.meeting);
    }
    match input.speech {
        None => off(&reasons.preparing),
        Some(CallSpeech::Failed) => off(&reasons.voice_failed),
        Some(CallSpeech::NoGpu) => off(&reasons.no_gpu),
        Some(CallSpeech::Ok | CallSpeech::Preparing) => None,
    }
}

/// EXP-1857: one of the member's own desktops as [`call_target`] reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallTargetDevice {
    pub id: String,
    pub name: String,
    pub online: bool,
    /// Its doctor row `speech` through [`call_speech_of`] (None = none yet).
    pub speech: Option<CallSpeech>,
    /// A meeting runs on it.
    pub meeting: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallTargetInput<'a> {
    /// THIS client's own desktop (None on phones and web).
    pub here: Option<&'a str>,
    /// The called agent's Runs on device.
    pub agent_device: Option<&'a str>,
    /// The member's own desktops, newest `lastSeenAt` first.
    pub desktops: &'a [CallTargetDevice],
    /// EXP-1858: the member's own servers (CLI daemons): a candidate only as
    /// the agent's device, never here nor a fallback.
    pub servers: &'a [CallTargetDevice],
    /// A call this client does not run already runs on the target.
    pub busy: bool,
}

/// Where a call's speech runs (`targetCases`); `device` None = disabled
/// with `reason`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallTarget {
    pub reason: Option<String>,
    /// The target desktop's id and name.
    pub device: Option<String>,
    pub name: Option<String>,
    /// The target is not the agent's own device.
    pub fallback: bool,
    /// The target is this client's own desktop: the IDE runs the call.
    pub local: bool,
}

impl CallTarget {
    pub fn enabled(&self) -> bool {
        self.reason.is_none()
    }
}

/// A desktop that can run a call now (a downloading voice rings until ready).
fn speaks(device: &CallTargetDevice) -> bool {
    device.online && !device.meeting && matches!(device.speech, Some(CallSpeech::Ok | CallSpeech::Preparing))
}

/// EXP-1857 (`targetCases`): where a call's speech runs. The first desktop
/// that speaks of: the agent's device (one of the member's own desktops,
/// EXP-1858: or servers), the desktop you're at, the other desktops newest
/// first. None speaks = disabled with the first candidate's reason.
pub fn call_target(input: &CallTargetInput<'_>) -> CallTarget {
    let by_id = |id: Option<&str>| id.and_then(|id| input.desktops.iter().find(|device| device.id == id));
    let agent_device =
        by_id(input.agent_device).or_else(|| input.agent_device.and_then(|id| input.servers.iter().find(|device| device.id == id)));
    let candidates: Vec<&CallTargetDevice> =
        agent_device.into_iter().chain(by_id(input.here)).chain(input.desktops.iter()).collect();
    let off = |reason: String| CallTarget { reason: Some(reason), device: None, name: None, fallback: false, local: false };
    let Some(target) = candidates.iter().copied().find(|device| speaks(device)) else {
        let Some(first) = candidates.first() else { return off(VOICE_CALL.copy.reasons.no_desktop.clone()) };
        let reason = call_unavailable(&CallAvailabilityInput {
            device: &first.name,
            online: first.online,
            speech: first.speech,
            meeting: first.meeting,
        });
        return off(reason.unwrap_or_default());
    };
    if input.busy {
        return off(VOICE_CALL.copy.reasons.busy.replace("{device}", &target.name));
    }
    CallTarget {
        reason: None,
        device: Some(target.id.clone()),
        name: Some(target.name.clone()),
        fallback: Some(target.id.as_str()) != input.agent_device,
        local: Some(target.id.as_str()) == input.here,
    }
}

/// #EXP-1878 (`tapCases`): what a press on the telephone does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallTap {
    pub call: bool,
    /// A phone's tap on a disabled telephone toasts its reason.
    pub toast: Option<String>,
}

/// #EXP-1878 (`tapCases`): an enabled telephone (`reason` None) calls; a
/// disabled one never does, and a PHONE's tap toasts the reason (web and
/// the desktop show it as the disabled control's tooltip).
pub fn call_tap(platform: CallPlatform, reason: Option<&str>) -> CallTap {
    match reason {
        None => CallTap { call: true, toast: None },
        Some(reason) => CallTap { call: false, toast: (platform == CallPlatform::Phone).then(|| reason.to_string()) },
    }
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
    /// EXP-1857: the device is not the agent's own (`copy.via` shows it).
    pub fallback: bool,
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
            fallback: false,
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
    /// EXP-1857: `fallback` = the device is not the agent's own.
    /// `language` stays on the wire for older callers; the machine ignores
    /// it since #EXP-1966 (no fixed greeting).
    Start { device: String, language: Option<String>, fallback: bool },
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

fn quoted(text: &str) -> String {
    format!("“{text}”")
}

fn failed_state(state: &CallState, code: &str) -> CallState {
    let failed = &VOICE_CALL.copy.failed;
    let text = failed.get(code).or_else(|| failed.get("dropped")).cloned().unwrap_or_default();
    CallState { toast: Some(text.replace("{device}", &state.device)), ..CallState::default() }
}

/// Rang and ready: it listens (#EXP-1966: no fixed greeting; the agent's
/// own opening line, if any, arrives as its first reply).
fn answered(state: CallState) -> CallState {
    CallState { phase: CallPhase::Listening, line: String::new(), toast: None, ..state }
}

/// One step of the call (the same machine on the voice device and a caller).
pub fn call_step(state: &CallState, event: &CallEvent) -> CallState {
    let at = |phase: CallPhase, line: String| CallState { phase, line, toast: None, ..state.clone() };
    if let CallEvent::Start { device, fallback, .. } = event {
        return CallState {
            phase: CallPhase::Ringing,
            device: device.clone(),
            fallback: *fallback,
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

/// The flow event as the caller receives it (`call_event {event}`,
/// voice-call.json `wire.control`): S-3 the ring's `rang`, the worker's
/// `ready` and the mic check's `noSound` too, so caller and host show the
/// same ring. None = an event the host never forwards (the caller's own
/// start, mute, speaker, end; a hang-up rides `call_end`; a host failure is
/// sent on its own before the `call_end`).
pub fn call_wire_event(event: &CallEvent) -> Option<serde_json::Value> {
    use serde_json::json;
    Some(match event {
        CallEvent::Rang => json!({ "t": "rang" }),
        CallEvent::Ready => json!({ "t": "ready" }),
        CallEvent::NoSound => json!({ "t": "noSound" }),
        CallEvent::Partial(text) => json!({ "t": "partial", "text": text }),
        CallEvent::Said(text) => json!({ "t": "said", "text": text }),
        CallEvent::SpeechStarted => json!({ "t": "speechStarted" }),
        CallEvent::Queued { count, text } => json!({ "t": "queued", "count": count, "text": text }),
        CallEvent::ReplyStarted(text) => json!({ "t": "replyStarted", "text": text }),
        CallEvent::ReplyFinished => json!({ "t": "replyFinished" }),
        _ => return None,
    })
}

/// #EXP-1441: a caller's side of [`call_wire_event`]: one flow event in its
/// wire form (`{t, ...}`, the `flowCases` event shape, `failed {code}`
/// included) as a [`CallEvent`]; None = not a flow event.
pub fn call_event_of_wire(value: &serde_json::Value) -> Option<CallEvent> {
    let text = || value["text"].as_str().unwrap_or_default().to_string();
    Some(match value["t"].as_str()? {
        "start" => CallEvent::Start {
            device: value["device"].as_str()?.to_string(),
            language: value["language"].as_str().map(str::to_string),
            fallback: value["fallback"].as_bool().unwrap_or(false),
        },
        "rang" => CallEvent::Rang,
        "ready" => CallEvent::Ready,
        "noSound" => CallEvent::NoSound,
        "partial" => CallEvent::Partial(text()),
        "said" => CallEvent::Said(text()),
        "speechStarted" => CallEvent::SpeechStarted,
        "queued" => CallEvent::Queued { count: value["count"].as_u64().unwrap_or(0) as usize, text: text() },
        "replyStarted" => CallEvent::ReplyStarted(text()),
        "replyFinished" => CallEvent::ReplyFinished,
        "mute" => CallEvent::Mute,
        "unmute" => CallEvent::Unmute,
        "speaker" => CallEvent::Speaker(value["on"].as_bool()?),
        "end" => CallEvent::End,
        "hungUp" => CallEvent::HungUp(value["reason"].as_str().map(str::to_string)),
        // A host failure with any code: one without copy fails as `dropped`.
        "failed" => CallEvent::Failed(value["code"].as_str().unwrap_or("dropped").to_string()),
        _ => return None,
    })
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
    /// EXP-1857: the device that speaks while the call falls back (a second
    /// status line); None otherwise.
    pub via: Option<String>,
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
    let via = state.fallback.then(|| copy.via.replace("{device}", &state.device));
    let view = |activity: &'static str, status: Option<String>, caption: &str, buttons: Vec<CallButton>, end: &str| CallView {
        activity,
        status,
        via: via.clone(),
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

/// EXP-1929 (`livenessCases`): an end of a call that got a `call_alive`
/// (`alive`) and has heard nothing from the relay for `silent_ms` treats the
/// path as dead (the call ends as dropped); before the first beat (an older
/// relay) there is no deadline.
pub fn call_path_dead(alive: bool, silent_ms: u64) -> bool {
    alive && silent_ms >= VOICE_CALL.constants.idle_timeout_ms
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

    /// #EXP-1941: one call id is answered once, for five minutes.
    #[test]
    fn a_call_id_is_answered_once() {
        let mut answered = AnsweredCalls::default();
        assert!(!answered.answered("c1", 0));
        answered.note("c1", 1_000);
        assert!(answered.answered("c1", 1_000) && answered.answered("c1", 1_000 + CALL_ANSWERED_ONCE_MS - 1));
        assert!(!answered.answered("c2", 2_000), "another id is another call");
        assert!(!answered.answered("c1", 1_000 + CALL_ANSWERED_ONCE_MS), "longer than an id lives");
        answered.note("c2", 1_000 + CALL_ANSWERED_ONCE_MS);
        assert_eq!(answered.0.len(), 1, "old ids are forgotten");
    }

    /// #EXP-1941: every row of `seal.modeCases`, phase and actions.
    #[test]
    fn the_call_seal_steps_replay_the_mode_cases() {
        let cases = fixture()["seal"]["modeCases"].as_array().unwrap().clone();
        assert!(cases.len() >= 30);
        for case in cases {
            let role: CallSealRole = serde_json::from_value(case["role"].clone()).unwrap();
            let mut state = CALL_SEAL_IDLE;
            let mut actions = Vec::new();
            for event in case["events"].as_array().unwrap() {
                let event: CallSealEvent = serde_json::from_value(event.clone()).unwrap_or_else(|err| panic!("{}: {err}", case["name"]));
                let (next, did) = call_seal_step(role, state, &event);
                state = next;
                actions.push(did);
            }
            assert_eq!(serde_json::json!(actions), case["actions"], "{}", case["name"]);
            assert_eq!(state.phase, serde_json::from_value::<CallSealPhase>(case["phase"].clone()).unwrap(), "{}", case["name"]);
        }
        assert_eq!(call_seal_wire_code("not_granted"), "dropped");
        assert_eq!(call_seal_wire_code("busy"), "busy");
        assert_eq!(VOICE_CALL.seal.max_lost_ms, crate::CALL_MAX_LOST_MS);
    }
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

    /// FEED-272: the tabs read Chat | Voice and a call opens on Voice.
    #[test]
    fn the_call_tabs_open_on_voice() {
        let tabs = &VOICE_CALL.tabs;
        assert_eq!(tabs.order, vec![CallTab::Chat, CallTab::Voice]);
        assert_eq!(tabs.initial, CallTab::Voice);
        assert_eq!((tabs.label(CallTab::Chat), tabs.label(CallTab::Voice)), ("Chat", "Voice"));
        assert_eq!(tabs.test_ids.tabs, "voice-call-tabs");
        assert_eq!(tabs.test_id(CallTab::Chat), "voice-call-tab-chat");
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
        for case in fixture["callSession"]["openingCases"].as_array().unwrap() {
            let expected = match case["opening"].as_str().unwrap() {
                "wait" => CallOpening::Wait,
                "speak" => CallOpening::Speak,
                "listen" => CallOpening::Listen,
                other => panic!("unknown opening {other}"),
            };
            let got = call_opening(case["readyAtMs"].as_u64(), case["openingAtMs"].as_u64(), case["nowMs"].as_u64().unwrap());
            assert_eq!(got, expected, "{case}");
        }
        for case in fixture["hostVoice"]["cases"].as_array().unwrap() {
            let voice = match case["clip"].as_str().unwrap() {
                "none" => CallVoice::None,
                "installed" => CallVoice::Installed,
                "installing" => CallVoice::Installing,
                "fetching" => CallVoice::Fetching,
                "failed" => CallVoice::Failed,
                other => panic!("unknown clip {other}"),
            };
            let expected = match case["ready"].as_str().unwrap() {
                "ready" => CallVoiceReady::Ready,
                "wait" => CallVoiceReady::Wait,
                "builtIn" => CallVoiceReady::BuiltIn,
                other => panic!("unknown answer {other}"),
            };
            assert_eq!(call_voice_ready(voice, case["waitedMs"].as_u64().unwrap()), expected, "{case}");
        }
        for case in fixture["livenessCases"].as_array().unwrap() {
            let input = &case["input"];
            let dead = call_path_dead(input["alive"].as_bool().unwrap(), input["silentMs"].as_u64().unwrap());
            assert_eq!(dead, case["expected"]["dead"].as_bool().unwrap(), "{case}");
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
                "failed" => CallSpeech::Failed,
                "preparing" => CallSpeech::Preparing,
                other => panic!("unknown speech {other}"),
            });
            let reason = call_unavailable(&CallAvailabilityInput {
                device: input["device"].as_str().unwrap(),
                online: input["online"].as_bool().unwrap(),
                speech,
                meeting: input["meeting"].as_bool().unwrap_or(false),
            });
            let expected = &case["expected"];
            assert_eq!(reason.is_none(), expected["enabled"].as_bool().unwrap(), "{}", case["name"]);
            assert_eq!(reason.as_deref(), expected["reason"].as_str(), "{}", case["name"]);
        }
    }

    fn speech(value: &Value) -> Option<CallSpeech> {
        value.as_str().map(|speech| match speech {
            "ok" => CallSpeech::Ok,
            "noGpu" => CallSpeech::NoGpu,
            "failed" => CallSpeech::Failed,
            "preparing" => CallSpeech::Preparing,
            other => panic!("unknown speech {other}"),
        })
    }

    #[test]
    fn target_matches_the_fixture() {
        for case in fixture()["targetCases"].as_array().unwrap() {
            let input = &case["input"];
            let devices = |key: &str| -> Vec<CallTargetDevice> {
                input[key]
                    .as_array()
                    .map(Vec::as_slice)
                    .unwrap_or_default()
                    .iter()
                    .map(|device| CallTargetDevice {
                        id: device["id"].as_str().unwrap().into(),
                        name: device["name"].as_str().unwrap().into(),
                        online: device["online"].as_bool().unwrap(),
                        speech: speech(&device["speech"]),
                        meeting: device["meeting"].as_bool().unwrap_or(false),
                    })
                    .collect()
            };
            let (desktops, servers) = (devices("desktops"), devices("servers"));
            let target = call_target(&CallTargetInput {
                here: input["here"].as_str(),
                agent_device: input["agentDevice"].as_str(),
                desktops: &desktops,
                servers: &servers,
                busy: input["busy"].as_bool().unwrap(),
            });
            let expected = &case["expected"];
            let name = &case["name"];
            assert_eq!(target.enabled(), expected["enabled"].as_bool().unwrap(), "{name}");
            assert_eq!(target.reason.as_deref(), expected["reason"].as_str(), "{name}");
            assert_eq!(target.device.as_deref(), expected["device"].as_str(), "{name}");
            assert_eq!(target.name.as_deref(), expected["name"].as_str(), "{name}");
            assert_eq!(target.fallback, expected["fallback"].as_bool().unwrap(), "{name}");
            assert_eq!(target.local, expected["local"].as_bool().unwrap(), "{name}");
        }
    }

    #[test]
    fn tap_matches_the_fixture() {
        for case in fixture()["tapCases"].as_array().unwrap() {
            let (input, expected, name) = (&case["input"], &case["expected"], &case["name"]);
            let tap = call_tap(platform(&input["platform"]), input["availability"]["reason"].as_str());
            assert_eq!(tap.call, expected["call"].as_bool().unwrap(), "{name}");
            assert_eq!(tap.toast.as_deref(), expected["toast"].as_str(), "{name}");
        }
    }

    #[test]
    fn speech_matches_the_fixture() {
        for case in fixture()["speechCases"].as_array().unwrap() {
            let expected = case["expected"].as_str().map(|speech| match speech {
                "ok" => CallSpeech::Ok,
                "noGpu" => CallSpeech::NoGpu,
                "failed" => CallSpeech::Failed,
                "preparing" => CallSpeech::Preparing,
                other => panic!("unknown speech {other}"),
            });
            assert_eq!(call_speech_of(case["state"].as_str()), expected, "{}", case["name"]);
        }
    }

    fn event(value: &Value) -> CallEvent {
        call_event_of_wire(value).unwrap_or_else(|| panic!("unknown event {value}"))
    }

    #[test]
    fn the_wire_events_round_trip() {
        for case in fixture()["flowCases"].as_array().unwrap() {
            for value in case["events"].as_array().unwrap() {
                let event = event(value);
                if let Some(wire) = call_wire_event(&event) {
                    assert_eq!(call_event_of_wire(&wire), Some(event), "{value}");
                }
            }
        }
        assert_eq!(call_event_of_wire(&serde_json::json!({ "t": "nope" })), None);
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
                    assert_eq!(view.via.as_deref(), expected["via"].as_str(), "{name}");
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
