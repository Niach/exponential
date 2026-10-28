//! NEW-57: the sample plumbing the Linux and Windows backends share (pulled
//! in with `#[path]` by each, so it compiles only where one of them does):
//! interleaved → mono, any rate → [`SAMPLE_RATE`] (linear, stateful across
//! buffers), and the per-app [`Mixer`] that folds the several streams ONE
//! app plays (a browser's tabs, an app's process tree) into its one channel.
//! Never across apps: every app keeps its own mixer (meeting.json CHANNELS).

#![allow(dead_code)]

use std::collections::HashMap;

use crate::apps::{Chunk, SAMPLE_RATE};

/// Samples per emitted chunk: 100 ms at [`SAMPLE_RATE`].
pub(crate) const CHUNK: usize = (SAMPLE_RATE / 10) as usize;

/// Interleaved frames → mono (the channels' mean).
pub(crate) fn downmix(interleaved: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    let scale = 1.0 / channels as f32;
    interleaved.chunks_exact(channels).map(|frame| frame.iter().sum::<f32>() * scale).collect()
}

/// Linear resampler from one rate to [`SAMPLE_RATE`], continuous across
/// [`Resampler::process`] calls (the previous buffer's last sample is the
/// left neighbour of the next one's first output).
#[derive(Clone, Debug)]
pub(crate) struct Resampler {
    from: u32,
    /// Input samples per output sample.
    step: f64,
    /// The next output's position, relative to the next buffer's first
    /// sample (in (-1, step - 1] between calls; -1 = the previous last one).
    pos: f64,
    prev: f32,
}

impl Resampler {
    pub(crate) fn new(from: u32) -> Self {
        let from = from.max(1);
        Resampler { from, step: from as f64 / SAMPLE_RATE as f64, pos: 0.0, prev: 0.0 }
    }

    pub(crate) fn from_rate(&self) -> u32 {
        self.from
    }

    pub(crate) fn process(&mut self, input: &[f32]) -> Vec<f32> {
        if self.from == SAMPLE_RATE {
            return input.to_vec();
        }
        let n = input.len();
        if n == 0 {
            return Vec::new();
        }
        let prev = self.prev;
        let at = |i: isize| if i < 0 { prev } else { input[i as usize] };
        let mut out = Vec::with_capacity((n as f64 / self.step) as usize + 2);
        let last = (n - 1) as f64;
        let mut pos = self.pos;
        while pos <= last {
            let i = pos.floor();
            let frac = (pos - i) as f32;
            let i = i as isize;
            let a = at(i);
            let sample = if frac == 0.0 { a } else { a + (at(i + 1) - a) * frac };
            out.push(sample);
            pos += self.step;
        }
        self.pos = pos - n as f64;
        self.prev = input[n - 1];
        out
    }
}

/// Mono at any rate (+ its channel count) → mono at [`SAMPLE_RATE`]: one per
/// captured stream, rebuilt when the stream's format changes.
#[derive(Clone, Debug)]
pub(crate) struct Converter {
    channels: usize,
    resampler: Resampler,
}

impl Converter {
    pub(crate) fn new(rate: u32, channels: usize) -> Self {
        Converter { channels: channels.max(1), resampler: Resampler::new(rate) }
    }

    pub(crate) fn matches(&self, rate: u32, channels: usize) -> bool {
        self.resampler.from_rate() == rate.max(1) && self.channels == channels.max(1)
    }

    pub(crate) fn convert(&mut self, interleaved: &[f32]) -> Vec<f32> {
        let mono = downmix(interleaved, self.channels);
        self.resampler.process(&mono)
    }
}

/// How far (in samples) the fastest stream of an app may run ahead of its
/// slowest before the slow one is skipped forward (a paused tab stops
/// producing; the app's channel must not stall behind it).
const SLACK: u64 = 3 * CHUNK as u64;

