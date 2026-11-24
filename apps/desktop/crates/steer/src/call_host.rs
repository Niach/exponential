//! NEW-55 (voice V4): the CALL HOST's socket on the voice device.
//!
//! A phone or web caller streams a call to the member's voice device over
//! the steer relay (`fixtures/voice-call.json`, TRANSPORT + AUDIO). The relay
//! rings the device on its control socket (`call_ring`), the IDE mints a
//! `call_host` ticket and [`connect`]s here: one WebSocket per call, BINARY
//! frames for audio (byte 0 = kind, bytes 1-2 = u16 BE sequence, then one
//! Opus packet of 20 ms), JSON text frames for control. This module only
//! moves and codes audio: the caller's Opus is decoded to mono 16 kHz PCM
//! for the speech worker ([`CallHostEvent::Audio`]), the worker's reply PCM
//! (mono, its native rate) is encoded into 20 ms Opus packets
//! ([`CallHost::send_speech`]). The call itself (what is said, sent, spoken)
//! is the IDE's (`etalk_gpui::voice_call`).
//!
//! #EXP-1941: a host the server told `seal` ([`connect_sealed`]) speaks the
//! SEALED wire instead (voice-call.json `seal`; `enet_core::call_seal` holds
//! the bytes): the caller's hello is a binary frame of type 1, the host's a
//! `call_event {t: "hello"}` with its tag; audio is one fixed-size sealed
//! frame per 20 ms either way (mute rides in its flag); every flow event but
//! `failed` is a `call_event {t: "sealed"}`. Dormant: nothing passes a key
//! source yet, so every call is today's.

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt as _, StreamExt as _};
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::Message;

use enet_core::call_seal::{hello_of_frame, host_hello_text, parse_hello, CallFinish, CallHandshake, CallRole, CallSeal, CallSealPad, CallSealRefusal};

use crate::call_keys::{CallHello, CallKeyFuture, CallSealing};
use crate::SteerRuntime;

/// voice-call.json `wire.kinds.up`: caller → host audio.
pub const KIND_UP: u8 = 1;
/// voice-call.json `wire.kinds.down`: host → caller audio.
pub const KIND_DOWN: u8 = 2;
/// voice-call.json `wire.maxFrameBytes`.
pub const MAX_FRAME_BYTES: usize = 1500;
/// voice-call.json `constants.frameMs`.
pub const FRAME_MS: u32 = etalk_codec::FRAME_MS;
/// voice-call.json `constants.upRate`.
pub const UP_RATE: u32 = 16_000;
/// voice-call.json `constants.downKbps`.
pub const DOWN_BITRATE: i32 = 32_000;

/// The `call_end` reason of a call that reached voice-call.json `maxCallMs`
/// (the relay's own, `expireCall`).
pub const MAX_LENGTH: &str = "max_length";

/// EXP-1929 (`livenessCases`): once the relay beat (`call_alive`), a call
/// that hears nothing from it for voice-call.json `idleTimeoutMs` is dropped.
pub fn idle_timeout() -> Duration {
    Duration::from_millis(etalk::call::VOICE_CALL.constants.idle_timeout_ms)
}

/// One call socket's timers: voice-call.json's (tests shorten them).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Timing {
    /// EXP-1929: `idleTimeoutMs`, the liveness deadline and every send's.
    pub idle: Duration,
    /// #EXP-1941: `seal.handshakeMs`, from the socket's open: a host told
    /// `seal` that is not sealed by then ends the call `no_key`.
    pub handshake: Duration,
    /// #EXP-1941: `maxCallMs`, from the socket's open: the host ends every
    /// call it answered itself, sealed or clear; the relay need not.
    pub max_call: Duration,
    /// #EXP-1941: `seal.helloEveryMs`: a caller sends its hello again this
    /// often until a host hello is taken.
    pub hello_every: Duration,
}

impl Timing {
    pub(crate) fn fixture() -> Self {
        let spec = &*etalk::call::VOICE_CALL;
        Timing {
            idle: idle_timeout(),
            handshake: Duration::from_millis(spec.seal.handshake_ms), max_call: Duration::from_millis(spec.constants.max_call_ms),
            hello_every: Duration::from_millis(spec.seal.hello_every_ms),
        }
    }
}

/// EXP-1929: the liveness deadline of one call socket. An older relay never
/// beats: then there is none.
pub(crate) struct Liveness {
    alive: bool,
    heard: tokio::time::Instant,
    timeout: Duration,
}

impl Liveness {
    pub(crate) fn new(timeout: Duration) -> Self {
        Liveness { alive: false, heard: tokio::time::Instant::now(), timeout }
    }

    /// Anything arrived from the relay; `beat` = it was `call_alive`.
    pub(crate) fn heard(&mut self, beat: bool) {
        self.alive |= beat;
        self.heard = tokio::time::Instant::now();
    }

    /// Resolves once the path is dead; never before the first beat.
    pub(crate) async fn dead(&self) {
        if !self.alive {
            return std::future::pending().await;
        }
        tokio::time::sleep_until(self.heard + self.timeout).await;
    }
}

/// EXP-1929: a send that cannot finish within the deadline (a peer that
/// stopped reading) fails the call instead of parking it.
pub(crate) async fn send_within<S>(sink: &mut S, message: Message, timeout: Duration) -> Result<(), String>
where
    S: futures_util::Sink<Message> + Unpin,
    S::Error: std::fmt::Display,
{
    match tokio::time::timeout(timeout, sink.send(message)).await {
        Ok(result) => result.map_err(|err| err.to_string()),
        Err(_) => Err("the call went silent".into()),
    }
}

/// #FEED-268: what ONE call socket saw, for the lines its pairing and its
/// close write: the call id, counts and instants. Never a frame's content.
pub(crate) struct LinkLog {
    side: &'static str,
    label: String,
    opened: std::time::Instant,
    ready_ms: Option<u128>,
    frames: u64,
    first_ms: Option<u128>,
    last: Option<std::time::Instant>,
    lost_ms: u64,
    closed: bool,
}

impl LinkLog {
    pub(crate) fn new(side: &'static str, label: String) -> Self {
        LinkLog { side, label, opened: std::time::Instant::now(), ready_ms: None, frames: 0, first_ms: None, last: None, lost_ms: 0, closed: false }
    }

    /// The pair is up (`call_ready`, or sealed: the first frame that opened).
    pub(crate) fn ready(&mut self) {
        if self.ready_ms.is_none() {
            let ms = self.opened.elapsed().as_millis();
            self.ready_ms = Some(ms);
            log::info!("call {} {}: the other side is on the line after {ms} ms", self.side, self.label);
        }
    }

    /// One decoded audio frame of the other side.
    pub(crate) fn audio(&mut self) {
        self.frames += 1;
        self.last = Some(std::time::Instant::now());
        if self.first_ms.is_none() {
            let ms = self.opened.elapsed().as_millis();
            self.first_ms = Some(ms);
            log::info!("call {} {}: the other side's first audio after {ms} ms", self.side, self.label);
        }
    }

