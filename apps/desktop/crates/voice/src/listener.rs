//! NEW-78: the worker's listening side: one thread that owns the
//! microphone while a Listen runs, the Silero VAD and Parakeet TDT 0.6B v3
//! through CrispASR ([`Engines`]), and turns `Listen` / `EndListen` /
//! `CancelListen` into Level, Partial, Heard, Empty and ListenFailed.
//!
//! Parakeet runs on the GPU when ggml finds one with [`MIN_GPU_BYTES`] free
//! (CUDA 10-30 ms per clip of up to 4 s on the 4090, so the open segment is
//! re-transcribed for partials), else on the CPU (110-540 ms per segment:
//! no partials, only the finished segments). The models (674 MB + 0.9 MB)
//! download on the first Listen, verified against their pinned sha256;
//! that Listen answers `ListenFailed { reason: downloading }`.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::time::{Duration, Instant};

use crispasr::{artifact, models, AsrSession, Build, Device, Runtime, VadStream};
use talk::{Event, ListenFailure};

use crate::capture::{self, Capture};
use crate::listen::{Transcriber, Utterance, Vad, RATE};
use crate::{emit, retry_after, runtime};

/// The CrispASR backend of the transcriber.
const BACKEND: &str = "parakeet";
/// A GPU needs this much free memory for Parakeet Q8_0 (measured 1,246 MiB).
pub const MIN_GPU_BYTES: u64 = 1_600_000_000;
/// A held key longer than this ends the Listen as if released.
pub const MAX_HOLD: Duration = Duration::from_secs(120);

/// The listening models, in `{data_dir}/talk/models/`.
const MODELS: [artifact::Artifact; 2] = [models::PARAKEET_TDT_0_6B_V3, models::SILERO_VAD_V6_2];

fn models_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("talk").join(models::MODELS_DIR)
}

/// The loaded transcriber and where the VAD model is.
pub struct Engines {
    runtime: &'static Runtime,
    asr: AsrSession<'static>,
    device: Device,
    vad_model: PathBuf,
}

impl Engines {
    /// Whether everything listening needs is on disk and verified.
    pub fn files_ready(data_dir: &Path) -> bool {
        let Some(build) = Build::for_this_machine() else { return false };
        let dir = models_dir(data_dir);
        (runtime::loaded().is_some() || runtime::is_ready(data_dir, build))
            && MODELS.iter().all(|model| artifact::is_ready(&dir, model))
    }

    /// Download and verify the runtime (when no runtime is loaded yet) and
    /// the models; `progress(received, total)` covers all of them.
    pub fn download(data_dir: &Path, mut progress: impl FnMut(u64, u64)) -> Result<(), String> {
        let build = Build::for_this_machine().ok_or("no speech runtime for this platform")?;
        let dir = models_dir(data_dir);
        let models_total: u64 = MODELS.iter().map(|model| model.size).sum();
        let runtime_total: u64 =
            if runtime::loaded().is_some() { 0 } else { build.artifacts().iter().map(|artifact| artifact.size).sum() };
        let total = runtime_total + models_total;
        runtime::get(data_dir, build, |received, _| progress(received, total))?;
        artifact::ensure_all(&dir, &MODELS, |received, _| progress(runtime_total + received, total)).map_err(|err| err.to_string())
    }

    /// Load the transcriber (the files must be ready): on the GPU when one
    /// has room and `cpu` is false, else on the CPU.
    pub fn load(data_dir: &Path, cpu: bool) -> Result<Engines, String> {
        let build = Build::for_this_machine().ok_or("no speech runtime for this platform")?;
        let runtime = runtime::get(data_dir, build, |_, _| {})?;
        let dir = models_dir(data_dir);
        for model in MODELS {
            artifact::verify(&dir, &model).map_err(|err| err.to_string())?;
        }
        let model = dir.join(models::PARAKEET_TDT_0_6B_V3.keep[0].1);
        let gpu = !cpu
            && runtime.build() == Build::Gpu
            && runtime.gpus().iter().any(|gpu| gpu.free_bytes >= MIN_GPU_BYTES);
        let started = Instant::now();
        let (asr, device) = match gpu.then(|| runtime.open_asr(BACKEND, &model, Device::Gpu)) {
            Some(Ok(asr)) => (asr, Device::Gpu),
            Some(Err(err)) => {
                eprintln!("speech model failed on the GPU, using the CPU: {err}");
                (runtime.open_asr(BACKEND, &model, Device::Cpu)?, Device::Cpu)
            }
            None => (runtime.open_asr(BACKEND, &model, Device::Cpu)?, Device::Cpu),
        };
        let mut engines = Engines { runtime, asr, device, vad_model: dir.join(models::SILERO_VAD_V6_2.keep[0].1) };
        // The first decode of a cold pipeline pays its setup; not the person.
        engines.asr.transcribe(&vec![0.0; RATE / 2], "en")?;
        eprintln!("speech model loaded on the {device:?} in {} ms", started.elapsed().as_millis());
        Ok(engines)
    }

