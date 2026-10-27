//! NEW-55: echo cancellation for LOCAL calls (the voice device's own
//! microphone next to its own speakers), `voice-call.json`: "the voice
//! device's own microphone runs through the worker's echo canceller with the
//! reply as the reference".
//!
//! #NEW-92: the far end is what the call's ONE output stream
//! ([`crate::sink::Stream`]) hands its device, silence included: its
//! [`FarEnd::tap`] copies every block, resampled to 16 kHz, into [`FarEnd`],
//! and the call's microphone loop takes as many reference samples as each
//! microphone chunk holds ([`FarEnd::take`]), so reference and microphone
//! run sample for sample from the stream's first block to Hangup and the
//! echo sits at ONE fixed delay behind its reference (the player's and the
//! recorder's latency, well inside the filter's [`TAIL_MS`]). A reference
//! block that arrives late is owed, never shifted. The pure-Rust canceller
//! [`decibri_aec`] (Apache-2.0; no C build, so no cmake or libclang on any
//! runner) then cleans the microphone, and [`Echo::state`] hands the call's
//! barge-in gate the reference's envelope (talk-listen.json `echoGate`). Push to talk and remote calls
//! never pass through here (a caller's OS cancels its own echo).

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::capture::Resampler;
use crate::listen::{EchoState, ECHO_WINDOW_MS, FRAME, RATE};
use crate::sink::Tap;

/// The echo path the filter covers (player + recorder latency + the room).
pub const TAIL_MS: usize = 300;
/// The most reference audio held (a reply pushed far ahead of a stalled
/// microphone never grows memory).
const MAX_FAR: usize = 60 * RATE;
/// #NEW-92: the most reference a stream may owe (a stall longer than this
/// is a new start, not a late block).
const MAX_OWED: usize = RATE / 2;

/// The far-end reference of the running local call. Off (and empty) while
/// no local call runs: pushes are ignored then.
#[derive(Default)]
pub struct FarEnd {
    enabled: AtomicBool,
    /// #NEW-92: live [`FarEnd::tap`]s (a stream plays): a microphone chunk
    /// that finds too little reference owes the rest instead of shifting.
    taps: AtomicUsize,
    queue: Mutex<Reference>,
}