/// The streams of ONE app summed into one timeline: each source writes at
/// its own cursor, everything every live source has written past is
/// emitted in [`CHUNK`]s.
#[derive(Debug, Default)]
pub(crate) struct Mixer {
    /// Absolute sample index of `mix[0]`.
    base: u64,
    mix: Vec<f32>,
    /// Each source's write cursor (absolute).
    sources: HashMap<u32, u64>,
}

impl Mixer {
    pub(crate) fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }

    pub(crate) fn has(&self, source: u32) -> bool {
        self.sources.contains_key(&source)
    }

    /// A new stream of this app: it starts at the most advanced cursor.
    pub(crate) fn add(&mut self, source: u32) {
        let start = self.sources.values().copied().max().unwrap_or(self.base);
        self.sources.entry(source).or_insert(start);
    }

    /// A stream ended: whatever the rest have written may go now; the last
    /// one leaving flushes the remainder (a short final chunk).
    pub(crate) fn remove(&mut self, source: u32) -> Vec<Vec<f32>> {
        self.sources.remove(&source);
        if self.sources.is_empty() {
            let rest: Vec<f32> = self.mix.drain(..).collect();
            self.base += rest.len() as u64;
            return if rest.is_empty() { Vec::new() } else { vec![rest] };
        }
        self.drain()
    }

    /// Mono [`SAMPLE_RATE`] samples of `source` (added implicitly).
    pub(crate) fn push(&mut self, source: u32, samples: &[f32]) -> Vec<Vec<f32>> {
        self.add(source);
        let cursor = self.sources[&source].max(self.base);
        let offset = (cursor - self.base) as usize;
        let end = offset + samples.len();
        if self.mix.len() < end {
            self.mix.resize(end, 0.0);
        }
        for (slot, sample) in self.mix[offset..end].iter_mut().zip(samples) {
            *slot += *sample;
        }
        self.sources.insert(source, cursor + samples.len() as u64);
        self.drain()
    }

    fn drain(&mut self) -> Vec<Vec<f32>> {
        let (Some(low), Some(high)) = (self.sources.values().min().copied(), self.sources.values().max().copied())
        else {
            return Vec::new();
        };
        let ready = low.max(high.saturating_sub(SLACK));
        let mut out = Vec::new();
        while ready.saturating_sub(self.base) >= CHUNK as u64 {
            let chunk: Vec<f32> = self.mix.drain(..CHUNK).map(|s| s.clamp(-1.0, 1.0)).collect();
            self.base += CHUNK as u64;
            out.push(chunk);
        }
        out
    }
}

/// A [`Mixer`] per app id (+ the app's label), the routing every backend
/// shares: `push` returns the finished chunks of that app's channel.
#[derive(Debug, Default)]
pub(crate) struct Mixers {
    apps: HashMap<String, (String, Mixer)>,
}

impl Mixers {
    pub(crate) fn add(&mut self, app: &str, label: &str, source: u32) {
        self.apps.entry(app.to_string()).or_insert_with(|| (label.to_string(), Mixer::default())).1.add(source);
    }

    pub(crate) fn push(&mut self, app: &str, label: &str, source: u32, samples: &[f32]) -> Vec<Chunk> {
        let (label, mixer) = self.apps.entry(app.to_string()).or_insert_with(|| (label.to_string(), Mixer::default()));
        let label = label.clone();
        mixer.push(source, samples).into_iter().map(|samples| chunk(app, &label, samples)).collect()
    }

    /// `source` ended; returns the chunks it unblocked (the app's last
    /// stream leaving drops the app's mixer).
    pub(crate) fn remove(&mut self, app: &str, source: u32) -> Vec<Chunk> {
        let Some((label, mixer)) = self.apps.get_mut(app) else { return Vec::new() };
        let label = label.clone();
        let out: Vec<Chunk> =
            mixer.remove(source).into_iter().map(|samples| chunk(app, &label, samples)).collect();
        if mixer.is_empty() {
            self.apps.remove(app);
        }
        out
    }

    pub(crate) fn apps(&self) -> impl Iterator<Item = &str> {
        self.apps.keys().map(String::as_str)
    }
}

