//! NEW-57: a MEETING's channels (`packages/domain-contract/fixtures/meeting.json`
//! CHANNELS, RESIDENCY, PAUSE): the microphone (`you`) and every captured
//! app (or the one `desktop` channel) each run the push-to-talk segment
//! rules of talk-listen.json on their OWN VAD stream, never a mix. Every
//! closed segment is transcribed at once (one at a time on the listening
//! thread: the transcriber is single-stream, so segments of different
//! channels queue there and never share a clip) and comes back as
//! `MeetingSegment` with epoch timestamps; a `you` segment the wake phrase
//! opens adds `MeetingWake`. Partials are never made. Paused = every
//! channel drops its audio and its open segment.
//!
//! Audio is never stored, except with `EXP_MEETING_AUDIO_RING=1` (debug,
//! opt-in): the last [`RING_SECONDS`] per channel in memory, written as
//! `<data dir>/meeting-debug/<channel>.wav` when the meeting stops.

use std::collections::{BTreeMap, VecDeque};
use std::path::Path;

use etalk::{Event, ListenFailure};

use crate::listen::{normalize_transcript, Segment, Segmenter, Transcriber, Vad, FRAME, RATE};

/// The debug ring's opt-in switch.
pub const RING_ENV: &str = "EXP_MEETING_AUDIO_RING";
/// How much audio the debug ring keeps per channel.
pub const RING_SECONDS: usize = 60;
/// A channel that sent nothing for this long closes its open segment (an
/// app that stopped playing mid-sentence).
pub const CHANNEL_IDLE_MS: i64 = 1_000;
/// The debug ring's directory under the worker's data dir.
pub const DEBUG_DIR: &str = "meeting-debug";
/// Dropped audio is drained in steps of at least this many samples.
const DRAIN_STEP: usize = RATE;

/// Whether the debug ring is on (`EXP_MEETING_AUDIO_RING=1`).
pub fn ring_enabled() -> bool {
    std::env::var(RING_ENV).is_ok_and(|value| value == "1")
}

/// One channel's speech stream.
struct Channel<V> {
    label: String,
    vad: V,
    /// The samples from absolute sample `base` on (older ones are dropped).
    audio: Vec<f32>,
    base: usize,
    segmenter: Segmenter,
    /// Epoch ms at which the newest sample was CAPTURED (the clock the
    /// segment times are read against).
    last_ms: i64,
    /// The worker's clock when it was last fed (the idle rule reads this:
    /// a backlog of old chunks is not silence).
    heard_ms: i64,
}

impl<V> Channel<V> {
    /// Epoch ms of absolute sample `sample`.
    fn at(&self, sample: usize) -> i64 {
        let end = self.base + self.audio.len();
        self.last_ms - (end.saturating_sub(sample) as i64 * 1000 / RATE as i64)
    }

    /// Frames `[start, end)` as a clip of what is held.
    fn clip(&self, segment: Segment) -> &[f32] {
        let clip = |frame: usize| frame.saturating_mul(FRAME).saturating_sub(self.base).min(self.audio.len());
        &self.audio[clip(segment.start)..clip(segment.end)]
    }
}

/// A meeting's channels and what they heard.
pub struct Meeting<V> {
    id: String,
    language: String,
    /// The wake phrase a `you` segment may open with (empty = none).
    phrase: String,
    channels: BTreeMap<String, Channel<V>>,
    paused: bool,
    /// The debug ring per channel (None = off: audio is never kept).
    rings: Option<BTreeMap<String, VecDeque<f32>>>,
    segments: usize,
    /// The newest `now_ms` [`Self::idle`] was given (the worker's clock).
    clock: i64,
}

