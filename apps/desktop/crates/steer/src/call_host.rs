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
//! is the IDE's (`ui::voice_call`).

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt as _, StreamExt as _};
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::Message;

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

/// EXP-1929 (`livenessCases`): once the relay beat (`call_alive`), a call
/// that hears nothing from it for voice-call.json `idleTimeoutMs` is dropped.
pub fn idle_timeout() -> Duration {
    Duration::from_millis(etalk::call::VOICE_CALL.constants.idle_timeout_ms)
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

/// What the socket reports to the IDE.
#[derive(Debug, Clone, PartialEq)]
pub enum CallHostEvent {
    /// `call_ready`: the caller is connected.
    Ready,
    /// The caller's audio, decoded: mono [`UP_RATE`] Hz.
    Audio(Vec<i16>),
    /// `call_mute`.
    Mute(bool),
    /// `call_end` from the caller (or the relay).
    Ended { reason: Option<String> },
    /// The socket failed or closed without `call_end`.
    Dropped(String),
}

enum Outbound {
    Event(Value),
    Speech { pcm: Vec<i16>, rate: u32 },
    /// Barge-in: drop the encoder's partial frame.
    Flush,
    End,
}

/// The live call socket; dropping it does NOT end the call (call [`end`]).
///
/// [`end`]: CallHost::end
#[derive(Clone)]
pub struct CallHost {
    tx: flume::Sender<Outbound>,
}

impl CallHost {
    /// A host whose socket is already gone: every send is dropped (the IDE's
    /// call lifecycle tests).
    #[doc(hidden)]
    pub fn detached() -> Self {
        Self { tx: flume::unbounded().0 }
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

    /// Every whole frame `pcm` completes, as wire frames.
    fn push(&mut self, pcm: &[i16]) -> Vec<Vec<u8>> {
        self.pending.extend_from_slice(pcm);
        let len = self.frame_len();
        let mut out = Vec::new();
        let mut packet = [0u8; MAX_FRAME_BYTES - 3];
        while self.pending.len() >= len {
            let frame: Vec<i16> = self.pending.drain(..len).collect();
            match self.opus.encode(&frame, &mut packet) {
                Ok(size) => {
                    out.push(audio_frame(KIND_DOWN, self.seq, &packet[..size]));
                    self.seq = self.seq.wrapping_add(1);
                }
                Err(err) => log::warn!("call host: opus encode failed: {err}"),
            }
        }
        out
    }
}

/// Dial `url` (the server-provided ticket URL, as-is) on the steer runtime;
/// `on_event` runs on a runtime thread.
pub fn connect(
    runtime: &Arc<SteerRuntime>,
    url: String,
    on_event: impl Fn(CallHostEvent) + Send + Sync + 'static,
) -> CallHost {
    let (tx, rx) = flume::unbounded::<Outbound>();
    runtime.handle().spawn(async move {
        if let Err(err) = run(&url, &rx, &on_event, idle_timeout()).await {
            on_event(CallHostEvent::Dropped(err));
        }
    });
    CallHost { tx }
}

async fn run(url: &str, rx: &flume::Receiver<Outbound>, on_event: &(impl Fn(CallHostEvent) + Send + Sync), timeout: Duration) -> Result<(), String> {
    let stream = crate::dial(url).await.map_err(|err| match err {
        crate::DialError::Unauthorized => "the call ticket was refused".to_string(),
        crate::DialError::Other(err) => err,
    })?;
    let (mut sink, mut source) = stream.split();
    let mut decoder = etalk_codec::Uplink::new(UP_RATE)?;
    let mut encoder: Option<Encoder> = None;
    let mut liveness = Liveness::new(timeout);
    loop {
        tokio::select! {
            () = liveness.dead() => return Err("the call went silent".into()),
            inbound = source.next() => {
                let Some(inbound) = inbound else { return Err("the relay closed the call".into()) };
                let inbound = inbound.map_err(|err| crate::redact_ticket(&err.to_string()))?;
                liveness.heard(false);
                match inbound {
                    Message::Binary(frame) => {
                        if let Some((KIND_UP, seq, packet)) = parse_audio_frame(&frame) {
                            let pcm = decoder.push(seq, packet);
                            if !pcm.is_empty() {
                                on_event(CallHostEvent::Audio(pcm));
                            }
                        }
                    }
                    Message::Text(text) => {
                        let Ok(value) = serde_json::from_str::<Value>(&text) else { continue };
                        match value["t"].as_str() {
                            Some("call_ready") => on_event(CallHostEvent::Ready),
                            Some("call_alive") => liveness.heard(true),
                            Some("call_mute") => on_event(CallHostEvent::Mute(value["muted"].as_bool().unwrap_or(false))),
                            Some("call_end") | Some("bye") => {
                                on_event(CallHostEvent::Ended { reason: value["reason"].as_str().or(value["outcome"].as_str()).map(str::to_string) });
                                return Ok(());
                            }
                            Some("error") => {
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
                        let frame = json!({ "t": "call_event", "event": event }).to_string();
                        send_within(&mut sink, Message::text(frame), timeout).await?;
                    }
                    Outbound::Speech { pcm, rate } => {
                        if encoder.as_ref().is_none_or(|encoder| encoder.rate != rate) {
                            encoder = Some(Encoder::new(rate)?);
                        }
                        for frame in encoder.as_mut().expect("set above").push(&pcm) {
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

    /// EXP-1929 (voice-gpui-call-2, voice-hosts-2, lens-cleanup-5): a relay
    /// that beat once and then went silent drops the call within the deadline.
    #[tokio::test]
    async fn a_call_that_goes_silent_drops() {
        let (url, _sent) = silent_relay(vec![text("call_ready"), text("call_alive")]).await;
        let events = std::sync::Mutex::new(Vec::new());
        let (_tx, rx) = flume::unbounded();
        let started = std::time::Instant::now();
        let ended = tokio::time::timeout(TEST_TIMEOUT * 10, run(&url, &rx, &|event| events.lock().unwrap().push(event), TEST_TIMEOUT)).await;
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
        let ended = tokio::time::timeout(TEST_TIMEOUT * 4, run(&url, &rx, &|_| {}, TEST_TIMEOUT)).await;
        assert!(ended.is_err(), "still in the call: {ended:?}");
    }

    /// EXP-1929: a NEW relay pings; a host (this one and every older build:
    /// tungstenite answers by itself while it reads) pongs while it says
    /// nothing, so the relay sees it alive.
    #[tokio::test]
    async fn a_silent_host_answers_the_relays_ping() {
        let (url, mut sent) = silent_relay(vec![text("call_ready"), Message::Ping(vec![7u8].into())]).await;
        let (_tx, rx) = flume::unbounded();
        tokio::spawn(async move { run(&url, &rx, &|_| {}, TEST_TIMEOUT).await });
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