    pub(crate) fn lost(&mut self, ms: u32) {
        self.lost_ms += u64::from(ms);
    }

    /// The socket is over: ONE line, the first reason wins.
    pub(crate) fn close(&mut self, how: &str) {
        if std::mem::replace(&mut self.closed, true) {
            return;
        }
        let never = || "never".to_string();
        log::info!(
            "call {} {}: socket closed ({how}) after {} ms: paired={} audio_frames={} first_audio={} last_audio={} lost={} ms",
            self.side,
            self.label,
            self.opened.elapsed().as_millis(),
            self.ready_ms.map_or_else(never, |ms| format!("{ms} ms")),
            self.frames,
            self.first_ms.map_or_else(never, |ms| format!("{ms} ms")),
            self.last.map_or_else(never, |at| format!("{} ms before the close", at.elapsed().as_millis())),
            self.lost_ms,
        );
    }
}

/// How a `call_end` reads in the close line (voice-call.json `hangUpRules`).
pub(crate) fn call_end_text(reason: Option<&str>) -> String {
    match reason {
        Some(reason) => format!("call_end reason={reason}"),
        None => "call_end without a reason: the other side hung up".to_string(),
    }
}

/// What the socket reports to the IDE.
#[derive(Debug, Clone, PartialEq)]
pub enum CallHostEvent {
    /// `call_ready`: the caller is connected.
    Ready,
    /// The caller's audio, decoded: mono [`UP_RATE`] Hz.
    Audio(Vec<i16>),
    /// #EXP-1941 (question 9), a sealed call only: this many ms of the
    /// caller's audio were lost before the next [`CallHostEvent::Audio`]
    /// (frames the relay skipped or held back, a packet its sender could not
    /// seal). The host logic never joins speech across it: it hands the
    /// count to the speech worker, which knows where an utterance begins.
    Lost { ms: u32 },
    /// `call_mute`.
    Mute(bool),
    /// `call_end` from the caller (or the relay).
    Ended { reason: Option<String> },
    /// The socket failed or closed without `call_end`.
    Dropped(String),
}

pub(crate) enum Outbound {
    Event(Value),
    Speech { pcm: Vec<i16>, rate: u32 },
    /// Barge-in: drop the encoder's partial frame.
    Flush,
    End,
}

/// The live call socket; [`end`] ends the call, and so does dropping every
/// handle (`call_end` either way).
///
/// [`end`]: CallHost::end
#[derive(Clone)]
pub struct CallHost {
    tx: flume::Sender<Outbound>,
}

impl CallHost {
    #[cfg(test)]
    pub(crate) fn from_sender(tx: flume::Sender<Outbound>) -> Self {
        Self { tx }
    }

    /// Forward one flow event (voice-call.json `flowCases` event) to the
    /// caller as `call_event`.
    pub fn send_event(&self, event: Value) {
        let _ = self.tx.send(Outbound::Event(event));
    }

    /// The reply's PCM (mono, `rate` Hz), encoded and sent in 20 ms frames.
    pub fn send_speech(&self, pcm: Vec<i16>, rate: u32) {
        let _ = self.tx.send(Outbound::Speech { pcm, rate });
    }

    /// Barge-in: forget audio not yet framed.
    pub fn flush(&self) {
        let _ = self.tx.send(Outbound::Flush);
    }

    /// `call_end`, then close.
    pub fn end(&self) {
        let _ = self.tx.send(Outbound::End);
    }
}

/// One audio frame on the wire.
pub fn audio_frame(kind: u8, seq: u16, packet: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(3 + packet.len());
    frame.push(kind);
    frame.extend_from_slice(&seq.to_be_bytes());
    frame.extend_from_slice(packet);
    frame
}

/// `(kind, seq, packet)` of a well-formed frame within the size cap.
pub fn parse_audio_frame(frame: &[u8]) -> Option<(u8, u16, &[u8])> {
    if frame.len() < 4 || frame.len() > MAX_FRAME_BYTES {
        return None;
    }
    Some((frame[0], u16::from_be_bytes([frame[1], frame[2]]), &frame[3..]))
}

/// Frames reply PCM into 20 ms Opus packets, keeping the remainder
/// (`call_codec`: `opus-rs` by default, the system libopus with the
/// `libopus` feature).
struct Encoder {
    rate: u32,
    opus: etalk_codec::Encoder,
    pending: Vec<i16>,
    seq: u16,
}

impl Encoder {
    fn new(rate: u32) -> Result<Self, String> {
        Ok(Encoder { rate, opus: etalk_codec::Encoder::new(rate, DOWN_BITRATE, None)?, pending: Vec::new(), seq: 0 })
    }

    fn frame_len(&self) -> usize {
        (self.rate * FRAME_MS / 1000) as usize
    }

    /// Every whole frame `pcm` completes: its seq and its packet, coded into
    /// a buffer of `room` bytes (the codec takes it as a hard limit). A frame
    /// that could not be coded: skipped as today (the seq moves only with a
    /// packet), or with `count_failed` kept as None under its own seq, for
    /// the sealed wire's empty frame.
    fn packets(&mut self, pcm: &[i16], room: usize, count_failed: bool) -> Vec<(u16, Option<Vec<u8>>)> {
        self.pending.extend_from_slice(pcm);
        let len = self.frame_len();
        let mut out = Vec::new();
        let mut packet = vec![0u8; room];
        while self.pending.len() >= len {
            let frame: Vec<i16> = self.pending.drain(..len).collect();
            let coded = self.opus.encode(&frame, &mut packet).map(|size| packet[..size].to_vec());
            if let Err(err) = &coded {
                log::warn!("call host: opus encode failed: {err}");
            }
            if coded.is_ok() || count_failed {
                out.push((self.seq, coded.ok()));
                self.seq = self.seq.wrapping_add(1);
            }
        }
        out
    }

    /// Every whole frame `pcm` completes, as today's wire frames.
    fn push(&mut self, pcm: &[i16]) -> Vec<Vec<u8>> {
        self.packets(pcm, MAX_FRAME_BYTES - 3, false).into_iter().filter_map(|(seq, packet)| Some(audio_frame(KIND_DOWN, seq, &packet?))).collect()
    }
}

/// #EXP-1941: the sealed wire of one call on the host, from "told `seal`" to
/// sealed (`call_seal_step`'s host rows).
struct Sealed<'a> {
    told: &'a CallSealing,
    /// The first well-formed caller hello was taken (once).
    hello_taken: bool,
    /// Offered (its hello went out), then sealed.
    seal: Option<CallSeal>,
    /// A caller frame opened: `Ready` went out.
    sealed: bool,
    /// Audio frames in a row that did not open, once sealed.
    failures: u32,
    /// Frames dropped before sealed, for the log (never their bytes).
    dropped: u64,
    muted: bool,
    /// Frames lost since the last packet that reached the decoder.
    lost: u32,
}

