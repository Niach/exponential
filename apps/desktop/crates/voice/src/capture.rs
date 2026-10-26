//! NEW-78: the microphone, mono 16 kHz, open only while a Listen runs.
//!
//! Linux reads raw f32 PCM from the session's sound server (`pw-record`,
//! else `parec`), resampled by PipeWire/PulseAudio itself, so the build
//! needs no ALSA headers (the playback side's pattern, [`crate::sink`]).
//! macOS and Windows open the default input device with cpal, downmix it
//! and resample to 16 kHz ([`Resampler`]).
//!
//! #NEW-54 (the #1238 review): the recorder hands its chunks to the listening
//! thread (which runs the VAD and the transcriber in turn) over a BOUNDED
//! channel of [`CAPACITY`] chunks; a stalled consumer drops the newest audio
//! ([`offer`]) instead of growing memory.

use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;

use talk::ListenFailure;

use crate::listen::RATE;

/// The chunks the capture channel holds (pw-record's 4 KiB reads are 64 ms;
/// cpal's callbacks 10-20 ms): a few seconds of slack before audio drops.
pub const CAPACITY: usize = 128;

/// Hand `samples` to the listening thread without ever blocking the
/// recorder: a full channel drops them (counted in `dropped`, logged on the
/// first of a run). `false` = the listener is gone (stop recording).
pub fn offer(tx: &std::sync::mpsc::SyncSender<Vec<f32>>, samples: Vec<f32>, dropped: &mut u64) -> bool {
    match tx.try_send(samples) {
        Ok(()) => {
            if *dropped > 0 {
                eprintln!("voice: the listener caught up after {dropped} dropped audio chunks");
                *dropped = 0;
            }
            true
        }
        Err(std::sync::mpsc::TrySendError::Full(_)) => {
            if *dropped == 0 {
                eprintln!("voice: the listener is behind; dropping audio");
            }
            *dropped += 1;
            true
        }
        Err(std::sync::mpsc::TrySendError::Disconnected(_)) => false,
    }
}

/// An open microphone. Dropping it closes the device.
pub struct Capture {
    samples: Receiver<Vec<f32>>,
    _closer: platform::Closer,
}

impl Capture {
    /// The next captured samples; `Disconnected` = the device went away.
    pub fn recv_timeout(&self, timeout: Duration) -> Result<Vec<f32>, RecvTimeoutError> {
        self.samples.recv_timeout(timeout)
    }
}

/// Open the default microphone.
pub fn open() -> Result<Capture, (ListenFailure, String)> {
    let (samples, closer) = platform::open()?;
    Ok(Capture { samples, _closer: closer })
}

/// A streaming linear resampler (mono). Speech sits well below the 8 kHz
/// Nyquist limit of 16 kHz, and the transcriber is robust to the little
/// aliasing a linear interpolator lets through.
#[derive(Debug, Clone)]
pub struct Resampler {
    /// Input samples per output sample.
    step: f64,
    /// The next output's position, in input samples after `previous`.
    position: f64,
    previous: Option<f32>,
}

impl Resampler {
    pub fn new(from: u32, to: u32) -> Self {
        Resampler { step: f64::from(from) / f64::from(to), position: 0.0, previous: None }
    }

    /// Resample the next `input` samples onto `out`.
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        if input.is_empty() {
            return;
        }
        if self.step == 1.0 {
            out.extend_from_slice(input);
            return;
        }
        let mut buffer = Vec::with_capacity(input.len() + 1);
        buffer.extend(self.previous);
        buffer.extend_from_slice(input);
        while self.position + 1.0 < buffer.len() as f64 {
            let index = self.position.floor() as usize;
            let fraction = (self.position - index as f64) as f32;
            out.push(buffer[index] * (1.0 - fraction) + buffer[index + 1] * fraction);
            self.position += self.step;
        }
        self.position -= (buffer.len() - 1) as f64;
        self.previous = buffer.last().copied();
    }
}

