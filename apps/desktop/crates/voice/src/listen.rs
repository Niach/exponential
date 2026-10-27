//! NEW-78: one utterance, from microphone samples to a transcript.
//!
//! Pure and engine-free: [`Utterance`] takes mono 16 kHz samples and two
//! engines behind traits ([`Vad`], [`Transcriber`]), so the same pipeline
//! runs on the microphone in the worker, on a WAV file in the tests, and on
//! fakes in CI. What it does with every 32 ms frame:
//!
//! - the [`Meter`] turns it into a [`talk::Event::Level`] (level + 8 bands,
//!   smoothed for drawing);
//! - the VAD scores it, and the [`Segmenter`] turns the scores into speech
//!   segments (Silero's rules: start at `threshold`, end after
//!   `min_silence` below `threshold - 0.15`, drop anything shorter than
//!   `min_speech`);
//! - every finished segment is transcribed at once, so release only waits
//!   for the last one; on the GPU the open segment is re-transcribed every
//!   [`PARTIAL_EVERY`] for [`talk::Event::Partial`].
//!
//! The constants and the pure functions are fixture-locked by
//! `packages/domain-contract/fixtures/talk-listen.json`.
//!
//! NEW-55: a [`Call`] runs the same pipeline continuously
//! (`voice-call.json`): every closed segment is answered at once as
//! [`talk::Event::Said`], the open one alone is the Partial, speech of
//! [`talk::BARGE_IN_MS`] is [`talk::Event::SpeechStarted`], segments close
//! after [`talk::CALL_MIN_SILENCE_MS`], and the audio before the next
//! possible segment is dropped so an hour-long call stays bounded.

use talk::{Event, ListenFailure, LEVEL_BANDS};

/// The sample rate everything here runs at.
pub const RATE: usize = 16_000;
/// Silero's frame: 512 samples = 32 ms. One Level per frame (~31 Hz).
pub const FRAME: usize = 512;
/// The level's floor and ceiling in dBFS (0 below, 1 above).
pub const FLOOR_DB: f32 = -60.0;
pub const CEILING_DB: f32 = -10.0;
/// How far one frame moves the drawn level towards the measured one:
/// fast up, slow down (a wave that jumps with the voice and settles).
pub const ATTACK: f32 = 0.6;
pub const RELEASE: f32 = 0.2;
/// The bands' edges in Hz, log-spaced from 80 Hz to 8 kHz.
pub const BAND_EDGES_HZ: [f32; LEVEL_BANDS + 1] = [80.0, 142.3, 253.0, 449.9, 800.0, 1422.6, 2529.8, 4498.6, 8000.0];
/// Segmentation (Silero's defaults, `min_silence` per NEW-54's plan).
pub const THRESHOLD: f32 = 0.5;
pub const NEG_THRESHOLD: f32 = THRESHOLD - 0.15;
pub const MIN_SPEECH_MS: usize = 250;
pub const MIN_SILENCE_MS: usize = 300;
/// Audio kept before a segment's first speech frame and after its last.
pub const PAD_MS: usize = 64;
/// A segment longer than this is cut (Parakeet decodes it in one pass).
pub const MAX_SEGMENT_MS: usize = 20_000;
/// How often the open segment is re-transcribed for a Partial.
pub const PARTIAL_EVERY: usize = RATE * 300 / 1000;

const fn frames(ms: usize) -> usize {
    (ms * RATE / 1000).div_ceil(FRAME)
}

/// A frame's level in 0..=1 from its RMS amplitude.
pub fn level_of_rms(rms: f32) -> f32 {
    let db = 20.0 * rms.max(1e-9).log10();
    ((db - FLOOR_DB) / (CEILING_DB - FLOOR_DB)).clamp(0.0, 1.0)
}

/// One smoothing step from the drawn value `previous` towards `measured`.
pub fn smooth(previous: f32, measured: f32) -> f32 {
    let rate = if measured > previous { ATTACK } else { RELEASE };
    previous + rate * (measured - previous)
}

/// Collapse whitespace, tighten the space before punctuation, and read a
/// transcript with no letter or digit (`"."`, `"-"`) as nothing.
pub fn normalize_transcript(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for word in text.split_whitespace() {
        let glued = word.chars().next().is_some_and(|ch| matches!(ch, '.' | ',' | '!' | '?' | ';' | ':' | '%' | ')'));
        if !out.is_empty() && !glued {
            out.push(' ');
        }
        out.push_str(word);
    }
    if out.chars().any(char::is_alphanumeric) { out } else { String::new() }
}