/// What one inbound frame of a sealed call asks of the socket.
enum Step {
    Nothing,
    /// The first well-formed hello: start the key source.
    Key(CallHello),
    /// End the call with this `seal.endReasons` reason.
    End(&'static str),
}

pub(crate) fn seal_pad() -> CallSealPad {
    let spec = &etalk::call::VOICE_CALL.seal;
    CallSealPad { audio_up: spec.audio_pad_step.up, audio_down: spec.audio_pad_step.down, control_pad: spec.control_pad_bytes, max_plain: spec.max_plain_bytes }
}

/// The silence that stands for `frames` lost frames, at most twice the
/// segmenter's closing silence: beyond that the utterance has ended anyway.
fn gap_silence(frames: u32) -> Vec<i16> {
    let frame = (UP_RATE * FRAME_MS / 1000) as usize;
    let most = (2 * etalk::call::VOICE_CALL.constants.min_silence_ms / u64::from(FRAME_MS)) as usize;
    vec![0; frame * (frames as usize).min(most)]
}

impl Sealed<'_> {
    fn reasons() -> &'static etalk::call::CallSealEndReasons {
        &etalk::call::VOICE_CALL.seal.end_reasons
    }

    /// A binary frame: the caller's hello, or sealed audio.
    fn binary(&mut self, frame: &[u8], decoder: &mut etalk_codec::Uplink, on_event: &impl Fn(CallHostEvent)) -> Step {
        if frame.get(1) == Some(&1) {
            // A hello: the first well-formed one is taken on arrival, once;
            // another changes nothing.
            return match hello_of_frame(frame).and_then(|hello| parse_hello(hello, CallRole::Caller).ok()) {
                Some(hello) if !self.hello_taken => {
                    self.hello_taken = true;
                    Step::Key(hello)
                }
                Some(_) => Step::Nothing,
                None => self.drop_frame(),
            };
        }
        let Some(opened) = self.seal.as_ref().map(|seal| seal.open_audio(frame)) else { return self.drop_frame() };
        let opened = match opened {
            Ok(opened) => opened,
            Err(refusal) if self.sealed => {
                // Counted, never decoded. The count finds a bug; it is no defence.
                self.failures += 1;
                if self.failures >= etalk::call::VOICE_CALL.seal.max_open_failures {
                    log::warn!("call host: {} frames in a row did not open ({refusal})", self.failures);
                    return Step::End(&Self::reasons().seal);
                }
                return Step::Nothing;
            }
            Err(_) => return self.drop_frame(),
        };
        self.failures = 0;
        if !self.sealed {
            // The caller's proof: only now is anyone on the line.
            self.sealed = true;
            log::info!("call host: sealed ({} frames dropped before)", self.dropped);
            on_event(CallHostEvent::Ready);
        }
        let muted = opened.flags & 1 != 0;
        if muted != self.muted {
            self.muted = muted;
            on_event(CallHostEvent::Mute(muted));
        }
        // Never joined across a gap: what the decoder does not recover (one
        // frame, from the next packet's FEC) stands as silence. A frame with
        // `len` 0 is one missing frame and reaches no decoder.
        self.lost = self.lost.saturating_add(opened.missing);
        if opened.packet.is_empty() {
            return Step::Nothing;
        }
        let lost = std::mem::take(&mut self.lost);
        if lost > 0 {
            // Every lost frame counts, one the decoder recovers too.
            on_event(CallHostEvent::Lost { ms: lost.saturating_mul(FRAME_MS) });
        }
        let decoded = decoder.push(opened.seq, &opened.packet);
        let recovered = u32::from(decoded.len() > (UP_RATE * FRAME_MS / 1000) as usize).min(lost);
        let mut pcm = gap_silence(lost - recovered);
        pcm.extend(decoded);
        if !pcm.is_empty() {
            on_event(CallHostEvent::Audio(pcm));
        }
        Step::Nothing
    }

    fn drop_frame(&mut self) -> Step {
        self.dropped += 1;
        Step::Nothing
    }

    /// One flow event as the frame to send: sealed, its `text` clipped so the
    /// padded plaintext fits; a `failed` stays clear (it must work before any
    /// key exists). None = nothing may leave yet (no caller frame opened).
    fn event(&self, event: Value) -> Result<Option<String>, &'static str> {
        if event["t"] == "failed" {
            return Ok(Some(json!({ "t": "call_event", "event": event }).to_string()));
        }
        let Some(seal) = self.seal.as_ref() else { return Ok(None) };
        let mut event = event;
        loop {
            let plain = json!({ "t": "call_event", "event": event }).to_string();
            match seal.seal_control(&plain) {
                Ok(data) => return Ok(Some(json!({ "t": "call_event", "event": { "t": "sealed", "data": data } }).to_string())),
                Err(CallSealRefusal::Unproven) => return Ok(None),
                Err(CallSealRefusal::TooLarge) => {
                    let over = plain.len().saturating_sub(seal_pad().max_plain).max(1);
                    let Some(Value::String(text)) = event.get_mut("text") else { return Ok(None) };
                    let mut keep = text.len().saturating_sub(over);
                    while !text.is_char_boundary(keep) {
                        keep -= 1;
                    }
                    if keep == text.len() {
                        return Ok(None);
                    }
                    text.truncate(keep);
                }
                Err(refusal) => {
                    log::warn!("call host: an event could not be sealed ({refusal})");
                    return Err(&Self::reasons().seal);
                }
            }
        }
    }

    /// Reply PCM as sealed frames, every one the same size. A frame the
    /// codec could not fit into one step leaves as an empty frame under its
    /// seq: 20 ms the caller's decoder conceals, never a second size.
    fn speech(&self, encoder: &mut Encoder, pcm: &[i16]) -> Result<Vec<Vec<u8>>, &'static str> {
        let Some(seal) = self.seal.as_ref().filter(|_| self.sealed) else { return Ok(Vec::new()) };
        let mut frames = Vec::new();
        for (seq, packet) in encoder.packets(pcm, seal_pad().audio_down, true) {
            frames.push(seal.seal_audio(seq, 0, packet.as_deref().unwrap_or(&[])).map_err(|refusal| {
                log::warn!("call host: a frame could not be sealed ({refusal})");
                Self::reasons().seal.as_str()
            })?);
        }
        Ok(frames)
    }
}

/// Dial `url` (the server-provided ticket URL, as-is) on the steer runtime;
/// `on_event` runs on a runtime thread.
pub fn connect(
    runtime: &Arc<SteerRuntime>,
    url: String,
    on_event: impl Fn(CallHostEvent) + Send + Sync + 'static,
) -> CallHost {
    connect_sealed(runtime, url, "", None, on_event)
}