    pub fn device(&self) -> Device {
        self.device
    }

    /// A fresh VAD stream (its recurrent state starts clean per Listen).
    pub fn vad(&self) -> Result<SileroVad, String> {
        Ok(SileroVad(self.runtime.open_vad(&self.vad_model)?))
    }
}

impl Transcriber for Engines {
    fn transcribe(&mut self, pcm: &[f32], language: &str) -> Result<String, String> {
        self.asr.transcribe(pcm, language)
    }

    fn partials(&self) -> bool {
        self.device == Device::Gpu
    }
}

/// Silero through CrispASR as the pipeline's [`Vad`].
pub struct SileroVad(VadStream<'static>);

impl Vad for SileroVad {
    fn push(&mut self, samples: &[f32]) -> Result<Vec<f32>, String> {
        self.0.push(samples)
    }
}

/// What the request reader hands the listening thread.
pub enum Job {
    Start { id: String, language: String },
    End { id: String },
    Cancel { id: String },
    /// The download thread finished.
    Downloaded(Result<(), String>),
}

/// The listen running now.
struct Active {
    utterance: Utterance,
    capture: Capture,
    vad: SileroVad,
    started: Instant,
}

enum Fetch {
    Idle { failures: u32, retry_at: Option<Instant> },
    Running,
}

/// Start the listening thread; it ends when the returned sender is dropped.
pub fn spawn(data_dir: PathBuf) -> (Sender<Job>, std::thread::JoinHandle<()>) {
    let (tx, rx) = mpsc::channel();
    let downloads = tx.clone();
    let thread = std::thread::Builder::new()
        .name("voice-listen".into())
        .spawn(move || listen_loop(&data_dir, &rx, &downloads))
        .expect("spawn the listening thread");
    (tx, thread)
}

/// The language Parakeet is told: `user_voice_settings.language`.
fn language_of(language: &str) -> &'static str {
    if language == "de" { "de" } else { "en" }
}