fn chunk(app: &str, label: &str, samples: Vec<f32>) -> Chunk {
    Chunk { channel: app.to_string(), label: label.to_string(), samples }
}

/// Splits a mono [`SAMPLE_RATE`] stream (the system channel) into
/// [`CHUNK`]s, keeping the remainder for the next call.
#[derive(Debug, Default)]
pub(crate) struct Chunker {
    pending: Vec<f32>,
}

impl Chunker {
    pub(crate) fn push(&mut self, samples: &[f32]) -> Vec<Vec<f32>> {
        self.pending.extend_from_slice(samples);
        let mut out = Vec::new();
        while self.pending.len() >= CHUNK {
            out.push(self.pending.drain(..CHUNK).collect());
        }
        out
    }
}

/// Little-endian f32 bytes → samples (a trailing partial sample dropped).
pub(crate) fn f32_le(bytes: &[u8]) -> Vec<f32> {
    bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect()
}

/// Little-endian i16 bytes → samples in [-1, 1).
pub(crate) fn i16_le(bytes: &[u8]) -> Vec<f32> {
    bytes.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect()
}

/// Whether `candidate` (an app's binary / exe stem) is this executable: the
/// IDE and its speech worker (the same executable re-run) are never
/// captured.
pub(crate) fn is_own_binary(candidate: &str) -> bool {
    own_stem().is_some_and(|own| same_binary(&own, candidate))
}

pub(crate) fn own_stem() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    Some(exe.file_stem()?.to_string_lossy().into_owned())
}

