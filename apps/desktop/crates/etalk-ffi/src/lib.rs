//! #EXP-1441: the UniFFI facade over eTalk for Swift (iOS, macOS) and Kotlin
//! (Android): the call machine Exponential's desktop, web and phones run.
//!
//! Shape of the contract:
//! - The CALL MACHINE (`etalk::call`, byte-locked by
//!   `packages/etalk/fixtures/voice-call.json`, whose `$comment` is the spec):
//!   pure free functions over plain records. `callIdle()` → `callStep(state,
//!   event)` → `callView(state, platform)` (its `via` names the desktop that
//!   speaks while the call falls back, #EXP-1857), `callTarget(input)` (where
//!   the call rings), `callUnavailable(input)`, `callTap(platform, reason)`,
//!   `callSpeechOf(row state)`, the ring (`callRingMs`, `callAnswerMs`,
//!   `callRingToneOn`, `callRingRandom`), `callTimer`, and the
//!   wire form of a flow event (`callEventOfWire` for an incoming `call_event`,
//!   `callWireEvent` for a host's). The host owns time: it draws one
//!   `callRingRandom()` per call and steps `rang` at `callRingMs(random)`.
//! - The CODEC (`etalk-codec`, Opus): `TalkEncoder` (mono i16 PCM of one
//!   `frameMs` frame in, one packet out), `TalkDecoder` (a packet in, i16 PCM
//!   out; an empty packet conceals a lost one; `decodeFec` recovers the lost
//!   frame before a packet from its in-band FEC), `TalkUplink` (the host's
//!   receive path by sequence number). Audio I/O stays native (AVAudioEngine,
//!   Oboe / AudioRecord); transport is eNet's call channel (`exp/enet/call/1`).
//! - `callSpecJson()` = the fixture itself (copy, icons, test ids, constants)
//!   for the natives' surfaces; `callConstants()` = its numbers as a record.
//! - A Rust panic never crosses the FFI: the call machine is total, and the
//!   codec turns an opus-rs panic into `TalkError.Codec` (a fresh coder
//!   behind it).

use std::sync::Mutex;

use etalk::call;
use etalk::call::{CallButton, CallEvent, CallPhase, CallPlatform, CallSpeech, CallState, CallTargetDevice, VOICE_CALL};

uniffi::setup_scaffolding!();

// --- errors ------------------------------------------------------------

#[derive(Debug, uniffi::Error)]
pub enum TalkError {
    /// Bad input from the host (a rate Opus cannot code, a frame size).
    Invalid { reason: String },
    /// The codec failed on this frame or packet; the coder stays usable.
    Codec { reason: String },
}

impl std::error::Error for TalkError {}

impl std::fmt::Display for TalkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TalkError::Invalid { reason } => write!(f, "invalid: {reason}"),
            TalkError::Codec { reason } => write!(f, "codec: {reason}"),
        }
    }
}

fn invalid(reason: String) -> TalkError {
    TalkError::Invalid { reason }
}

// --- the call machine ---------------------------------------------------

/// Where the telephone is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum TalkCallPlatform {
    Desktop,
    Phone,
    Web,
}

impl From<TalkCallPlatform> for CallPlatform {
    fn from(platform: TalkCallPlatform) -> Self {
        match platform {
            TalkCallPlatform::Desktop => CallPlatform::Desktop,
            TalkCallPlatform::Phone => CallPlatform::Phone,
            TalkCallPlatform::Web => CallPlatform::Web,
        }
    }
}

/// The voice device's doctor row `speech` as a caller reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum TalkCallSpeech {
    Ok,
    Preparing,
    NoGpu,
    Failed,
}

impl From<CallSpeech> for TalkCallSpeech {
    fn from(speech: CallSpeech) -> Self {
        match speech {
            CallSpeech::Ok => TalkCallSpeech::Ok,
            CallSpeech::Preparing => TalkCallSpeech::Preparing,
            CallSpeech::NoGpu => TalkCallSpeech::NoGpu,
            CallSpeech::Failed => TalkCallSpeech::Failed,
        }
    }
}

impl From<TalkCallSpeech> for CallSpeech {
    fn from(speech: TalkCallSpeech) -> Self {
        match speech {
            TalkCallSpeech::Ok => CallSpeech::Ok,
            TalkCallSpeech::Preparing => CallSpeech::Preparing,
            TalkCallSpeech::NoGpu => CallSpeech::NoGpu,
            TalkCallSpeech::Failed => CallSpeech::Failed,
        }
    }
}