/// #EXP-1941: [`connect`] for a host the server told the call's mode.
/// `sealing` None = today's wire, byte for byte. Some = the host speaks ONLY
/// the sealed wire: it answers the caller's hello with its own once its key
/// source gave the key, reports [`CallHostEvent::Ready`] when the first
/// caller frame OPENED (never at `call_ready`), and sends nothing but that
/// hello and a clear `failed` before. Never clear: no key within
/// `seal.handshakeMs` ends the call `no_key`.
/// `call_id` names the call in this socket's log lines (#FEED-268).
pub fn connect_sealed(
    runtime: &Arc<SteerRuntime>,
    url: String,
    call_id: &str,
    sealing: Option<CallSealing>,
    on_event: impl Fn(CallHostEvent) + Send + Sync + 'static,
) -> CallHost {
    let (tx, rx) = flume::unbounded::<Outbound>();
    let seen = Arc::new(std::sync::Mutex::new(LinkLog::new("host", call_id.to_string())));
    let (log, sealed) = (Arc::clone(&seen), sealing.is_some());
    log::info!("call host {call_id}: joining the relay ({})", if sealed { "sealed" } else { "clear" });
    // #FEED-268: every event passes the log first (counts, never content).
    let on_event = move |event: CallHostEvent| {
        {
            let mut log = log.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            match &event {
                CallHostEvent::Ready => log.ready(),
                CallHostEvent::Audio(_) => log.audio(),
                CallHostEvent::Lost { ms } => log.lost(*ms),
                CallHostEvent::Mute(_) => {}
                CallHostEvent::Ended { reason } => log.close(&call_end_text(reason.as_deref())),
                CallHostEvent::Dropped(err) => log.close(&format!("dropped: {err}")),
            }
        }
        on_event(event);
    };
    runtime.handle().spawn(async move {
        if let Err(err) = run(&url, &rx, &on_event, Timing::fixture(), sealing.as_ref()).await {
            on_event(CallHostEvent::Dropped(err));
        }
        seen.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).close("this host hung up");
    });
    CallHost { tx }
}

