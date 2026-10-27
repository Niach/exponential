//! NEW-55: echo cancellation for LOCAL calls (the voice device's own
//! microphone next to its own speakers), `voice-call.json`: "the voice
//! device's own microphone runs through the worker's echo canceller with the
//! reply as the reference".
//!
//! The far end is what the sink plays: [`TapSink`] copies every pushed reply
//! sample, resampled to 16 kHz, into [`FarEnd`] BEFORE the player gets it, and
//! the call's microphone loop takes as many reference samples as each
//! microphone chunk holds ([`FarEnd::take`]), so the reference runs at the
//! microphone's clock and always LEADS its echo (by the player's and the
//! recorder's latency, well inside the filter's [`TAIL_MS`]). The pure-Rust
//! canceller [`decibri_aec`] (Apache-2.0; no C build, so no cmake or
//! libclang on any runner) then cleans the microphone. Push to talk and remote calls never pass
//! through here (a caller's OS cancels its own echo).

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::capture::Resampler;
use crate::listen::RATE;
use crate::sink::Sink;

/// The echo path the filter covers (player + recorder latency + the room).
pub const TAIL_MS: usize = 300;
/// The most reference audio held (a reply pushed far ahead of a stalled
/// microphone never grows memory).
const MAX_FAR: usize = 60 * RATE;

/// The far-end reference of the running local call. Off (and empty) while
/// no local call runs: pushes are ignored then.
#[derive(Default)]
pub struct FarEnd {
    enabled: AtomicBool,
    queue: Mutex<VecDeque<f32>>,
}

impl FarEnd {
    /// A local call started (`true`) or ended (`false`, which also clears).
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::SeqCst);
        if !enabled {
            self.clear();
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    /// Reference samples (16 kHz) about to play.
    pub fn push(&self, samples: &[f32]) {
        if !self.enabled() {
            return;
        }
        let mut queue = self.lock();
        queue.extend(samples);
        let over = queue.len().saturating_sub(MAX_FAR);
        queue.drain(..over);
    }

    /// The reference for the next `n` microphone samples (silence past what
    /// was played).
    pub fn take(&self, n: usize) -> Vec<f32> {
        let mut queue = self.lock();
        let have = n.min(queue.len());
        let mut out: Vec<f32> = queue.drain(..have).collect();
        out.resize(n, 0.0);
        out
    }

    /// The reply stopped: nothing more plays.
    pub fn clear(&self) {
        self.lock().clear();
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, VecDeque<f32>> {
        self.queue.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// A sink that hands the far end a copy of what it plays.
pub struct TapSink {
    inner: Arc<dyn Sink>,
    far: Arc<FarEnd>,
    resampler: Mutex<Resampler>,
}

impl TapSink {
    /// Wrap `inner`, which plays mono audio at `rate` Hz.
    pub fn new(inner: Arc<dyn Sink>, far: Arc<FarEnd>, rate: u32) -> Self {
        TapSink { inner, far, resampler: Mutex::new(Resampler::new(rate, RATE as u32)) }
    }
}

impl Sink for TapSink {
    fn push(&self, samples: &[f32]) -> Result<(), String> {
        if self.far.enabled() {
            let mut reference = Vec::with_capacity(samples.len());
            self.resampler.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).process(samples, &mut reference);
            self.far.push(&reference);
        }
        self.inner.push(samples)
    }

    fn finish(&self) {
        self.inner.finish();
    }

    fn stop(&self) {
        self.inner.stop();
        self.far.clear();
    }
}

/// The canceller of one local call (lives on the listening thread).
pub struct EchoCanceller {
    aec: Option<decibri_aec::Aec>,
}

impl Default for EchoCanceller {
    fn default() -> Self {
        Self::new()
    }
}

impl EchoCanceller {
    pub fn new() -> Self {
        let mut config = decibri_aec::AecConfig::default();
        config.sample_rate = RATE as u32;
        config.tail_ms = TAIL_MS as u16;
        let aec = decibri_aec::Aec::new(config)
            .map_err(|err| log::warn!("[voice] echo canceller unavailable, the microphone stays raw: {err}"))
            .ok();
        EchoCanceller { aec }
    }

    /// Cancel `far`'s echo out of `near` (`far` = what played over the same
    /// span). The canceller re-blocks internally, so a call may return a
    /// little more or less than it took; nothing is lost.
    pub fn process(&mut self, near: &[f32], far: &[f32]) -> Vec<f32> {
        let Some(aec) = self.aec.as_mut() else { return near.to_vec() };
        aec.feed_reference(far);
        let mut out = Vec::with_capacity(near.len());
        if let Err(err) = aec.process(near, &mut out) {
            log::warn!("[voice] echo canceller failed, this chunk stays raw: {err}");
            return near.to_vec();
        }
        out
    }
}

/// A running local call's echo path: the canceller, and the far end it
/// switched on (off again when the call ends).
pub struct Echo {
    canceller: EchoCanceller,
    far: Arc<FarEnd>,
}

impl Echo {
    pub fn new(far: Arc<FarEnd>) -> Self {
        far.set_enabled(true);
        Echo { canceller: EchoCanceller::new(), far }
    }

    /// The microphone's `near` samples without the reply's echo.
    pub fn cancel(&mut self, near: &[f32]) -> Vec<f32> {
        let far = self.far.take(near.len());
        self.canceller.process(near, &far)
    }
}

impl Drop for Echo {
    fn drop(&mut self) {
        self.far.set_enabled(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|sample| sample * sample).sum::<f32>() / samples.len().max(1) as f32).sqrt()
    }

    /// A reply clip (speech-like: our built-in voice) played into a room
    /// that echoes it 20 ms later at 60 %: after the filter converges the
    /// microphone holds far less of it.
    #[test]
    fn the_reply_is_cancelled_out_of_the_microphone() {
        let reply = crate::capture::read_wav(crate::builtin::for_language("en").wav).unwrap();
        let delay = RATE / 50;
        let near: Vec<f32> =
            (0..reply.len()).map(|index| if index >= delay { 0.6 * reply[index - delay] } else { 0.0 }).collect();
        let mut canceller = EchoCanceller::new();
        let mut out = Vec::new();
        for (near, far) in near.chunks(320).zip(reply.chunks(320)) {
            out.extend(canceller.process(near, far));
        }
        assert!(out.len().abs_diff(near.len()) < RATE / 10, "{} of {}", out.len(), near.len());
        // The last 4 s, after convergence.
        let len = out.len().min(near.len());
        let tail = len - 4 * RATE;
        let (before, after) = (rms(&near[tail..len]), rms(&out[tail..len]));
        assert!(after < before * 0.3, "echo {before:.4} → {after:.4}");
    }

    #[test]
    fn the_far_end_is_off_outside_a_local_call_and_bounded() {
        let far = Arc::new(FarEnd::default());
        far.push(&[1.0; 10]);
        assert_eq!(far.take(4), [0.0; 4], "off: nothing is kept");
        {
            let _echo = Echo::new(far.clone());
            far.push(&[0.5; 3]);
            assert_eq!(far.take(5), [0.5, 0.5, 0.5, 0.0, 0.0]);
            far.push(&vec![0.1; MAX_FAR + 100]);
            assert_eq!(far.lock().len(), MAX_FAR);
        }
        assert!(!far.enabled(), "the call ended");
        assert!(far.lock().is_empty());
    }
}
