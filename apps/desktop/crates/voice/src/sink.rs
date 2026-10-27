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
//! echo stay one fixed delay apart for the whole call.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
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

/// Open the call's stream for mono audio at `rate` Hz; `tap` sees every block.
pub fn open_stream(rate: u32, tap: Tap) -> Result<Arc<dyn Stream>, String> {
    platform::open_stream(rate, tap, None)
}

/// [`open_stream`] on the sound server's node `target` (Linux; the
/// `echo_check` example's loopback). macOS and Windows play on the default
/// device regardless.
pub fn open_stream_on(rate: u32, tap: Tap, target: &str) -> Result<Arc<dyn Stream>, String> {
    platform::open_stream(rate, tap, Some(target.to_string()))
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
/// `rate` (the device's on cpal, the voice's on Linux).
struct Queue {
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
        if queue.is_empty() && self.closing.load(Ordering::SeqCst) {
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
    pub fn open_stream(rate: u32, mut tap: super::Tap, target: Option<String>) -> Result<Arc<dyn super::Stream>, String> {
        let queue = Arc::new(super::Queue::new(rate));
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
            let child = self.child.lock().ok().and_then(|mut guard| guard.take());
            if let Some(mut child) = child {
                let _ = child.wait();
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
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod platform {
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

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

    /// #NEW-92: a call's stream on the default output device; the device's
    /// callback taps every block it plays (at the device's rate, mono).
    pub fn open_stream(rate: u32, tap: super::Tap, _target: Option<String>) -> Result<Arc<dyn super::Stream>, String> {
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<Arc<super::Queue>, String>>();
        let tap = Arc::new(Mutex::new(tap));
        std::thread::Builder::new()
            .name("voice-call-audio".into())
            .spawn(move || {
                let result = (|| {
                    let host = cpal::default_host();
                    let device = host.default_output_device().ok_or("no audio output device")?;
                    let supported = device.default_output_config().map_err(|err| err.to_string())?;
                    let channels = supported.channels() as usize;
                    let queue = Arc::new(super::Queue::new(supported.sample_rate().0));
                    let config: cpal::StreamConfig = supported.config();
                    let stream = match supported.sample_format() {
                        cpal::SampleFormat::F32 => build_tapped::<f32>(&device, &config, channels, queue.clone(), tap.clone()),
                        cpal::SampleFormat::I16 => build_tapped::<i16>(&device, &config, channels, queue.clone(), tap.clone()),
                        cpal::SampleFormat::U16 => build_tapped::<u16>(&device, &config, channels, queue.clone(), tap.clone()),
                        other => Err(format!("unsupported sample format {other:?}")),
                    }?;
                    stream.play().map_err(|err| err.to_string())?;
                    Ok::<_, String>((stream, queue))
                })();
                match result {
                    Ok((stream, queue)) => {
                        let _ = ready_tx.send(Ok(queue.clone()));
                        // Open until the last handle drops and the queue played.
                        while !(queue.closing.load(Ordering::SeqCst) && queue.lock().is_empty()) && queue.failed().is_none() {
                            std::thread::sleep(Duration::from_millis(20));
                        }
                        // The device's own buffer still holds the last few milliseconds.
                        std::thread::sleep(Duration::from_millis(150));
                        drop(stream);
                    }
                    Err(err) => {
                        let _ = ready_tx.send(Err(err));
                    }
                }
            })
            .map_err(|err| err.to_string())?;
        let queue = ready_rx.recv().map_err(|_| "audio thread ended".to_string())??;
        Ok(Arc::new(super::Handle::new(queue, rate)))
    }

    fn build_tapped<T>(
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        channels: usize,
        queue: Arc<super::Queue>,
        tap: Arc<Mutex<super::Tap>>,
    ) -> Result<cpal::Stream, String>
    where
        T: SizedSample + FromSample<f32>,
    {
        let failing = queue.clone();
        device
            .build_output_stream(
                config,
                move |data: &mut [T], _| {
                    let frames = data.len() / channels.max(1);
                    let Some(block) = queue.take(frames) else { return data.iter_mut().for_each(|slot| *slot = T::from_sample(0.0)) };
                    for (frame, sample) in data.chunks_mut(channels).zip(&block) {
                        for slot in frame.iter_mut() {
                            *slot = T::from_sample(*sample);
                        }
                    }
                    if let Ok(mut tap) = tap.try_lock() {
                        (*tap)(&block);
                    }
                },
                move |err| {
                    eprintln!("voice: audio stream error: {err}");
                    if matches!(err, cpal::StreamError::DeviceNotAvailable) {
                        failing.fail(err.to_string());
                    }
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
}