pub(crate) async fn run(
    url: &str,
    rx: &flume::Receiver<Outbound>,
    on_event: &(impl Fn(CallHostEvent) + Send + Sync),
    timing: Timing,
    sealing: Option<&CallSealing>,
) -> Result<(), String> {
    let stream = crate::dial(url).await.map_err(|err| match err {
        crate::DialError::Unauthorized => "the call ticket was refused".to_string(),
        crate::DialError::Other(err) => err,
    })?;
    // The caller's hello as it arrived: the transcript holds these bytes.
    let mut sealed_hello: Option<Vec<u8>> = None;
    let timeout = timing.idle;
    let opened_at = tokio::time::Instant::now();
    let (mut sink, mut source) = stream.split();
    let mut decoder = etalk_codec::Uplink::new(UP_RATE)?;
    let mut encoder: Option<Encoder> = None;
    let mut liveness = Liveness::new(timeout);
    let mut sealed = sealing.map(|told| Sealed { told, hello_taken: false, seal: None, sealed: false, failures: 0, dropped: 0, muted: false, lost: 0 });
    // Keying: the key source's one answer. Dropped with the call: it stops.
    let mut key_job: Option<CallKeyFuture<'_>> = None;
    // The socket itself ends the call: `call_end {reason}`, and the host logic hears it.
    macro_rules! end_call {
        ($reason:expr) => {{
            let reason: &str = $reason;
            log::info!("call host: ending the call ({reason})");
            let _ = send_within(&mut sink, Message::text(json!({ "t": "call_end", "reason": reason }).to_string()), timeout).await;
            let _ = sink.close().await;
            on_event(CallHostEvent::Ended { reason: Some(reason.to_string()) });
            return Ok(());
        }};
    }
    loop {
        tokio::select! {
            () = liveness.dead() => return Err("the call went silent".into()),
            () = tokio::time::sleep_until(opened_at + timing.max_call) => end_call!(MAX_LENGTH),
            () = tokio::time::sleep_until(opened_at + timing.handshake), if sealed.as_ref().is_some_and(|sealed| !sealed.sealed) => {
                end_call!(&Sealed::reasons().no_key)
            }
            key = async { key_job.as_mut().expect("guarded").await }, if key_job.is_some() => {
                key_job = None;
                let sealed = sealed.as_mut().expect("only a sealed call has a key source");
                let told = sealed.told;
                let caller_hello = sealed_hello.take().expect("the hello that started the source");
                let finished = key.and_then(|key| {
                    let handshake = CallHandshake::new(CallRole::Host, Some(&key.root_id), key.epoch).map_err(|refusal| refusal.to_string())?;
                    let input = CallFinish {
                        secret: &key.secret,
                        call_id: &told.call_id,
                        agent_id: &told.agent_id,
                        device_id: &told.device_id,
                        peer_hello: &caller_hello,
                        root_id: None,
                        epoch: 0,
                        pad: seal_pad(),
                    };
                    handshake.finish(&input).map_err(|refusal| refusal.to_string())
                });
                match finished {
                    Ok(seal) => {
                        // Offered: its hello with the tag, once.
                        let hello = host_hello_text(&seal.host_hello().expect("a host's finish gives its hello"));
                        sealed.seal = Some(seal);
                        send_within(&mut sink, Message::text(json!({ "t": "call_event", "event": { "t": "hello", "data": hello } }).to_string()), timeout).await?;
                    }
                    Err(reason) => {
                        log::warn!("call host: no key for the call: {reason}");
                        end_call!(&Sealed::reasons().no_key)
                    }
                }
            }
            inbound = source.next() => {
                let Some(inbound) = inbound else { return Err("the relay closed the call".into()) };
                let inbound = inbound.map_err(|err| crate::redact_ticket(&err.to_string()))?;
                liveness.heard(false);
                match inbound {
                    Message::Binary(frame) => match sealed.as_mut() {
                        None => {
                            if let Some((KIND_UP, seq, packet)) = parse_audio_frame(&frame) {
                                let pcm = decoder.push(seq, packet);
                                if !pcm.is_empty() {
                                    on_event(CallHostEvent::Audio(pcm));
                                }
                            }
                        }
                        Some(sealed) => match sealed.binary(&frame, &mut decoder, on_event) {
                            Step::Nothing => {}
                            Step::Key(hello) => {
                                sealed_hello = Some(frame[2..].to_vec());
                                let told = sealed.told;
                                key_job = Some(told.keys.key(hello));
                            }
                            Step::End(reason) => end_call!(reason),
                        },
                    },
                    Message::Text(text) => {
                        let Ok(value) = serde_json::from_str::<Value>(&text) else { continue };
                        match (value["t"].as_str(), sealed.as_mut()) {
                            (Some("call_ready"), None) => on_event(CallHostEvent::Ready),
                            // Sealed: `Ready` is the first frame that opens; the relay writes this one.
                            (Some("call_ready"), Some(_)) => {}
                            (Some("call_alive"), _) => liveness.heard(true),
                            (Some("call_mute"), None) => on_event(CallHostEvent::Mute(value["muted"].as_bool().unwrap_or(false))),
                            // Sealed: mute rides in the audio's flag. Clear content ends a sealed call.
                            (Some("call_mute"), Some(sealed)) if sealed.sealed => end_call!(&Sealed::reasons().seal),
                            (Some("call_mute"), Some(sealed)) => {
                                sealed.drop_frame();
                            }
                            (Some("call_end") | Some("bye"), _) => {
                                on_event(CallHostEvent::Ended { reason: value["reason"].as_str().or(value["outcome"].as_str()).map(str::to_string) });
                                return Ok(());
                            }
                            (Some("error"), _) => {
                                return Err(value["code"].as_str().unwrap_or("error").to_string());
                            }
                            _ => {}
                        }
                    }
                    Message::Close(_) => {
                        on_event(CallHostEvent::Ended { reason: None });
                        return Ok(());
                    }
                    _ => {}
                }
            }
            outbound = rx.recv_async() => {
                let Ok(outbound) = outbound else {
                    // Every handle dropped without End: hang up.
                    let _ = send_within(&mut sink, Message::text(json!({ "t": "call_end" }).to_string()), timeout).await;
                    return Ok(());
                };
                match outbound {
                    Outbound::Event(event) => {
                        let frame = match sealed.as_ref() {
                            None => Some(json!({ "t": "call_event", "event": event }).to_string()),
                            Some(sealed) => match sealed.event(event) {
                                Ok(frame) => frame,
                                Err(reason) => end_call!(reason),
                            },
                        };
                        match frame {
                            Some(frame) => send_within(&mut sink, Message::text(frame), timeout).await?,
                            None => log::warn!("call host: an event was not sent (no caller frame opened yet, or it cannot be sealed)"),
                        }
                    }
                    Outbound::Speech { pcm, rate } => {
                        if encoder.as_ref().is_none_or(|encoder| encoder.rate != rate) {
                            encoder = Some(Encoder::new(rate)?);
                        }
                        let encoder = encoder.as_mut().expect("set above");
                        let frames = match sealed.as_ref() {
                            None => encoder.push(&pcm),
                            Some(sealed) => match sealed.speech(encoder, &pcm) {
                                Ok(frames) => frames,
                                Err(reason) => end_call!(reason),
                            },
                        };
                        for frame in frames {
                            send_within(&mut sink, Message::Binary(frame.into()), timeout).await?;
                        }
                    }
                    Outbound::Flush => {
                        if let Some(encoder) = encoder.as_mut() {
                            encoder.pending.clear();
                        }
                    }
                    Outbound::End => {
                        let _ = send_within(&mut sink, Message::text(json!({ "t": "call_end" }).to_string()), timeout).await;
                        let _ = sink.close().await;
                        return Ok(());
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wave review of #1265: the wire constants are voice-call.json's
    /// (`etalk::call::VOICE_CALL`), never free-standing numbers.
    #[test]
    fn wire_constants_match_the_fixture() {
        let spec = &*etalk::call::VOICE_CALL;
        assert_eq!(KIND_UP, spec.wire.kinds.up);
        assert_eq!(KIND_DOWN, spec.wire.kinds.down);
        assert_eq!(MAX_FRAME_BYTES, spec.wire.max_frame_bytes);
        assert_eq!(FRAME_MS, spec.constants.frame_ms);
        assert_eq!(UP_RATE, spec.constants.up_rate);
        assert_eq!(DOWN_BITRATE, i32::try_from(spec.constants.down_kbps * 1000).unwrap());
    }

    /// A fake relay on localhost: it admits one call socket, sends `frames`,
    /// then goes silent with the socket open (a path that died without a FIN)
    /// and reports what the call sent it, pongs included.
    async fn silent_relay(frames: Vec<Message>) -> (String, tokio::sync::mpsc::UnboundedReceiver<Message>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/", listener.local_addr().unwrap());
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
            for frame in frames {
                ws.send(frame).await.unwrap();
            }
            while let Some(Ok(message)) = ws.next().await {
                let _ = tx.send(message);
            }
            std::future::pending::<()>().await;
        });
        (url, rx)
    }

    fn text(t: &str) -> Message {
        Message::text(json!({ "t": t }).to_string())
    }

    const TEST_TIMEOUT: Duration = Duration::from_millis(300);
    /// The idle deadline shortened; the two others far away.
    const TEST_TIMING: Timing = Timing { idle: TEST_TIMEOUT, handshake: Duration::from_secs(600), max_call: Duration::from_secs(600), hello_every: Duration::from_millis(50) };

    // --- #EXP-1941: the sealed wire against a scripted relay -----------------

    use crate::call_keys::FixedKeySource;
    use enet_core::call_seal::{hello_frame, host_hello_of_text, CallFinish, CallHandshake, CallRole, CallSeal, CallSealPad};

    const ROOT: &str = "11111111-1111-4111-8111-111111111111";
    const CALL: &str = "22222222-2222-4222-8222-222222222222";
    const AGENT: &str = "33333333-3333-4333-8333-333333333333";
    const SECRET: [u8; 32] = [7; 32];

    /// A relay the test drives: what it sends the host, what the host sent it
    /// (text and binary frames only).
    async fn scripted_relay() -> (String, tokio::sync::mpsc::UnboundedSender<Message>, tokio::sync::mpsc::UnboundedReceiver<Message>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/", listener.local_addr().unwrap());
        let (to_host, mut script) = tokio::sync::mpsc::unbounded_channel::<Message>();
        let (sent, from_host) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let (mut sink, mut source) = tokio_tungstenite::accept_async(tcp).await.unwrap().split();
            loop {
                tokio::select! {
                    frame = script.recv() => match frame {
                        Some(frame) => sink.send(frame).await.unwrap(),
                        None => return std::future::pending().await,
                    },
                    frame = source.next() => match frame {
                        Some(Ok(frame)) if frame.is_text() || frame.is_binary() => { let _ = sent.send(frame); }
                        Some(Ok(_)) => {}
                        _ => return std::future::pending().await,
                    },
                }
            }
        });
        (url, to_host, from_host)
    }

    fn sealing(key: Option<(String, u32, [u8; 32])>, delay: Duration) -> CallSealing {
        CallSealing { call_id: CALL.into(), agent_id: AGENT.into(), device_id: "desk-7f3a".into(), caller_target: None, keys: Arc::new(FixedKeySource { key, delay }) }
    }

    fn finish<'a>(peer_hello: &'a [u8]) -> CallFinish<'a> {
        CallFinish { secret: &SECRET, call_id: CALL, agent_id: AGENT, device_id: "desk-7f3a", peer_hello, root_id: Some(ROOT), epoch: 4, pad: CallSealPad::default() }
    }

    /// A host told `seal`, running against the scripted relay.
    struct SealedHost {
        to_host: tokio::sync::mpsc::UnboundedSender<Message>,
        from_host: tokio::sync::mpsc::UnboundedReceiver<Message>,
        events: Arc<std::sync::Mutex<Vec<CallHostEvent>>>,
        host: CallHost,
        ended: tokio::task::JoinHandle<Result<(), String>>,
    }

    impl SealedHost {
        async fn start(sealing: CallSealing, timing: Timing) -> Self {
            let (url, to_host, from_host) = scripted_relay().await;
            let events = Arc::new(std::sync::Mutex::new(Vec::new()));
            let (tx, rx) = flume::unbounded();
            let seen = events.clone();
            let ended = tokio::spawn(async move { run(&url, &rx, &move |event| seen.lock().unwrap().push(event), timing, Some(&sealing)).await });
            SealedHost { to_host, from_host, events, host: CallHost { tx }, ended }
        }

        /// The next frame the host sent, as JSON (a text frame) within `wait`.
        async fn text(&mut self, wait: Duration) -> Option<Value> {
            match tokio::time::timeout(wait, self.from_host.recv()).await {
                Ok(Some(Message::Text(text))) => serde_json::from_str(&text).ok(),
                Ok(Some(other)) => panic!("not a text frame: {other:?}"),
                _ => None,
            }
        }

        fn events(&self) -> Vec<CallHostEvent> {
            self.events.lock().unwrap().clone()
        }

        /// One sealed frame up; back when the host logic has `events` events
        /// (a frame crosses the relay; `send_event` does not).
        async fn up(&self, frame: Vec<u8>, events: usize) {
            self.to_host.send(Message::Binary(frame.into())).unwrap();
            for _ in 0..500 {
                if self.events().len() >= events {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            panic!("the host logic has {} events, not {events}", self.events().len());
        }

        /// The caller's hello goes up; the host's comes back; the caller finishes.
        async fn handshake(&mut self) -> CallSeal {
            let caller = CallHandshake::new(CallRole::Caller, None, 0).unwrap();
            self.to_host.send(Message::Binary(hello_frame(&caller.hello().unwrap()).into())).unwrap();
            let hello = self.text(Duration::from_secs(5)).await.expect("the host's hello");
            assert_eq!((hello["t"].as_str(), hello["event"]["t"].as_str()), (Some("call_event"), Some("hello")), "{hello}");
            let hello = host_hello_of_text(hello["event"]["data"].as_str().unwrap()).expect("70 bytes");
            caller.finish(&finish(&hello)).expect("the tag opens")
        }
    }

    /// One 20 ms frame of a tone as the caller's Opus.
    fn up_packet(encoder: &mut etalk_codec::Encoder) -> Vec<u8> {
        let tone: Vec<i16> = (0..320).map(|i| ((i as f32 * 440.0 * std::f32::consts::TAU / 16_000.0).sin() * 8000.0) as i16).collect();
        let mut packet = [0u8; 120];
        let size = encoder.encode(&tone, &mut packet).unwrap();
        packet[..size].to_vec()
    }

    const QUIET: Duration = Duration::from_millis(150);

    /// The cut's first test: a hello gets a hello whose tag opens; `Ready`
    /// comes only after the first opened frame, never at `call_ready`; and
    /// nothing but that hello leaves the host before it.
    #[tokio::test]
    async fn a_hello_gets_a_hello_and_ready_waits_for_the_first_opened_frame() {
        let mut call = SealedHost::start(sealing(Some((ROOT.into(), 4, SECRET)), Duration::ZERO), TEST_TIMING).await;
        call.to_host.send(text("call_ready")).unwrap();
        let caller = call.handshake().await;
        // The relay repeats `call_ready` and the hello; the host logic talks early.
        call.to_host.send(text("call_ready")).unwrap();
        call.to_host.send(Message::Binary(hello_frame(&CallHandshake::new(CallRole::Caller, None, 0).unwrap().hello().unwrap()).into())).unwrap();
        call.host.send_event(json!({ "t": "said", "text": "canary-7f3a" }));
        call.host.send_speech(vec![1000; 4800], 24_000);
        assert_eq!(call.text(QUIET).await, None, "nothing but the hello before a caller frame opened");
        assert_eq!(call.events(), vec![], "no Ready at call_ready");

        let mut up = etalk_codec::Encoder::new(UP_RATE, 24_000, Some(10)).unwrap();
        call.up(caller.seal_audio(0, 1, &up_packet(&mut up)).unwrap(), 3).await;
        call.host.send_event(json!({ "t": "said", "text": "canary-7f3a" }));
        let sealed = call.text(Duration::from_secs(5)).await.expect("a sealed event");
        assert_eq!(sealed["event"]["t"], "sealed", "{sealed}");
        assert!(!sealed.to_string().contains("canary"));
        let plain = caller.open_control(sealed["event"]["data"].as_str().unwrap()).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&plain).unwrap(), json!({ "t": "call_event", "event": { "t": "said", "text": "canary-7f3a" } }));
        let events = call.events();
        assert_eq!(events[..2], [CallHostEvent::Ready, CallHostEvent::Mute(true)], "Ready at the first opened frame, mute from its flag");
        assert!(matches!(&events[2], CallHostEvent::Audio(pcm) if pcm.len() == 320), "{events:?}");

        // Speech goes down sealed, every frame one size; a clear `failed` still crosses.
        call.host.send_speech(vec![1000; 960], 24_000);
        call.host.send_event(json!({ "t": "failed", "code": "busy" }));
        for seq in 0..2 {
            let Some(Message::Binary(frame)) = call.from_host.recv().await else { panic!("a down frame") };
            assert_eq!(frame.len(), 27 + 160);
            assert_eq!(caller.open_audio(&frame).unwrap().seq, seq);
        }
        assert_eq!(call.text(Duration::from_secs(5)).await, Some(json!({ "t": "call_event", "event": { "t": "failed", "code": "busy" } })));
    }

    /// Question 9: the host never joins across a gap. Frames the relay
    /// dropped stand as silence; ONE lost frame, and an empty frame (a packet
    /// its sender could not seal), the decoder recovers from the next packet.
    #[tokio::test]
    async fn a_gap_is_never_joined() {
        let mut call = SealedHost::start(sealing(Some((ROOT.into(), 4, SECRET)), Duration::ZERO), TEST_TIMING).await;
        let caller = call.handshake().await;
        let mut up = etalk_codec::Encoder::new(UP_RATE, 24_000, Some(10)).unwrap();
        let frames: Vec<Vec<u8>> = (0..9u16)
            .map(|seq| if seq == 6 { caller.seal_audio(seq, 0, &[]).unwrap() } else { caller.seal_audio(seq, 0, &up_packet(&mut up)).unwrap() })
            .collect();
        // The relay drops 2, 3 and 4; 6 is the caller's own empty frame.
        for seq in [0, 1, 5, 6, 7] {
            call.to_host.send(Message::Binary(frames[seq].clone().into())).unwrap();
        }
        call.up(frames[8].clone(), 8).await;
        let lost: Vec<u32> = call.events().into_iter().filter_map(|event| if let CallHostEvent::Lost { ms } = event { Some(ms) } else { None }).collect();
        assert_eq!(lost, [60, 20], "the host logic hears of every loss, a recovered one too");
        let audio: Vec<Vec<i16>> = call.events().into_iter().filter_map(|event| if let CallHostEvent::Audio(pcm) = event { Some(pcm) } else { None }).collect();
        assert_eq!(audio.iter().map(Vec::len).collect::<Vec<_>>(), [320, 320, 4 * 320, 2 * 320, 320], "0, 1, three lost + 5, the empty one recovered + 7, 8");
        assert!(audio[2][..3 * 320].iter().all(|sample| *sample == 0), "the three lost frames are silence");
        let closing = etalk::call::VOICE_CALL.constants.min_silence_ms as usize;
        assert_eq!(gap_silence(1_000_000).len(), 320 * 2 * closing / 20, "a long gap: no more than twice the closing silence");
    }

    /// A host whose key holder is gone ends `no_key` at once, never clear.
    #[tokio::test]
    async fn no_key_ends_the_call_at_once() {
        let mut call = SealedHost::start(sealing(None, Duration::ZERO), TEST_TIMING).await;
        call.to_host.send(Message::Binary(hello_frame(&CallHandshake::new(CallRole::Caller, None, 0).unwrap().hello().unwrap()).into())).unwrap();
        assert_eq!(call.text(Duration::from_secs(5)).await, Some(json!({ "t": "call_end", "reason": "no_key" })));
        assert_eq!(call.events(), vec![CallHostEvent::Ended { reason: Some("no_key".into()) }]);
    }

    /// Before sealed: a clear `call_mute`, a frame that does not open and a
    /// hello that is not well-formed are dropped. Once sealed: clear content
    /// ends the call `seal`.
    #[tokio::test]
    async fn clear_content_ends_a_sealed_call() {
        let mut call = SealedHost::start(sealing(Some((ROOT.into(), 4, SECRET)), Duration::ZERO), TEST_TIMING).await;
        let mute = Message::text(json!({ "t": "call_mute", "muted": true }).to_string());
        call.to_host.send(mute.clone()).unwrap();
        call.to_host.send(Message::Binary(vec![1, 1, 0, 0].into())).unwrap();
        call.to_host.send(Message::Binary(audio_frame(KIND_UP, 0, &[1, 2, 3]).into())).unwrap();
        let caller = call.handshake().await;
        call.to_host.send(mute.clone()).unwrap();
        call.to_host.send(Message::Binary(vec![1; 147].into())).unwrap();
        assert_eq!(call.text(QUIET).await, None);
        assert_eq!(call.events(), vec![], "dropped, the call goes on");
        call.up(caller.seal_audio(0, 0, &[]).unwrap(), 1).await;
        call.to_host.send(mute).unwrap();
        assert_eq!(call.text(Duration::from_secs(5)).await, Some(json!({ "t": "call_end", "reason": "seal" })));
        assert_eq!(call.events(), vec![CallHostEvent::Ready, CallHostEvent::Ended { reason: Some("seal".into()) }]);
    }

    /// Once sealed, `maxOpenFailures` frames in a row that do not open
    /// (clear audio, a replay, a forgery) end the call `seal`; one good frame
    /// between them starts the count again.
    #[tokio::test]
    async fn frames_that_do_not_open_are_counted() {
        let most = etalk::call::VOICE_CALL.seal.max_open_failures;
        let mut call = SealedHost::start(sealing(Some((ROOT.into(), 4, SECRET)), Duration::ZERO), TEST_TIMING).await;
        let caller = call.handshake().await;
        let first = caller.seal_audio(0, 0, &[]).unwrap();
        call.to_host.send(Message::Binary(first.clone().into())).unwrap();
        for _ in 0..most - 1 {
            call.to_host.send(Message::Binary(first.clone().into())).unwrap();
        }
        call.to_host.send(Message::Binary(caller.seal_audio(1, 0, &[]).unwrap().into())).unwrap();
        for _ in 0..most - 1 {
            call.to_host.send(Message::Binary(audio_frame(KIND_UP, 7, &[9; 60]).into())).unwrap();
        }
        assert_eq!(call.text(QUIET).await, None, "one below the limit, twice");
        call.to_host.send(Message::Binary(first.into())).unwrap();
        assert_eq!(call.text(Duration::from_secs(5)).await, Some(json!({ "t": "call_end", "reason": "seal" })));
    }

    /// The host ends EVERY call it answered at `maxCallMs`, a clear one too:
    /// a relay that keeps the room cannot hold a host without end.
    #[tokio::test]
    async fn a_clear_call_ends_at_the_longest_call() {
        let (url, _to_host, mut from_host) = scripted_relay().await;
        let events = std::sync::Mutex::new(Vec::new());
        let (_tx, rx) = flume::unbounded();
        let timing = Timing { max_call: TEST_TIMEOUT, ..TEST_TIMING };
        assert_eq!(run(&url, &rx, &|event| events.lock().unwrap().push(event), timing, None).await, Ok(()));
        assert_eq!(from_host.recv().await, Some(Message::text(json!({ "t": "call_end", "reason": "max_length" }).to_string())));
        assert_eq!(*events.lock().unwrap(), vec![CallHostEvent::Ended { reason: Some("max_length".into()) }]);
    }

    /// A text above `maxPlainBytes` is clipped, never split, and still opens.
    #[tokio::test]
    async fn a_long_text_is_clipped_to_one_frame() {
        let mut call = SealedHost::start(sealing(Some((ROOT.into(), 4, SECRET)), Duration::ZERO), TEST_TIMING).await;
        let caller = call.handshake().await;
        call.up(caller.seal_audio(0, 0, &[]).unwrap(), 1).await;
        call.host.send_event(json!({ "t": "replyStarted", "text": "grüß \"dich\" ".repeat(900) }));
        let sealed = call.text(Duration::from_secs(5)).await.expect("a sealed event");
        assert!(sealed.to_string().len() <= 8192, "under the relay's event cap");
        let plain = caller.open_control(sealed["event"]["data"].as_str().unwrap()).unwrap();
        let most = etalk::call::VOICE_CALL.seal.max_plain_bytes;
        assert!(plain.len() <= most && plain.len() > most / 2, "{}", plain.len());
        let event: Value = serde_json::from_str(&plain).unwrap();
        assert!(event["event"]["text"].as_str().unwrap().starts_with("grüß \"dich\" grüß"));
    }

    /// Measured for the pad step (`seal.audioPadStep.down`): the reply
    /// encoder takes a buffer of one step as a hard limit, also on noise, the
    /// loudest thing a codec can be handed.
    #[test]
    fn the_reply_encoder_keeps_to_one_step() {
        assert_eq!(seal_pad(), CallSealPad::default(), "the fixture's sizes are the core's defaults");
        let step = seal_pad().audio_down;
        let mut noise = 0x2545_f491_4f6c_dd1du64;
        let pcm: Vec<i16> = (0..24_000)
            .map(|_| {
                noise ^= noise << 13;
                noise ^= noise >> 7;
                noise ^= noise << 17;
                (noise >> 48) as i16
            })
            .collect();
        let (mut free, mut held) = (Encoder::new(24_000).unwrap(), Encoder::new(24_000).unwrap());
        let largest = |packets: Vec<(u16, Option<Vec<u8>>)>| (packets.iter().filter(|(_, packet)| packet.is_none()).count(), packets.iter().filter_map(|(_, packet)| Some(packet.as_ref()?.len())).max().unwrap_or(0));
        let (free, held) = (largest(free.packets(&pcm, MAX_FRAME_BYTES - 3, true)), largest(held.packets(&pcm, step, true)));
        println!("reply encoder on noise, 50 frames: largest packet {} bytes into {} bytes, {} into one step of {step} ({} frames not coded)", free.1, MAX_FRAME_BYTES - 3, held.1, held.0);
        assert!(held.1 <= step);
    }

    /// A host that the relay rang and then starves (no hello, no
    /// `call_ready`) ends `no_key` at `handshakeMs` from its socket's open.
    #[tokio::test]
    async fn a_host_that_never_gets_a_hello_ends_at_the_handshake_deadline() {
        let timing = Timing { handshake: TEST_TIMEOUT, ..TEST_TIMING };
        let mut call = SealedHost::start(sealing(Some((ROOT.into(), 4, SECRET)), Duration::ZERO), timing).await;
        let started = std::time::Instant::now();
        assert_eq!(call.text(TEST_TIMEOUT * 10).await, Some(json!({ "t": "call_end", "reason": "no_key" })));
        assert!(started.elapsed() >= TEST_TIMEOUT);
        assert_eq!((&mut call.ended).await.unwrap(), Ok(()));
        assert_eq!(call.events(), vec![CallHostEvent::Ended { reason: Some("no_key".into()) }]);
    }

    /// EXP-1929 (voice-gpui-call-2, voice-hosts-2, lens-cleanup-5): a relay
    /// that beat once and then went silent drops the call within the deadline.
    #[tokio::test]
    async fn a_call_that_goes_silent_drops() {
        let (url, _sent) = silent_relay(vec![text("call_ready"), text("call_alive")]).await;
        let events = std::sync::Mutex::new(Vec::new());
        let (_tx, rx) = flume::unbounded();
        let started = std::time::Instant::now();
        let ended = tokio::time::timeout(TEST_TIMEOUT * 10, run(&url, &rx, &|event| events.lock().unwrap().push(event), TEST_TIMING, None)).await;
        assert_eq!(ended.expect("the dead call ends"), Err("the call went silent".to_string()));
        assert!(started.elapsed() >= TEST_TIMEOUT);
        assert_eq!(*events.lock().unwrap(), vec![CallHostEvent::Ready]);
    }

    /// EXP-1929: an OLDER relay never beats: a silent call is never dropped
    /// for it (a listening host and a muted-free caller say nothing).
    #[tokio::test]
    async fn an_older_relay_without_a_beat_keeps_the_call() {
        let (url, _sent) = silent_relay(vec![text("call_ready")]).await;
        let (_tx, rx) = flume::unbounded();
        let ended = tokio::time::timeout(TEST_TIMEOUT * 4, run(&url, &rx, &|_| {}, TEST_TIMING, None)).await;
        assert!(ended.is_err(), "still in the call: {ended:?}");
    }

    /// EXP-1929: a NEW relay pings; a host (this one and every older build:
    /// tungstenite answers by itself while it reads) pongs while it says
    /// nothing, so the relay sees it alive.
    #[tokio::test]
    async fn a_silent_host_answers_the_relays_ping() {
        let (url, mut sent) = silent_relay(vec![text("call_ready"), Message::Ping(vec![7u8].into())]).await;
        let (_tx, rx) = flume::unbounded();
        tokio::spawn(async move { run(&url, &rx, &|_| {}, TEST_TIMING, None).await });
        let pong = tokio::time::timeout(TEST_TIMEOUT, sent.recv()).await.expect("a pong").unwrap();
        assert_eq!(pong, Message::Pong(vec![7u8].into()));
    }

    #[test]
    fn frames_round_trip() {
        let frame = audio_frame(KIND_DOWN, 0xfffe, &[7, 8, 9]);
        assert_eq!(frame, vec![2, 0xff, 0xfe, 7, 8, 9]);
        assert_eq!(parse_audio_frame(&frame), Some((KIND_DOWN, 0xfffe, &[7u8, 8, 9][..])));
    }

    #[test]
    fn a_bad_frame_is_refused() {
        assert_eq!(parse_audio_frame(&[1, 0, 0]), None);
        assert_eq!(parse_audio_frame(&vec![1; MAX_FRAME_BYTES + 1]), None);
    }

    #[test]
    fn speech_is_framed_every_20_ms_and_decodes() {
        let mut encoder = Encoder::new(24_000).unwrap();
        // 50 ms of a 440 Hz tone: two whole frames, 10 ms kept.
        let tone: Vec<i16> = (0..1200).map(|i| ((i as f32 * 440.0 * std::f32::consts::TAU / 24_000.0).sin() * 8000.0) as i16).collect();
        let frames = encoder.push(&tone);
        assert_eq!(frames.len(), 2);
        assert_eq!(encoder.pending.len(), 240);
        assert_eq!(parse_audio_frame(&frames[1]).unwrap().1, 1);
        assert!(frames.iter().all(|frame| frame.len() <= MAX_FRAME_BYTES));
        // The caller's side, at 16 kHz: one frame = 320 samples.
        let mut up = etalk_codec::Encoder::new(UP_RATE, 24_000, Some(10)).unwrap();
        let mut packet = [0u8; 1500];
        let size = up.encode(&[0i16; 320], &mut packet).unwrap();
        let mut decoder = etalk_codec::Uplink::new(UP_RATE).unwrap();
        assert_eq!(decoder.push(0, &packet[..size]).len(), 320);
    }

    #[test]
    fn only_opus_rates_are_coded() {
        assert!(Encoder::new(22_050).is_err());
        assert!(Encoder::new(48_000).is_ok());
    }
}
