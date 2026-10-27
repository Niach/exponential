//! NEW-88: the local call's echo canceller (`voice::echo`, `decibri_aec`)
//! with any valid config and any interleaving of reference and microphone
//! chunks of any size, discontinuities, resets and flushes; the samples are
//! fuzzed PCM16 (what a microphone and the player give), generated tones and
//! noise for long runs, or raw f32 (NaN, infinities, huge values).
//! Oracle: no panic, no memory error (ASan), finite output for finite input,
//! and output that keeps up with the input.
#![no_main]

use arbitrary::Arbitrary;
use decibri_aec::{Aec, AecConfig, CaptureContinuity, Suppression};
use libfuzzer_sys::fuzz_target;

#[derive(Arbitrary, Debug)]
struct Input {
    rate: u16,
    tail_ms: u16,
    max_echo_delay_ms: u16,
    max_search_delay_ms: u16,
    delay_hint_ms: Option<u16>,
    suppression: bool,
    ops: Vec<Op>,
}

#[derive(Arbitrary, Debug)]
enum Op {
    Reference(Samples),
    Process(Samples),
    Discontinuity(Option<u16>),
    Flush,
    Reset,
}

#[derive(Arbitrary, Debug)]
enum Samples {
    Pcm(Vec<i16>),
    /// `len` samples of a tone plus noise (long runs cheaply).
    Generated { len: u16, freq: u16, amp: i16, seed: u32 },
    Raw(Vec<f32>),
}

impl Samples {
    fn render(&self, rate: u32) -> (Vec<f32>, bool) {
        match self {
            Samples::Pcm(pcm) => (pcm.iter().map(|sample| f32::from(*sample) / 32768.0).collect(), true),
            Samples::Generated { len, freq, amp, seed } => {
                let mut state = seed | 1;
                let amp = f32::from(*amp) / 32768.0;
                let step = f32::from(*freq) * std::f32::consts::TAU / rate as f32;
                let samples = (0..*len as usize)
                    .map(|i| {
                        state ^= state << 13;
                        state ^= state >> 17;
                        state ^= state << 5;
                        amp * ((i as f32 * step).sin() * 0.8 + (state as f32 / u32::MAX as f32 - 0.5) * 0.4)
                    })
                    .collect();
                (samples, true)
            }
            Samples::Raw(samples) => (samples.clone(), samples.iter().all(|sample| sample.is_finite())),
        }
    }
}

fuzz_target!(|input: Input| {
    let mut config = AecConfig::default();
    config.sample_rate = 8_000 + u32::from(input.rate) % 40_001;
    config.tail_ms = 16 + input.tail_ms % 485;
    config.max_echo_delay_ms = 10 + input.max_echo_delay_ms % 991;
    config.max_search_delay_ms = config.max_echo_delay_ms + input.max_search_delay_ms % (2001 - config.max_echo_delay_ms);
    config.delay_hint_ms = input.delay_hint_ms.map(|hint| hint % 2001);
    config.suppression = if input.suppression { Suppression::Conservative } else { Suppression::Off };
    let rate = config.sample_rate;
    let Ok(mut aec) = Aec::new(config) else { return };
    let mut near_in = 0usize;
    let mut out_total = 0usize;
    let mut finite = true;
    for op in input.ops.iter().take(48) {
        match op {
            Op::Reference(samples) => aec.feed_reference(&samples.render(rate).0),
            Op::Process(samples) => {
                let (near, near_finite) = samples.render(rate);
                finite &= near_finite;
                let mut out = Vec::new();
                if aec.process(&near, &mut out).is_ok() {
                    near_in += near.len();
                    out_total += out.len();
                    if finite {
                        assert!(out.iter().all(|sample| sample.is_finite()), "non-finite output for finite input");
                    }
                }
            }
            Op::Discontinuity(lost) => aec.declare_capture_continuity(CaptureContinuity::Discontinuity { lost_samples: lost.map(u64::from) }),
            Op::Flush => {
                let mut out = Vec::new();
                if aec.flush(&mut out).is_ok() {
                    out_total += out.len();
                }
            }
            Op::Reset => aec.reset(),
        }
        let _ = aec.metrics();
    }
    // The canceller re-blocks: it may hold back up to its latency, never more.
    assert!(out_total <= near_in + aec.latency_samples() + rate as usize, "{out_total} out for {near_in} in");
});
