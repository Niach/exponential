//! EXP-1857: the CALLER's socket on a desktop IDE (`fixtures/voice-call.json`
//! TRANSPORT + AUDIO), the twin of [`crate::call_host`]: the IDE calls an
//! agent whose speech runs on ANOTHER of the member's desktops and streams
//! the call to it exactly like web and the phones do.
//!
//! `voice.startCall` mints the caller ticket; [`connect`] dials its `url`
//! as-is. BINARY frames carry audio (byte 0 = kind, bytes 1-2 = u16 BE
//! sequence, then one Opus packet of 20 ms): the microphone goes UP as mono
//! [`UP_RATE`] Hz Opus at [`UP_BITRATE`] with in-band FEC, the host's reply
//! comes DOWN and is decoded at [`DOWN_RATE`] Hz through a [`JITTER_MS`]
//! jitter buffer ([`Downlink`]). JSON text frames are control: `call_ready`
//! (the pair is up: the microphone's frames flow from then on), `call_event`
//! (one flow event the host stepped; the caller steps it too), `call_mute`
//! (to the host), `call_end`, the relay's `error {code}` and `bye`. MUTE
//! keeps sending: the frames are encoded silence. A `speechStarted` event
//! (barge-in) flushes what the jitter buffer holds. The call itself (the
//! machine, the panel, the ring tone) is the IDE's (`etalk_gpui::voice_call`).
//!
//! #EXP-1941: a caller the server told `seal` ([`connect_sealed`]) speaks the
//! SEALED wire instead (voice-call.json `seal`, `call_seal_step`'s caller
//! rows): its hello goes up as a binary frame of type 1, the host's comes
//! back as `call_event {t: "hello"}` with a tag, and only a `finish` that
//! opened that tag gives the sealer the microphone leaves through. Mute is a
//! flag in the sealed audio. Dormant: nothing passes a key source yet.

use std::sync::Arc;

use futures_util::{SinkExt as _, StreamExt as _};
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::Message;

use enet_core::call_seal::{hello_frame, host_hello_of_text, parse_hello, CallFinish, CallHandshake, CallRole, CallSeal};

use crate::call_host::{audio_frame, parse_audio_frame, seal_pad, send_within, Liveness, Timing, FRAME_MS, KIND_DOWN, KIND_UP, MAX_FRAME_BYTES, UP_RATE};
use crate::call_keys::{CallKeyFuture, CallSealing};
use crate::SteerRuntime;

/// voice-call.json `constants.downRate`: the host's reply audio.
pub const DOWN_RATE: u32 = 24_000;
/// voice-call.json `constants.upKbps`.
pub const UP_BITRATE: i32 = 24_000;
/// The loss the uplink's in-band FEC is tuned for (the web caller's).
const UP_LOSS_PERCENT: i32 = 10;
/// voice-call.json `constants.jitterMs`.
pub const JITTER_MS: u32 = 40;

/// What the socket reports to the IDE.
#[derive(Debug, Clone, PartialEq)]
pub enum CallCallerEvent {
    /// `call_ready`: the host joined; the microphone's frames flow now.
    Ready,
    /// `call_event`: one flow event the host stepped (its JSON).
    Event(Value),
    /// The host's reply, decoded and de-jittered: mono [`DOWN_RATE`] Hz.
    Audio(Vec<i16>),
    /// `call_end` from the host (or the relay), with its reason.
    Ended { reason: Option<String> },
    /// The relay refused or ended the call with a code (`error {code}`,
    /// `bye {outcome}`): `no_answer`, `busy`, `device_offline`, ...
    Failed(String),
    /// The socket failed or closed without `call_end`.
    Dropped(String),
}

pub(crate) enum Outbound {
    /// Microphone PCM, mono [`UP_RATE`] Hz.
    Audio(Vec<i16>),
    Mute(bool),
    End,
}

/// The live caller socket; dropping every handle hangs up.
#[derive(Clone)]
pub struct CallCaller {
    tx: flume::Sender<Outbound>,
}

impl CallCaller {
    /// A caller whose socket is already gone (the IDE's lifecycle tests).
    #[doc(hidden)]
    pub fn detached() -> Self {
        Self { tx: flume::unbounded().0 }
    }

    /// The microphone's PCM (mono [`UP_RATE`] Hz), framed and sent.
    pub fn send_audio(&self, pcm: Vec<i16>) {
        let _ = self.tx.send(Outbound::Audio(pcm));
    }

    /// `call_mute`; the frames go on as silence.
    pub fn set_muted(&self, muted: bool) {
        let _ = self.tx.send(Outbound::Mute(muted));
    }

    /// `call_end`, then close.
    pub fn end(&self) {
        let _ = self.tx.send(Outbound::End);
    }
}

/// `a` comes after `b` on the u16 sequence (wrapping).
pub fn seq_after(a: u16, b: u16) -> bool {
    (a.wrapping_sub(b) as i16) > 0
}

/// One control frame as the caller reads it.
#[derive(Debug, Clone, PartialEq)]
pub enum Control {
    Ready,
    Event(Value),
    End { reason: Option<String> },
    Error(String),
    Bye(Option<String>),
    /// EXP-1929: the relay's beat (`call_alive`).
    Alive,
}

/// A text frame's control (None = not one the caller reads).
pub fn parse_control(text: &str) -> Option<Control> {
    let value: Value = serde_json::from_str(text).ok()?;
    let text = |key: &str| value[key].as_str().map(str::to_string);
    Some(match value["t"].as_str()? {
        "call_ready" => Control::Ready,
        "call_event" => Control::Event(value.get("event").filter(|event| event.is_object())?.clone()),
        "call_end" => Control::End { reason: text("reason") },
        "error" => Control::Error(text("code").unwrap_or_else(|| "dropped".into())),
        "bye" => Control::Bye(text("outcome")),
        "call_alive" => Control::Alive,
        _ => return None,
    })
}

/// The microphone into 20 ms Opus `up` frames, the remainder kept; muted =
/// encoded silence (the frames never stop).
pub struct UpFramer {
    opus: etalk_codec::Encoder,
    pending: Vec<i16>,
    seq: u16,
    muted: bool,
}