pub(crate) fn same_binary(own: &str, candidate: &str) -> bool {
    let strip = |s: &str| {
        let base = s.rsplit(['/', '\\']).next().unwrap_or(s).to_ascii_lowercase();
        base.strip_suffix(".exe").map(str::to_string).unwrap_or(base)
    };
    !own.is_empty() && strip(own) == strip(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downmix_averages_frames() {
        assert_eq!(downmix(&[1.0, 0.0, 0.5, 0.5, -1.0, 1.0], 2), vec![0.5, 0.5, 0.0]);
        assert_eq!(downmix(&[0.25, 0.75], 1), vec![0.25, 0.75]);
        // A partial frame at the end is dropped.
        assert_eq!(downmix(&[1.0, 1.0, 1.0], 2), vec![1.0]);
    }

    #[test]
    fn resampler_passes_16k_through() {
        let mut r = Resampler::new(16_000);
        assert_eq!(r.process(&[0.1, 0.2, 0.3]), vec![0.1, 0.2, 0.3]);
    }

    #[test]
    fn resampler_decimates_48k_by_three() {
        let mut r = Resampler::new(48_000);
        let input: Vec<f32> = (0..12).map(|i| i as f32).collect();
        assert_eq!(r.process(&input), vec![0.0, 3.0, 6.0, 9.0]);
        // Continuous across buffers: the next output is input index 12.
        let next: Vec<f32> = (12..18).map(|i| i as f32).collect();
        assert_eq!(r.process(&next), vec![12.0, 15.0]);
    }

    #[test]
    fn resampler_interpolates_across_buffer_edges() {
        // 8 kHz → 16 kHz: every other output sits halfway between inputs,
        // including between the last of one buffer and the first of the next.
        let mut r = Resampler::new(8_000);
        assert_eq!(r.process(&[0.0, 1.0]), vec![0.0, 0.5, 1.0]);
        assert_eq!(r.process(&[2.0, 3.0]), vec![1.5, 2.0, 2.5, 3.0]);
    }

    #[test]
    fn resampler_rate_holds_over_many_odd_buffers() {
        // 44.1 kHz in uneven buffers: one second in ≈ one second out.
        let mut r = Resampler::new(44_100);
        let mut produced = 0usize;
        let mut fed = 0usize;
        for size in [441usize, 1, 999, 7, 2048].iter().cycle().take(200) {
            let input = vec![0.5f32; *size];
            fed += size;
            produced += r.process(&input).len();
        }
        let expected = fed as f64 * 16_000.0 / 44_100.0;
        assert!((produced as f64 - expected).abs() <= 2.0, "{produced} vs {expected}");
    }

    #[test]
    fn converter_downmixes_then_resamples() {
        let mut c = Converter::new(32_000, 2);
        assert!(c.matches(32_000, 2));
        assert!(!c.matches(48_000, 2));
        assert_eq!(c.convert(&[1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0]), vec![0.5, 1.0]);
    }

    #[test]
    fn mixer_single_source_emits_whole_chunks() {
        let mut m = Mixer::default();
        assert!(m.push(1, &vec![0.1; CHUNK - 1]).is_empty());
        let out = m.push(1, &vec![0.1; CHUNK + 10]);
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|c| c.len() == CHUNK));
        // The last source leaving flushes the remainder.
        let rest = m.remove(1);
        assert_eq!(rest.len(), 1);
        assert_eq!(rest[0].len(), 9);
        assert!(m.is_empty());
    }

    #[test]
    fn mixer_sums_sources_of_one_app_aligned() {
        let mut m = Mixer::default();
        m.add(1);
        m.add(2);
        assert!(m.push(1, &vec![0.25; CHUNK]).is_empty(), "waits for the second source");
        let out = m.push(2, &vec![0.5; CHUNK]);
        assert_eq!(out, vec![vec![0.75; CHUNK]]);
    }

    #[test]
    fn mixer_skips_a_stalled_source_after_the_slack() {
        let mut m = Mixer::default();
        m.add(1);
        m.add(2);
        let mut emitted = 0;
        for _ in 0..4 {
            emitted += m.push(1, &vec![0.1; CHUNK]).len();
        }
        assert_eq!(emitted, 1, "only what lies beyond the slack goes");
        // The stalled source resumes at the emitted edge, not in the past.
        m.push(2, &vec![0.1; CHUNK]);
        assert!(m.sources[&2] >= m.base);
    }

    #[test]
    fn mixer_clamps_the_sum() {
        let mut m = Mixer::default();
        m.add(1);
        m.add(2);
        m.push(1, &vec![0.8; CHUNK]);
        let out = m.push(2, &vec![0.8; CHUNK]);
        assert_eq!(out[0][0], 1.0);
    }

    #[test]
    fn mixers_never_mix_across_apps() {
        let mut m = Mixers::default();
        let a = m.push("firefox", "Firefox", 1, &vec![0.2; CHUNK]);
        let b = m.push("zoom", "Zoom", 2, &vec![0.4; CHUNK]);
        assert_eq!(a.len(), 1);
        assert_eq!((a[0].channel.as_str(), a[0].label.as_str(), a[0].samples[0]), ("firefox", "Firefox", 0.2));
        assert_eq!((b[0].channel.as_str(), b[0].samples[0]), ("zoom", 0.4));
        assert!(m.remove("zoom", 2).is_empty());
        assert_eq!(m.apps().collect::<Vec<_>>(), vec!["firefox"]);
    }

    #[test]
    fn chunker_keeps_the_remainder() {
        let mut c = Chunker::default();
        assert!(c.push(&vec![0.0; CHUNK / 2]).is_empty());
        assert_eq!(c.push(&vec![0.0; CHUNK]).len(), 1);
        assert_eq!(c.pending.len(), CHUNK / 2);
    }

    #[test]
    fn byte_decoders() {
        assert_eq!(f32_le(&[0, 0, 128, 63, 0, 0]), vec![1.0]);
        assert_eq!(i16_le(&[0, 128, 0, 64]), vec![-1.0, 0.5]);
    }

    #[test]
    fn own_binary_matches_by_stem() {
        assert!(same_binary("exponential", "exponential"));
        assert!(same_binary("Exponential", "C:\\Program Files\\Exponential\\exponential.exe"));
        assert!(same_binary("exponential", "/opt/exponential/exponential"));
        assert!(!same_binary("exponential", "firefox"));
        assert!(!same_binary("", ""));
    }
}