/// Join the segments' transcripts into one utterance.
pub fn join_transcripts<'a>(parts: impl IntoIterator<Item = &'a str>) -> String {
    let joined: Vec<&str> = parts.into_iter().filter(|part| !part.trim().is_empty()).collect();
    normalize_transcript(&joined.join(" "))
}

/// The drawn level and bands, one [`Meter::frame`] per [`FRAME`].
#[derive(Debug, Clone, Default)]
pub struct Meter {
    level: f32,
    bands: [f32; LEVEL_BANDS],
}

impl Meter {
    /// Measure one frame of [`FRAME`] samples and return the smoothed
    /// `(level, bands)`.
    pub fn frame(&mut self, samples: &[f32]) -> (f32, [f32; LEVEL_BANDS]) {
        let (level, bands) = measure(samples);
        self.level = smooth(self.level, level);
        for (drawn, measured) in self.bands.iter_mut().zip(bands) {
            *drawn = smooth(*drawn, measured);
        }
        (round3(self.level), self.bands.map(round3))
    }
}

fn round3(value: f32) -> f32 {
    (value * 1000.0).round() / 1000.0
}

/// The unsmoothed level and bands of one frame: the RMS of the frame, and
/// per band the RMS of its share of a Hann-windowed spectrum (Parseval, so a
/// band holding all of the energy reads like the level).
pub fn measure(samples: &[f32]) -> (f32, [f32; LEVEL_BANDS]) {
    let n = samples.len();
    if n == 0 {
        return (0.0, [0.0; LEVEL_BANDS]);
    }
    let rms = (samples.iter().map(|sample| sample * sample).sum::<f32>() / n as f32).sqrt();
    let size = n.next_power_of_two();
    let mut re = vec![0f32; size];
    let mut im = vec![0f32; size];
    let mut window_power = 0f32;
    for (index, sample) in samples.iter().enumerate() {
        let window = 0.5 - 0.5 * (std::f32::consts::TAU * index as f32 / n as f32).cos();
        window_power += window * window;
        re[index] = sample * window;
    }
    fft(&mut re, &mut im);
    let bin_hz = RATE as f32 / size as f32;
    let mut bands = [0f32; LEVEL_BANDS];
    for (band, value) in bands.iter_mut().enumerate() {
        let (low, high) = (BAND_EDGES_HZ[band], BAND_EDGES_HZ[band + 1]);
        let power: f32 = (1..size / 2)
            .filter(|bin| {
                let hz = *bin as f32 * bin_hz;
                hz >= low && hz < high
            })
            .map(|bin| re[bin] * re[bin] + im[bin] * im[bin])
            .sum();
        // One-sided spectrum: each bin stands for its mirror too.
        *value = level_of_rms((2.0 * power / (size as f32 * window_power)).sqrt());
    }
    (level_of_rms(rms), bands)
}

/// In-place radix-2 FFT (`re.len()` a power of two).
fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let angle = -std::f32::consts::TAU / len as f32;
        for start in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let (sin, cos) = (angle * k as f32).sin_cos();
                let (a, b) = (start + k, start + k + len / 2);
                let (tr, ti) = (re[b] * cos - im[b] * sin, re[b] * sin + im[b] * cos);
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
            }
        }
        len <<= 1;
    }
}

/// A finished speech segment, in frames: `[start, end)`, padding included.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segment {
    pub start: usize,
    pub end: usize,
}

/// Speech probabilities, one per frame, into segments.
#[derive(Debug, Clone)]
pub struct Segmenter {
    /// NEW-55: the silence (in frames) that closes a segment:
    /// [`MIN_SILENCE_MS`] for push to talk, [`talk::CALL_MIN_SILENCE_MS`] in
    /// a call.
    min_silence: usize,
    /// NEW-55: the speech frames (at or above [`THRESHOLD`]) of the open
    /// segment.
    voiced: usize,
    /// Frames scored so far.
    frame: usize,
    /// The first speech frame of the open segment.
    speech_start: Option<usize>,
    /// The first frame of the silence run inside the open segment.
    silence_start: Option<usize>,
    /// Where the previous segment ended (the next one's padding stops there).
    floor: usize,
}

impl Default for Segmenter {
    /// Push to talk's: [`MIN_SILENCE_MS`].
    fn default() -> Self {
        Segmenter::new(MIN_SILENCE_MS)
    }
}

impl Segmenter {
    /// NEW-55: a segmenter closing segments after `min_silence_ms`.
    pub fn new(min_silence_ms: usize) -> Self {
        Segmenter { min_silence: frames(min_silence_ms), voiced: 0, frame: 0, speech_start: None, silence_start: None, floor: 0 }
    }