impl UpFramer {
    pub fn new() -> Result<Self, String> {
        Ok(UpFramer { opus: etalk_codec::Encoder::new(UP_RATE, UP_BITRATE, Some(UP_LOSS_PERCENT))?, pending: Vec::new(), seq: 0, muted: false })
    }

    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
    }

    /// Every whole frame `pcm` completes, as today's wire frames.
    pub fn push(&mut self, pcm: &[i16]) -> Vec<Vec<u8>> {
        self.packets(pcm, MAX_FRAME_BYTES - 3, false).into_iter().filter_map(|(seq, packet)| Some(audio_frame(KIND_UP, seq, &packet?))).collect()
    }

    /// Every whole frame `pcm` completes: its seq and its packet, coded into
    /// a buffer of `room` bytes (a hard limit for the codec). A frame that
    /// could not be coded: skipped as today (the seq moves only with a
    /// packet), or with `count_failed` kept as None under its own seq: the
    /// sealed wire sends an empty frame in its place, which takes that seq.
    fn packets(&mut self, pcm: &[i16], room: usize, count_failed: bool) -> Vec<(u16, Option<Vec<u8>>)> {
        if self.muted {
            self.pending.extend(std::iter::repeat_n(0, pcm.len()));
        } else {
            self.pending.extend_from_slice(pcm);
        }
        let len = (UP_RATE * FRAME_MS / 1000) as usize;
        let mut out = Vec::new();
        let mut packet = vec![0u8; room];
        while self.pending.len() >= len {
            let frame: Vec<i16> = self.pending.drain(..len).collect();
            let coded = self.opus.encode(&frame, &mut packet).map(|size| packet[..size].to_vec());
            if let Err(err) = &coded {
                log::warn!("call caller: opus encode failed: {err}");
            }
            if coded.is_ok() || count_failed {
                out.push((self.seq, coded.ok()));
                self.seq = self.seq.wrapping_add(1);
            }
        }
        out
    }
}

/// A failure code as it came over the wire (`error`, `bye`, `failed`). The
/// codes about the caller's OWN device (`seal.ownDeviceCodes`) are never
/// taken from it: a relay or a host that sends one would send a person to
/// change permissions; it reads `dropped`.
pub fn wire_code(code: String) -> String {
    etalk::call::call_seal_wire_code(&code)
}

/// #EXP-1941: the sealed wire of one call on the caller, from "told `seal`"
/// to sealed (`call_seal_step`'s caller rows).
struct Sealed<'a> {
    told: &'a CallSealing,
    handshake: CallHandshake,
    /// The hello frame: made once per socket, resent unchanged.
    hello: Vec<u8>,
    /// The first `call_ready`: offered, and the handshake's clock.
    offered_at: Option<tokio::time::Instant>,
    /// The first well-formed host hello, taken once.
    host_hello: Option<Vec<u8>>,
    /// `finish` opened the host's tag: only now may the microphone leave.
    seal: Option<CallSeal>,
    failures: u32,
}

impl<'a> Sealed<'a> {
    fn new(told: &'a CallSealing) -> Result<Self, String> {
        let (root, epoch) = match &told.caller_target {
            Some((root, epoch)) => (Some(root.as_str()), *epoch),
            None => (None, 0),
        };
        let handshake = CallHandshake::new(CallRole::Caller, root, epoch).map_err(|refusal| refusal.to_string())?;
        let hello = hello_frame(&handshake.hello().map_err(|refusal| refusal.to_string())?);
        Ok(Sealed { told, handshake, hello, offered_at: None, host_hello: None, seal: None, failures: 0 })
    }
}



/// A [`JITTER_MS`] jitter buffer: the reply's first samples (and the first
/// after a flush) wait until that much is held, then everything flows.
pub struct Jitter {
    target: usize,
    held: Vec<i16>,
    primed: bool,
}

impl Jitter {
    pub fn new(rate: u32) -> Self {
        Jitter { target: (rate * JITTER_MS / 1000) as usize, held: Vec::new(), primed: false }
    }

    /// What may play now.
    pub fn push(&mut self, pcm: &[i16]) -> Vec<i16> {
        if self.primed {
            return pcm.to_vec();
        }
        self.held.extend_from_slice(pcm);
        if self.held.len() < self.target {
            return Vec::new();
        }
        self.primed = true;
        std::mem::take(&mut self.held)
    }

    /// Barge-in: drop what is held; the next reply primes again.
    pub fn flush(&mut self) {
        self.held.clear();
        self.primed = false;
    }
}

/// The host's `down` frames: late ones dropped (worthless in a realtime
/// call), one lost packet recovered from the next one's FEC, decoded at
/// [`DOWN_RATE`] and de-jittered.
pub struct Downlink {
    decoder: etalk_codec::Uplink,
    last: Option<u16>,
    jitter: Jitter,
}

impl Downlink {
    pub fn new() -> Result<Self, String> {
        Ok(Downlink { decoder: etalk_codec::Uplink::new(DOWN_RATE)?, last: None, jitter: Jitter::new(DOWN_RATE) })
    }

    /// The PCM that may play now after frame `seq`.
    pub fn push(&mut self, seq: u16, packet: &[u8]) -> Vec<i16> {
        if self.last.is_some_and(|last| !seq_after(seq, last)) {
            return Vec::new();
        }
        self.last = Some(seq);
        let pcm = self.decoder.push(seq, packet);
        self.jitter.push(&pcm)
    }

    pub fn flush(&mut self) {
        self.jitter.flush();
    }
}

/// Dial `url` (the caller ticket's, as-is) on the steer runtime; `on_event`
/// runs on a runtime thread.
pub fn connect(runtime: &Arc<SteerRuntime>, url: String, on_event: impl Fn(CallCallerEvent) + Send + Sync + 'static) -> CallCaller {
    connect_sealed(runtime, url, None, on_event)
}

/// #EXP-1941: [`connect`] for a caller the server told the call's mode
/// (`voice.startCall`'s `seal`). `sealing` None = today's wire. Some = the
/// sealed wire only: at `call_ready` the caller sends its hello (again every
/// `seal.helloEveryMs`), takes the first well-formed host hello, asks its key
/// source and `finish`es. The capture path seals through the [`CallSeal`]
/// that gives, and that exists only when the host's tag opened: no frame of
/// the microphone leaves before. [`CallCallerEvent::Ready`] comes then. Not
/// sealed `seal.handshakeMs` after the first `call_ready`, or anything that
/// is not the sealed wire: the call fails `dropped`. Never clear.
pub fn connect_sealed(
    runtime: &Arc<SteerRuntime>,
    url: String,
    sealing: Option<CallSealing>,
    on_event: impl Fn(CallCallerEvent) + Send + Sync + 'static,
) -> CallCaller {
    let (tx, rx) = flume::unbounded::<Outbound>();
    runtime.handle().spawn(async move {
        if let Err(err) = run(&url, &rx, &on_event, Timing::fixture(), sealing.as_ref()).await {
            on_event(CallCallerEvent::Dropped(err));
        }
    });
    CallCaller { tx }
}

