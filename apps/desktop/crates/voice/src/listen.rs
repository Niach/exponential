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
#[derive(Debug, Clone, Default)]
pub struct Segmenter {
    /// Frames scored so far.
    frame: usize,
    /// The first speech frame of the open segment.
    speech_start: Option<usize>,
    /// The first frame of the silence run inside the open segment.
    silence_start: Option<usize>,
    /// Where the previous segment ended (the next one's padding stops there).
    floor: usize,
}

impl Segmenter {
    /// Score the next frame; a segment that just finished comes back.
    pub fn push(&mut self, probability: f32) -> Option<Segment> {
        let index = self.frame;
        self.frame += 1;
        let Some(start) = self.speech_start else {
            if probability >= THRESHOLD {
                self.speech_start = Some(index);
            }
            return None;
        };
        if probability >= THRESHOLD {
            self.silence_start = None;
        } else if probability < NEG_THRESHOLD && self.silence_start.is_none() {
            self.silence_start = Some(index);
        }
        if let Some(silence) = self.silence_start {
            if self.frame - silence >= frames(MIN_SILENCE_MS) {
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