/// EXP-1857: one of the member's own desktops (EXP-1858: or servers) as
/// [`call_target`] reads it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TalkCallTargetDevice {
    pub id: String,
    pub name: String,
    pub online: bool,
    /// Its doctor row `speech` through [`call_speech_of`] (None = none yet).
    pub speech: Option<TalkCallSpeech>,
    /// A meeting runs on it.
    pub meeting: bool,
}

impl From<TalkCallTargetDevice> for CallTargetDevice {
    fn from(device: TalkCallTargetDevice) -> Self {
        CallTargetDevice {
            id: device.id,
            name: device.name,
            online: device.online,
            speech: device.speech.map(Into::into),
            meeting: device.meeting,
        }
    }
}

/// `targetCases` input.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TalkCallTargetInput {
    /// THIS client's own desktop (None on phones and web).
    pub here: Option<String>,
    /// The called agent's Runs on device.
    pub agent_device: Option<String>,
    /// The member's own desktops, newest `lastSeenAt` first.
    pub desktops: Vec<TalkCallTargetDevice>,
    /// EXP-1858: the member's own servers (CLI daemons): a candidate only as
    /// the agent's device, never here nor a fallback.
    pub servers: Vec<TalkCallTargetDevice>,
    /// A call this client does not run already runs on the target.
    pub busy: bool,
}

/// Where a call's speech runs; `device` None = the telephone is disabled
/// with `reason`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TalkCallTarget {
    pub enabled: bool,
    pub reason: Option<String>,
    /// The target's id and name.
    pub device: Option<String>,
    pub name: Option<String>,
    /// The target is not the agent's own device: the call starts with
    /// `fallback` (the view's `via`).
    pub fallback: bool,
    /// The target is this client's own desktop: it runs the call itself.
    pub local: bool,
}

/// What a press on the telephone does (`tapCases`).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TalkCallTap {
    pub call: bool,
    /// A phone's tap on a disabled telephone toasts its reason.
    pub toast: Option<String>,
}

/// `availabilityCases` input (EXP-1991): ONE desktop.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TalkCallAvailabilityInput {
    /// The desktop's name.
    pub device: String,
    pub online: bool,
    /// None = no `speech` row yet.
    pub speech: Option<TalkCallSpeech>,
    /// A meeting runs on it.
    pub meeting: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum TalkCallPhase {
    Hidden,
    Ringing,
    Listening,
    Thinking,
    Speaking,
}

impl From<CallPhase> for TalkCallPhase {
    fn from(phase: CallPhase) -> Self {
        match phase {
            CallPhase::Hidden => TalkCallPhase::Hidden,
            CallPhase::Ringing => TalkCallPhase::Ringing,
            CallPhase::Listening => TalkCallPhase::Listening,
            CallPhase::Thinking => TalkCallPhase::Thinking,
            CallPhase::Speaking => TalkCallPhase::Speaking,
        }
    }
}

impl From<TalkCallPhase> for CallPhase {
    fn from(phase: TalkCallPhase) -> Self {
        match phase {
            TalkCallPhase::Hidden => CallPhase::Hidden,
            TalkCallPhase::Ringing => CallPhase::Ringing,
            TalkCallPhase::Listening => CallPhase::Listening,
            TalkCallPhase::Thinking => CallPhase::Thinking,
            TalkCallPhase::Speaking => CallPhase::Speaking,
        }
    }
}

/// The call's state; the host keeps it and steps it with [`call_step`].
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TalkCallState {
    pub phase: TalkCallPhase,
    pub muted: bool,
    /// Phones: the loudspeaker (false = the earpiece).
    pub speaker: bool,
    pub device: String,
    /// EXP-1857: the device is not the agent's own (the view's `via`).
    #[uniffi(default = false)]
    pub fallback: bool,
    /// The ring's earliest answer passed.
    pub rang: bool,
    /// The voice device's warm-up is done.
    pub ready: bool,
    /// The microphone stayed silent during the ring.
    pub no_sound: bool,
    /// The last utterance, quoted.
    pub said: String,
    pub line: String,
    /// Set when the call just ended with a failure: show it as a toast.
    pub toast: Option<String>,
}

