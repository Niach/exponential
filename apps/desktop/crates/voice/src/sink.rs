//! Where synthesized samples go: one [`Sink`] per utterance.
//!
//! Linux pipes raw f32 PCM into the session's sound server (`pw-play`, else
//! `paplay`): every PipeWire or PulseAudio desktop has one, and the build
//! needs no ALSA headers. macOS and Windows open the default output device
//! with cpal and resample the voice's rate to the device's.

use std::sync::Arc;

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
