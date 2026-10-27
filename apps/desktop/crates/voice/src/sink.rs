//! Where synthesized samples go: one [`Sink`] per utterance.
//!
//! Linux pipes raw f32 PCM into the session's sound server (`pw-play`, else
//! `paplay`): every PipeWire or PulseAudio desktop has one, and the build
//! needs no ALSA headers. macOS and Windows open the default output device
//! with cpal and resample the voice's rate to the device's.
//!
//! #NEW-92: a LOCAL call plays through ONE [`Stream`] instead, open from the
//! call's first reply to Hangup and playing silence between replies: a
//! player per reply started with a different latency each time (55 → 72 →
//! 112 ms measured), so the echo canceller lost the delay it had converged
//! on at every reply. The stream hands each block it gives the device to its
//! [`Tap`] (the canceller's reference, silence included), so reference and
//! echo stay one fixed delay apart for the whole call. #1281 review: the tap
//! is built for the rate the stream hands its device ([`MakeTap`]: the
//! device's own on cpal, 48 or 44.1 kHz, the voice's on Linux), and on cpal
//! the device's real-time callback only moves samples through two
//! lock-free [`Ring`]s; the stream's own thread resamples, taps and feeds.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(any(target_os = "macos", target_os = "windows", test))]
use std::sync::atomic::{AtomicU32, AtomicUsize};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

/// One utterance's audio path.
pub trait Sink: Send + Sync {
    /// Queue samples (mono, at the rate the sink was opened with). `Err` =
    /// the output takes no audio (a stopped sink answers `Ok`).
    fn push(&self, samples: &[f32]) -> Result<(), String>;
    /// Let what is queued play out, then close.
    fn finish(&self);
    /// Silence now and close.
    fn stop(&self);
}

/// Open a sink for mono audio at `rate` Hz on this machine.
pub fn open(rate: u32) -> Result<Arc<dyn Sink>, String> {
    platform::open(rate)
}

/// #NEW-92: every block a [`Stream`] hands its device (mono, at the rate the
/// stream was opened with), in order, silence included.
pub type Tap = Box<dyn FnMut(&[f32]) + Send>;

/// #1281 review: builds a stream's [`Tap`] for the rate (Hz) its blocks
/// come at, which only the opened device knows; again after a rebuild.
pub type MakeTap = Box<dyn FnMut(u32) -> Tap + Send>;

/// #NEW-92: a local call's output, open until the last handle drops (then
/// what is queued plays out and the device closes).
pub trait Stream: Send + Sync {
    /// Queue samples (mono, at the stream's rate). `Err` = the output failed.
    fn push(&self, samples: &[f32]) -> Result<(), String>;
    /// Wait until everything queued went to the device (bounded by what is
    /// queued plus a second), or a [`Stream::flush`].
    fn drain(&self);
    /// Drop everything queued; the stream plays silence again at once.
    fn flush(&self);
    /// Why the output stopped, if it did (a dead stream is replaced).
    fn failed(&self) -> Option<String>;
}

/// Open the call's stream for mono audio at `rate` Hz; the tap `make_tap`
/// builds for the device's rate sees every block.
pub fn open_stream(rate: u32, make_tap: MakeTap) -> Result<Arc<dyn Stream>, String> {
    platform::open_stream(rate, make_tap, None)
}

/// [`open_stream`] on the sound server's node `target` (Linux; the
/// `echo_check` example's loopback). macOS and Windows play on the default
/// device regardless.
pub fn open_stream_on(rate: u32, make_tap: MakeTap, target: &str) -> Result<Arc<dyn Stream>, String> {
    platform::open_stream(rate, make_tap, Some(target.to_string()))
}

/// #NEW-92: one reply on a call's [`Stream`]: `finish` waits for it to reach
/// the device, `stop` drops what is still queued; the stream stays open.
pub struct Reply {
    stream: Arc<dyn Stream>,
    /// Under this lock a push and a stop never interleave: nothing pushed
    /// after the flush.
    stopped: Mutex<bool>,
}

impl Reply {
    pub fn new(stream: Arc<dyn Stream>) -> Self {
        Reply { stream, stopped: Mutex::new(false) }
    }
}

impl Sink for Reply {
    fn push(&self, samples: &[f32]) -> Result<(), String> {
        let stopped = self.stopped.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if *stopped {
            return Ok(());
        }
        self.stream.push(samples)
    }

    fn finish(&self) {
        self.stream.drain();
    }

    fn stop(&self) {
        let mut stopped = self.stopped.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        *stopped = true;
        self.stream.flush();
    }
}

/// #NEW-92: the samples a [`Stream`] still has to hand its device, at
/// `rate` (the voice's: Linux's player takes it as is, cpal's [`Pump`]
/// resamples to the device's).
pub(crate) struct Queue {
    samples: Mutex<VecDeque<f32>>,
    /// Signalled whenever the queue empties (drained or flushed) or fails.
    emptied: Condvar,
    /// The last handle dropped: play out what is queued, then close.
    closing: AtomicBool,
    failed: Mutex<Option<String>>,
    rate: u32,
}