fn listen_loop(data_dir: &Path, rx: &Receiver<Job>, downloads: &Sender<Job>) {
    let mut engines: Option<Engines> = None;
    let mut fetch = Fetch::Idle { failures: 0, retry_at: None };
    let mut active: Option<Active> = None;
    loop {
        let job = if active.is_some() {
            match rx.try_recv() {
                Ok(job) => Some(job),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => break,
            }
        } else {
            match rx.recv() {
                Ok(job) => Some(job),
                Err(_) => break,
            }
        };
        if let Some(job) = job {
            match job {
                Job::Start { id, language } => {
                    // A new Listen replaces the running one (no answer for it).
                    drop(active.take());
                    active = start(data_dir, id, language_of(&language), &mut engines, &mut fetch, downloads);
                }
                Job::End { id } => {
                    if active.as_ref().is_some_and(|running| running.utterance.id() == id) {
                        let running = active.take().expect("checked");
                        finish(running, engines.as_mut().expect("a running Listen has engines"));
                    }
                }
                Job::Cancel { id } => {
                    if active.as_ref().is_some_and(|running| running.utterance.id() == id) {
                        active = None;
                    }
                }
                Job::Downloaded(result) => match result {
                    Ok(()) => {
                        fetch = Fetch::Idle { failures: 0, retry_at: None };
                        emit(&Event::ModelReady);
                    }
                    Err(err) => {
                        let failures = match fetch {
                            Fetch::Idle { failures, .. } => failures + 1,
                            Fetch::Running => 1,
                        };
                        let wait = retry_after(failures);
                        eprintln!("speech model unavailable (attempt {failures}, next in {}s): {err}", wait.as_secs());
                        emit(&Event::DownloadFailed { error: err });
                        fetch = Fetch::Idle { failures, retry_at: Some(Instant::now() + wait) };
                    }
                },
            }
            continue;
        }
        let Some(running) = active.as_mut() else { continue };
        let engines = engines.as_mut().expect("a running Listen has engines");
        match running.capture.recv_timeout(Duration::from_millis(20)) {
            Ok(samples) => {
                let mut sink = |event: Event| emit(&event);
                if let Err((reason, detail)) = running.utterance.feed(&samples, &mut running.vad, engines, &mut sink) {
                    let id = running.utterance.id().to_string();
                    active = None;
                    emit(&Event::ListenFailed { id, reason, detail });
                } else if running.started.elapsed() >= MAX_HOLD {
                    finish(active.take().expect("running"), engines);
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                let running = active.take().expect("running");
                if running.utterance.seconds() == 0.0 {
                    let id = running.utterance.id().to_string();
                    emit(&Event::ListenFailed { id, reason: ListenFailure::NoMicrophone, detail: "the microphone closed".into() });
                } else {
                    // The device went away mid-utterance: answer what was heard.
                    finish(running, engines);
                }
            }
        }
    }
}

/// Open the microphone first (its samples queue while the engines load),
/// then the engines and the VAD.
fn start(
    data_dir: &Path,
    id: String,
    language: &str,
    engines: &mut Option<Engines>,
    fetch: &mut Fetch,
    downloads: &Sender<Job>,
) -> Option<Active> {
    let fail = |reason: ListenFailure, detail: String| {
        emit(&Event::ListenFailed { id: id.clone(), reason, detail });
        None
    };
    if engines.is_none() && !Engines::files_ready(data_dir) {
        match fetch {
            Fetch::Running => {}
            Fetch::Idle { retry_at: Some(at), .. } if Instant::now() < *at => {
                return fail(ListenFailure::Unavailable, "the speech model failed to download; retrying later".into());
            }
            Fetch::Idle { .. } => {
                *fetch = Fetch::Running;
                let data_dir = data_dir.to_path_buf();
                let downloads = downloads.clone();
                let spawned = std::thread::Builder::new().name("voice-listen-download".into()).spawn(move || {
                    let mut last = Instant::now() - Duration::from_secs(1);
                    let result = Engines::download(&data_dir, |received, total| {
                        if last.elapsed() >= Duration::from_millis(250) || received == total {
                            last = Instant::now();
                            emit(&Event::Downloading { received, total });
                        }
                    });
                    let _ = downloads.send(Job::Downloaded(result));
                });
                if let Err(err) = spawned {
                    *fetch = Fetch::Idle { failures: 0, retry_at: None };
                    return fail(ListenFailure::Unavailable, err.to_string());
                }
            }
        }
        return fail(ListenFailure::Downloading, "the speech model is downloading".into());
    }
    if let Some(reason) = microphone_refused() {
        return fail(reason, "the microphone is not allowed".into());
    }
    let capture = match capture::open() {
        Ok(capture) => capture,
        Err((reason, detail)) => return fail(reason, detail),
    };
    if engines.is_none() {
        match Engines::load(data_dir, false) {
            Ok(loaded) => *engines = Some(loaded),
            Err(err) => {
                eprintln!("speech model failed to load: {err}");
                return fail(ListenFailure::Unavailable, err);
            }
        }
    }
    let vad = match engines.as_ref().expect("loaded above").vad() {
        Ok(vad) => vad,
        Err(err) => return fail(ListenFailure::Unavailable, err),
    };
    Some(Active { utterance: Utterance::new(id, language), capture, vad, started: Instant::now() })
}

/// The key was released: take what the microphone still holds, close it,
/// transcribe the rest.
fn finish(running: Active, engines: &mut Engines) {
    let Active { mut utterance, capture, mut vad, .. } = running;
    let mut sink = |event: Event| emit(&event);
    let mut result = Ok(());
    while let Ok(samples) = capture.recv_timeout(Duration::ZERO) {
        result = utterance.feed(&samples, &mut vad, engines, &mut sink);
        if result.is_err() {
            break;
        }
    }
    drop(capture);
    let id = utterance.id().to_string();
    if let Err((reason, detail)) = result.and_then(|()| utterance.finish(engines, &mut sink)) {
        emit(&Event::ListenFailed { id, reason, detail });
    }
}

/// macOS answers a refused microphone with silence, so ask first; the
/// first Listen (not yet decided) opens the device and macOS prompts.
fn microphone_refused() -> Option<ListenFailure> {
    talk::microphone::refused().then_some(ListenFailure::NotGranted)
}
