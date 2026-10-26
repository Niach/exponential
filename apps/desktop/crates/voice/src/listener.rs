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
//!
//! Transcripts never reach stderr (the host logs it at info): ggml and
//! whisper logs pass `crispasr`'s warnings-only filter, and Parakeet's own
//! `fprintf`s in v0.8.45 print counts, timings and paths only (checked in
//! its source and on the pinned library, GPU and CPU, 2026-10-26); its one
//! token dump sits behind `CRISPASR_PARAKEET_DEBUG`, which the host strips
//! from the worker's environment.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
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
}

/// The Listen the reader asked for and the listener has not ended yet. A
/// Speak that arrives while one is set is not played: the reply would
/// sound into the open microphone and be transcribed.
#[derive(Default)]
pub struct Listening(Mutex<Option<String>>);

impl Listening {
    /// The reader sent `Listen { id }`.
    pub fn begin(&self, id: &str) {
        *self.lock() = Some(id.to_string());
    }

    /// `id` ended (heard, empty, failed, cancelled or replaced); a newer
    /// Listen stays set.
    pub fn end(&self, id: &str) {
        let mut current = self.lock();
        if current.as_deref() == Some(id) {
            *current = None;
        }
    }

    pub fn active(&self) -> bool {
        self.lock().is_some()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<String>> {
        self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// The microphone as the listening loop reads it.
pub trait Mic {
    /// The next captured samples; `Disconnected` = the device went away.
    fn recv_timeout(&self, timeout: Duration) -> Result<Vec<f32>, RecvTimeoutError>;
}

impl Mic for Capture {
    fn recv_timeout(&self, timeout: Duration) -> Result<Vec<f32>, RecvTimeoutError> {
        Capture::recv_timeout(self, timeout)
    }
}

/// Everything a Listen needs besides the pipeline: [`Ears`] in the worker,
/// fakes in the tests.
pub trait Hearing {
    type Engines: Transcriber;
    type Vad: Vad;
    type Mic: Mic;
    /// Make the engines ready (start a download, load them); `Err` fails
    /// the Listen. Runs BEFORE the microphone opens.
    fn prepare(&mut self) -> Result<(), (ListenFailure, String)>;
    /// Whether a download runs (the loop wakes to see it end).
    fn downloading(&self) -> bool;
    /// Take a finished download's result.
    fn poll(&mut self);
    /// A fresh VAD, then the microphone (after `prepare` succeeded).
    fn open(&mut self) -> Result<(Self::Vad, Self::Mic), (ListenFailure, String)>;
    /// The engines (after `prepare` succeeded).
    fn engines(&mut self) -> &mut Self::Engines;
}

/// The listen running now.
struct Active<H: Hearing> {
    utterance: Utterance,
    mic: H::Mic,
    vad: H::Vad,
    started: Instant,
}

enum Fetch {
    Idle { failures: u32, retry_at: Option<Instant> },
    /// The download thread answers on its OWN channel, so the listening
    /// thread holds no sender to its request channel and sees EOF.
    Running(Receiver<Result<(), String>>),
}

/// The worker's [`Hearing`]: CrispASR's engines, the real microphone.
pub struct Ears {
    data_dir: PathBuf,
    engines: Option<Engines>,
    fetch: Fetch,
}

impl Ears {
    pub fn new(data_dir: PathBuf) -> Self {
        Ears { data_dir, engines: None, fetch: Fetch::Idle { failures: 0, retry_at: None } }
    }

    fn start_download(&mut self) -> Result<(), String> {
        let (done, result) = mpsc::channel();
        let data_dir = self.data_dir.clone();
        std::thread::Builder::new()
            .name("voice-listen-download".into())
            .spawn(move || {
                let mut last = Instant::now() - Duration::from_secs(1);
                let downloaded = Engines::download(&data_dir, |received, total| {
                    if last.elapsed() >= Duration::from_millis(250) || received == total {
                        last = Instant::now();
                        emit(&Event::Downloading { received, total });
                    }
                });
                let _ = done.send(downloaded);
            })
            .map_err(|err| err.to_string())?;
        self.fetch = Fetch::Running(result);
        Ok(())
    }

    fn downloaded(&mut self, result: Result<(), String>) {
        match result {
            Ok(()) => {
                self.fetch = Fetch::Idle { failures: 0, retry_at: None };
                emit(&Event::ModelReady);
            }
            Err(err) => {
                let failures = match self.fetch {
                    Fetch::Idle { failures, .. } => failures + 1,
                    Fetch::Running(_) => 1,
                };
                let wait = retry_after(failures);
                eprintln!("speech model unavailable (attempt {failures}, next in {}s): {err}", wait.as_secs());
                emit(&Event::DownloadFailed { error: err });
                self.fetch = Fetch::Idle { failures, retry_at: Some(Instant::now() + wait) };
            }
        }
    }
}

impl Hearing for Ears {
    type Engines = Engines;
    type Vad = SileroVad;
    type Mic = Capture;

    fn prepare(&mut self) -> Result<(), (ListenFailure, String)> {
        if self.engines.is_some() {
            return microphone_refused().map_or(Ok(()), |reason| Err((reason, "the microphone is not allowed".into())));
        }
        if !Engines::files_ready(&self.data_dir) {
            match self.fetch {
                Fetch::Running(_) => {}
                Fetch::Idle { retry_at: Some(at), .. } if Instant::now() < at => {
                    return Err((ListenFailure::Unavailable, "the speech model failed to download; retrying later".into()));
                }
                Fetch::Idle { .. } => {
                    self.start_download().map_err(|err| (ListenFailure::Unavailable, err))?;
                }
            }
            return Err((ListenFailure::Downloading, "the speech model is downloading".into()));
        }
        if let Some(reason) = microphone_refused() {
            return Err((reason, "the microphone is not allowed".into()));
        }
        // Loaded before the microphone opens: the first Listen never holds
        // it through the re-hash and the model load.
        match Engines::load(&self.data_dir, false) {
            Ok(loaded) => {
                self.engines = Some(loaded);
                Ok(())
            }
            Err(err) => {
                eprintln!("speech model failed to load: {err}");
                Err((ListenFailure::Unavailable, err))
            }
        }
    }

    fn downloading(&self) -> bool {
        matches!(self.fetch, Fetch::Running(_))
    }

    fn poll(&mut self) {
        let Fetch::Running(result) = &self.fetch else { return };
        match result.try_recv() {
            Ok(downloaded) => self.downloaded(downloaded),
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => self.downloaded(Err("the download stopped".into())),
        }
    }

    fn open(&mut self) -> Result<(SileroVad, Capture), (ListenFailure, String)> {
        let vad = self.engines().vad().map_err(|err| (ListenFailure::Unavailable, err))?;
        Ok((vad, capture::open()?))
    }

    fn engines(&mut self) -> &mut Engines {
        self.engines.as_mut().expect("prepared")
    }
}

/// Start the listening thread on the [`Hearing`] `make` builds there; it
/// ends when the returned sender is dropped (EOF), closing any microphone.
pub fn spawn<H: Hearing>(
    make: impl FnOnce() -> H + Send + 'static,
    listening: Arc<Listening>,
) -> (Sender<Job>, std::thread::JoinHandle<()>) {
    let (tx, rx) = mpsc::channel();
    let thread = std::thread::Builder::new()
        .name("voice-listen".into())
        .spawn(move || listen_loop(&rx, &mut make(), &listening))
        .expect("spawn the listening thread");
    (tx, thread)
}

/// The language Parakeet is told: `user_voice_settings.language`.
fn language_of(language: &str) -> &'static str {
    if language == "de" { "de" } else { "en" }
}

fn listen_loop<H: Hearing>(rx: &Receiver<Job>, hearing: &mut H, listening: &Listening) {
    let mut active: Option<Active<H>> = None;
    // Jobs read while the engines loaded, handled before new ones.
    let mut backlog: VecDeque<Job> = VecDeque::new();
    loop {
        hearing.poll();
        let job = if let Some(job) = backlog.pop_front() {
            Some(job)
        } else if active.is_some() {
            match rx.try_recv() {
                Ok(job) => Some(job),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => break,
            }
        } else if hearing.downloading() {
            match rx.recv_timeout(Duration::from_millis(250)) {
                Ok(job) => Some(job),
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => break,
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
                    if let Some(replaced) = active.take() {
                        listening.end(replaced.utterance.id());
                    }
                    active = start(hearing, rx, &mut backlog, id.clone(), language_of(&language));
                    if active.is_none() {
                        listening.end(&id);
                    }
                }
                Job::End { id } => {
                    if active.as_ref().is_some_and(|running| running.utterance.id() == id) {
                        let running = active.take().expect("checked");
                        finish(running, hearing.engines());
                    }
                    listening.end(&id);
                }
                Job::Cancel { id } => {
                    if active.as_ref().is_some_and(|running| running.utterance.id() == id) {
                        active = None;
                    }
                    listening.end(&id);
                }
            }
            continue;
        }
        let Some(running) = active.as_mut() else { continue };
        let engines = hearing.engines();
        match running.mic.recv_timeout(Duration::from_millis(20)) {
            Ok(samples) => {
                let mut sink = |event: Event| emit(&event);
                if let Err((reason, detail)) = running.utterance.feed(&samples, &mut running.vad, engines, &mut sink) {
                    let id = running.utterance.id().to_string();
                    active = None;
                    listening.end(&id);
                    emit(&Event::ListenFailed { id, reason, detail });
                } else if running.started.elapsed() >= MAX_HOLD {
                    let running = active.take().expect("running");
                    let id = running.utterance.id().to_string();
                    finish(running, engines);
                    listening.end(&id);
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                let running = active.take().expect("running");
                let id = running.utterance.id().to_string();
                if running.utterance.seconds() == 0.0 {
                    emit(&Event::ListenFailed {
                        id: id.clone(),
                        reason: ListenFailure::NoMicrophone,
                        detail: "the microphone closed".into(),
                    });
                } else {
                    // The device went away mid-utterance: answer what was heard.
                    finish(running, engines);
                }
                listening.end(&id);
            }
        }
    }
    // EOF: `active` drops here, closing the microphone.
}

/// The engines first (a download, a load), then the VAD and the microphone.
/// An End, Cancel or newer Listen that arrived while the engines loaded
/// wins before the microphone opens.
fn start<H: Hearing>(
    hearing: &mut H,
    rx: &Receiver<Job>,
    backlog: &mut VecDeque<Job>,
    id: String,
    language: &str,
) -> Option<Active<H>> {
    let fail = |reason: ListenFailure, detail: String| {
        emit(&Event::ListenFailed { id: id.clone(), reason, detail });
        None
    };
    if let Err((reason, detail)) = hearing.prepare() {
        return fail(reason, detail);
    }
    backlog.extend(rx.try_iter());
    let ends = |job: &Job| matches!(job, Job::End { id: other } | Job::Cancel { id: other } if *other == id);
    if let Some(index) = backlog.iter().position(ends) {
        if let Some(Job::End { .. }) = backlog.remove(index) {
            // Released before the microphone opened: nothing was heard.
            emit(&Event::Empty { id: id.clone() });
        }
        return None;
    }
    if backlog.iter().any(|job| matches!(job, Job::Start { .. })) {
        // Replaced before it began (no answer, as for a running one).
        return None;
    }
    let (vad, mic) = match hearing.open() {
        Ok(opened) => opened,
        Err((reason, detail)) => return fail(reason, detail),
    };
    Some(Active { utterance: Utterance::new(id, language), mic, vad, started: Instant::now() })
}

/// The key was released: take what the microphone still holds, close it,
/// transcribe the rest.
fn finish<H: Hearing>(running: Active<H>, engines: &mut H::Engines) {
    let Active { mut utterance, mic, mut vad, .. } = running;
    let mut sink = |event: Event| emit(&event);
    let mut result = Ok(());
    while let Ok(samples) = mic.recv_timeout(Duration::ZERO) {
        result = utterance.feed(&samples, &mut vad, engines, &mut sink);
        if result.is_err() {
            break;
        }
    }
    drop(mic);
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

/// A [`Hearing`] without models or a device, for the loop's tests.
#[cfg(test)]
pub(crate) mod fake {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use talk::ListenFailure;

    use super::{Hearing, Mic};
    use crate::listen::{Transcriber, Vad};

    pub struct Silent;

    impl Vad for Silent {
        fn push(&mut self, _samples: &[f32]) -> Result<Vec<f32>, String> {
            Ok(Vec::new())
        }
    }

    impl Transcriber for Silent {
        fn transcribe(&mut self, _pcm: &[f32], _language: &str) -> Result<String, String> {
            Ok(String::new())
        }

        fn partials(&self) -> bool {
            false
        }
    }

    /// A microphone that stays open (the test holds its sender) and
    /// records when it closes.
    pub struct FakeMic {
        samples: Receiver<Vec<f32>>,
        _open: Sender<Vec<f32>>,
        closed: Arc<AtomicBool>,
    }

    impl Mic for FakeMic {
        fn recv_timeout(&self, timeout: Duration) -> Result<Vec<f32>, RecvTimeoutError> {
            self.samples.recv_timeout(timeout)
        }
    }

    impl Drop for FakeMic {
        fn drop(&mut self) {
            self.closed.store(true, Ordering::SeqCst);
        }
    }

    /// What the test sees: each opened microphone's closed flag, and the
    /// gate `prepare` waits on (a slow engine load).
    #[derive(Clone)]
    pub struct Probe {
        pub opened: Arc<Mutex<Vec<Arc<AtomicBool>>>>,
        pub opened_tx: Sender<()>,
        pub loads: Arc<Mutex<Option<Receiver<()>>>>,
    }

    impl Probe {
        /// A probe whose engine load waits for `release` when `slow`.
        pub fn new(slow: bool) -> (Probe, Receiver<()>, Option<Sender<()>>) {
            let (opened_tx, opened) = mpsc::channel();
            let (release, loads) = mpsc::channel();
            let probe = Probe {
                opened: Arc::default(),
                opened_tx,
                loads: Arc::new(Mutex::new(slow.then_some(loads))),
            };
            (probe, opened, slow.then_some(release))
        }

        pub fn hearing(&self) -> FakeHearing {
            FakeHearing { probe: self.clone(), engines: Silent }
        }
    }

    pub struct FakeHearing {
        probe: Probe,
        engines: Silent,
    }

    impl Hearing for FakeHearing {
        type Engines = Silent;
        type Vad = Silent;
        type Mic = FakeMic;

        fn prepare(&mut self) -> Result<(), (ListenFailure, String)> {
            if let Some(loads) = self.probe.loads.lock().unwrap().take() {
                let _ = loads.recv();
            }
            Ok(())
        }

        fn downloading(&self) -> bool {
            false
        }

        fn poll(&mut self) {}

        fn open(&mut self) -> Result<(Silent, FakeMic), (ListenFailure, String)> {
            let (open, samples) = mpsc::channel();
            let closed = Arc::new(AtomicBool::new(false));
            self.probe.opened.lock().unwrap().push(closed.clone());
            let _ = self.probe.opened_tx.send(());
            Ok((Silent, FakeMic { samples, _open: open, closed }))
        }

        fn engines(&mut self) -> &mut Silent {
            &mut self.engines
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::fake::Probe;
    use super::*;

    #[test]
    fn the_listening_gate_ends_only_its_own_listen() {
        let listening = Listening::default();
        assert!(!listening.active());
        listening.begin("u1");
        listening.begin("u2");
        listening.end("u1");
        assert!(listening.active(), "u2 replaced u1 and still runs");
        listening.end("u2");
        assert!(!listening.active());
    }

    #[test]
    fn a_release_during_the_engine_load_never_opens_the_microphone() {
        let (probe, _opened, release) = Probe::new(true);
        let listening = Arc::new(Listening::default());
        let hearing = probe.hearing();
        let (tx, thread) = spawn(move || hearing, listening.clone());
        listening.begin("u1");
        tx.send(Job::Start { id: "u1".into(), language: "en".into() }).unwrap();
        tx.send(Job::End { id: "u1".into() }).unwrap();
        release.unwrap().send(()).unwrap();
        drop(tx);
        thread.join().unwrap();
        assert!(probe.opened.lock().unwrap().is_empty(), "the microphone never opened");
        assert!(!listening.active());
    }
}