impl From<CallState> for TalkCallState {
    fn from(state: CallState) -> Self {
        TalkCallState {
            phase: state.phase.into(),
            muted: state.muted,
            speaker: state.speaker,
            device: state.device,
            fallback: state.fallback,
            rang: state.rang,
            ready: state.ready,
            no_sound: state.no_sound,
            said: state.said,
            line: state.line,
            toast: state.toast,
        }
    }
}

impl From<TalkCallState> for CallState {
    fn from(state: TalkCallState) -> Self {
        CallState {
            phase: state.phase.into(),
            muted: state.muted,
            speaker: state.speaker,
            device: state.device,
            fallback: state.fallback,
            rang: state.rang,
            ready: state.ready,
            no_sound: state.no_sound,
            said: state.said,
            line: state.line,
            toast: state.toast,
        }
    }
}

/// One flow event (`flowCases` events; a caller gets the host's as `call_event`).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum TalkCallEvent {
    /// EXP-1857: `fallback` = the device is not the agent's own
    /// ([`TalkCallTarget::fallback`]).
    Start {
        device: String,
        language: Option<String>,
        #[uniffi(default = false)]
        fallback: bool,
    },
    /// The ring's earliest answer ([`call_ring_ms`]) passed.
    Rang,
    /// The voice device's warm-up is done.
    Ready,
    /// The microphone stayed silent during the ring.
    NoSound,
    Partial { text: String },
    Said { text: String },
    SpeechStarted,
    Queued { count: u32, text: String },
    ReplyStarted { text: String },
    ReplyFinished,
    Mute,
    Unmute,
    Speaker { on: bool },
    End,
    /// The other side's `call_end`: a reason fails it with the toast; none ends it silently.
    HungUp { reason: Option<String> },
    Failed { code: String },
}

impl From<TalkCallEvent> for CallEvent {
    fn from(event: TalkCallEvent) -> Self {
        match event {
            TalkCallEvent::Start { device, language, fallback } => CallEvent::Start { device, language, fallback },
            TalkCallEvent::Rang => CallEvent::Rang,
            TalkCallEvent::Ready => CallEvent::Ready,
            TalkCallEvent::NoSound => CallEvent::NoSound,
            TalkCallEvent::Partial { text } => CallEvent::Partial(text),
            TalkCallEvent::Said { text } => CallEvent::Said(text),
            TalkCallEvent::SpeechStarted => CallEvent::SpeechStarted,
            TalkCallEvent::Queued { count, text } => CallEvent::Queued { count: count as usize, text },
            TalkCallEvent::ReplyStarted { text } => CallEvent::ReplyStarted(text),
            TalkCallEvent::ReplyFinished => CallEvent::ReplyFinished,
            TalkCallEvent::Mute => CallEvent::Mute,
            TalkCallEvent::Unmute => CallEvent::Unmute,
            TalkCallEvent::Speaker { on } => CallEvent::Speaker(on),
            TalkCallEvent::End => CallEvent::End,
            TalkCallEvent::HungUp { reason } => CallEvent::HungUp(reason),
            TalkCallEvent::Failed { code } => CallEvent::Failed(code),
        }
    }
}

impl From<CallEvent> for TalkCallEvent {
    fn from(event: CallEvent) -> Self {
        match event {
            CallEvent::Start { device, language, fallback } => TalkCallEvent::Start { device, language, fallback },
            CallEvent::Rang => TalkCallEvent::Rang,
            CallEvent::Ready => TalkCallEvent::Ready,
            CallEvent::NoSound => TalkCallEvent::NoSound,
            CallEvent::Partial(text) => TalkCallEvent::Partial { text },
            CallEvent::Said(text) => TalkCallEvent::Said { text },
            CallEvent::SpeechStarted => TalkCallEvent::SpeechStarted,
            CallEvent::Queued { count, text } => TalkCallEvent::Queued { count: count.min(u32::MAX as usize) as u32, text },
            CallEvent::ReplyStarted(text) => TalkCallEvent::ReplyStarted { text },
            CallEvent::ReplyFinished => TalkCallEvent::ReplyFinished,
            CallEvent::Mute => TalkCallEvent::Mute,
            CallEvent::Unmute => TalkCallEvent::Unmute,
            CallEvent::Speaker(on) => TalkCallEvent::Speaker { on },
            CallEvent::End => TalkCallEvent::End,
            CallEvent::HungUp(reason) => TalkCallEvent::HungUp { reason },
            CallEvent::Failed(code) => TalkCallEvent::Failed { code },
        }
    }
}