    /// Score the next frame; a segment that just finished comes back.
    pub fn push(&mut self, probability: f32) -> Option<Segment> {
        let index = self.frame;
        self.frame += 1;
        let Some(start) = self.speech_start else {
            if probability >= THRESHOLD {
                self.speech_start = Some(index);
                self.voiced = 1;
            }
            return None;
        };
        if probability >= THRESHOLD {
            self.silence_start = None;
            self.voiced += 1;
        } else if probability < NEG_THRESHOLD && self.silence_start.is_none() {
            self.silence_start = Some(index);
        }
        if let Some(silence) = self.silence_start {
            if self.frame - silence >= self.min_silence {
                return self.close(start, silence);
            }
        }
        if self.frame - start >= frames(MAX_SEGMENT_MS) {
            let cut = self.close(start, self.frame);
            // Speech goes on: the next segment starts right here.
            self.speech_start = Some(self.frame);
            return cut;
        }
        None
    }

    /// NEW-55: the speech in the open segment, in frames (0 = none open).
    pub fn open_voiced(&self) -> usize {
        if self.speech_start.is_some() { self.voiced } else { 0 }
    }

    /// NEW-55: the first frame any segment still to come can reach back to
    /// (the open one's padded start, else the padding before the next frame,
    /// never before the previous segment's end). Audio before it is never
    /// read again.
    pub fn keep_from(&self) -> usize {
        self.open_start().unwrap_or_else(|| self.frame.saturating_sub(frames(PAD_MS)).max(self.floor))
    }

    /// No more frames (the key was released): close the open segment.
    pub fn finish(&mut self) -> Option<Segment> {
        let start = self.speech_start?;
        let speech_end = self.silence_start.unwrap_or(self.frame);
        self.close(start, speech_end)
    }

    /// The open segment's padded start, while one is open.
    pub fn open_start(&self) -> Option<usize> {
        self.speech_start.map(|start| start.saturating_sub(frames(PAD_MS)).max(self.floor))
    }

    fn close(&mut self, start: usize, speech_end: usize) -> Option<Segment> {
        let padded_start = start.saturating_sub(frames(PAD_MS)).max(self.floor);
        let end = (speech_end + frames(PAD_MS)).min(self.frame);
        self.speech_start = None;
        self.silence_start = None;
        self.voiced = 0;
        if speech_end - start < frames(MIN_SPEECH_MS) {
            return None;
        }
        self.floor = end;
        Some(Segment { start: padded_start, end })
    }
}

/// The voice activity detector: speech probability per [`FRAME`].
pub trait Vad {
    /// Score `samples` (any length, continuing the stream): one probability
    /// per whole frame they complete.
    fn push(&mut self, samples: &[f32]) -> Result<Vec<f32>, String>;
}

/// Speech to text.
pub trait Transcriber {
    /// Transcribe mono 16 kHz `pcm` spoken in `language` (en | de).
    fn transcribe(&mut self, pcm: &[f32], language: &str) -> Result<String, String>;
    /// Whether it is fast enough to re-transcribe while the key is held.
    fn partials(&self) -> bool;
}

/// One Listen, from its first sample to Heard or Empty.
pub struct Utterance {
    id: String,
    language: String,
    audio: Vec<f32>,
    /// Samples already metered (a whole number of frames).
    metered: usize,
    meter: Meter,
    segmenter: Segmenter,
    /// The finished segments' transcripts, in order.
    heard: Vec<String>,
    /// `audio.len()` at the last partial decode.
    partial_at: usize,
    last_partial: String,
}