/// Read a WAV file (PCM 16-bit or float 32-bit, any rate and channels) as
/// mono 16 kHz samples: what the tests and the `listen` example feed in
/// place of the microphone.
pub fn read_wav(bytes: &[u8]) -> Result<Vec<f32>, String> {
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("not a RIFF/WAVE file".into());
    }
    let mut format: Option<(u16, u16, u32, u16)> = None;
    let mut offset = 12;
    while offset + 8 <= bytes.len() {
        let id = &bytes[offset..offset + 4];
        let size = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().expect("4 bytes")) as usize;
        let body = &bytes[offset + 8..(offset + 8 + size).min(bytes.len())];
        if id == b"fmt " && body.len() >= 16 {
            let word = |at: usize| u16::from_le_bytes([body[at], body[at + 1]]);
            let rate = u32::from_le_bytes(body[4..8].try_into().expect("4 bytes"));
            format = Some((word(0), word(2), rate, word(14)));
        } else if id == b"data" {
            let (kind, channels, rate, bits) = format.ok_or("data before fmt")?;
            let samples: Vec<f32> = match (kind, bits) {
                (1, 16) => body.chunks_exact(2).map(|pair| f32::from(i16::from_le_bytes([pair[0], pair[1]])) / 32768.0).collect(),
                (3, 32) => body.chunks_exact(4).map(|quad| f32::from_le_bytes(quad.try_into().expect("4 bytes"))).collect(),
                // WAVE_FORMAT_EXTENSIBLE: the subformat decides; 16 bit = PCM, 32 = float in practice.
                (0xFFFE, 16) => body.chunks_exact(2).map(|pair| f32::from(i16::from_le_bytes([pair[0], pair[1]])) / 32768.0).collect(),
                (0xFFFE, 32) => body.chunks_exact(4).map(|quad| f32::from_le_bytes(quad.try_into().expect("4 bytes"))).collect(),
                _ => return Err(format!("unsupported WAV format {kind} at {bits} bit")),
            };
            let channels = usize::from(channels.max(1));
            let mono: Vec<f32> = samples.chunks(channels).map(|frame| frame.iter().sum::<f32>() / channels as f32).collect();
            let mut out = Vec::new();
            Resampler::new(rate, RATE as u32).process(&mono, &mut out);
            return Ok(out);
        }
        offset += 8 + size + (size & 1);
    }
    Err("no data chunk".into())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod platform {
    use std::io::Read;
    use std::process::{Child, Command, Stdio};
    use std::sync::mpsc::{self, Receiver};

    use talk::ListenFailure;

    /// Kills the recorder when dropped (its reader then sees EOF).
    pub struct Closer(Child);

    impl Drop for Closer {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    pub fn open() -> Result<(Receiver<Vec<f32>>, Closer), (ListenFailure, String)> {
        let rate = crate::listen::RATE.to_string();
        let candidates: [(&str, Vec<String>); 2] = [
            (
                "pw-record",
                vec![
                    "--rate".into(),
                    rate.clone(),
                    "--channels".into(),
                    "1".into(),
                    "--format".into(),
                    "f32".into(),
                    // Small node buffers: a Level every 32 ms needs them.
                    "--latency".into(),
                    "20ms".into(),
                    "-".into(),
                ],
            ),
            (
                "parec",
                vec![
                    "--raw".into(),
                    "--format=float32le".into(),
                    format!("--rate={rate}"),
                    "--channels=1".into(),
                    "--latency-msec=20".into(),
                ],
            ),
        ];
        let mut errors = Vec::new();
        for (program, args) in candidates {
            // Absolute, from the system dirs: never a binary the PATH plants.
            let Some(path) = talk::system_binary(program) else {
                errors.push(format!("{program}: not in {}", talk::SYSTEM_BIN_DIRS.join(", ")));
                continue;
            };
            match Command::new(path).args(&args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn() {
                Ok(mut child) => {
                    let mut stdout = child.stdout.take().expect("piped");
                    let (tx, rx) = mpsc::sync_channel(super::CAPACITY);
                    let spawned = std::thread::Builder::new().name("voice-capture".into()).spawn(move || {
                        let mut buffer = vec![0u8; 4096];
                        let mut carry: Vec<u8> = Vec::new();
                        let mut dropped = 0;
                        loop {
                            match stdout.read(&mut buffer) {
                                Ok(0) | Err(_) => break,
                                Ok(read) => {
                                    carry.extend_from_slice(&buffer[..read]);
                                    let whole = carry.len() / 4 * 4;
                                    let samples: Vec<f32> = carry[..whole]
                                        .chunks_exact(4)
                                        .map(|quad| f32::from_le_bytes(quad.try_into().expect("4 bytes")))
                                        .collect();
                                    carry.drain(..whole);
                                    if !super::offer(&tx, samples, &mut dropped) {
                                        break;
                                    }
                                }
                            }
                        }
                    });
                    if let Err(err) = spawned {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err((ListenFailure::NoMicrophone, err.to_string()));
                    }
                    return Ok((rx, Closer(child)));
                }
                Err(err) => errors.push(format!("{program}: {err}")),
            }
        }
        Err((ListenFailure::NoMicrophone, format!("no recorder (needs PipeWire or PulseAudio): {}", errors.join("; "))))
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod platform {
    use std::sync::mpsc::{self, Receiver, Sender, SyncSender};

    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use cpal::{FromSample, SizedSample};
    use talk::ListenFailure;

    use super::Resampler;

    /// Ends the audio thread (which owns the stream: cpal's `Stream` is
    /// not `Send` on every backend) and waits for it.
    pub struct Closer {
        done: Option<Sender<()>>,
        thread: Option<std::thread::JoinHandle<()>>,
    }

    impl Drop for Closer {
        fn drop(&mut self) {
            drop(self.done.take());
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }

    pub fn open() -> Result<(Receiver<Vec<f32>>, Closer), (ListenFailure, String)> {
        let (tx, rx) = mpsc::sync_channel::<Vec<f32>>(super::CAPACITY);
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), (ListenFailure, String)>>();
        let (done_tx, done_rx) = mpsc::channel::<()>();
        let thread = std::thread::Builder::new()
            .name("voice-capture".into())
            .spawn(move || {
                let result = (|| {
                    let host = cpal::default_host();
                    let device =
                        host.default_input_device().ok_or((ListenFailure::NoMicrophone, "no audio input device".to_string()))?;
                    let supported = device.default_input_config().map_err(|err| (ListenFailure::NoMicrophone, err.to_string()))?;
                    let channels = usize::from(supported.channels());
                    let rate = supported.sample_rate().0;
                    let config: cpal::StreamConfig = supported.config();
                    let stream = match supported.sample_format() {
                        cpal::SampleFormat::F32 => build::<f32>(&device, &config, channels, rate, tx.clone()),
                        cpal::SampleFormat::I16 => build::<i16>(&device, &config, channels, rate, tx.clone()),
                        cpal::SampleFormat::U16 => build::<u16>(&device, &config, channels, rate, tx.clone()),
                        other => Err(format!("unsupported sample format {other:?}")),
                    }
                    .map_err(|err| (ListenFailure::NoMicrophone, err))?;
                    stream.play().map_err(|err| (ListenFailure::NoMicrophone, err.to_string()))?;
                    Ok::<_, (ListenFailure, String)>(stream)
                })();
                match result {
                    Ok(stream) => {
                        let _ = ready_tx.send(Ok(()));
                        let _ = done_rx.recv();
                        drop(stream);
                    }
                    Err(err) => {
                        let _ = ready_tx.send(Err(err));
                    }
                }
            })
            .map_err(|err| (ListenFailure::NoMicrophone, err.to_string()))?;
        ready_rx.recv().map_err(|_| (ListenFailure::NoMicrophone, "audio thread ended".to_string()))??;
        Ok((rx, Closer { done: Some(done_tx), thread: Some(thread) }))
    }

    fn build<T>(
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        channels: usize,
        rate: u32,
        tx: SyncSender<Vec<f32>>,
    ) -> Result<cpal::Stream, String>
    where
        T: SizedSample,
        f32: FromSample<T>,
    {
        let mut resampler = Resampler::new(rate, crate::listen::RATE as u32);
        let mut dropped = 0;
        device
            .build_input_stream(
                config,
                move |data: &[T], _| {
                    let mono: Vec<f32> = data
                        .chunks(channels.max(1))
                        .map(|frame| frame.iter().map(|sample| sample.to_sample::<f32>()).sum::<f32>() / channels.max(1) as f32)
                        .collect();
                    let mut out = Vec::with_capacity(mono.len());
                    resampler.process(&mono, &mut out);
                    // Never blocks the audio callback.
                    super::offer(&tx, out, &mut dropped);
                },
                |err| eprintln!("voice: microphone stream error: {err}"),
                None,
            )
            .map_err(|err| err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_resampler_keeps_time_across_chunks() {
        let input: Vec<f32> = (0..48_000).map(|index| (index as f32 / 48.0).sin()).collect();
        let mut whole = Vec::new();
        Resampler::new(48_000, 16_000).process(&input, &mut whole);
        let mut chunked = Vec::new();
        let mut resampler = Resampler::new(48_000, 16_000);
        for chunk in input.chunks(441) {
            resampler.process(chunk, &mut chunked);
        }
        assert!((whole.len() as i64 - 16_000).abs() <= 1, "{}", whole.len());
        assert_eq!(whole.len(), chunked.len());
        assert!(whole.iter().zip(&chunked).all(|(a, b)| (a - b).abs() < 1e-5));
        // 24 kHz → 16 kHz lands between source samples and interpolates.
        let mut out = Vec::new();
        Resampler::new(24_000, 16_000).process(&[0.0, 3.0, 6.0, 9.0], &mut out);
        assert_eq!(out, [0.0, 4.5]);
    }

    #[test]
    fn a_builtin_clip_reads_as_mono_16k() {
        let samples = read_wav(crate::builtin::for_language("en").wav).unwrap();
        // 9.12 s at 24 kHz.
        assert!((samples.len() as f32 / RATE as f32 - 9.12).abs() < 0.05, "{}", samples.len());
        assert!(samples.iter().any(|sample| sample.abs() > 0.1));
        assert!(read_wav(b"not a wav").is_err());
    }

    #[test]
    fn a_stalled_listener_drops_audio_instead_of_growing() {
        let (tx, rx) = std::sync::mpsc::sync_channel(2);
        let mut dropped = 0;
        for chunk in 0..5 {
            assert!(offer(&tx, vec![chunk as f32], &mut dropped));
        }
        assert_eq!(dropped, 3, "the channel holds two, the rest drop");
        assert_eq!(rx.try_iter().collect::<Vec<_>>(), [vec![0.0], vec![1.0]]);
        assert!(offer(&tx, vec![9.0], &mut dropped));
        assert_eq!(dropped, 0, "caught up");
        drop(rx);
        assert!(!offer(&tx, vec![1.0], &mut dropped), "a gone listener stops the recorder");
    }
}