/// One round call button, in [`TalkCallView::buttons`] order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum TalkCallButton {
    Mute,
    Speaker,
    End,
}

impl From<CallButton> for TalkCallButton {
    fn from(button: CallButton) -> Self {
        match button {
            CallButton::Mute => TalkCallButton::Mute,
            CallButton::Speaker => TalkCallButton::Speaker,
            CallButton::End => TalkCallButton::End,
        }
    }
}

/// What the call surface shows.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TalkCallView {
    /// ringing | listening | thinking | speaking | muted.
    pub activity: String,
    /// The line under the name; None = the call's timer ([`call_timer`]).
    pub status: Option<String>,
    /// EXP-1857: `Voice on {device}` while the call falls back (a second
    /// status line); None otherwise.
    pub via: Option<String>,
    pub caption: String,
    pub muted: bool,
    pub speaker: bool,
    pub buttons: Vec<TalkCallButton>,
    /// The end button's label (shown under it while ringing).
    pub end: String,
}

/// The fixture's `constants` and `wire` numbers.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct TalkCallConstants {
    pub min_ring_ms: u64,
    pub ring_jitter_ms: u64,
    pub ring_tone_ms: u64,
    pub ring_gap_ms: u64,
    pub ring_low_hz: f32,
    pub ring_high_hz: f32,
    pub max_ring_ms: u64,
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
    /// `wire.kinds.up` / `.down`.
    pub kind_up: u8,
    pub kind_down: u8,
    /// `wire.maxFrameBytes`: a bigger frame is dropped.
    pub max_frame_bytes: u32,
}

/// `CALL_IDLE`: no call.
#[uniffi::export]
pub fn call_idle() -> TalkCallState {
    CallState::default().into()
}

/// One step of the call (the same machine on the voice device and a caller).
#[uniffi::export]
pub fn call_step(state: TalkCallState, event: TalkCallEvent) -> TalkCallState {
    call::call_step(&state.into(), &event.into()).into()
}

/// None = no call surface.
#[uniffi::export]
pub fn call_view(state: TalkCallState, platform: TalkCallPlatform) -> Option<TalkCallView> {
    call::call_view(&state.into(), platform.into()).map(|view| TalkCallView {
        activity: view.activity.to_string(),
        status: view.status,
        via: view.via,
        caption: view.caption,
        muted: view.muted,
        speaker: view.speaker,
        buttons: view.buttons.into_iter().map(Into::into).collect(),
        end: view.end,
    })
}

/// None = the desktop can take the call; Some = it cannot, with the reason.
#[uniffi::export]
pub fn call_unavailable(input: TalkCallAvailabilityInput) -> Option<String> {
    call::call_unavailable(&call::CallAvailabilityInput {
        device: &input.device,
        online: input.online,
        speech: input.speech.map(Into::into),
        meeting: input.meeting,
    })
}

/// EXP-1857 (`targetCases`): where a call's speech runs: the first that
/// speaks of the agent's device (EXP-1858: a desktop or a server of the
/// member's), the desktop you're at, the other desktops newest first. None
/// speaks = disabled with the first candidate's reason.
#[uniffi::export]
pub fn call_target(input: TalkCallTargetInput) -> TalkCallTarget {
    let devices = |devices: Vec<TalkCallTargetDevice>| -> Vec<CallTargetDevice> { devices.into_iter().map(Into::into).collect() };
    let (desktops, servers) = (devices(input.desktops), devices(input.servers));
    let target = call::call_target(&call::CallTargetInput {
        here: input.here.as_deref(),
        agent_device: input.agent_device.as_deref(),
        desktops: &desktops,
        servers: &servers,
        busy: input.busy,
    });
    TalkCallTarget {
        enabled: target.enabled(),
        reason: target.reason,
        device: target.device,
        name: target.name,
        fallback: target.fallback,
        local: target.local,
    }
}