impl Queue {
    fn new(rate: u32) -> Self {
        Queue { samples: Mutex::new(VecDeque::new()), emptied: Condvar::new(), closing: AtomicBool::new(false), failed: Mutex::new(None), rate }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, VecDeque<f32>> {
        self.samples.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn push(&self, samples: impl IntoIterator<Item = f32>) -> Result<(), String> {
        if let Some(err) = self.failed() {
            return Err(err);
        }
        self.lock().extend(samples);
        Ok(())
    }

    /// The next `n` samples for the device, silence past what is queued.
    /// `None` = closing and nothing left: the stream ends.
    fn take(&self, n: usize) -> Option<Vec<f32>> {
        let mut queue = self.lock();
        if queue.is_empty() && self.closing() {
            return None;
        }
        let have = n.min(queue.len());
        let mut out: Vec<f32> = queue.drain(..have).collect();
        if have > 0 && queue.is_empty() {
            self.emptied.notify_all();
        }
        drop(queue);
        out.resize(n, 0.0);
        Some(out)
    }

    fn drain(&self) {
        let mut queue = self.lock();
        let budget = Duration::from_secs_f64(queue.len() as f64 / f64::from(self.rate.max(1))) + Duration::from_secs(1);
        let deadline = Instant::now() + budget;
        while !queue.is_empty() && self.failed().is_none() {
            let now = Instant::now();
            if now >= deadline {
                eprintln!("voice: the call's audio output did not drain in time");
                return;
            }
            queue = self.emptied.wait_timeout(queue, deadline - now).unwrap_or_else(|poisoned| poisoned.into_inner()).0;
        }
    }

    fn flush(&self) {
        self.lock().clear();
        self.emptied.notify_all();
    }

    fn fail(&self, err: String) {
        *self.failed.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(err);
        self.lock().clear();
        self.emptied.notify_all();
    }

    fn failed(&self) -> Option<String> {
        self.failed.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone()
    }

    fn closing(&self) -> bool {
        self.closing.load(Ordering::SeqCst)
    }
}

#[cfg(any(target_os = "macos", target_os = "windows", test))]
/// #1281 review: a single-producer single-consumer ring of samples, lock-
/// and allocation-free on both ends (a device's real-time callback pops
/// from one and pushes into another). Each slot holds an `f32`'s bits.
pub(crate) struct Ring {
    slots: Box<[AtomicU32]>,
    /// Samples pushed and popped so far (wrapping); `head - tail` = held.
    head: AtomicUsize,
    tail: AtomicUsize,
}

#[cfg(any(target_os = "macos", target_os = "windows", test))]
impl Ring {
    pub(crate) fn new(capacity: usize) -> Self {
        Ring { slots: (0..capacity.max(1)).map(|_| AtomicU32::new(0)).collect(), head: AtomicUsize::new(0), tail: AtomicUsize::new(0) }
    }

    pub(crate) fn len(&self) -> usize {
        self.head.load(Ordering::Acquire).wrapping_sub(self.tail.load(Ordering::Acquire))
    }

    /// The producer's side; `false` = full (the sample is dropped).
    pub(crate) fn push(&self, sample: f32) -> bool {
        let head = self.head.load(Ordering::Relaxed);
        if head.wrapping_sub(self.tail.load(Ordering::Acquire)) >= self.slots.len() {
            return false;
        }
        self.slots[head % self.slots.len()].store(sample.to_bits(), Ordering::Relaxed);
        self.head.store(head.wrapping_add(1), Ordering::Release);
        true
    }

    /// The consumer's side.
    pub(crate) fn pop(&self) -> Option<f32> {
        let tail = self.tail.load(Ordering::Relaxed);
        if tail == self.head.load(Ordering::Acquire) {
            return None;
        }
        let sample = f32::from_bits(self.slots[tail % self.slots.len()].load(Ordering::Relaxed));
        self.tail.store(tail.wrapping_add(1), Ordering::Release);
        Some(sample)
    }
}

#[cfg(any(target_os = "macos", target_os = "windows", test))]
/// #1281 review: how far the [`Pump`] keeps the device's ring filled ahead.
const PUMP_AHEAD_MS: u32 = 40;

#[cfg(any(target_os = "macos", target_os = "windows", test))]
/// #1281 review: the non-real-time half of a cpal call stream, run by its
/// own thread every few ms: it keeps the device's `out` ring
/// [`PUMP_AHEAD_MS`] ahead (the queue resampled from the voice's rate to
/// the device's), and hands what the callback reports `played` (at the
/// device's rate, silence included; `lost` = played samples the full ring
/// dropped, tapped as silence so the count holds) to the tap.
pub(crate) struct Pump {
    resampler: crate::capture::Resampler,
    voice_rate: u32,
    device_rate: u32,
    tap: Tap,
    pub(crate) out: Arc<Ring>,
    pub(crate) played: Arc<Ring>,
    pub(crate) lost: Arc<AtomicUsize>,
    block: Vec<f32>,
}

#[cfg(any(target_os = "macos", target_os = "windows", test))]
impl Pump {
    pub(crate) fn new(voice_rate: u32, device_rate: u32, tap: Tap) -> Self {
        Pump {
            resampler: crate::capture::Resampler::new(voice_rate, device_rate),
            voice_rate,
            device_rate,
            tap,
            out: Arc::new(Ring::new(device_rate as usize / 4)),
            played: Arc::new(Ring::new(device_rate as usize)),
            lost: Arc::default(),
            block: Vec::new(),
        }
    }

    /// Top the device's ring up from `queue`; `false` = the queue closed
    /// and played out (nothing more comes).
    pub(crate) fn fill(&mut self, queue: &Queue) -> bool {
        let ahead = (self.device_rate * PUMP_AHEAD_MS / 1000) as usize;
        let need = ahead.saturating_sub(self.out.len());
        if need == 0 {
            return true;
        }
        let voice = (need as u64 * u64::from(self.voice_rate)).div_ceil(u64::from(self.device_rate)) as usize;
        let Some(samples) = queue.take(voice) else { return false };
        self.block.clear();
        self.resampler.process(&samples, &mut self.block);
        for sample in &self.block {
            if !self.out.push(*sample) {
                break;
            }
        }
        true
    }

    /// Tap what the device played since the last call.
    pub(crate) fn tap_played(&mut self) {
        self.block.clear();
        while let Some(sample) = self.played.pop() {
            self.block.push(sample);
        }
        let lost = self.lost.swap(0, Ordering::Relaxed);
        self.block.resize(self.block.len() + lost, 0.0);
        if !self.block.is_empty() {
            (self.tap)(&self.block);
        }
    }

    /// The device's side, one frame: what the callback plays (silence when
    /// the pump fell behind), and that it played it.
    pub(crate) fn device_sample(out: &Ring, played: &Ring, lost: &AtomicUsize) -> f32 {
        let sample = out.pop().unwrap_or(0.0);
        if !played.push(sample) {
            lost.fetch_add(1, Ordering::Relaxed);
        }
        sample
    }
}

/// The handle [`open_stream`] returns: dropping the last one closes the
/// stream once what is queued has played. Pushes are resampled from the
/// voice's rate to the queue's.
struct Handle {
    queue: Arc<Queue>,
    resampler: Mutex<crate::capture::Resampler>,
}

impl Handle {
    fn new(queue: Arc<Queue>, rate: u32) -> Self {
        let resampler = Mutex::new(crate::capture::Resampler::new(rate, queue.rate));
        Handle { queue, resampler }
    }
}

impl Stream for Handle {
    fn push(&self, samples: &[f32]) -> Result<(), String> {
        let mut out = Vec::with_capacity(samples.len());
        self.resampler.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).process(samples, &mut out);
        self.queue.push(out)
    }

    fn drain(&self) {
        self.queue.drain();
    }

    fn flush(&self) {
        self.queue.flush();
    }

    fn failed(&self) -> Option<String> {
        self.queue.failed()
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        self.queue.closing.store(true, Ordering::SeqCst);
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod platform {
    use std::io::Write;
    use std::process::{Child, ChildStdin, Command, Stdio};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use super::Sink;

    /// The player's stdin and the player itself sit behind SEPARATE locks:
    /// a write blocked on a full pipe holds only `stdin`, so `stop` can
    /// still take `child` and kill it at once (the blocked write then fails
    /// with a broken pipe and returns).
    ///
    /// #1231 review: `pw-play` exists on a desktop without a PipeWire audio
    /// session too, and exits at once there; the first write then fails
    /// with a broken pipe and the NEXT player takes the audio.
    pub struct PipeSink {
        stdin: Mutex<Option<ChildStdin>>,
        child: Mutex<Option<Child>>,
        stopped: AtomicBool,
        rate: u32,
        /// The next player to try (an index into [`candidates`]).
        next: Mutex<usize>,
        /// A write went through: the player plays, a later failure is final.
        written: AtomicBool,
    }

    /// Samples per write: ~85 ms at 24 kHz, so a stop lands between chunks.
    const CHUNK: usize = 2048;
    /// How long a player that refused the first write gets to exit.
    const EXIT_GRACE: Duration = Duration::from_millis(250);

    /// The players, in order: PipeWire's, then PulseAudio's.
    fn candidates(rate: u32) -> [(&'static str, Vec<String>); 2] {
        [
            (
                "pw-play",
                vec!["--format".into(), "f32".into(), "--rate".into(), rate.to_string(), "--channels".into(), "1".into(), "-".into()],
            ),
            (
                "paplay",
                vec!["--raw".into(), "--format=float32le".into(), format!("--rate={rate}"), "--channels=1".into()],
            ),
        ]
    }

    fn spawn(program: &str, args: &[String]) -> Result<Child, String> {
        // Absolute, from the system dirs: never a binary the PATH plants.
        let path = talk::system_binary(program).ok_or_else(|| format!("{program}: not in {}", talk::SYSTEM_BIN_DIRS.join(", ")))?;
        Command::new(path)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|err| format!("{program}: {err}"))
    }

    pub fn open(rate: u32) -> Result<Arc<dyn Sink>, String> {
        let mut errors = Vec::new();
        for (index, (program, args)) in candidates(rate).iter().enumerate() {
            match spawn(program, args) {
                Ok(mut child) => {
                    let stdin = child.stdin.take();
                    return Ok(Arc::new(PipeSink {
                        stdin: Mutex::new(stdin),
                        child: Mutex::new(Some(child)),
                        stopped: AtomicBool::new(false),
                        rate,
                        next: Mutex::new(index + 1),
                        written: AtomicBool::new(false),
                    }));
                }
                Err(err) => errors.push(err),
            }
        }
        Err(format!("no audio player (needs PipeWire or PulseAudio): {}", errors.join("; ")))
    }

    /// #NEW-92: the pipe into a call's player holds one page (4 KiB = 43 ms
    /// at 24 kHz): the blocking writes then pace the stream at the player's
    /// own clock, and a stop leaves at most that much (plus the player's
    /// buffer) to play out.
    #[cfg(target_os = "linux")]
    const STREAM_PIPE_BYTES: i32 = 4096;

    /// #NEW-92: a call's stream: one player, fed 20 ms blocks (silence when
    /// nothing is queued) from its own thread until the last handle drops.
    pub fn open_stream(rate: u32, mut make_tap: super::MakeTap, target: Option<String>) -> Result<Arc<dyn super::Stream>, String> {
        let queue = Arc::new(super::Queue::new(rate));
        // The player takes the voice's rate as is: the tap sees that rate.
        let mut tap = make_tap(rate);
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let feeding = queue.clone();
        std::thread::Builder::new()
            .name("voice-call-audio".into())
            .spawn(move || feed(rate, target.as_deref(), &feeding, &mut tap, ready_tx))
            .map_err(|err| err.to_string())?;
        ready_rx.recv().map_err(|_| "the audio thread ended".to_string())??;
        Ok(Arc::new(super::Handle::new(queue, rate)))
    }

    /// Start the first player from `candidates[*next..]` that spawns.
    fn start(rate: u32, target: Option<&str>, next: &mut usize) -> Result<(Child, ChildStdin), String> {
        let mut errors = Vec::new();
        let candidates = candidates(rate);
        while let Some((program, args)) = candidates.get(*next) {
            *next += 1;
            let mut args = args.clone();
            if let Some(target) = target {
                args.insert(0, if *program == "pw-play" { format!("--target={target}") } else { format!("--device={target}") });
            }
            match spawn(program, &args) {
                Ok(mut child) => {
                    let Some(stdin) = child.stdin.take() else { continue };
                    #[cfg(target_os = "linux")]
                    {
                        use std::os::fd::AsRawFd as _;
                        // SAFETY: a valid pipe descriptor we own; failure only
                        // leaves the default size.
                        unsafe { libc::fcntl(stdin.as_raw_fd(), libc::F_SETPIPE_SZ, STREAM_PIPE_BYTES) };
                    }
                    return Ok((child, stdin));
                }
                Err(err) => errors.push(err),
            }
        }
        Err(format!("no audio player (needs PipeWire or PulseAudio): {}", errors.join("; ")))
    }

    fn feed(rate: u32, target: Option<&str>, queue: &super::Queue, tap: &mut super::Tap, ready: std::sync::mpsc::Sender<Result<(), String>>) {
        let mut next = 0;
        let (mut child, mut stdin) = match start(rate, target, &mut next) {
            Ok(player) => player,
            Err(err) => {
                let _ = ready.send(Err(err));
                return;
            }
        };
        let _ = ready.send(Ok(()));
        let block = (rate / 50).max(1) as usize;
        let mut written = false;
        while let Some(samples) = queue.take(block) {
            let bytes: Vec<u8> = samples.iter().flat_map(|sample| sample.to_le_bytes()).collect();
            while stdin.write_all(&bytes).is_err() {
                let _ = child.kill();
                let _ = child.wait();
                // #1231 review: a player that refused the very first block
                // (pw-play without a PipeWire session) hands over to the next.
                let replaced = if written { Err("the audio player stopped taking audio".to_string()) } else { start(rate, target, &mut next) };
                match replaced {
                    Ok(player) => {
                        (child, stdin) = player;
                        eprintln!("audio output: the next player takes over");
                    }
                    Err(err) => {
                        queue.fail(err);
                        return;
                    }
                }
            }
            written = true;
            tap(&samples);
        }
        // Closed: the player plays out its pipe and exits.
        drop(stdin);
        let _ = child.wait();
    }

    impl PipeSink {
        fn kill(&self) {
            let child = self.child.lock().ok().and_then(|mut guard| guard.take());
            if let Some(mut child) = child {
                let _ = child.kill();
                let _ = child.wait();
            }
        }

        /// Whether the player has exited (given [`EXIT_GRACE`] to do so).
        fn exited(&self) -> bool {
            let deadline = std::time::Instant::now() + EXIT_GRACE;
            loop {
                let Ok(mut guard) = self.child.lock() else { return false };
                let Some(child) = guard.as_mut() else { return false };
                match child.try_wait() {
                    Ok(Some(_)) => return true,
                    Ok(None) if std::time::Instant::now() < deadline => {}
                    _ => return false,
                }
                drop(guard);
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        /// Start the next player, if there is one.
        fn next_player(&self) -> Result<(), String> {
            let candidates = candidates(self.rate);
            let mut next = self.next.lock().map_err(|_| "the audio player lock is poisoned".to_string())?;
            let mut errors = vec!["the player exited before taking audio".to_string()];
            while let Some((program, args)) = candidates.get(*next) {
                *next += 1;
                match spawn(program, args) {
                    Ok(mut child) => {
                        let stdin = child.stdin.take();
                        if let Ok(mut guard) = self.stdin.lock() {
                            *guard = stdin;
                        }
                        if let Ok(mut guard) = self.child.lock() {
                            *guard = Some(child);
                        }
                        eprintln!("audio output: {program} takes over");
                        if self.stopped.load(Ordering::SeqCst) {
                            // A Stop landed meanwhile: nothing outlives it.
                            self.kill();
                        }
                        return Ok(());
                    }
                    Err(err) => errors.push(err),
                }
            }
            Err(format!("no audio player took the audio (needs PipeWire or PulseAudio): {}", errors.join("; ")))
        }
    }

    impl Sink for PipeSink {
        fn push(&self, samples: &[f32]) -> Result<(), String> {
            for chunk in samples.chunks(CHUNK) {
                let bytes: Vec<u8> = chunk.iter().flat_map(|sample| sample.to_le_bytes()).collect();
                loop {
                    if self.stopped.load(Ordering::SeqCst) {
                        return Ok(());
                    }
                    let Ok(mut guard) = self.stdin.lock() else { return Err("the audio pipe lock is poisoned".into()) };
                    let written = guard.as_mut().is_some_and(|stdin| stdin.write_all(&bytes).is_ok());
                    if written {
                        self.written.store(true, Ordering::SeqCst);
                        break;
                    }
                    guard.take();
                    drop(guard);
                    if self.stopped.load(Ordering::SeqCst) {
                        // The kill broke the pipe: not a failure.
                        return Ok(());
                    }
                    let refused = !self.written.load(Ordering::SeqCst) && self.exited();
                    self.kill();
                    if !refused {
                        return Err("the audio player stopped taking audio".into());
                    }
                    // The same chunk again, through the next player.
                    self.next_player()?;
                }
            }
            Ok(())
        }

        fn finish(&self) {
            // Closing stdin lets the player drain and exit on its own.
            drop(self.stdin.lock().ok().and_then(|mut guard| guard.take()));
            // #NEW-66: the player stays in `child` while it drains (polled,
            // the lock released between polls) so a Stop during the final
            // flush still kills it; it no longer plays a pipe buffer out.
            loop {
                let Ok(mut guard) = self.child.lock() else { return };
                let Some(child) = guard.as_mut() else { return };
                if !matches!(child.try_wait(), Ok(None)) {
                    guard.take();
                    return;
                }
                drop(guard);
                std::thread::sleep(Duration::from_millis(20));
            }
        }

        fn stop(&self) {
            self.stopped.store(true, Ordering::SeqCst);
            // Kill first (never waits on `stdin`), then drop the pipe if free.
            self.kill();
            if let Ok(mut guard) = self.stdin.try_lock() {
                guard.take();
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn stop_reaches_the_player_during_the_final_flush() {
            // A "player" that keeps draining for 30 s after its input closes.
            let mut child = Command::new("/bin/sh")
                .args(["-c", "cat >/dev/null; sleep 30"])
                .stdin(Stdio::piped())
                .spawn()
                .unwrap();
            let stdin = child.stdin.take();
            let sink = Arc::new(PipeSink {
                stdin: Mutex::new(stdin),
                child: Mutex::new(Some(child)),
                stopped: AtomicBool::new(false),
                rate: 24_000,
                next: Mutex::new(2),
                written: AtomicBool::new(true),
            });
            let started = std::time::Instant::now();
            let flushing = sink.clone();
            let finish = std::thread::spawn(move || flushing.finish());
            std::thread::sleep(Duration::from_millis(200));
            sink.stop();
            finish.join().unwrap();
            assert!(started.elapsed() < Duration::from_secs(5), "Stop killed the draining player");
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod platform {
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use cpal::{FromSample, SizedSample};

    use super::Sink;

    struct Shared {
        queue: Mutex<VecDeque<f32>>,
        stopped: AtomicBool,
    }

    /// cpal's `Stream` is not `Send` on every backend: it lives on its own
    /// thread, which the sink parks until the utterance ends.
    pub struct DeviceSink {
        shared: Arc<Shared>,
        source_rate: u32,
        device_rate: u32,
        /// Linear-resampler state: the fractional read position and the last
        /// source sample (to interpolate across pushes).
        resampler: Mutex<(f64, f32)>,
        done: Mutex<Option<std::sync::mpsc::Sender<()>>>,
        thread: Mutex<Option<std::thread::JoinHandle<()>>>,
    }

    pub fn open(rate: u32) -> Result<Arc<dyn Sink>, String> {
        let shared = Arc::new(Shared { queue: Mutex::new(VecDeque::new()), stopped: AtomicBool::new(false) });
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<u32, String>>();
        let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
        let thread_shared = shared.clone();
        let thread = std::thread::Builder::new()
            .name("voice-audio".into())
            .spawn(move || {
                let result = (|| {
                    let host = cpal::default_host();
                    let device = host.default_output_device().ok_or("no audio output device")?;
                    let supported = device.default_output_config().map_err(|err| err.to_string())?;
                    let channels = supported.channels() as usize;
                    let device_rate = supported.sample_rate().0;
                    let config: cpal::StreamConfig = supported.config();
                    let stream = match supported.sample_format() {
                        cpal::SampleFormat::F32 => build::<f32>(&device, &config, channels, thread_shared.clone()),
                        cpal::SampleFormat::I16 => build::<i16>(&device, &config, channels, thread_shared.clone()),
                        cpal::SampleFormat::U16 => build::<u16>(&device, &config, channels, thread_shared.clone()),
                        other => Err(format!("unsupported sample format {other:?}")),
                    }?;
                    stream.play().map_err(|err| err.to_string())?;
                    Ok::<_, String>((stream, device_rate))
                })();
                match result {
                    Ok((stream, device_rate)) => {
                        let _ = ready_tx.send(Ok(device_rate));
                        let _ = done_rx.recv();
                        drop(stream);
                    }
                    Err(err) => {
                        let _ = ready_tx.send(Err(err));
                    }
                }
            })
            .map_err(|err| err.to_string())?;
        let device_rate = ready_rx.recv().map_err(|_| "audio thread ended".to_string())??;
        Ok(Arc::new(DeviceSink {
            shared,
            source_rate: rate,
            device_rate,
            resampler: Mutex::new((0.0, 0.0)),
            done: Mutex::new(Some(done_tx)),
            thread: Mutex::new(Some(thread)),
        }))
    }

    fn build<T>(
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        channels: usize,
        shared: Arc<Shared>,
    ) -> Result<cpal::Stream, String>
    where
        T: SizedSample + FromSample<f32>,
    {
        device
            .build_output_stream(
                config,
                move |data: &mut [T], _| {
                    let mut queue = shared.queue.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                    for frame in data.chunks_mut(channels) {
                        let sample = if shared.stopped.load(Ordering::Relaxed) { 0.0 } else { queue.pop_front().unwrap_or(0.0) };
                        for slot in frame.iter_mut() {
                            *slot = T::from_sample(sample);
                        }
                    }
                },
                |err| eprintln!("voice: audio stream error: {err}"),
                None,
            )
            .map_err(|err| err.to_string())
    }

    /// #NEW-92: a call's stream on the default output device. #1281
    /// review: the tap is built for the DEVICE's rate (48 or 44.1 kHz, not
    /// the voice's), the callback only moves samples through the [`Pump`]'s
    /// lock-free rings, and the stream is rebuilt (the default device
    /// again, a new tap at its rate) after any stream error and when the
    /// default output device changes.
    pub fn open_stream(rate: u32, mut make_tap: super::MakeTap, _target: Option<String>) -> Result<Arc<dyn super::Stream>, String> {
        let queue = Arc::new(super::Queue::new(rate));
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let running = queue.clone();
        std::thread::Builder::new()
            .name("voice-call-audio".into())
            .spawn(move || {
                let mut device = match Device::open(rate, &mut make_tap) {
                    Ok(device) => device,
                    Err(err) => {
                        let _ = ready_tx.send(Err(err));
                        return;
                    }
                };
                let _ = ready_tx.send(Ok(()));
                loop {
                    match device.run(&running) {
                        Ended::Closed => return,
                        Ended::Rebuild(why) => {
                            eprintln!("voice: the call's audio output is rebuilt ({why})");
                            drop(device);
                            match reopen(rate, &mut make_tap) {
                                Ok(next) => device = next,
                                Err(err) => {
                                    running.fail(err);
                                    return;
                                }
                            }
                        }
                    }
                }
            })
            .map_err(|err| err.to_string())?;
        ready_rx.recv().map_err(|_| "audio thread ended".to_string())??;
        Ok(Arc::new(super::Handle::new(queue, rate)))
    }

    /// How often the stream's thread pumps and taps.
    const PUMP_EVERY: Duration = Duration::from_millis(5);
    /// How often it asks which output device is the default.
    const DEVICE_CHECK_EVERY: Duration = Duration::from_secs(1);
    /// A rebuild's attempts (a device that just changed may need a moment).
    const REBUILD_ATTEMPTS: u32 = 5;

    fn reopen(rate: u32, make_tap: &mut super::MakeTap) -> Result<Device, String> {
        let mut last = String::new();
        for _ in 0..REBUILD_ATTEMPTS {
            match Device::open(rate, make_tap) {
                Ok(device) => return Ok(device),
                Err(err) => last = err,
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        Err(last)
    }

    enum Ended {
        Closed,
        Rebuild(String),
    }

    /// One opened output device of a call's stream.
    struct Device {
        stream: cpal::Stream,
        name: Option<String>,
        pump: super::Pump,
        /// The stream's error callback fired.
        broken: Arc<Mutex<Option<String>>>,
    }

    impl Device {
        fn open(rate: u32, make_tap: &mut super::MakeTap) -> Result<Device, String> {
            let host = cpal::default_host();
            let device = host.default_output_device().ok_or("no audio output device")?;
            let supported = device.default_output_config().map_err(|err| err.to_string())?;
            let channels = supported.channels() as usize;
            let device_rate = supported.sample_rate().0;
            let pump = super::Pump::new(rate, device_rate, make_tap(device_rate));
            let broken = Arc::new(Mutex::new(None));
            let config: cpal::StreamConfig = supported.config();
            let stream = match supported.sample_format() {
                cpal::SampleFormat::F32 => build_pumped::<f32>(&device, &config, channels, &pump, broken.clone()),
                cpal::SampleFormat::I16 => build_pumped::<i16>(&device, &config, channels, &pump, broken.clone()),
                cpal::SampleFormat::U16 => build_pumped::<u16>(&device, &config, channels, &pump, broken.clone()),
                other => Err(format!("unsupported sample format {other:?}")),
            }?;
            stream.play().map_err(|err| err.to_string())?;
            Ok(Device { stream, name: device.name().ok(), pump, broken })
        }

        /// Pump until the queue closed and played out, or the device needs a
        /// rebuild.
        fn run(&mut self, queue: &super::Queue) -> Ended {
            let mut checked = Instant::now();
            loop {
                if queue.failed().is_some() {
                    return Ended::Closed;
                }
                self.pump.tap_played();
                if let Some(err) = self.broken.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).take() {
                    return Ended::Rebuild(err);
                }
                if checked.elapsed() >= DEVICE_CHECK_EVERY {
                    checked = Instant::now();
                    let default = cpal::default_host().default_output_device().and_then(|device| device.name().ok());
                    if default.is_some() && default != self.name {
                        return Ended::Rebuild("the default output device changed".into());
                    }
                }
                if !self.pump.fill(queue) {
                    // Closed: the device plays out its ring and its own buffer.
                    let deadline = Instant::now() + Duration::from_millis(500);
                    while self.pump.out.len() > 0 && Instant::now() < deadline {
                        std::thread::sleep(PUMP_EVERY);
                        self.pump.tap_played();
                    }
                    std::thread::sleep(Duration::from_millis(150));
                    let _ = self.stream.pause();
                    return Ended::Closed;
                }
                std::thread::sleep(PUMP_EVERY);
            }
        }
    }

    fn build_pumped<T>(
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        channels: usize,
        pump: &super::Pump,
        broken: Arc<Mutex<Option<String>>>,
    ) -> Result<cpal::Stream, String>
    where
        T: SizedSample + FromSample<f32>,
    {
        let (out, played, lost) = (pump.out.clone(), pump.played.clone(), pump.lost.clone());
        device
            .build_output_stream(
                config,
                // Real time: no lock, no allocation.
                move |data: &mut [T], _| {
                    for frame in data.chunks_mut(channels.max(1)) {
                        let sample = T::from_sample(super::Pump::device_sample(&out, &played, &lost));
                        for slot in frame.iter_mut() {
                            *slot = sample;
                        }
                    }
                },
                move |err| {
                    eprintln!("voice: audio stream error: {err}");
                    *broken.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(err.to_string());
                },
                None,
            )
            .map_err(|err| err.to_string())
    }

    impl DeviceSink {
        fn close(&self) {
            if let Some(done) = self.done.lock().ok().and_then(|mut guard| guard.take()) {
                let _ = done.send(());
            }
            if let Some(thread) = self.thread.lock().ok().and_then(|mut guard| guard.take()) {
                let _ = thread.join();
            }
        }
    }

    impl Sink for DeviceSink {
        fn push(&self, samples: &[f32]) -> Result<(), String> {
            if samples.is_empty() {
                return Ok(());
            }
            let step = self.source_rate as f64 / self.device_rate as f64;
            let mut state = self.resampler.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            let (mut position, mut previous) = *state;
            let mut out = Vec::with_capacity((samples.len() as f64 / step) as usize + 2);
            // `position` indexes into [previous, samples...]; 0.0 = previous.
            while position < samples.len() as f64 {
                let index = position.floor() as usize;
                let fraction = (position - index as f64) as f32;
                let a = if index == 0 { previous } else { samples[index - 1] };
                let b = samples[index.min(samples.len() - 1)];
                out.push(a + (b - a) * fraction);
                position += step;
            }
            position -= samples.len() as f64;
            previous = *samples.last().unwrap();
            *state = (position, previous);
            drop(state);
            if let Ok(mut queue) = self.shared.queue.lock() {
                queue.extend(out);
            }
            Ok(())
        }

        fn finish(&self) {
            // #1231 review: the drain is bounded by what is queued (at the
            // device's rate) plus a second; a stalled device is closed.
            let queued = self.shared.queue.lock().map(|queue| queue.len()).unwrap_or(0);
            let budget = Duration::from_secs_f64(queued as f64 / f64::from(self.device_rate.max(1))) + Duration::from_secs(1);
            let started = std::time::Instant::now();
            loop {
                let empty = self.shared.queue.lock().map(|queue| queue.is_empty()).unwrap_or(true);
                if empty || self.shared.stopped.load(Ordering::Relaxed) {
                    break;
                }
                if started.elapsed() >= budget {
                    eprintln!("voice: the audio device did not drain in time; closing");
                    self.close();
                    return;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            // The device's own buffer still holds the last few milliseconds.
            std::thread::sleep(Duration::from_millis(150));
            self.close();
        }

        fn stop(&self) {
            self.shared.stopped.store(true, Ordering::Relaxed);
            if let Ok(mut queue) = self.shared.queue.lock() {
                queue.clear();
            }
            self.close();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stream without a device: the test takes blocks itself.
    fn stream(rate: u32) -> (Arc<Queue>, Arc<dyn Stream>) {
        let queue = Arc::new(Queue::new(rate));
        (queue.clone(), Arc::new(Handle::new(queue, rate)))
    }

    /// #NEW-92: the device always gets a full block: the reply, then silence;
    /// the stream ends only once closed AND played out.
    #[test]
    fn a_stream_plays_silence_between_replies_and_plays_out_before_closing() {
        let (queue, stream) = stream(16_000);
        assert_eq!(queue.take(3), Some(vec![0.0; 3]), "silence before any reply");
        stream.push(&[0.5, 0.5]).unwrap();
        assert_eq!(queue.take(3), Some(vec![0.5, 0.5, 0.0]));
        stream.push(&[0.25; 4]).unwrap();
        drop(stream);
        assert_eq!(queue.take(3), Some(vec![0.25; 3]), "closing: what is queued still plays");
        assert_eq!(queue.take(3), Some(vec![0.25, 0.0, 0.0]));
        assert_eq!(queue.take(3), None, "closed and played out");
    }

    /// #NEW-92: a reply's stop drops what is queued (the stream stays open)
    /// and nothing it pushes afterwards plays; the next reply plays.
    #[test]
    fn a_stopped_reply_is_flushed_and_the_stream_goes_on() {
        let (queue, stream) = stream(16_000);
        let reply = Reply::new(stream.clone());
        reply.push(&[0.5; 100]).unwrap();
        reply.stop();
        reply.finish();
        reply.push(&[0.5; 100]).unwrap();
        assert_eq!(queue.take(4), Some(vec![0.0; 4]), "nothing of the stopped reply plays");
        let next = Reply::new(stream);
        next.push(&[0.75; 2]).unwrap();
        assert_eq!(queue.take(2), Some(vec![0.75; 2]));
    }

    /// #NEW-92: `finish` waits until the device took the reply.
    #[test]
    fn a_reply_finishes_once_the_device_took_it() {
        let (queue, stream) = stream(16_000);
        let reply = Reply::new(stream);
        reply.push(&[0.5; 1600]).unwrap();
        let device = std::thread::spawn(move || {
            for _ in 0..10 {
                std::thread::sleep(Duration::from_millis(5));
                queue.take(160);
            }
        });
        let started = Instant::now();
        reply.finish();
        assert!(started.elapsed() >= Duration::from_millis(40), "waited for the device");
        device.join().unwrap();
    }

    /// #NEW-92: a failed output fails the next push and never hangs a drain.
    #[test]
    fn a_failed_stream_refuses_audio() {
        let (queue, stream) = stream(16_000);
        stream.push(&[0.5; 16_000]).unwrap();
        queue.fail("gone".into());
        stream.drain();
        assert_eq!(stream.failed().as_deref(), Some("gone"));
        assert_eq!(stream.push(&[0.5]), Err("gone".into()));
    }

    /// #NEW-92: pushes are resampled to the device's rate.
    #[test]
    fn a_stream_resamples_to_the_device_rate() {
        let queue = Arc::new(Queue::new(48_000));
        let stream = Handle::new(queue.clone(), 24_000);
        stream.push(&[0.5; 240]).unwrap();
        let queued = queue.lock().len();
        assert!((478..=481).contains(&queued), "{queued}");
    }

    /// #1281 review: a device at 48 or 44.1 kHz under a 24 kHz voice: the
    /// tap is built for the device's rate, so one second the device played
    /// is one second of 16 kHz reference, the reply's tone at its pitch
    /// (a tap built for the voice's rate stretched it about 2×).
    #[test]
    fn the_reference_runs_at_the_device_rate() {
        use crate::echo::{Echo, FarEnd};
        const VOICE: u32 = 24_000;
        for device_rate in [48_000, 44_100] {
            let far = Arc::new(FarEnd::default());
            let _echo = Echo::new(far.clone());
            let queue = Arc::new(Queue::new(VOICE));
            let stream = Handle::new(queue.clone(), VOICE);
            let mut make_tap: MakeTap = Box::new({
                let far = far.clone();
                move |rate| FarEnd::tap(&far, rate)
            });
            let mut pump = Pump::new(VOICE, device_rate, make_tap(device_rate));
            // One second of a 1 kHz tone, then the device runs 2 s in 10 ms
            // callbacks.
            let tone: Vec<f32> = (0..VOICE).map(|at| 0.5 * (std::f32::consts::TAU * 1000.0 * at as f32 / VOICE as f32).sin()).collect();
            stream.push(&tone).unwrap();
            let callback = device_rate as usize / 100;
            for _ in 0..200 {
                assert!(pump.fill(&queue));
                for _ in 0..callback {
                    Pump::device_sample(&pump.out, &pump.played, &pump.lost);
                }
                pump.tap_played();
            }
            let reference = far.take(2 * 16_000 + 2_000);
            let heard: Vec<f32> = reference.iter().copied().skip_while(|sample| sample.abs() < 1e-3).collect();
            let tail = reference.iter().rposition(|sample| sample.abs() >= 1e-3).unwrap() + 1;
            let lead = reference.len() - heard.len();
            let length = tail - lead;
            assert!(length.abs_diff(16_000) < 100, "{device_rate}: the tone is {length} reference samples");
            let crossings = heard[..length].windows(2).filter(|pair| (pair[0] < 0.0) != (pair[1] < 0.0)).count();
            assert!(crossings.abs_diff(2_000) < 20, "{device_rate}: {crossings} zero crossings (1 kHz = 2000)");
            // Nothing past the two device seconds.
            assert!(reference[2 * 16_000 + 100..].iter().all(|sample| *sample == 0.0), "{device_rate}");
        }
    }

    /// #1281 review: the callback's ring is lock-free and bounded: a full
    /// ring drops (and the pump taps the drop as silence, so the count holds).
    #[test]
    fn a_ring_holds_its_capacity_in_order() {
        let ring = Ring::new(3);
        assert!(ring.push(1.0) && ring.push(2.0) && ring.push(3.0));
        assert!(!ring.push(4.0), "full");
        assert_eq!((ring.pop(), ring.len()), (Some(1.0), 2));
        assert!(ring.push(4.0));
        assert_eq!([ring.pop(), ring.pop(), ring.pop(), ring.pop()], [Some(2.0), Some(3.0), Some(4.0), None]);
    }
}