pub(crate) async fn run(
    url: &str,
    rx: &flume::Receiver<Outbound>,
    on_event: &(impl Fn(CallCallerEvent) + Send + Sync),
    timing: Timing,
    sealing: Option<&CallSealing>,
) -> Result<(), String> {
    let timeout = timing.idle;
    let reasons = &etalk::call::VOICE_CALL.seal.end_reasons;
    let mut sealed = sealing.map(Sealed::new).transpose()?;
    let mut key_job: Option<CallKeyFuture<'_>> = None;
    let mut hello_tick = tokio::time::interval(timing.hello_every);
    hello_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let stream = crate::dial(url).await.map_err(|err| match err {
        crate::DialError::Unauthorized => "the call ticket was refused".to_string(),
        crate::DialError::Other(err) => err,
    })?;
    let (mut sink, mut source) = stream.split();
    let mut up = UpFramer::new()?;
    let mut down = Downlink::new()?;
    let mut ready = false;
    let mut muted = false;
    let mut heard_down = false;
    // EXP-1929: the caller's own frames never stop, so only the relay's
    // side tells whether the path still carries anything.
    let mut liveness = Liveness::new(timeout);
    // The caller gives the call up: `call_end {reason}`, and the person reads `dropped`.
    macro_rules! fail_call {
        ($reason:expr, $why:expr) => {{
            let reason: &str = $reason;
            log::warn!("call caller: the sealed call failed ({reason}): {}", $why);
            let _ = send_within(&mut sink, Message::text(json!({ "t": "call_end", "reason": reason }).to_string()), timeout).await;
            let _ = sink.close().await;
            on_event(CallCallerEvent::Failed("dropped".into()));
            return Ok(());
        }};
    }
    // One flow event for the IDE, clear or opened.
    macro_rules! deliver {
        ($event:expr) => {{
            let mut event: Value = $event;
            log::info!("call caller: host event {}", event["t"].as_str().unwrap_or("?"));
            // Barge-in: the reply still buffered must not keep talking over the member.
            if event["t"] == "speechStarted" {
                down.flush();
            }
            if let Some(code) = event.get("code").and_then(Value::as_str).filter(|_| event["t"] == "failed") {
                event["code"] = wire_code(code.to_string()).into();
            }
            on_event(CallCallerEvent::Event(event));
        }};
    }
    loop {
        let offered = sealed.as_ref().and_then(|sealed| sealed.offered_at.filter(|_| sealed.seal.is_none()));
        let helloing = sealed.as_ref().is_some_and(|sealed| sealed.offered_at.is_some() && sealed.host_hello.is_none());
        tokio::select! {
            () = liveness.dead() => return Err("the call went silent".into()),
            () = async { tokio::time::sleep_until(offered.expect("guarded") + timing.handshake).await }, if offered.is_some() => {
                fail_call!(&reasons.no_key, "not sealed in time")
            }
            _ = hello_tick.tick(), if helloing => {
                let hello = sealed.as_ref().expect("guarded").hello.clone();
                send_within(&mut sink, Message::Binary(hello.into()), timeout).await?;
            }
            key = async { key_job.as_mut().expect("guarded").await }, if key_job.is_some() => {
                key_job = None;
                let sealing = sealed.as_mut().expect("only a sealed call has a key source");
                let key = match key {
                    Ok(key) => key,
                    Err(why) => fail_call!(&reasons.no_key, why),
                };
                let finished = {
                    // The root and the epoch of the caller's OWN answer: `finish`
                    // refuses a host hello that names others, and one whose tag does not open.
                    let input = CallFinish {
                        secret: &key.secret,
                        call_id: &sealing.told.call_id,
                        agent_id: &sealing.told.agent_id,
                        device_id: &sealing.told.device_id,
                        peer_hello: sealing.host_hello.as_deref().expect("the hello that started the source"),
                        root_id: Some(&key.root_id),
                        epoch: key.epoch,
                        pad: seal_pad(),
                    };
                    sealing.handshake.finish(&input)
                };
                match finished {
                    Ok(seal) => {
                        log::info!("call caller: sealed");
                        sealing.seal = Some(seal);
                        ready = true;
                        on_event(CallCallerEvent::Ready);
                    }
                    Err(refusal) => fail_call!(&reasons.seal, refusal),
                }
            }
            inbound = source.next() => {
                let Some(inbound) = inbound else { return Err("the relay closed the call".into()) };
                let inbound = inbound.map_err(|err| crate::redact_ticket(&err.to_string()))?;
                liveness.heard(false);
                match inbound {
                    Message::Binary(frame) => match sealed.as_mut() {
                        None => {
                            if let Some((KIND_DOWN, seq, packet)) = parse_audio_frame(&frame) {
                                if !heard_down {
                                    heard_down = true;
                                    log::info!("call caller: the host's audio arrived");
                                }
                                let pcm = down.push(seq, packet);
                                if !pcm.is_empty() {
                                    on_event(CallCallerEvent::Audio(pcm));
                                }
                            }
                        }
                        // Told, before `call_ready`: nothing is stepped yet.
                        Some(sealing) if sealing.offered_at.is_none() => {}
                        Some(sealing) => {
                            // Before sealed a host sends no binary frame at all.
                            let Some(seal) = sealing.seal.as_ref() else { fail_call!(&reasons.seal, "a binary frame before sealed") };
                            match seal.open_audio(&frame) {
                                Ok(opened) => {
                                    sealing.failures = 0;
                                    // An empty frame carries no packet: the decoder conceals it.
                                    let pcm = if opened.packet.is_empty() { Vec::new() } else { down.push(opened.seq, &opened.packet) };
                                    if !pcm.is_empty() {
                                        on_event(CallCallerEvent::Audio(pcm));
                                    }
                                }
                                Err(refusal) => {
                                    // Counted, never decoded.
                                    sealing.failures += 1;
                                    if sealing.failures >= etalk::call::VOICE_CALL.seal.max_open_failures {
                                        fail_call!(&reasons.seal, format!("{} frames in a row did not open ({refusal})", sealing.failures))
                                    }
                                }
                            }
                        }
                    },
                    Message::Text(text) => match (parse_control(&text), sealed.as_mut()) {
                        (Some(Control::Ready), None) => {
                            log::info!("call caller: the host answered (call_ready)");
                            ready = true;
                            if muted {
                                let frame = json!({ "t": "call_mute", "muted": true }).to_string();
                                send_within(&mut sink, Message::text(frame), timeout).await?;
                            }
                            on_event(CallCallerEvent::Ready);
                        }
                        // Sealed: offered. The hello goes up now and on every
                        // tick until a host hello is taken; again changes nothing.
                        (Some(Control::Ready), Some(sealing)) => {
                            if sealing.offered_at.is_none() {
                                sealing.offered_at = Some(tokio::time::Instant::now());
                                hello_tick.reset_immediately();
                            }
                        }
                        (Some(Control::Event(event)), None) => deliver!(event),
                        // A clear `failed` ends the call with its toast in every state.
                        (Some(Control::Event(event)), Some(_)) if event["t"] == "failed" => deliver!(event),
                        // Told, before `call_ready`: nothing is stepped yet.
                        (Some(Control::Event(_)), Some(sealing)) if sealing.offered_at.is_none() => {}
                        (Some(Control::Event(event)), Some(sealing)) if event["t"] == "hello" => {
                            // The first well-formed one is taken on arrival, once.
                            let hello = event["data"].as_str().and_then(host_hello_of_text).and_then(|hello| Some((parse_hello(&hello, CallRole::Host).ok()?, hello)));
                            match hello {
                                Some((parsed, hello)) if sealing.host_hello.is_none() => {
                                    sealing.host_hello = Some(hello);
                                    let told = sealing.told;
                                    key_job = Some(told.keys.key(parsed));
                                }
                                Some(_) => {}
                                None => log::warn!("call caller: a hello that is not well-formed was dropped"),
                            }
                        }
                        (Some(Control::Event(event)), Some(sealing)) if event["t"] == "sealed" => {
                            let Some(seal) = sealing.seal.as_ref() else { fail_call!(&reasons.seal, "a sealed event before sealed") };
                            // Opened here, in arrival order: the counter is strict.
                            match seal.open_control(event["data"].as_str().unwrap_or("")) {
                                Ok(plain) => match parse_control(&plain) {
                                    Some(Control::Event(inner)) if inner["t"] != "hello" && inner["t"] != "sealed" => deliver!(inner),
                                    _ => log::warn!("call caller: an opened frame that is no flow event was dropped"),
                                },
                                Err(refusal) => fail_call!(&reasons.seal, format!("an event did not open ({refusal})")),
                            }
                        }
                        // Clear content: before sealed and after.
                        (Some(Control::Event(_)), Some(_)) => fail_call!(&reasons.seal, "a clear event in a sealed call"),
                        (Some(Control::End { reason }), _) => {
                            log::info!("call caller: call_end ({})", reason.as_deref().unwrap_or("hung up"));
                            on_event(CallCallerEvent::Ended { reason });
                            let _ = sink.close().await;
                            return Ok(());
                        }
                        (Some(Control::Error(code)), _) => {
                            log::info!("call caller: the relay refused the call: {code}");
                            on_event(CallCallerEvent::Failed(wire_code(code)));
                            let _ = sink.close().await;
                            return Ok(());
                        }
                        (Some(Control::Bye(outcome)), _) => {
                            log::info!("call caller: bye ({})", outcome.as_deref().unwrap_or("-"));
                            on_event(CallCallerEvent::Failed(wire_code(outcome.unwrap_or_else(|| "dropped".into()))));
                            let _ = sink.close().await;
                            return Ok(());
                        }
                        (Some(Control::Alive), _) => liveness.heard(true),
                        (None, _) => {}
                    },
                    Message::Close(_) => {
                        log::info!("call caller: the relay closed the call");
                        return Err("the relay closed the call".into());
                    }
                    _ => {}
                }
            }
            outbound = rx.recv_async() => {
                let Ok(outbound) = outbound else {
                    // Every handle dropped without End: hang up.
                    let _ = send_within(&mut sink, Message::text(json!({ "t": "call_end" }).to_string()), timeout).await;
                    let _ = sink.close().await;
                    return Ok(());
                };
                match outbound {
                    Outbound::Audio(pcm) => {
                        // Before `call_ready` nobody listens; sealed, before the
                        // host's tag opened nothing of the microphone may leave.
                        if !ready {
                            continue;
                        }
                        let frames = match sealed.as_ref() {
                            None => up.push(&pcm),
                            Some(sealing) => {
                                // The ONLY way out for a sealed call's microphone.
                                let Some(seal) = sealing.seal.as_ref() else { continue };
                                let mut frames = Vec::new();
                                for (seq, packet) in up.packets(&pcm, seal_pad().audio_up, true) {
                                    match seal.seal_audio(seq, u8::from(muted), packet.as_deref().unwrap_or(&[])) {
                                        Ok(frame) => frames.push(frame),
                                        Err(refusal) => fail_call!(&reasons.seal, format!("a frame could not be sealed ({refusal})")),
                                    }
                                }
                                frames
                            }
                        };
                        for frame in frames {
                            send_within(&mut sink, Message::Binary(frame.into()), timeout).await?;
                        }
                    }
                    Outbound::Mute(now) => {
                        muted = now;
                        up.set_muted(now);
                        // Sealed: mute rides in every audio frame's flag; the relay cannot see it.
                        if ready && sealed.is_none() {
                            let frame = json!({ "t": "call_mute", "muted": now }).to_string();
                            send_within(&mut sink, Message::text(frame), timeout).await?;
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
    use std::time::Duration;

    /// EXP-1929 (voice-gpui-call-2): a dialled call whose relay beat once
    /// and then went silent drops within the deadline, though its own
    /// microphone frames keep going out.
    #[tokio::test]
    async fn a_dialled_call_that_goes_silent_drops() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/", listener.local_addr().unwrap());
        tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
            for t in ["call_ready", "call_alive"] {
                ws.send(Message::text(json!({ "t": t }).to_string())).await.unwrap();
            }
            while ws.next().await.is_some() {}
        });
        let timeout = Duration::from_millis(300);
        let timing = Timing { idle: timeout, ..TEST_TIMING };
        let (tx, rx) = flume::unbounded();
        tokio::spawn(async move {
            loop {
                if tx.send(Outbound::Audio(vec![0; 320])).is_err() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        });
        let events = std::sync::Mutex::new(Vec::new());
        let ended = tokio::time::timeout(timeout * 10, run(&url, &rx, &|event| events.lock().unwrap().push(event), timing, None)).await;
        assert_eq!(ended.expect("the dead call ends"), Err("the call went silent".to_string()));
        assert_eq!(*events.lock().unwrap(), vec![CallCallerEvent::Ready]);
    }

    // --- #EXP-1941: a sealed call end to end, through a relay that records ----

    use crate::call_host::{self, CallHost, CallHostEvent};
    use crate::call_keys::FixedKeySource;
    use std::sync::Mutex;

    const TEST_TIMING: Timing = Timing { idle: Duration::from_secs(600), handshake: Duration::from_secs(600), max_call: Duration::from_secs(600), hello_every: Duration::from_millis(40) };
    const ROOT: &str = "11111111-1111-4111-8111-111111111111";
    const CALL: &str = "22222222-2222-4222-8222-222222222222";
    const AGENT: &str = "33333333-3333-4333-8333-333333333333";
    const DEVICE: &str = "desk-7f3a";
    const SECRET: [u8; 32] = [7; 32];
    const CANARY: &str = "canary-7f3a";

    fn sealing(key: Option<(&str, u32, [u8; 32])>) -> CallSealing {
        CallSealing {
            call_id: CALL.into(),
            agent_id: AGENT.into(),
            device_id: DEVICE.into(),
            caller_target: None,
            keys: Arc::new(FixedKeySource { key: key.map(|(root, epoch, secret)| (root.to_string(), epoch, secret)), delay: Duration::from_millis(20) }),
        }
    }

    fn good() -> Option<CallSealing> {
        Some(sealing(Some((ROOT, 4, SECRET))))
    }

    /// What the relay does with one frame: `up` = from the caller. The frames
    /// it forwards in its place (none = dropped).
    type Tamper = Box<dyn FnMut(bool, &Message) -> Vec<Message> + Send>;

    fn forward() -> Tamper {
        Box::new(|_, frame| vec![frame.clone()])
    }

    /// A relay in the process: it pairs the host socket (the first to
    /// connect) with the caller's, says `call_ready` to both, forwards what
    /// `tamper` gives and RECORDS every text and binary frame it was sent.
    async fn relay(mut tamper: Tamper) -> (String, Arc<Mutex<Vec<(bool, Message)>>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/", listener.local_addr().unwrap());
        let record = Arc::new(Mutex::new(Vec::new()));
        let seen = record.clone();
        tokio::spawn(async move {
            let mut ends = Vec::new();
            for _ in 0..2 {
                let (tcp, _) = listener.accept().await.unwrap();
                ends.push(tokio_tungstenite::accept_async(tcp).await.unwrap().split());
            }
            let (mut to_caller, mut from_caller) = ends.pop().unwrap();
            let (mut to_host, mut from_host) = ends.pop().unwrap();
            let ready = Message::text(json!({ "t": "call_ready" }).to_string());
            let _ = to_host.send(ready.clone()).await;
            let _ = to_caller.send(ready).await;
            loop {
                let (up, frame) = tokio::select! {
                    frame = from_caller.next() => (true, frame),
                    frame = from_host.next() => (false, frame),
                };
                let Some(Ok(frame)) = frame else { break };
                if !(frame.is_text() || frame.is_binary()) {
                    continue;
                }
                seen.lock().unwrap().push((up, frame.clone()));
                for out in tamper(up, &frame) {
                    let _ = if up { to_host.send(out).await } else { to_caller.send(out).await };
                }
            }
            // One end left: the relay closes the room.
            let _ = to_host.close().await;
            let _ = to_caller.close().await;
        });
        (url, record)
    }

    /// A host and a caller of this crate, paired through the relay.
    struct Pair {
        host: CallHost,
        caller: CallCaller,
        host_events: Arc<Mutex<Vec<CallHostEvent>>>,
        caller_events: Arc<Mutex<Vec<CallCallerEvent>>>,
        record: Arc<Mutex<Vec<(bool, Message)>>>,
    }

    impl Pair {
        async fn start(tamper: Tamper, host: Option<CallSealing>, caller: Option<CallSealing>, timing: Timing) -> Self {
            Self::start_with(tamper, host, caller, timing, timing).await
        }

        /// Each end on its own clock.
        async fn start_with(tamper: Tamper, host: Option<CallSealing>, caller: Option<CallSealing>, host_timing: Timing, timing: Timing) -> Self {
            let (url, record) = relay(tamper).await;
            let (host_events, caller_events) = (Arc::new(Mutex::new(Vec::new())), Arc::new(Mutex::new(Vec::new())));
            let (host_tx, host_rx) = flume::unbounded();
            let (events, at) = (host_events.clone(), url.clone());
            tokio::spawn(async move {
                let on_event = move |event| events.lock().unwrap().push(event);
                if let Err(err) = call_host::run(&at, &host_rx, &on_event, host_timing, host.as_ref()).await {
                    on_event(CallHostEvent::Dropped(err));
                }
            });
            // The relay pairs in connection order: the host first.
            tokio::time::sleep(Duration::from_millis(50)).await;
            let (caller_tx, caller_rx) = flume::unbounded();
            let events = caller_events.clone();
            tokio::spawn(async move {
                let on_event = move |event| events.lock().unwrap().push(event);
                if let Err(err) = run(&url, &caller_rx, &on_event, timing, caller.as_ref()).await {
                    on_event(CallCallerEvent::Dropped(err));
                }
            });
            Pair { host: CallHost::from_sender(host_tx), caller: CallCaller { tx: caller_tx }, host_events, caller_events, record }
        }

        /// Wait until `done` holds for the two ends' events.
        async fn until(&self, what: &str, done: impl Fn(&[CallHostEvent], &[CallCallerEvent]) -> bool) {
            for _ in 0..600 {
                if done(&self.host_events.lock().unwrap(), &self.caller_events.lock().unwrap()) {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            panic!("never: {what} (host: {}, caller: {})", self.host_kinds().join(" "), self.caller_kinds().join(" "));
        }

        fn host_kinds(&self) -> Vec<String> {
            self.host_events.lock().unwrap().iter().map(|event| match event {
                CallHostEvent::Audio(_) => "audio".into(),
                other => format!("{other:?}"),
            }).collect()
        }

        fn caller_kinds(&self) -> Vec<String> {
            self.caller_events.lock().unwrap().iter().map(|event| match event {
                CallCallerEvent::Audio(_) => "audio".into(),
                other => format!("{other:?}"),
            }).collect()
        }

        /// The microphone for `ms`, in 20 ms chunks at real pace.
        async fn speak(&self, ms: u64) {
            for _ in 0..ms / 20 {
                self.caller.send_audio(tone(UP_RATE, 320));
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        }

        fn binary_lengths(&self, up: bool) -> Vec<usize> {
            let mut lengths: Vec<usize> = self.record.lock().unwrap().iter().filter(|(dir, frame)| *dir == up && frame.is_binary()).map(|(_, frame)| frame.len()).collect();
            lengths.sort_unstable();
            lengths.dedup();
            lengths
        }

        /// The canary is in no frame the relay saw: as text, base64 or base64url.
        fn holds_canary(&self) -> bool {
            let forms = [CANARY.to_string(), data_encoding::BASE64.encode(CANARY.as_bytes()), data_encoding::BASE64URL_NOPAD.encode(CANARY.as_bytes())];
            self.record.lock().unwrap().iter().any(|(_, frame)| {
                let bytes = frame.clone().into_data();
                let text = String::from_utf8_lossy(&bytes);
                forms.iter().any(|form| text.contains(form.trim_end_matches('=')))
            })
        }
    }

    fn is_ready(events: &[CallHostEvent]) -> bool {
        events.contains(&CallHostEvent::Ready)
    }

    fn failed_dropped(events: &[CallCallerEvent]) -> bool {
        events.contains(&CallCallerEvent::Failed("dropped".into()))
    }

    /// The whole sealed call: audio and captions arrive, the relay's record
    /// holds no canary and ONE frame length per direction, `Ready` comes with
    /// the first opened frame, mute rides in the flag, and a hello and a
    /// `call_ready` that the relay repeats change nothing.
    #[tokio::test]
    async fn a_sealed_call_end_to_end() {
        let ready = Message::text(json!({ "t": "call_ready" }).to_string());
        let again = ready.clone();
        let repeating: Tamper = Box::new(move |_, frame| if frame.is_binary() && frame.len() == 56 || frame.to_string().contains("\"hello\"") { vec![frame.clone(), again.clone(), frame.clone()] } else { vec![frame.clone()] });
        let call = Pair::start(repeating, good(), good(), TEST_TIMING).await;
        call.caller.send_audio(tone(UP_RATE, 3200));
        call.until("the caller is sealed", |_, caller| caller.contains(&CallCallerEvent::Ready)).await;
        assert!(!is_ready(&call.host_events.lock().unwrap()), "the host is not Ready before a caller frame opened");
        assert_eq!(call.binary_lengths(true), [56], "nothing but the hello went up before the tag opened");
        call.speak(200).await;
        call.until("the host hears the caller", |host, _| is_ready(host) && host.iter().any(|event| matches!(event, CallHostEvent::Audio(_)))).await;
        call.host.send_event(json!({ "t": "said", "text": CANARY }));
        call.host.send_event(json!({ "t": "replyStarted", "text": CANARY }));
        call.host.send_speech(tone(DOWN_RATE, 4800), DOWN_RATE);
        call.until("the caption and the reply arrive", |_, caller| {
            caller.contains(&CallCallerEvent::Event(json!({ "t": "replyStarted", "text": CANARY }))) && caller.iter().any(|event| matches!(event, CallCallerEvent::Audio(_)))
        }).await;
        call.caller.set_muted(true);
        call.speak(100).await;
        call.until("mute arrives through the flag", |host, _| host.contains(&CallHostEvent::Mute(true))).await;
        call.caller.end();
        call.until("the host hears the hang-up", |host, _| host.contains(&CallHostEvent::Ended { reason: None })).await;

        assert!(!call.holds_canary(), "the relay read the call");
        assert_eq!(call.binary_lengths(true), [56, 27 + 120], "the hello, then ONE frame length up");
        assert_eq!(call.binary_lengths(false), [27 + 160], "ONE frame length down");
        let texts: Vec<String> = call.record.lock().unwrap().iter().filter(|(_, frame)| frame.is_text()).map(|(_, frame)| frame.to_string()).collect();
        assert!(texts.iter().all(|text| !text.contains("call_mute")), "no clear mute in a sealed call");
        assert_eq!(call.caller_kinds().iter().filter(|kind| kind.as_str() == "Ready").count(), 1);
    }

    /// The control: the same call unsealed DOES show the canary, and its
    /// frames are today's (a 3-byte header, a clear `call_mute`).
    #[tokio::test]
    async fn a_legacy_pair_sends_todays_bytes() {
        let call = Pair::start(forward(), None, None, TEST_TIMING).await;
        call.until("both ends are ready", |host, caller| is_ready(host) && caller.contains(&CallCallerEvent::Ready)).await;
        call.speak(100).await;
        call.caller.set_muted(true);
        call.host.send_event(json!({ "t": "said", "text": CANARY }));
        call.until("the caption arrives", |host, caller| host.contains(&CallHostEvent::Mute(true)) && caller.contains(&CallCallerEvent::Event(json!({ "t": "said", "text": CANARY })))).await;
        assert!(call.holds_canary(), "the probe finds the canary in a clear call");
        let record = call.record.lock().unwrap();
        assert!(record.iter().any(|(up, frame)| *up && frame.is_text() && frame.to_string().contains("call_mute")));
        let first = record.iter().find(|(up, frame)| *up && frame.is_binary()).unwrap().1.clone().into_data();
        assert_eq!(first[..3], [KIND_UP, 0, 0], "kind, then the u16 seq");
    }

    /// The proof both ways: a relay that answers in the host's place with a
    /// hello of its own eph (any tag, also the real host's) gets NO sealed
    /// frame at all, and the caller fails at once.
    #[tokio::test]
    async fn a_swapped_host_eph_gets_no_frame() {
        for real_tag in [false, true] {
            let swapping: Tamper = Box::new(move |up, frame| {
                let Some(mut event) = (!up).then(|| serde_json::from_str::<Value>(&frame.to_string()).ok()).flatten().filter(|value| value["event"]["t"] == "hello") else { return vec![frame.clone()] };
                let mut hello = host_hello_of_text(event["event"]["data"].as_str().unwrap()).unwrap();
                let own = CallHandshake::new(CallRole::Caller, None, 0).unwrap().hello().unwrap();
                hello[22..54].copy_from_slice(&own[22..]);
                if !real_tag {
                    hello[54..].fill(0x5a);
                }
                event["event"]["data"] = data_encoding::BASE64URL_NOPAD.encode(&hello).into();
                vec![Message::text(event.to_string())]
            });
            let call = Pair::start(swapping, good(), good(), TEST_TIMING).await;
            call.speak(100).await;
            call.until("the caller fails", |_, caller| failed_dropped(caller)).await;
            call.speak(100).await;
            assert_eq!(call.binary_lengths(true), [56], "only hellos ever left the caller (real tag: {real_tag})");
            assert!(!call.caller_kinds().contains(&"Ready".to_string()));
            assert!(!is_ready(&call.host_events.lock().unwrap()));
        }
    }

    /// A relay that swaps the CALLER's eph gets the host's hello and nothing
    /// else: the caller's `finish` fails on the tag, no frame opens at the host.
    #[tokio::test]
    async fn a_swapped_caller_eph_gets_the_hello_and_nothing_else() {
        let swapping: Tamper = Box::new(|up, frame| {
            if !(up && frame.is_binary() && frame.len() == 56) {
                return vec![frame.clone()];
            }
            let mut hello = frame.clone().into_data().to_vec();
            hello[24..].copy_from_slice(&CallHandshake::new(CallRole::Caller, None, 0).unwrap().hello().unwrap()[22..]);
            vec![Message::Binary(hello.into())]
        });
        let call = Pair::start(swapping, good(), good(), Timing { handshake: Duration::from_millis(600), ..TEST_TIMING }).await;
        call.speak(100).await;
        call.until("the caller fails, the host ends", |host, caller| failed_dropped(caller) && host.iter().any(|event| matches!(event, CallHostEvent::Ended { .. } | CallHostEvent::Dropped(_)))).await;
        assert_eq!(call.binary_lengths(true), [56]);
        assert_eq!(call.binary_lengths(false), Vec::<usize>::new(), "the host sent no audio");
        let down: Vec<String> = call.record.lock().unwrap().iter().filter(|(up, _)| !up).map(|(_, frame)| frame.to_string()).collect();
        assert!(down.iter().all(|text| text.contains("\"hello\"") || text.contains("call_end")), "nothing but its hello: {}", down.len());
        assert!(!is_ready(&call.host_events.lock().unwrap()));
    }

    /// A relay that keeps the host's hello back gets no frame either; the
    /// caller ends at `handshakeMs` on its own clock, though the relay drops
    /// the host's `call_end` and keeps beating.
    #[tokio::test]
    async fn a_stripped_hello_ends_at_the_handshake_deadline() {
        let alive = Message::text(json!({ "t": "call_alive" }).to_string());
        let stripping: Tamper = Box::new(move |up, frame| if up { vec![frame.clone()] } else { vec![alive.clone()] });
        let call = Pair::start_with(stripping, good(), good(), TEST_TIMING, Timing { handshake: Duration::from_millis(400), ..TEST_TIMING }).await;
        let started = std::time::Instant::now();
        call.speak(100).await;
        call.until("the caller gives up by itself", |_, caller| failed_dropped(caller)).await;
        assert!(started.elapsed() >= Duration::from_millis(400));
        assert_eq!(call.binary_lengths(true), [56]);
        assert!(call.record.lock().unwrap().iter().any(|(up, frame)| *up && frame.to_string().contains("no_key")), "the caller said why");
    }

    /// Another device, another agent, another key, another root, an epoch
    /// below: the host's tag does not open (or names another root), the
    /// caller fails `dropped` and neither end sends content.
    #[tokio::test]
    async fn a_wrong_end_never_reaches_sealed() {
        let wrong: Vec<CallSealing> = vec![
            CallSealing { device_id: "desk-7f3b".into(), ..sealing(Some((ROOT, 4, SECRET))) },
            CallSealing { agent_id: "33333333-3333-4333-8333-333333333334".into(), ..sealing(Some((ROOT, 4, SECRET))) },
            sealing(Some((ROOT, 4, [8; 32]))),
            sealing(Some(("11111111-1111-4111-8111-111111111112", 4, SECRET))),
            sealing(Some((ROOT, 3, SECRET))),
            sealing(None),
        ];
        for caller in wrong {
            let call = Pair::start(forward(), good(), Some(caller), TEST_TIMING).await;
            call.speak(100).await;
            call.until("the caller fails", |_, caller| failed_dropped(caller)).await;
            assert_eq!(call.binary_lengths(true), [56]);
            assert_eq!(call.binary_lengths(false), Vec::<usize>::new());
            assert!(!is_ready(&call.host_events.lock().unwrap()));
        }
    }

    /// A flipped byte and a replayed frame are dropped and the call goes on;
    /// a deleted control frame ends it (`seal`): the caller reads `dropped`.
    #[tokio::test]
    async fn tampering_with_a_sealed_call() {
        let (mut ups, mut downs) = (0, 0);
        let tampering: Tamper = Box::new(move |up, frame| {
            if up && frame.is_binary() && frame.len() == 147 {
                ups += 1;
                let mut flipped = frame.clone().into_data().to_vec();
                flipped[40] ^= 1;
                // The third frame is flipped; the fifth goes twice.
                return match ups { 3 => vec![Message::Binary(flipped.into())], 5 => vec![frame.clone(), frame.clone()], _ => vec![frame.clone()] };
            }
            if !up && frame.to_string().contains("\"sealed\"") {
                downs += 1;
                // The second sealed event is deleted.
                return if downs == 2 { vec![] } else { vec![frame.clone()] };
            }
            vec![frame.clone()]
        });
        let call = Pair::start(tampering, good(), good(), TEST_TIMING).await;
        call.until("sealed", |_, caller| caller.contains(&CallCallerEvent::Ready)).await;
        call.speak(300).await;
        call.until("the host heard around the bad frames", |host, _| host.iter().filter(|event| matches!(event, CallHostEvent::Audio(_))).count() >= 8).await;
        assert!(call.host_events.lock().unwrap().contains(&CallHostEvent::Lost { ms: 20 }), "the flipped frame is a lost one, never decoded");
        for text in ["one", "two", "three"] {
            call.host.send_event(json!({ "t": "partial", "text": text }));
        }
        call.until("the caller ends the call", |_, caller| failed_dropped(caller)).await;
        let caller = call.caller_events.lock().unwrap();
        assert!(caller.contains(&CallCallerEvent::Event(json!({ "t": "partial", "text": "one" }))));
        assert!(!caller.contains(&CallCallerEvent::Event(json!({ "t": "partial", "text": "three" }))), "nothing after the hole");
        assert!(call.record.lock().unwrap().iter().any(|(up, frame)| *up && frame.is_text() && frame.to_string().contains("call_end") && frame.to_string().contains("\"seal\"")));
    }

    /// A packet the codec could not fit into one step leaves as an empty
    /// frame under ITS seq (the seq moves on), and the frame is the same size.
    #[test]
    fn an_empty_frame_takes_the_seq_of_the_packet_it_replaces() {
        let mut up = UpFramer::new().unwrap();
        let packets = up.packets(&tone(UP_RATE, 640), 0, true);
        assert_eq!(packets.iter().map(|(seq, packet)| (*seq, packet.is_some())).collect::<Vec<_>>(), [(0, false), (1, false)]);
        assert_eq!(up.packets(&tone(UP_RATE, 320), 120, true)[0].0, 2);
        // Today's wire: a frame that could not be coded is skipped, the seq stays.
        let mut legacy = UpFramer::new().unwrap();
        assert!(legacy.packets(&tone(UP_RATE, 320), 0, false).is_empty());
        assert_eq!(parse_audio_frame(&legacy.push(&tone(UP_RATE, 320))[0]).unwrap().1, 0);
        // Measured for the pad step: the microphone's largest packet on noise.
        let mut noise = 0x2545_f491_4f6c_dd1du64;
        let pcm: Vec<i16> = (0..16_000).map(|_| { noise ^= noise << 13; noise ^= noise >> 7; noise ^= noise << 17; (noise >> 48) as i16 }).collect();
        let largest = |room: usize| UpFramer::new().unwrap().packets(&pcm, room, true).iter().map(|(_, packet)| packet.as_ref().map_or(usize::MAX, Vec::len)).max().unwrap();
        println!("microphone encoder on noise, 50 frames: largest packet {} bytes into {} bytes, {} into one step of 120", largest(MAX_FRAME_BYTES - 3), MAX_FRAME_BYTES - 3, largest(120));
        assert!(largest(120) <= 120);
    }

    /// The three codes about the caller's own device are never taken from the wire.
    #[test]
    fn a_wire_code_about_the_own_device_reads_dropped() {
        for code in ["no_microphone", "not_granted", "unsupported"] {
            assert_eq!(wire_code(code.into()), "dropped");
        }
        assert_eq!(wire_code("busy".into()), "busy");
    }

    #[test]
    fn the_beat_is_read() {
        assert!(matches!(parse_control(r#"{"t":"call_alive"}"#), Some(Control::Alive)));
    }

    #[test]
    fn constants_match_the_fixture() {
        let spec = &*etalk::call::VOICE_CALL;
        assert_eq!(DOWN_RATE, spec.constants.down_rate);
        assert_eq!(UP_BITRATE, i32::try_from(spec.constants.up_kbps * 1000).unwrap());
        assert_eq!(JITTER_MS, u32::try_from(spec.constants.jitter_ms).unwrap());
    }

    fn tone(rate: u32, len: usize) -> Vec<i16> {
        (0..len).map(|i| ((i as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin() * 8000.0) as i16).collect()
    }

    /// 20 ms of 16 kHz per `up` frame, numbered, the remainder kept; the
    /// host's decoder reads them back.
    #[test]
    fn the_microphone_goes_up_in_20_ms_frames() {
        let mut up = UpFramer::new().unwrap();
        let frames = up.push(&tone(UP_RATE, 700));
        assert_eq!(frames.len(), 2);
        let (kind, seq, packet) = parse_audio_frame(&frames[1]).unwrap();
        assert_eq!((kind, seq), (KIND_UP, 1));
        assert!(frames.iter().all(|frame| frame.len() <= MAX_FRAME_BYTES));
        let mut host = etalk_codec::Uplink::new(UP_RATE).unwrap();
        assert_eq!(host.push(0, packet).len(), 320);
        assert_eq!(up.push(&tone(UP_RATE, 260)).len(), 1, "60 kept + 260 = one more frame");
    }

    /// Mute never stops the frames: they carry silence.
    #[test]
    fn a_muted_microphone_sends_silence() {
        let mut up = UpFramer::new().unwrap();
        up.set_muted(true);
        let mut host = etalk_codec::Uplink::new(UP_RATE).unwrap();
        let mut loudest = 0i32;
        for (seq, frame) in (0..10).flat_map(|_| up.push(&tone(UP_RATE, 320))).enumerate() {
            let (_, _, packet) = parse_audio_frame(&frame).unwrap();
            loudest = host.push(seq as u16, packet).iter().fold(loudest, |max, sample| max.max(i32::from(*sample).abs()));
        }
        assert!(loudest < 200, "silence on the wire ({loudest})");
    }

    /// The reply waits for `jitterMs` of audio, then flows; a flush primes it again.
    #[test]
    fn the_jitter_buffer_primes_then_flows() {
        let mut jitter = Jitter::new(DOWN_RATE);
        assert!(jitter.push(&[1; 480]).is_empty(), "20 ms held");
        assert_eq!(jitter.push(&[2; 480]).len(), 960, "40 ms: everything held plays");
        assert_eq!(jitter.push(&[3; 480]), vec![3; 480], "primed: straight through");
        jitter.flush();
        assert!(jitter.push(&[4; 480]).is_empty(), "barge-in: primes again");
    }

    /// Down frames decode at 24 kHz; a late or repeated one is dropped.
    #[test]
    fn late_down_frames_are_dropped() {
        let mut encoder = etalk_codec::Encoder::new(DOWN_RATE, 32_000, None).unwrap();
        let mut packet = [0u8; 1500];
        let size = encoder.encode(&tone(DOWN_RATE, 480), &mut packet).unwrap();
        let mut down = Downlink::new().unwrap();
        assert!(down.push(5, &packet[..size]).is_empty(), "priming");
        assert_eq!(down.push(6, &packet[..size]).len(), 960);
        assert!(down.push(6, &packet[..size]).is_empty(), "a repeat");
        assert!(down.push(4, &packet[..size]).is_empty(), "late");
        assert_eq!(down.push(7, &packet[..size]).len(), 480);
        assert!(seq_after(0, u16::MAX) && !seq_after(u16::MAX, 0), "the sequence wraps");
    }

    #[test]
    fn control_frames_parse() {
        assert_eq!(parse_control(r#"{"t":"call_ready"}"#), Some(Control::Ready));
        assert_eq!(
            parse_control(r#"{"t":"call_event","event":{"t":"rang"}}"#),
            Some(Control::Event(json!({ "t": "rang" })))
        );
        assert_eq!(parse_control(r#"{"t":"call_event"}"#), None, "no event");
        assert_eq!(parse_control(r#"{"t":"call_end","reason":"idle"}"#), Some(Control::End { reason: Some("idle".into()) }));
        assert_eq!(parse_control(r#"{"t":"call_end"}"#), Some(Control::End { reason: None }));
        assert_eq!(parse_control(r#"{"t":"error","code":"no_answer"}"#), Some(Control::Error("no_answer".into())));
        assert_eq!(parse_control(r#"{"t":"bye","outcome":"busy"}"#), Some(Control::Bye(Some("busy".into()))));
        assert_eq!(parse_control(r#"{"t":"call_ring"}"#), None);
        assert_eq!(parse_control("not json"), None);
        // The host's flow events are the IDE's machine's.
        let event = json!({ "t": "start", "device": "mint", "fallback": true });
        assert!(matches!(etalk::call::call_event_of_wire(&event), Some(etalk::call::CallEvent::Start { fallback: true, .. })));
    }
}