/// #EXP-1878 (`tapCases`): a press on the telephone whose `reason` is
/// [`call_target`]'s (None = enabled): it calls, or (a phone's tap on a
/// disabled one) toasts the reason; web and desktop show it as the disabled
/// control's tooltip.
#[uniffi::export]
pub fn call_tap(platform: TalkCallPlatform, reason: Option<String>) -> TalkCallTap {
    let tap = call::call_tap(platform.into(), reason.as_deref());
    TalkCallTap { call: tap.call, toast: tap.toast }
}

/// The voice device's doctor row `speech` state (None = no row yet).
#[uniffi::export]
pub fn call_speech_of(state: Option<String>) -> Option<TalkCallSpeech> {
    call::call_speech_of(state.as_deref()).map(Into::into)
}

/// The ring's earliest answer, ms after it started, for the host's one
/// `random` in [0, 1).
#[uniffi::export]
pub fn call_ring_ms(random: f64) -> u64 {
    call::call_ring_ms(random)
}

/// When the ring ends (answered); None = never (no ready, or one at or past `maxRingMs`).
#[uniffi::export]
pub fn call_answer_ms(random: f64, ready_ms: Option<u64>) -> Option<u64> {
    call::call_answer_ms(random, ready_ms)
}

/// Whether the ring tone sounds `ms` after the ring started.
#[uniffi::export]
pub fn call_ring_tone_on(ms: i64) -> bool {
    call::call_ring_tone_on(ms)
}

/// A fresh random in [0, 1) for [`call_ring_ms`] (the host draws one per call).
#[uniffi::export]
pub fn call_ring_random() -> f64 {
    call::call_ring_random()
}

/// The call's timer since it was answered: mm:ss, h:mm:ss from an hour.
#[uniffi::export]
pub fn call_timer(ms: u64) -> String {
    call::call_timer(ms)
}

/// An incoming `call_event`'s `event` (JSON text, `{t, ...}`); None = not a
/// flow event (or not JSON).
#[uniffi::export]
pub fn call_event_of_wire(json: String) -> Option<TalkCallEvent> {
    let value: serde_json::Value = serde_json::from_str(&json).ok()?;
    call::call_event_of_wire(&value).map(Into::into)
}

/// The host's side: the event as the caller receives it (JSON text); None =
/// the host never forwards it.
#[uniffi::export]
pub fn call_wire_event(event: TalkCallEvent) -> Option<String> {
    call::call_wire_event(&event.into()).map(|value| value.to_string())
}

/// voice-call.json itself: copy, icons, test ids, constants, wire.
#[uniffi::export]
pub fn call_spec_json() -> String {
    call::VOICE_CALL_JSON.to_string()
}

#[uniffi::export]
pub fn call_constants() -> TalkCallConstants {
    let c = &VOICE_CALL.constants;
    let wire = &VOICE_CALL.wire;
    TalkCallConstants {
        min_ring_ms: c.min_ring_ms,
        ring_jitter_ms: c.ring_jitter_ms,
        ring_tone_ms: c.ring_tone_ms,
        ring_gap_ms: c.ring_gap_ms,
        ring_low_hz: c.ring_low_hz,
        ring_high_hz: c.ring_high_hz,
        max_ring_ms: c.max_ring_ms,
        no_sound_level: c.no_sound_level,
        ring_timeout_ms: c.ring_timeout_ms,
        max_frames_per_second: c.max_frames_per_second,
        max_call_ms: c.max_call_ms,
        idle_timeout_ms: c.idle_timeout_ms,
        min_silence_ms: c.min_silence_ms,
        barge_in_ms: c.barge_in_ms,
        frame_ms: c.frame_ms,
        up_rate: c.up_rate,
        down_rate: c.down_rate,
        up_kbps: c.up_kbps,
        down_kbps: c.down_kbps,
        jitter_ms: c.jitter_ms,
        kind_up: wire.kinds.up,
        kind_down: wire.kinds.down,
        max_frame_bytes: wire.max_frame_bytes as u32,
    }
}

// --- the codec -----------------------------------------------------------

/// The biggest Opus packet (RFC 6716: 1275 bytes a frame).
const MAX_PACKET: usize = 1275;

/// Samples per channel of one [`etalk_codec::FRAME_MS`] frame at `rate`.
fn frame_samples(rate: u32) -> usize {
    (rate * etalk_codec::FRAME_MS / 1000) as usize
}

/// f32 PCM in -1..1 as i16 samples.
fn to_i16(samples: &[f32]) -> Vec<i16> {
    samples.iter().map(|sample| (sample.clamp(-1., 1.) * 32767.) as i16).collect()
}