#[derive(Default)]
struct Reference {
    samples: VecDeque<f32>,
    /// Samples the microphone already took as silence: the next pushes skip
    /// them, so the alignment holds.
    owed: usize,
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
        let skip = queue.owed.min(samples.len());
        queue.owed -= skip;
        queue.samples.extend(&samples[skip..]);
        let over = queue.samples.len().saturating_sub(MAX_FAR);
        queue.samples.drain(..over);
    }

    /// The reference for the next `n` microphone samples (silence past what
    /// was played).
    pub fn take(&self, n: usize) -> Vec<f32> {
        let streaming = self.taps.load(Ordering::SeqCst) > 0;
        let mut queue = self.lock();
        let have = n.min(queue.samples.len());
        let mut out: Vec<f32> = queue.samples.drain(..have).collect();
        if streaming && self.enabled() {
            queue.owed += n - have;
            if queue.owed > MAX_OWED {
                queue.owed = 0;
            }
        }
        out.resize(n, 0.0);
        out
    }

    /// Nothing more plays.
    pub fn clear(&self) {
        let mut queue = self.lock();
        queue.samples.clear();
        queue.owed = 0;
    }

    /// #NEW-92: a [`Tap`] for a stream at `rate` Hz: every block it plays
    /// becomes reference (while a local call runs).
    pub fn tap(far: &Arc<FarEnd>, rate: u32) -> Tap {
        struct Tapping {
            far: Arc<FarEnd>,
            resampler: Resampler,
            reference: Vec<f32>,
        }
        impl Drop for Tapping {
            fn drop(&mut self) {
                self.far.taps.fetch_sub(1, Ordering::SeqCst);
            }
        }
        far.taps.fetch_add(1, Ordering::SeqCst);
        let mut tapping = Tapping { far: far.clone(), resampler: Resampler::new(rate, RATE as u32), reference: Vec::new() };
        Box::new(move |samples: &[f32]| {
            let Tapping { far, resampler, reference } = &mut tapping;
            reference.clear();
            resampler.process(samples, reference);
            far.push(reference);
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Reference> {
        self.queue.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
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

    /// The canceller's own numbers (the `echo_check` example prints them).
    pub fn metrics(&self) -> Option<decibri_aec::AecMetrics> {
        self.aec.as_ref().map(decibri_aec::Aec::metrics)
    }
}

/// The level of `samples` in dBFS (-120 for silence).
pub fn dbfs(samples: &[f32]) -> f32 {
    let power = samples.iter().map(|sample| sample * sample).sum::<f32>() / samples.len().max(1) as f32;
    10.0 * power.max(1e-12).log10()
}

/// A running local call's echo path: the canceller, and the far end it
/// switched on (off again when the call ends).
pub struct Echo {
    canceller: EchoCanceller,
    far: Arc<FarEnd>,
    /// #NEW-92: the levels (dBFS) of the reference's 32 ms blocks over the
    /// last [`ECHO_WINDOW_MS`], and the block being filled.
    blocks: VecDeque<f32>,
    block: Vec<f32>,
    /// The reference of the chunk cancelled last.
    reference: Vec<f32>,
}

impl Echo {
    pub fn new(far: Arc<FarEnd>) -> Self {
        far.set_enabled(true);
        Echo { canceller: EchoCanceller::new(), far, blocks: VecDeque::new(), block: Vec::new(), reference: Vec::new() }
    }

    /// The microphone's `near` samples without the reply's echo.
    pub fn cancel(&mut self, near: &[f32]) -> Vec<f32> {
        self.reference = self.far.take(near.len());
        for sample in &self.reference {
            self.block.push(*sample);
            if self.block.len() == FRAME {
                self.blocks.push_back(dbfs(&self.block));
                self.block.clear();
                if self.blocks.len() > (ECHO_WINDOW_MS * RATE / 1000).div_ceil(FRAME) {
                    self.blocks.pop_front();
                }
            }
        }
        self.canceller.process(near, &self.reference)
    }

    /// The reference the last [`Echo::cancel`] used (`echo_check` dumps it).
    pub fn reference(&self) -> &[f32] {
        &self.reference
    }

    /// The canceller's own numbers (`echo_check` prints them).
    pub fn metrics(&self) -> Option<decibri_aec::AecMetrics> {
        self.canceller.metrics()
    }

    /// #NEW-92: what the barge-in gate needs about the chunk just cancelled:
    /// the reference's envelope (its loudest block over the window).
    pub fn state(&self) -> EchoState {
        let reference_db = self.blocks.iter().copied().fold(EchoState::default().reference_db, f32::max);
        EchoState { reference_db }
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
            assert_eq!(far.lock().samples.len(), MAX_FAR);
        }
        assert!(!far.enabled(), "the call ended");
        assert!(far.lock().samples.is_empty());
    }

    /// #NEW-92: while a stream taps the far end, a microphone chunk that
    /// finds too little reference owes the rest: the late block's first
    /// samples are skipped, so reference and microphone stay aligned.
    #[test]
    fn a_late_reference_block_is_owed_never_shifted() {
        let far = Arc::new(FarEnd::default());
        let _echo = Echo::new(far.clone());
        // No stream: silence is just silence (the reply has not started).
        assert_eq!(far.take(3), [0.0; 3]);
        far.push(&[1.0, 2.0]);
        assert_eq!(far.take(2), [1.0, 2.0], "nothing owed before a stream");
        let mut tap = FarEnd::tap(&far, RATE as u32);
        tap(&[1.0, 2.0]);
        assert_eq!(far.take(4), [1.0, 2.0, 0.0, 0.0], "two samples owed");
        tap(&[3.0, 4.0, 5.0, 6.0]);
        assert_eq!(far.take(2), [5.0, 6.0], "the owed two were skipped");
        drop(tap);
        assert_eq!(far.taps.load(Ordering::SeqCst), 0);
        assert_eq!(far.take(2), [0.0; 2]);
        assert_eq!(far.lock().owed, 0, "nothing owed once the stream closed");
    }

    /// #NEW-92: the gate's word on the reply: its reference's loudest block
    /// over the last `ECHO_WINDOW_MS`; audible until that window is quiet.
    #[test]
    fn the_reply_is_audible_until_its_window_has_passed() {
        let far = Arc::new(FarEnd::default());
        let mut echo = Echo::new(far.clone());
        echo.cancel(&[0.0; FRAME]);
        assert!(!echo.state().replying(), "nothing played yet");
        far.push(&[0.1; FRAME]);
        echo.cancel(&[0.0; FRAME]);
        assert!((echo.state().reference_db - -20.0).abs() < 0.01, "{:?}", echo.state());
        let window = (ECHO_WINDOW_MS * RATE / 1000).div_ceil(FRAME);
        for _ in 0..window - 1 {
            echo.cancel(&[0.0; FRAME]);
        }
        assert!(echo.state().replying(), "still in the window");
        echo.cancel(&[0.0; FRAME]);
        assert!(!echo.state().replying(), "the window has passed");
    }
}