impl<V: Vad> Meeting<V> {
    pub fn new(id: impl Into<String>, language: impl Into<String>, phrase: impl Into<String>, ring: bool) -> Self {
        Meeting {
            id: id.into(),
            language: language.into(),
            phrase: phrase.into(),
            channels: BTreeMap::new(),
            paused: false,
            rings: ring.then(BTreeMap::new),
            segments: 0,
            clock: 0,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn paused(&self) -> bool {
        self.paused
    }

    /// The segments emitted so far.
    pub fn segments(&self) -> usize {
        self.segments
    }

    /// The channels heard so far (`you` first when present, then by id).
    pub fn channels(&self) -> Vec<&str> {
        self.channels.keys().map(String::as_str).collect()
    }

    /// Pause or resume: pausing drops every open segment and what is held
    /// (resuming starts each channel afresh, with a new VAD stream).
    pub fn set_paused(&mut self, paused: bool) {
        if paused && !self.paused {
            self.channels.clear();
        }
        self.paused = paused;
    }

    /// Whether `channel` needs a VAD before [`Self::feed`] (it is new); never
    /// while paused (its audio is dropped).
    pub fn needs(&self, channel: &str) -> bool {
        !self.paused && !self.channels.contains_key(channel)
    }

    /// Start `channel` with its own VAD stream.
    pub fn add(&mut self, channel: &str, label: &str, vad: V) {
        self.channels.insert(
            channel.to_string(),
            Channel { label: label.to_string(), vad, audio: Vec::new(), base: 0, segmenter: Segmenter::default(), last_ms: 0, heard_ms: 0 },
        );
    }

    /// `samples` of `channel` (mono 16 kHz) whose LAST one was captured at
    /// `now_ms` (the chunk's capture time, never when the worker read it:
    /// a backlog does not shift the segment times); the segments they
    /// close are transcribed and emitted. Dropped while paused, or for a
    /// channel never [`Self::add`]ed.
    pub fn feed(
        &mut self,
        channel: &str,
        label: &str,
        samples: &[f32],
        now_ms: i64,
        transcriber: &mut dyn Transcriber,
        emit: &mut dyn FnMut(Event),
    ) -> Result<(), (ListenFailure, String)> {
        if self.paused {
            return Ok(());
        }
        let Some(stream) = self.channels.get_mut(channel) else { return Ok(()) };
        if let Some(rings) = self.rings.as_mut() {
            let ring = rings.entry(channel.to_string()).or_default();
            ring.extend(samples.iter().copied());
            let over = ring.len().saturating_sub(RING_SECONDS * RATE);
            ring.drain(..over);
        }
        if stream.label != label {
            stream.label = label.to_string();
        }
        stream.audio.extend_from_slice(samples);
        stream.last_ms = now_ms;
        stream.heard_ms = now_ms.max(self.clock);
        let probabilities = stream.vad.push(samples).map_err(|err| (ListenFailure::Failed, err))?;
        let closed: Vec<Segment> = probabilities.into_iter().filter_map(|probability| stream.segmenter.push(probability)).collect();
        for segment in closed {
            self.close(channel, segment, transcriber, emit)?;
        }
        let Some(stream) = self.channels.get_mut(channel) else { return Ok(()) };
        // Bounded: nothing before the next possible segment is read again.
        let keep = stream.segmenter.keep_from() * FRAME;
        if keep >= stream.base + DRAIN_STEP {
            let drop = (keep - stream.base).min(stream.audio.len());
            stream.audio.drain(..drop);
            stream.base += drop;
        }
        Ok(())
    }

    /// A channel silent for [`CHANNEL_IDLE_MS`] closes its open segment
    /// where the speech ended (an app that stopped playing mid-phrase).
    pub fn idle(&mut self, now_ms: i64, transcriber: &mut dyn Transcriber, emit: &mut dyn FnMut(Event)) -> Result<(), (ListenFailure, String)> {
        self.clock = self.clock.max(now_ms);
        let idle: Vec<(String, Segment)> = self
            .channels
            .iter_mut()
            .filter(|(_, stream)| now_ms - stream.heard_ms >= CHANNEL_IDLE_MS && stream.segmenter.open_start().is_some())
            .filter_map(|(channel, stream)| Some((channel.clone(), stream.segmenter.finish()?)))
            .collect();
        for (channel, segment) in idle {
            self.close(&channel, segment, transcriber, emit)?;
        }
        Ok(())
    }

    /// A draining stop: every channel's open segment closes where its
    /// speech ended and is transcribed now.
    pub fn finish(&mut self, transcriber: &mut dyn Transcriber, emit: &mut dyn FnMut(Event)) -> Result<(), (ListenFailure, String)> {
        let open: Vec<(String, Segment)> =
            self.channels.iter_mut().filter_map(|(channel, stream)| Some((channel.clone(), stream.segmenter.finish()?))).collect();
        for (channel, segment) in open {
            self.close(&channel, segment, transcriber, emit)?;
        }
        Ok(())
    }

    fn close(
        &mut self,
        channel: &str,
        segment: Segment,
        transcriber: &mut dyn Transcriber,
        emit: &mut dyn FnMut(Event),
    ) -> Result<(), (ListenFailure, String)> {
        let Some(stream) = self.channels.get(channel) else { return Ok(()) };
        let clip = stream.clip(segment);
        if clip.is_empty() {
            return Ok(());
        }
        let text = normalize_transcript(&transcriber.transcribe(clip, &self.language).map_err(|err| (ListenFailure::Failed, err))?);
        if text.is_empty() {
            return Ok(());
        }
        let woke = channel == etalk::YOU_CHANNEL
            && !self.phrase.trim().is_empty()
            && crate::wake::wake_match(&self.phrase, &text).is_some();
        let (start_ms, end_ms) = (stream.at(segment.start * FRAME), stream.at(segment.end * FRAME));
        self.segments += 1;
        emit(Event::MeetingSegment {
            id: self.id.clone(),
            channel: channel.to_string(),
            label: stream.label.clone(),
            text,
            start_ms,
            end_ms,
            // CrispASR's session API hands back segment text only: the
            // segment's own start and end stand for its words.
            words: Vec::new(),
        });
        if woke {
            emit(Event::MeetingWake { id: self.id.clone() });
        }
        Ok(())
    }

    /// The meeting ended: open segments are dropped; with the debug ring on,
    /// each channel's last [`RING_SECONDS`] go to `<dir>/<channel>.wav`.
    /// Answers the files written.
    pub fn stop(self, dir: Option<&Path>) -> Vec<std::path::PathBuf> {
        let (Some(rings), Some(dir)) = (self.rings, dir) else { return Vec::new() };
        if std::fs::create_dir_all(dir).is_err() {
            return Vec::new();
        }
        let mut written = Vec::new();
        for (channel, ring) in rings {
            let path = dir.join(format!("{}.wav", file_name(&channel)));
            let samples: Vec<f32> = ring.into_iter().collect();
            match std::fs::write(&path, wav(&samples)) {
                Ok(()) => written.push(path),
                Err(err) => eprintln!("meeting: the debug ring was not written ({err})"),
            }
        }
        written
    }
}

/// A channel id as a file name (`us.zoom.xos` stays, `/` and the like go).
fn file_name(channel: &str) -> String {
    let name: String =
        channel.chars().map(|ch| if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') { ch } else { '_' }).collect();
    if name.trim_matches('.').is_empty() { "channel".into() } else { name }
}

/// Mono 16 kHz f32 as a 16-bit PCM WAV.
pub fn wav(samples: &[f32]) -> Vec<u8> {
    let data = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(RATE as u32).to_le_bytes());
    out.extend_from_slice(&(RATE as u32 * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for &sample in samples {
        out.extend_from_slice(&etalk::to_i16(sample).to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::listener::fake::EnergyVad;

    /// Names each clip by its count (alpha, bravo, ...) and its loudness, so
    /// a test sees which channel's audio a clip held.
    #[derive(Default)]
    struct Clips(Vec<f32>);

    impl Transcriber for Clips {
        fn transcribe(&mut self, pcm: &[f32], _language: &str) -> Result<String, String> {
            let peak = pcm.iter().fold(0f32, |peak, sample| peak.max(sample.abs()));
            self.0.push(peak);
            const NAMES: [&str; 6] = ["alpha", "bravo", "charlie", "delta", "echo", "foxtrot"];
            Ok(format!("{} at {peak:.1}", NAMES[(self.0.len() - 1) % NAMES.len()]))
        }

        fn partials(&self) -> bool {
            false
        }
    }

    fn tone(seconds: f32, amplitude: f32) -> Vec<f32> {
        (0..(seconds * RATE as f32) as usize)
            .map(|index| amplitude * (std::f32::consts::TAU * 440.0 * index as f32 / RATE as f32).sin())
            .collect()
    }

    /// 0.2 s quiet, `seconds` of speech at `amplitude`, 0.5 s quiet.
    fn phrase(seconds: f32, amplitude: f32) -> Vec<f32> {
        [tone(0.2, 0.0), tone(seconds, amplitude), tone(0.5, 0.0)].concat()
    }

    fn segments(events: &[Event]) -> Vec<(String, String, String, i64, i64)> {
        events
            .iter()
            .filter_map(|event| match event {
                Event::MeetingSegment { channel, label, text, start_ms, end_ms, .. } => {
                    Some((channel.clone(), label.clone(), text.clone(), *start_ms, *end_ms))
                }
                _ => None,
            })
            .collect()
    }

    /// Feed `streams` interleaved in 20 ms chunks, the clock moving with
    /// them from `t0`.
    fn interleave(meeting: &mut Meeting<EnergyVad>, streams: &[(&str, &str, Vec<f32>)], t0: i64, clips: &mut Clips) -> Vec<Event> {
        let mut events = Vec::new();
        let chunk = RATE / 50;
        let longest = streams.iter().map(|(_, _, audio)| audio.len()).max().unwrap_or(0);
        let mut at = 0;
        while at < longest {
            for (channel, label, audio) in streams {
                if at >= audio.len() {
                    continue;
                }
                if meeting.needs(channel) {
                    meeting.add(channel, label, EnergyVad::default());
                }
                let end = (at + chunk).min(audio.len());
                let now = t0 + (end * 1000 / RATE) as i64;
                meeting.feed(channel, label, &audio[at..end], now, clips, &mut |event| events.push(event)).unwrap();
            }
            at += chunk;
        }
        events
    }

    #[test]
    fn two_channels_interleaved_are_segmented_apart_and_never_mixed() {
        let mut meeting = Meeting::new("m1", "en", "", false);
        let mut clips = Clips::default();
        // `you` speaks loud at 0.2-1.0 s; Zoom softly at 0.7-1.9 s: their
        // speech overlaps in time.
        let you = phrase(0.8, 0.8);
        let zoom = [tone(0.5, 0.0), phrase(1.2, 0.3)].concat();
        let t0 = 1_800_000_000_000;
        let events = interleave(&mut meeting, &[("you", "You", you), ("us.zoom.xos", "Zoom", zoom)], t0, &mut clips);
        let got = segments(&events);
        assert_eq!(got.len(), 2, "{got:?}");
        let you = got.iter().find(|segment| segment.0 == "you").expect("a you segment");
        let zoom = got.iter().find(|segment| segment.0 == "us.zoom.xos").expect("a zoom segment");
        assert_eq!(you.1, "You");
        assert_eq!(zoom.1, "Zoom");
        // Each clip held only its own channel's audio.
        assert!(you.2.ends_with("at 0.8"), "{you:?}");
        assert!(zoom.2.ends_with("at 0.3"), "{zoom:?}");
        // Epoch times on the shared clock (the padding reaches 64 ms out).
        assert!((you.3 - (t0 + 200)).abs() <= 100, "you starts ~0.2 s in: {}", you.3 - t0);
        assert!((zoom.3 - (t0 + 700)).abs() <= 100, "zoom starts ~0.7 s in: {}", zoom.3 - t0);
        assert!(you.4 > you.3 && zoom.4 > zoom.3);
        assert!((zoom.4 - (t0 + 1900)).abs() <= 150, "zoom ends ~1.9 s in: {}", zoom.4 - t0);
        assert_eq!(meeting.channels(), ["us.zoom.xos", "you"]);
        assert_eq!(meeting.segments(), 2);
    }

    #[test]
    fn the_wake_phrase_opening_a_you_segment_is_a_flush_trigger() {
        // The fake transcriber's first clip opens with `alpha`: the phrase
        // matches it only, and only on `you`.
        let mut meeting = Meeting::new("m1", "en", "Alpha", false);
        let mut clips = Clips::default();
        let events = interleave(&mut meeting, &[("you", "You", [phrase(0.6, 0.5), phrase(0.6, 0.5)].concat())], 0, &mut clips);
        let kinds: Vec<&str> = events
            .iter()
            .map(|event| match event {
                Event::MeetingSegment { .. } => "segment",
                Event::MeetingWake { .. } => "wake",
                _ => "other",
            })
            .collect();
        assert_eq!(kinds, ["segment", "wake", "segment"]);

        let mut meeting = Meeting::new("m2", "en", "Alpha", false);
        let events = interleave(&mut meeting, &[("desktop", "Desktop audio", phrase(0.6, 0.5))], 0, &mut Clips::default());
        assert!(!events.iter().any(|event| matches!(event, Event::MeetingWake { .. })), "never on another channel");
    }

    #[test]
    fn paused_channels_drop_their_audio_and_open_segments() {
        let mut meeting = Meeting::new("m1", "en", "", false);
        let mut clips = Clips::default();
        let mut events = Vec::new();
        meeting.add("you", "You", EnergyVad::default());
        // Half a phrase, then the pause: the open segment goes.
        let speech = tone(0.6, 0.5);
        meeting.feed("you", "You", &speech, 600, &mut clips, &mut |event| events.push(event)).unwrap();
        meeting.set_paused(true);
        assert!(!meeting.needs("you"), "no VAD while paused");
        meeting.feed("you", "You", &phrase(0.6, 0.5), 2_000, &mut clips, &mut |event| events.push(event)).unwrap();
        assert!(events.is_empty(), "nothing heard while paused: {events:?}");
        meeting.set_paused(false);
        assert!(meeting.needs("you"), "a fresh stream after resuming");
        meeting.add("you", "You", EnergyVad::default());
        meeting.feed("you", "You", &phrase(0.6, 0.5), 4_000, &mut clips, &mut |event| events.push(event)).unwrap();
        assert_eq!(segments(&events).len(), 1);
    }

    #[test]
    fn a_channel_gone_quiet_closes_its_open_segment() {
        let mut meeting = Meeting::new("m1", "en", "", false);
        let mut clips = Clips::default();
        let mut events = Vec::new();
        meeting.add("app", "App", EnergyVad::default());
        meeting.feed("app", "App", &[tone(0.2, 0.0), tone(0.8, 0.4)].concat(), 10_000, &mut clips, &mut |event| events.push(event)).unwrap();
        meeting.idle(10_500, &mut clips, &mut |event| events.push(event)).unwrap();
        assert!(events.is_empty(), "not idle yet");
        meeting.idle(11_000, &mut clips, &mut |event| events.push(event)).unwrap();
        let got = segments(&events);
        assert_eq!(got.len(), 1);
        assert!((got[0].4 - 10_000).abs() <= 50, "it ends where the audio stopped: {}", got[0].4);
    }

    /// meeting.json CHANNELS: a segment is dated by its chunks' CAPTURE
    /// time; a backlog read 30 s late neither shifts it nor reads as
    /// silence to the idle rule.
    #[test]
    fn segment_times_follow_the_capture_time_when_fed_late() {
        let mut meeting = Meeting::new("m1", "en", "", false);
        let mut clips = Clips::default();
        let mut events = Vec::new();
        meeting.add("app", "App", EnergyVad::default());
        let captured0 = 1_000_000;
        let worker_now = captured0 + 30_000;
        meeting.idle(worker_now, &mut clips, &mut |event| events.push(event)).unwrap();
        // 0.2 s quiet, 0.8 s speech, 0.5 s quiet, in 20 ms chunks each
        // stamped with its own capture time; the worker reads them all now.
        let audio = phrase(0.8, 0.4);
        let chunk = RATE / 50;
        let mut at = 0;
        while at < audio.len() {
            let end = (at + chunk).min(audio.len());
            let captured = captured0 + (end * 1000 / RATE) as i64;
            meeting.feed("app", "App", &audio[at..end], captured, &mut clips, &mut |event| events.push(event)).unwrap();
            if at == 6 * chunk {
                // Mid-phrase, the worker's clock ticks on: no idle close.
                meeting.idle(worker_now + 100, &mut clips, &mut |event| events.push(event)).unwrap();
                assert!(segments(&events).is_empty(), "a backlog is not silence");
            }
            at = end;
        }
        let got = segments(&events);
        assert_eq!(got.len(), 1, "one segment");
        let (start, end) = (got[0].3, got[0].4);
        // The speech is captured over [+200, +1000] ms (the segmenter's
        // padding may widen it a little); the read time is 30 s later.
        assert!((captured0..=captured0 + 250).contains(&start), "starts at the capture time: {start}");
        assert!((captured0 + 900..=captured0 + 1_500).contains(&end), "ends at the capture time: {end}");
    }

    #[test]
    fn a_draining_stop_closes_every_open_segment() {
        let mut meeting = Meeting::new("m1", "en", "", false);
        let mut clips = Clips::default();
        let mut events = Vec::new();
        for (channel, amplitude) in [("you", 0.8), ("zoom", 0.3)] {
            meeting.add(channel, channel, EnergyVad::default());
            let audio = [tone(0.2, 0.0), tone(0.8, amplitude)].concat();
            meeting.feed(channel, channel, &audio, 1_000, &mut clips, &mut |event| events.push(event)).unwrap();
        }
        assert!(events.is_empty(), "both still talking");
        meeting.finish(&mut clips, &mut |event| events.push(event)).unwrap();
        let got = segments(&events);
        assert_eq!(got.len(), 2, "{got:?}");
        assert!(got.iter().any(|segment| segment.0 == "you" && segment.2.ends_with("at 0.8")));
        assert!(got.iter().any(|segment| segment.0 == "zoom" && segment.2.ends_with("at 0.3")));
    }

    #[test]
    fn a_long_meeting_holds_bounded_audio() {
        let mut meeting = Meeting::new("m1", "en", "", false);
        let mut clips = Clips::default();
        meeting.add("you", "You", EnergyVad::default());
        for second in 0..30 {
            let audio = if second % 3 == 0 { phrase(0.5, 0.5) } else { tone(1.0, 0.0) };
            meeting.feed("you", "You", &audio, second * 1000, &mut clips, &mut |_| {}).unwrap();
        }
        assert!(meeting.channels["you"].audio.len() <= 3 * RATE, "held {}", meeting.channels["you"].audio.len());
    }

    #[test]
    fn the_debug_ring_keeps_the_last_minute_and_writes_wavs() {
        let dir = std::env::temp_dir().join(format!("meeting-ring-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut meeting = Meeting::new("m1", "en", "", true);
        let mut clips = Clips::default();
        meeting.add("you", "You", EnergyVad::default());
        meeting.add("a/b", "B", EnergyVad::default());
        for _ in 0..70 {
            meeting.feed("you", "You", &tone(1.0, 0.0), 0, &mut clips, &mut |_| {}).unwrap();
        }
        meeting.feed("a/b", "B", &tone(0.5, 0.0), 0, &mut clips, &mut |_| {}).unwrap();
        let written = meeting.stop(Some(&dir));
        assert_eq!(written, [dir.join("a_b.wav"), dir.join("you.wav")]);
        assert_eq!(std::fs::metadata(dir.join("you.wav")).unwrap().len(), 44 + (RING_SECONDS * RATE * 2) as u64);
        assert_eq!(std::fs::metadata(dir.join("a_b.wav")).unwrap().len(), 44 + (RATE / 2 * 2) as u64);
        let _ = std::fs::remove_dir_all(&dir);

        let mut off = Meeting::new("m2", "en", "", false);
        off.add("you", "You", EnergyVad::default());
        off.feed("you", "You", &tone(1.0, 0.1), 0, &mut clips, &mut |_| {}).unwrap();
        assert!(off.stop(Some(&dir)).is_empty(), "no ring: nothing is ever written");
        assert!(!dir.exists());
    }
}