/// Which Opus implementation this build codes with (`opus-rs` | `libopus`).
#[uniffi::export]
pub fn codec_backend() -> String {
    etalk_codec::BACKEND.to_string()
}

/// A mono VoIP Opus encoder: one `frameMs` frame of i16 PCM in, one packet out.
#[derive(uniffi::Object)]
pub struct TalkEncoder {
    inner: Mutex<etalk_codec::Encoder>,
    frame: usize,
}

#[uniffi::export]
impl TalkEncoder {
    /// `rate` = 8000, 12000, 16000, 24000 or 48000 Hz (the call's uplink:
    /// `upRate` at `upKbps`); `loss_percent` = Some turns in-band FEC on.
    #[uniffi::constructor]
    pub fn new(rate: u32, bitrate: i32, loss_percent: Option<i32>) -> Result<Self, TalkError> {
        let inner = etalk_codec::Encoder::new(rate, bitrate, loss_percent).map_err(invalid)?;
        Ok(TalkEncoder { inner: Mutex::new(inner), frame: frame_samples(rate) })
    }

    /// Samples one frame takes.
    pub fn frame_samples(&self) -> u32 {
        self.frame as u32
    }

    /// One frame (exactly [`TalkEncoder::frame_samples`] mono samples) as one packet.
    pub fn encode(&self, pcm: Vec<i16>) -> Result<Vec<u8>, TalkError> {
        if pcm.len() != self.frame {
            return Err(invalid(format!("a frame is {} samples, got {}", self.frame, pcm.len())));
        }
        let mut packet = vec![0u8; MAX_PACKET];
        let mut inner = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let size = inner.encode(&pcm, &mut packet).map_err(|reason| TalkError::Codec { reason })?;
        packet.truncate(size);
        Ok(packet)
    }
}

/// A mono Opus decoder: one packet in, one `frameMs` frame of i16 PCM out.
#[derive(uniffi::Object)]
pub struct TalkDecoder {
    inner: Mutex<etalk_codec::Decoder>,
    frame: usize,
}

#[uniffi::export]
impl TalkDecoder {
    /// `rate` = the stream's rate (the call's downlink: `downRate`).
    #[uniffi::constructor]
    pub fn new(rate: u32) -> Result<Self, TalkError> {
        let inner = etalk_codec::Decoder::new(rate, 1).map_err(invalid)?;
        Ok(TalkDecoder { inner: Mutex::new(inner), frame: frame_samples(rate) })
    }

    pub fn frame_samples(&self) -> u32 {
        self.frame as u32
    }

    /// One packet as PCM; an EMPTY packet conceals a lost frame.
    pub fn decode(&self, packet: Vec<u8>) -> Result<Vec<i16>, TalkError> {
        let mut out = vec![0f32; self.frame];
        let mut inner = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let size = inner.decode(&packet, &mut out).map_err(|reason| TalkError::Codec { reason })?;
        Ok(to_i16(&out[..size.min(self.frame)]))
    }

    /// The frame lost BEFORE `packet`, from `packet`'s in-band FEC; decode
    /// `packet` itself next with [`TalkDecoder::decode`].
    pub fn decode_fec(&self, packet: Vec<u8>) -> Result<Vec<i16>, TalkError> {
        let mut out = vec![0f32; self.frame];
        let mut inner = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let size = inner.decode_fec(&packet, &mut out).map_err(|reason| TalkError::Codec { reason })?;
        Ok(to_i16(&out[..size.min(self.frame)]))
    }
}

/// The call host's receive path: a peer's mono packets by sequence number;
/// one lost packet is recovered from the next one's in-band FEC.
#[derive(uniffi::Object)]
pub struct TalkUplink {
    inner: Mutex<etalk_codec::Uplink>,
}

#[uniffi::export]
impl TalkUplink {
    #[uniffi::constructor]
    pub fn new(rate: u32) -> Result<Self, TalkError> {
        Ok(TalkUplink { inner: Mutex::new(etalk_codec::Uplink::new(rate).map_err(invalid)?) })
    }

    /// The PCM of packet `seq` (and of the one lost before it); empty when it does not decode.
    pub fn push(&self, seq: u16, packet: Vec<u8>) -> Vec<i16> {
        self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).push(seq, &packet)
    }
}