impl Utterance {
    pub fn new(id: impl Into<String>, language: impl Into<String>) -> Self {
        Utterance {
            id: id.into(),
            language: language.into(),
            audio: Vec::new(),
            metered: 0,
            meter: Meter::default(),
            segmenter: Segmenter::default(),
            heard: Vec::new(),
            partial_at: 0,
            last_partial: String::new(),
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    /// Seconds of audio heard so far.
    pub fn seconds(&self) -> f32 {
        self.audio.len() as f32 / RATE as f32
    }

    /// Take the next captured samples; Level, Partial events go to `emit`.
    pub fn feed(
        &mut self,
        samples: &[f32],
        vad: &mut dyn Vad,
        transcriber: &mut dyn Transcriber,
        emit: &mut dyn FnMut(Event),
    ) -> Result<(), (ListenFailure, String)> {
        self.audio.extend_from_slice(samples);
        while self.metered + FRAME <= self.audio.len() {
            let (level, bands) = self.meter.frame(&self.audio[self.metered..self.metered + FRAME]);
            self.metered += FRAME;
            emit(Event::Level { id: self.id.clone(), level, bands });
        }
        let probabilities = vad.push(samples).map_err(|err| (ListenFailure::Failed, err))?;
        for probability in probabilities {
            if let Some(segment) = self.segmenter.push(probability) {
                self.transcribe(segment, transcriber)?;
            }
        }
        if transcriber.partials() && self.audio.len() - self.partial_at >= PARTIAL_EVERY {
            if let Some(start) = self.segmenter.open_start() {
                self.partial_at = self.audio.len();
                let open = &self.audio[(start * FRAME).min(self.audio.len())..];
                let current = transcriber.transcribe(open, &self.language).map_err(|err| (ListenFailure::Failed, err))?;
                let text = join_transcripts(self.heard.iter().map(String::as_str).chain([current.as_str()]));
                if !text.is_empty() && text != self.last_partial {
                    self.last_partial = text.clone();
                    emit(Event::Partial { id: self.id.clone(), text });
                }
            }
        }
        Ok(())
    }

    /// The key was released: transcribe what is left and answer Heard or
    /// Empty.
    pub fn finish(mut self, transcriber: &mut dyn Transcriber, emit: &mut dyn FnMut(Event)) -> Result<(), (ListenFailure, String)> {
        if let Some(segment) = self.segmenter.finish() {
            self.transcribe(segment, transcriber)?;
        }
        let text = join_transcripts(self.heard.iter().map(String::as_str));
        emit(if text.is_empty() { Event::Empty { id: self.id } } else { Event::Heard { id: self.id, text } });
        Ok(())
    }

    fn transcribe(&mut self, segment: Segment, transcriber: &mut dyn Transcriber) -> Result<(), (ListenFailure, String)> {
        let start = (segment.start * FRAME).min(self.audio.len());
        let end = (segment.end * FRAME).min(self.audio.len());
        let text = transcriber.transcribe(&self.audio[start..end], &self.language).map_err(|err| (ListenFailure::Failed, err))?;
        let text = normalize_transcript(&text);
        if !text.is_empty() {
            self.heard.push(text);
        }
        Ok(())
    }
}

/// NEW-55: a call, from its first sample to Hangup: Level per frame, Said
/// per closed segment, Partial for the open one (GPU only), SpeechStarted
/// once per utterance at [`talk::BARGE_IN_MS`] of speech. Never Heard or
/// Empty.
pub struct Call {
    id: String,
    language: String,
    /// The samples from absolute sample `base` on (older ones are dropped).
    audio: Vec<f32>,
    base: usize,
    /// Samples already metered (absolute, a whole number of frames).
    metered: usize,
    meter: Meter,
    segmenter: Segmenter,
    /// SpeechStarted went out for the open utterance.
    announced: bool,
    /// The absolute sample count at the last partial decode.
    partial_at: usize,
    last_partial: String,
}

/// NEW-55: dropped audio is drained in steps of at least this many samples
/// (one memmove a second, not one per chunk).
const DRAIN_STEP: usize = RATE;

impl Call {
    pub fn new(id: impl Into<String>, language: impl Into<String>) -> Self {
        Call {
            id: id.into(),
            language: language.into(),
            audio: Vec::new(),
            base: 0,
            metered: 0,
            meter: Meter::default(),
            segmenter: Segmenter::new(talk::CALL_MIN_SILENCE_MS),
            announced: false,
            partial_at: 0,
            last_partial: String::new(),
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    /// The samples held now (bounded: see [`Segmenter::keep_from`]).
    pub fn buffered(&self) -> usize {
        self.audio.len()
    }

    /// Muted: the open utterance and everything held are dropped; the next
    /// audio (after unmute, with a fresh VAD) starts clean.
    pub fn reset(&mut self) {
        *self = Call { meter: std::mem::take(&mut self.meter), ..Call::new(std::mem::take(&mut self.id), std::mem::take(&mut self.language)) };
    }

    /// Take the next samples; Level, Partial, SpeechStarted and Said go to
    /// `emit`.
    pub fn feed(
        &mut self,
        samples: &[f32],
        vad: &mut dyn Vad,
        transcriber: &mut dyn Transcriber,
        emit: &mut dyn FnMut(Event),
    ) -> Result<(), (ListenFailure, String)> {
        self.audio.extend_from_slice(samples);
        let end = self.base + self.audio.len();
        while self.metered + FRAME <= end {
            let at = self.metered - self.base;
            let (level, bands) = self.meter.frame(&self.audio[at..at + FRAME]);
            self.metered += FRAME;
            emit(Event::Level { id: self.id.clone(), level, bands });
        }
        let probabilities = vad.push(samples).map_err(|err| (ListenFailure::Failed, err))?;
        for probability in probabilities {
            if let Some(segment) = self.segmenter.push(probability) {
                let text = normalize_transcript(&self.transcribe(segment.start, segment.end, transcriber)?);
                self.last_partial.clear();
                if !text.is_empty() {
                    emit(Event::Said { id: self.id.clone(), text });
                }
            }
            if self.segmenter.open_start().is_none() {
                self.announced = false;
            } else if !self.announced && self.segmenter.open_voiced() >= frames(talk::BARGE_IN_MS) {
                self.announced = true;
                emit(Event::SpeechStarted { id: self.id.clone() });
            }
        }
        if transcriber.partials() && end - self.partial_at >= PARTIAL_EVERY {
            if let Some(start) = self.segmenter.open_start() {
                self.partial_at = end;
                let text = normalize_transcript(&self.transcribe(start, usize::MAX, transcriber)?);
                if !text.is_empty() && text != self.last_partial {
                    self.last_partial = text.clone();
                    emit(Event::Partial { id: self.id.clone(), text });
                }
            }
        }
        // Bounded: nothing before the next possible segment (nor anything
        // not metered yet) is ever read again.
        let keep = (self.segmenter.keep_from() * FRAME).min(self.metered);
        if keep >= self.base + DRAIN_STEP {
            self.audio.drain(..keep - self.base);
            self.base = keep;
        }
        Ok(())
    }

    /// Transcribe frames `[start, end)` (`end` past the audio = all of it).
    fn transcribe(&self, start: usize, end: usize, transcriber: &mut dyn Transcriber) -> Result<String, (ListenFailure, String)> {
        let clip = |frame: usize| frame.saturating_mul(FRAME).saturating_sub(self.base).min(self.audio.len());
        transcriber.transcribe(&self.audio[clip(start)..clip(end)], &self.language).map_err(|err| (ListenFailure::Failed, err))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A VAD on loudness alone: speech = a frame above -40 dBFS.
    struct EnergyVad(Vec<f32>);

    impl Vad for EnergyVad {
        fn push(&mut self, samples: &[f32]) -> Result<Vec<f32>, String> {
            self.0.extend_from_slice(samples);
            let whole = self.0.len() / FRAME * FRAME;
            let out = self.0[..whole].chunks(FRAME).map(|frame| if measure(frame).0 > 0.4 { 0.9 } else { 0.05 }).collect();
            self.0.drain(..whole);
            Ok(out)
        }
    }

    /// Names each clip by its length, so the tests see what was cut.
    struct Lengths {
        partials: bool,
        clips: Vec<usize>,
    }

    impl Transcriber for Lengths {
        fn transcribe(&mut self, pcm: &[f32], _language: &str) -> Result<String, String> {
            self.clips.push(pcm.len());
            Ok(format!(" clip{} ", self.clips.len()))
        }
        fn partials(&self) -> bool {
            self.partials
        }
    }

    fn tone(seconds: f32, amplitude: f32) -> Vec<f32> {
        (0..(seconds * RATE as f32) as usize)
            .map(|index| amplitude * (std::f32::consts::TAU * 440.0 * index as f32 / RATE as f32).sin())
            .collect()
    }

    fn run(audio: &[f32], partials: bool, chunk: usize) -> (Vec<Event>, Vec<usize>) {
        let mut utterance = Utterance::new("u1", "en");
        let mut vad = EnergyVad(Vec::new());
        let mut transcriber = Lengths { partials, clips: Vec::new() };
        let mut events = Vec::new();
        for samples in audio.chunks(chunk) {
            utterance.feed(samples, &mut vad, &mut transcriber, &mut |event| events.push(event)).unwrap();
        }
        utterance.finish(&mut transcriber, &mut |event| events.push(event)).unwrap();
        (events, transcriber.clips)
    }

    #[test]
    fn two_phrases_are_two_segments_and_one_heard() {
        let audio = [tone(0.5, 0.0), tone(1.0, 0.3), tone(0.6, 0.0), tone(0.8, 0.3), tone(0.2, 0.0)].concat();
        // Any chunking: the capture hands over whatever it has.
        for chunk in [160, 512, 1000, 4800] {
            let (events, clips) = run(&audio, false, chunk);
            assert_eq!(clips.len(), 2, "chunk {chunk}");
            assert!(clips[0] >= RATE && clips[0] <= RATE + 2 * frames(PAD_MS) * FRAME + FRAME, "{clips:?}");
            let levels = events.iter().filter(|event| matches!(event, Event::Level { .. })).count();
            assert_eq!(levels, audio.len() / FRAME);
            assert_eq!(events.last(), Some(&Event::Heard { id: "u1".into(), text: "clip1 clip2".into() }));
            assert!(!events.iter().any(|event| matches!(event, Event::Partial { .. })));
        }
    }

    #[test]
    fn a_spoken_wav_runs_through_like_the_microphone() {
        // Our own Qwen3-TTS clip (assets/README.md): three sentences.
        let audio = crate::capture::read_wav(crate::builtin::for_language("en").wav).unwrap();
        let (events, clips) = run(&audio, false, RATE / 50);
        assert!(!clips.is_empty() && clips.len() <= 6, "{clips:?}");
        let heard: usize = clips.iter().sum();
        assert!(heard > audio.len() / 2 && heard <= audio.len(), "speech is most of the clip: {heard} of {}", audio.len());
        assert!(matches!(events.last(), Some(Event::Heard { .. })));
        let levels: Vec<f32> = events.iter().filter_map(|event| if let Event::Level { level, .. } = event { Some(*level) } else { None }).collect();
        assert_eq!(levels.len(), audio.len() / FRAME);
        assert!(levels.iter().all(|level| (0.0..=1.0).contains(level)));
        assert!(levels.iter().any(|level| *level > 0.5), "speech moves the wave");
    }

    #[test]
    fn a_tap_without_speech_is_empty() {
        let (events, clips) = run(&tone(0.4, 0.001), false, 512);
        assert!(clips.is_empty());
        assert_eq!(events.last(), Some(&Event::Empty { id: "u1".into() }));
    }

    #[test]
    fn releasing_mid_phrase_transcribes_the_open_segment() {
        let (events, clips) = run(&[tone(0.3, 0.0), tone(0.9, 0.3)].concat(), false, 512);
        assert_eq!(clips.len(), 1);
        assert_eq!(events.last(), Some(&Event::Heard { id: "u1".into(), text: "clip1".into() }));
    }

    #[test]
    fn partials_follow_the_open_segment_on_the_gpu() {
        let (events, _) = run(&[tone(0.2, 0.0), tone(1.5, 0.3), tone(0.5, 0.0)].concat(), true, 1600);
        let partials: Vec<&Event> = events.iter().filter(|event| matches!(event, Event::Partial { .. })).collect();
        assert!(partials.len() >= 3, "{partials:?}");
        assert!(matches!(events.last(), Some(Event::Heard { .. })));
    }

    fn call(audio: &[f32], partials: bool, chunk: usize) -> (Vec<Event>, Vec<usize>) {
        let mut call = Call::new("c1", "en");
        let mut vad = EnergyVad(Vec::new());
        let mut transcriber = Lengths { partials, clips: Vec::new() };
        let mut events = Vec::new();
        for samples in audio.chunks(chunk) {
            call.feed(samples, &mut vad, &mut transcriber, &mut |event| events.push(event)).unwrap();
        }
        (events, transcriber.clips)
    }

    fn said(events: &[Event]) -> Vec<&str> {
        events.iter().filter_map(|event| if let Event::Said { text, .. } = event { Some(text.as_str()) } else { None }).collect()
    }

    /// NEW-55: a call answers each utterance as it ends, never Heard/Empty.
    #[test]
    fn a_call_says_each_utterance_as_it_ends() {
        let audio = [tone(0.5, 0.0), tone(1.0, 0.3), tone(0.6, 0.0), tone(0.8, 0.3), tone(0.5, 0.0)].concat();
        for chunk in [320, 512, 1000, 4800] {
            let (events, clips) = call(&audio, false, chunk);
            assert_eq!(clips.len(), 2, "chunk {chunk}");
            assert_eq!(said(&events), ["clip1", "clip2"], "chunk {chunk}");
            assert!(!events.iter().any(|event| matches!(event, Event::Heard { .. } | Event::Empty { .. } | Event::Partial { .. })));
            let levels = events.iter().filter(|event| matches!(event, Event::Level { .. })).count();
            assert_eq!(levels, audio.len() / FRAME);
        }
        // The first Said comes in the silence after the first phrase, before
        // the second one starts.
        let (events, _) = call(&audio, false, 512);
        let first_said = events.iter().position(|event| matches!(event, Event::Said { .. })).unwrap();
        let levels_before = events[..first_said].iter().filter(|event| matches!(event, Event::Level { .. })).count();
        assert!(levels_before * FRAME < (RATE as f32 * 2.1) as usize, "said after {levels_before} frames");
    }

    /// NEW-55: 250 ms of silence closes a call's segment (push to talk: 300).
    #[test]
    fn a_call_closes_after_250_ms_of_silence() {
        let mut call = Segmenter::new(talk::CALL_MIN_SILENCE_MS);
        let mut push_to_talk = Segmenter::default();
        let (mut call_closed, mut ptt_closed) = (None, None);
        for frame in 0..60 {
            let probability = if frame < 20 { 0.9 } else { 0.05 };
            if call.push(probability).is_some() {
                call_closed.get_or_insert(frame);
            }
            if push_to_talk.push(probability).is_some() {
                ptt_closed.get_or_insert(frame);
            }
        }
        // Silence starts at frame 20; 250 ms = 8 frames, 300 ms = 10.
        assert_eq!(call_closed, Some(27));
        assert_eq!(ptt_closed, Some(29));
    }

    /// NEW-55: SpeechStarted once per utterance, at 250 ms of speech.
    #[test]
    fn speech_started_fires_once_at_250_ms_of_speech() {
        let audio = [tone(0.3, 0.0), tone(1.0, 0.3), tone(0.5, 0.0), tone(0.1, 0.3), tone(0.5, 0.0), tone(0.6, 0.3), tone(0.5, 0.0)].concat();
        let (events, _) = call(&audio, false, 512);
        let started: Vec<usize> = events
            .iter()
            .enumerate()
            .filter(|(_, event)| matches!(event, Event::SpeechStarted { .. }))
            .map(|(index, _)| events[..index].iter().filter(|event| matches!(event, Event::Level { .. })).count())
            .collect();
        // Two utterances (the 100 ms blip never reaches 250 ms).
        assert_eq!(started.len(), 2, "{started:?}");
        // Speech starts at frame 9 (0.3 s); 250 ms = 8 speech frames.
        let first = started[0] as f32 * FRAME as f32 / RATE as f32;
        assert!((0.53..=0.6).contains(&first), "SpeechStarted at {first} s");
        assert_eq!(said(&events).len(), 2);
    }

    /// NEW-55: the open utterance alone is the Partial (the GPU's).
    #[test]
    fn a_calls_partial_is_the_open_utterance_only() {
        let audio = [tone(0.2, 0.0), tone(1.0, 0.3), tone(0.5, 0.0), tone(1.0, 0.3), tone(0.5, 0.0)].concat();
        let (events, clips) = call(&audio, true, 1600);
        let said_at = events.iter().position(|event| matches!(event, Event::Said { .. })).unwrap();
        let partials_after: Vec<&Event> = events[said_at..].iter().filter(|event| matches!(event, Event::Partial { .. })).collect();
        assert!(!partials_after.is_empty());
        // Lengths names every decode; a Partial never joins an earlier Said.
        for event in partials_after {
            let Event::Partial { text, .. } = event else { unreachable!() };
            assert!(!text.contains(' '), "only the open segment: {text}");
        }
        assert!(clips.len() > 2);
    }

    /// NEW-55: a long call stays bounded (speech and silence alike): the
    /// buffer peaks within the first phrase and never grows past it, so an
    /// hour holds what ten minutes hold (ten minutes keep the test fast).
    #[test]
    fn a_long_call_holds_a_bounded_buffer() {
        let mut call = Call::new("c1", "en");
        let mut vad = EnergyVad(Vec::new());
        let mut transcriber = Lengths { partials: false, clips: Vec::new() };
        let phrase = [tone(4.0, 0.3), tone(1.0, 0.0)].concat();
        let mut most = 0;
        let mut first = 0;
        let mut said = 0;
        // 120 x 5 s = ten minutes.
        for round in 0..120 {
            for samples in phrase.chunks(320) {
                call.feed(samples, &mut vad, &mut transcriber, &mut |event| said += usize::from(matches!(event, Event::Said { .. })))
                    .unwrap();
                most = most.max(call.buffered());
            }
            if round == 1 {
                first = most;
            }
        }
        assert_eq!(said, 120);
        assert!(most <= 6 * RATE, "at most the open phrase plus a second: {most}");
        assert_eq!(most, first, "no growth after the first phrases");
        // One long monologue is cut at MAX_SEGMENT_MS and stays bounded too.
        let mut call = Call::new("c2", "en");
        let speech = tone(120.0, 0.3);
        let mut most = 0;
        for samples in speech.chunks(1600) {
            call.feed(samples, &mut vad, &mut transcriber, &mut |_| {}).unwrap();
            most = most.max(call.buffered());
        }
        assert!(most <= (MAX_SEGMENT_MS / 1000 + 2) * RATE, "{most}");
    }

    /// NEW-55: muting drops the open utterance.
    #[test]
    fn a_reset_drops_the_open_utterance() {
        let mut call = Call::new("c1", "en");
        let mut vad = EnergyVad(Vec::new());
        let mut transcriber = Lengths { partials: false, clips: Vec::new() };
        let mut events = Vec::new();
        call.feed(&tone(1.0, 0.3), &mut vad, &mut transcriber, &mut |event| events.push(event)).unwrap();
        call.reset();
        assert_eq!(call.buffered(), 0);
        let mut vad = EnergyVad(Vec::new());
        call.feed(&tone(1.0, 0.0), &mut vad, &mut transcriber, &mut |event| events.push(event)).unwrap();
        assert!(said(&events).is_empty(), "the muted phrase is never said");
        assert_eq!(call.id(), "c1");
    }



    const FIXTURE: &str = include_str!("../../../../../packages/domain-contract/fixtures/talk-listen.json");

    fn fixture() -> serde_json::Value {
        serde_json::from_str(FIXTURE).unwrap()
    }

    fn f(value: &serde_json::Value) -> f32 {
        value.as_f64().unwrap() as f32
    }

    #[test]
    fn the_constants_are_the_fixture() {
        let fixture = fixture();
        let c = &fixture["constants"];
        assert_eq!(c["rate"], RATE);
        assert_eq!(c["frame"], FRAME);
        assert_eq!(c["frame"], crispasr::VAD_FRAME);
        assert_eq!(f(&c["floorDb"]), FLOOR_DB);
        assert_eq!(f(&c["ceilingDb"]), CEILING_DB);
        assert_eq!(f(&c["attack"]), ATTACK);
        assert_eq!(f(&c["release"]), RELEASE);
        let edges: Vec<f32> = c["bandEdgesHz"].as_array().unwrap().iter().map(f).collect();
        assert_eq!(edges, BAND_EDGES_HZ);
        assert_eq!(f(&c["threshold"]), THRESHOLD);
        assert!((f(&c["negThreshold"]) - NEG_THRESHOLD).abs() < 1e-6);
        assert_eq!(c["minSpeechMs"], MIN_SPEECH_MS);
        assert_eq!(c["minSilenceMs"], MIN_SILENCE_MS);
        assert_eq!(c["padMs"], PAD_MS);
        assert_eq!(c["maxSegmentMs"], MAX_SEGMENT_MS);
        assert_eq!(c["partialEveryMs"].as_u64().unwrap() as usize * RATE / 1000, PARTIAL_EVERY);
    }

    #[test]
    fn levels_and_smoothing_follow_the_fixture() {
        let fixture = fixture();
        for case in fixture["levelOfRms"].as_array().unwrap() {
            let level = level_of_rms(f(&case["rms"]));
            assert!((level - f(&case["level"])).abs() < 1e-4, "{case}: {level}");
        }
        for case in fixture["smoothing"].as_array().unwrap() {
            let mut drawn = f(&case["from"]);
            for (measured, expected) in case["measured"].as_array().unwrap().iter().zip(case["drawn"].as_array().unwrap()) {
                drawn = smooth(drawn, f(measured));
                assert!((drawn - f(expected)).abs() < 1e-4, "{}: {drawn}", case["name"]);
            }
        }
    }

    #[test]
    fn transcripts_normalize_like_the_fixture() {
        let fixture = fixture();
        for case in fixture["normalize"].as_array().unwrap() {
            assert_eq!(normalize_transcript(case["in"].as_str().unwrap()), case["out"].as_str().unwrap(), "{case}");
        }
        for case in fixture["join"].as_array().unwrap() {
            let parts: Vec<&str> = case["parts"].as_array().unwrap().iter().map(|part| part.as_str().unwrap()).collect();
            assert_eq!(join_transcripts(parts), case["out"].as_str().unwrap(), "{case}");
        }
    }

    #[test]
    fn segments_follow_the_fixture() {
        let fixture = fixture();
        for case in fixture["segments"].as_array().unwrap() {
            let mut segmenter = Segmenter::default();
            let mut segments = Vec::new();
            for run in case["probabilities"].as_array().unwrap() {
                for _ in 0..run[1].as_u64().unwrap() {
                    segments.extend(segmenter.push(f(&run[0])));
                }
            }
            segments.extend(segmenter.finish());
            let segments: Vec<[usize; 2]> = segments.iter().map(|segment| [segment.start, segment.end]).collect();
            let expected: Vec<[usize; 2]> = serde_json::from_value(case["segments"].clone()).unwrap();
            assert_eq!(segments, expected, "{}", case["name"]);
        }
    }

    #[test]
    fn a_full_scale_sine_reads_full_in_its_band_only() {
        let sine: Vec<f32> = (0..FRAME).map(|index| 0.5 * (std::f32::consts::TAU * 350.0 * index as f32 / RATE as f32).sin()).collect();
        let (level, bands) = measure(&sine);
        assert_eq!(level, 1.0);
        // 350 Hz sits mid-way in the third band (253-450 Hz).
        let loudest = bands.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).unwrap().0;
        assert_eq!(loudest, 2, "{bands:?}");
        assert!(bands[2] > 0.95 && bands[0] < 0.5 && bands[7] < 0.5, "{bands:?}");
    }
}
