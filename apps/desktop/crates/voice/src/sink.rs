//! Where synthesized samples go: one [`Sink`] per utterance.
//!
//! Linux pipes raw f32 PCM into the session's sound server (`pw-play`, else
//! `paplay`): every PipeWire or PulseAudio desktop has one, and the build
//! needs no ALSA headers. macOS and Windows open the default output device
//! with cpal and resample the voice's rate to the device's.

use std::sync::Arc;

/// One utterance's audio path.
pub trait Sink: Send + Sync {
    /// Queue samples (mono, at the rate the sink was opened with).
    fn push(&self, samples: &[f32]);
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

    use super::Sink;

    /// The player's stdin and the player itself sit behind SEPARATE locks:
    /// a write blocked on a full pipe holds only `stdin`, so `stop` can
    /// still take `child` and kill it at once (the blocked write then fails
    /// with a broken pipe and returns).
    pub struct PipeSink {
        stdin: Mutex<Option<ChildStdin>>,
        child: Mutex<Option<Child>>,
        stopped: AtomicBool,
    }

    /// Samples per write: ~85 ms at 24 kHz, so a stop lands between chunks.
    const CHUNK: usize = 2048;

    pub fn open(rate: u32) -> Result<Arc<dyn Sink>, String> {
        let candidates: [(&str, Vec<String>); 2] = [
            (
                "pw-play",
                vec!["--format".into(), "f32".into(), "--rate".into(), rate.to_string(), "--channels".into(), "1".into(), "-".into()],
            ),
            (
                "paplay",
                vec!["--raw".into(), "--format=float32le".into(), format!("--rate={rate}"), "--channels=1".into()],
            ),
        ];
        let mut errors = Vec::new();
        for (program, args) in candidates {
            match Command::new(program)
                .args(&args)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                Ok(mut child) => {
                    let stdin = child.stdin.take();
                    return Ok(Arc::new(PipeSink {
                        stdin: Mutex::new(stdin),
                        child: Mutex::new(Some(child)),
                        stopped: AtomicBool::new(false),
                    }));
                }
                Err(err) => errors.push(format!("{program}: {err}")),
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
    }

    impl Sink for PipeSink {
        fn push(&self, samples: &[f32]) {
            for chunk in samples.chunks(CHUNK) {
                if self.stopped.load(Ordering::SeqCst) {
                    return;
                }
                let bytes: Vec<u8> = chunk.iter().flat_map(|sample| sample.to_le_bytes()).collect();
                let Ok(mut guard) = self.stdin.lock() else { return };
                let failed = guard.as_mut().is_none_or(|stdin| stdin.write_all(&bytes).is_err());
                if failed {
                    guard.take();
                    drop(guard);
                    self.kill();
                    return;
                }
            }
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
        fn push(&self, samples: &[f32]) {
            if samples.is_empty() {
                return;
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
        }

        fn finish(&self) {
            loop {
                let empty = self.shared.queue.lock().map(|queue| queue.is_empty()).unwrap_or(true);
                if empty || self.shared.stopped.load(Ordering::Relaxed) {
                    break;
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
