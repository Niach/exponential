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

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt as _, StreamExt as _};
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::Message;

use crate::call_host::{audio_frame, idle_timeout, parse_audio_frame, send_within, Liveness, FRAME_MS, KIND_DOWN, KIND_UP, MAX_FRAME_BYTES, UP_RATE};
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

enum Outbound {
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

    /// Every whole frame `pcm` completes, as wire frames.
    pub fn push(&mut self, pcm: &[i16]) -> Vec<Vec<u8>> {
        if self.muted {
            self.pending.extend(std::iter::repeat_n(0, pcm.len()));
        } else {
            self.pending.extend_from_slice(pcm);
        }
        let len = (UP_RATE * FRAME_MS / 1000) as usize;
        let mut out = Vec::new();
        let mut packet = [0u8; MAX_FRAME_BYTES - 3];
        while self.pending.len() >= len {
            let frame: Vec<i16> = self.pending.drain(..len).collect();
            match self.opus.encode(&frame, &mut packet) {
                Ok(size) => {
                    out.push(audio_frame(KIND_UP, self.seq, &packet[..size]));
                    self.seq = self.seq.wrapping_add(1);
                }
                Err(err) => log::warn!("call caller: opus encode failed: {err}"),
            }
        }
        out
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
    let (tx, rx) = flume::unbounded::<Outbound>();
    runtime.handle().spawn(async move {
        if let Err(err) = run(&url, &rx, &on_event, idle_timeout()).await {
            on_event(CallCallerEvent::Dropped(err));
        }
    });
    CallCaller { tx }
}

async fn run(url: &str, rx: &flume::Receiver<Outbound>, on_event: &(impl Fn(CallCallerEvent) + Send + Sync), timeout: Duration) -> Result<(), String> {
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
    loop {
        tokio::select! {
            () = liveness.dead() => return Err("the call went silent".into()),
            inbound = source.next() => {
                let Some(inbound) = inbound else { return Err("the relay closed the call".into()) };
                let inbound = inbound.map_err(|err| crate::redact_ticket(&err.to_string()))?;
                liveness.heard(false);
                match inbound {
                    Message::Binary(frame) => {
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
                    Message::Text(text) => match parse_control(&text) {
                        Some(Control::Ready) => {
                            log::info!("call caller: the host answered (call_ready)");
                            ready = true;
                            if muted {
                                let frame = json!({ "t": "call_mute", "muted": true }).to_string();
                                send_within(&mut sink, Message::text(frame), timeout).await?;
                            }
                            on_event(CallCallerEvent::Ready);
                        }
                        Some(Control::Event(event)) => {
                            log::info!("call caller: host event {}", event["t"].as_str().unwrap_or("?"));
                            // Barge-in: the reply still buffered must not
                            // keep talking over the member.
                            if event["t"] == "speechStarted" {
                                down.flush();
                            }
                            on_event(CallCallerEvent::Event(event));
                        }
                        Some(Control::End { reason }) => {
                            log::info!("call caller: call_end ({})", reason.as_deref().unwrap_or("hung up"));
                            on_event(CallCallerEvent::Ended { reason });
                            let _ = sink.close().await;
                            return Ok(());
                        }
                        Some(Control::Error(code)) => {
                            log::info!("call caller: the relay refused the call: {code}");
                            on_event(CallCallerEvent::Failed(code));
                            let _ = sink.close().await;
                            return Ok(());
                        }
                        Some(Control::Bye(outcome)) => {
                            log::info!("call caller: bye ({})", outcome.as_deref().unwrap_or("-"));
                            on_event(CallCallerEvent::Failed(outcome.unwrap_or_else(|| "dropped".into())));
                            let _ = sink.close().await;
                            return Ok(());
                        }
                        Some(Control::Alive) => liveness.heard(true),
                        None => {}
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
                        // Before `call_ready` nobody listens.
                        if !ready {
                            continue;
                        }
                        for frame in up.push(&pcm) {
                            send_within(&mut sink, Message::Binary(frame.into()), timeout).await?;
                        }
                    }
                    Outbound::Mute(now) => {
                        muted = now;
                        up.set_muted(now);
                        if ready {
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
        let ended = tokio::time::timeout(timeout * 10, run(&url, &rx, &|event| events.lock().unwrap().push(event), timeout)).await;
        assert_eq!(ended.expect("the dead call ends"), Err("the call went silent".to_string()));
        assert_eq!(*events.lock().unwrap(), vec![CallCallerEvent::Ready]);
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
