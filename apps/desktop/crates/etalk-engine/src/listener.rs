//! NEW-78: the worker's listening side: one thread that owns the
//! microphone while a Listen runs, the Silero VAD and Parakeet TDT 0.6B v3
//! through CrispASR ([`Engines`]), and turns `Listen` / `EndListen` /
//! `CancelListen` into Level, Partial, Heard, Empty and ListenFailed.
//!
//! #NEW-56: `Wake` keeps the microphone open between Listens for the wake
//! word ([`crate::wake`]): Silero scores it, the start of each speech
//! segment goes once through the resident Parakeet, and a match starts a
//! turn the thread ends by itself (Woke, then Level, Partial, Heard or
//! Empty). A Listen borrows the open microphone; the wake word resumes after
//! every turn, Listen and call. A call owns the microphone until Hangup: it
//! closes the wake word's, the scan pauses for the whole call, and a Listen
//! during it is refused (Empty) (talk-listen.json `micOwnerCases`).
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
//!
//! NEW-55: the same thread runs CALLS (`voice-call.json`): a [`Job::Call`]
//! listens continuously through [`Call`] (Said per utterance, SpeechStarted
//! at barge-in, which first runs [`Links::barge`] to stop the reply), from
//! the microphone through the echo canceller ([`crate::echo`]) or, for a
//! remote call, from [`Job::Audio`] only. Mute drops the audio; Hangup ends
//! it without an answer.
//!
//! S-3: a call WARMS UP while it rings: it is pending until the engines are
//! up (a download included: the call waits for it, `Downloading` keeps
//! coming, and only a failed download or load fails it, `unavailable`),
//! then its microphone (local) or stream intake (remote) opens, and once
//! the voice is settled too ([`Links::voice_ready`]) `CallListening` goes
//! out. Nothing is heard before it (the canceller keeps pace, nothing is
//! metered or transcribed).
//!
//! NEW-57: the same thread runs a MEETING ([`crate::meeting`]): it owns the
//! microphone (channel `you`) and the desktop audio [`Hearing::apps`] opens,
//! once no Listen or Call runs, until MeetingStop; the wake word's scan
//! pauses for it (the `you` channel carries the phrase) and comes back
//! after. The reader answers a Listen during it with MeetingFlushNow and
//! Empty, and refuses a Call ([`Listening::meeting`]).

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use etalk_crispasr::{artifact, models, AsrSession, Build, Device, Runtime, VadStream};
use etalk::{Event, ListenFailure};

use crate::capture::{self, Capture};
use crate::echo::{Echo, FarEnd};
use crate::listen::{Call, Transcriber, Utterance, Vad, RATE};
use crate::meeting::Meeting;
use crate::wake::{turn_ended, WakeScanner};
use crate::{emit, retry_after, runtime};

/// The CrispASR backend of the transcriber.
const BACKEND: &str = "parakeet";
/// A GPU needs this much free memory for Parakeet Q8_0 (measured 1,246 MiB).
pub const MIN_GPU_BYTES: u64 = 1_600_000_000;
/// A held key longer than this ends the Listen as if released.
pub const MAX_HOLD: Duration = Duration::from_secs(120);
/// #NEW-56: how long the wake word waits before opening the microphone
/// again after it failed (no device, the model still downloading).
pub const WAKE_RETRY: Duration = Duration::from_secs(5);

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
        // #NEW-80: under the runtime's download lock: the speaker never
        // fetches into the same dir at once.
        runtime::fetching(|| artifact::ensure_all(&dir, &MODELS, |received, _| progress(runtime_total + received, total)))
            .map_err(|err| err.to_string())
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
    /// NEW-55: start call `id` (replaces a Listen or Call).
    Call { id: String, language: String, remote: bool },
    /// NEW-55: remote call `id`'s audio, mono 16 kHz.
    Audio { id: String, samples: Vec<f32> },
    Mute { id: String, muted: bool },
    Hangup { id: String },
    /// #NEW-56: listen for `phrase` between Listens (empty = off).
    Wake { phrase: String, language: String },
    /// NEW-57: hold meeting `id` (replaces a running one); `selection` =
    /// the desktop audio (None = the microphone only).
    Meeting { id: String, language: String, phrase: String, you_label: String, selection: Option<etalk::apps::Selection> },
    /// `drain`: the open segments are emitted first, then MeetingStopped.
    MeetingStop { id: String, drain: bool },
    MeetingPause { id: String, paused: bool },
}

impl Job {
    /// A Listen or a Call: it replaces whatever runs.
    fn starts(&self) -> bool {
        matches!(self, Job::Start { .. } | Job::Call { .. })
    }
}

/// NEW-55: what the reader asked the listener for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Push to talk: replies are refused while it runs.
    Listen,
    /// A call: replies play (barge-in stops them); `remote` ones stream.
    Call { remote: bool },
}

/// The Listen or Call the reader asked for and the listener has not ended
/// yet. A Speak that arrives while a LISTEN is set is not played: the reply
/// would sound into the open microphone and be transcribed (NEW-55: a call
/// plays it, its echo cancelled or, remote, streamed to the caller).
#[derive(Default)]
pub struct Listening(Mutex<Option<(String, Mode)>>, Mutex<Option<String>>);

impl Listening {
    /// The reader sent `Listen { id }` (or #NEW-56 the wake word woke):
    /// false while a call runs, which owns the microphone until Hangup
    /// (talk-listen.json `micOwnerCases`).
    #[must_use]
    pub fn begin(&self, id: &str) -> bool {
        // NEW-57: nor while a meeting holds the microphone.
        if self.meeting().is_some() {
            return false;
        }
        let mut current = self.lock();
        if matches!(*current, Some((_, Mode::Call { .. }))) {
            return false;
        }
        *current = Some((id.to_string(), Mode::Listen));
        true
    }

    /// NEW-55: the reader sent `Call { id, remote }`.
    pub fn begin_call(&self, id: &str, remote: bool) {
        *self.lock() = Some((id.to_string(), Mode::Call { remote }));
    }

    /// `id` ended (heard, empty, failed, cancelled or replaced); a newer
    /// Listen stays set.
    pub fn end(&self, id: &str) {
        let mut current = self.lock();
        if current.as_ref().is_some_and(|(current, _)| current == id) {
            *current = None;
        }
    }

    pub fn active(&self) -> bool {
        self.lock().is_some()
    }

    /// Push to talk runs: a Speak is answered unplayed.
    pub fn refuses_speech(&self) -> bool {
        matches!(*self.lock(), Some((_, Mode::Listen)))
    }

    /// NEW-55: a call runs (it owns the microphone until Hangup).
    pub fn in_call(&self) -> bool {
        matches!(*self.lock(), Some((_, Mode::Call { .. })))
    }

    /// #NEW-92: the id of the LOCAL call that runs, if one does.
    pub fn local_call(&self) -> Option<String> {
        match &*self.lock() {
            Some((id, Mode::Call { remote: false })) => Some(id.clone()),
            _ => None,
        }
    }

    /// NEW-55: a remote call runs: replies stream as SpeechAudio.
    pub fn remote_call(&self) -> bool {
        matches!(*self.lock(), Some((_, Mode::Call { remote: true })))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<(String, Mode)>> {
        self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// NEW-57: the reader sent `Meeting { id }` (it replaces any other).
    pub fn begin_meeting(&self, id: &str) {
        *self.meeting_lock() = Some(id.to_string());
    }

    /// NEW-57: meeting `id` stopped (a newer one stays).
    pub fn end_meeting(&self, id: &str) {
        let mut meeting = self.meeting_lock();
        if meeting.as_deref() == Some(id) {
            *meeting = None;
        }
    }

    /// NEW-57: the meeting asked for (it owns the microphone): push to
    /// talk becomes its flush trigger, a call is refused.
    pub fn meeting(&self) -> Option<String> {
        self.meeting_lock().clone()
    }

    fn meeting_lock(&self) -> std::sync::MutexGuard<'_, Option<String>> {
        self.1.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
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

/// NEW-57: the desktop audio as the meeting reads it.
pub trait AppAudio {
    /// The next chunk of any listened app; `Disconnected` = the capture ended.
    fn recv_timeout(&self, timeout: Duration) -> Result<etalk::apps::Chunk, RecvTimeoutError>;
}

impl AppAudio for etalk::apps::AppCapture {
    fn recv_timeout(&self, timeout: Duration) -> Result<etalk::apps::Chunk, RecvTimeoutError> {
        etalk::apps::AppCapture::recv_timeout(self, timeout)
    }
}

/// Everything a Listen needs besides the pipeline: [`Ears`] in the worker,
/// fakes in the tests.
pub trait Hearing {
    type Engines: Transcriber;
    type Vad: Vad;
    type Mic: Mic;
    /// Make the engines ready (start a download, load them); `Err` fails
    /// the Listen. Runs BEFORE the microphone opens; `microphone` = it will
    /// open (NEW-55: a remote call never does, so needs no permission).
    fn prepare(&mut self, microphone: bool) -> Result<(), (ListenFailure, String)>;
    /// Whether a download runs (the loop wakes to see it end).
    fn downloading(&self) -> bool;
    /// Take a finished download's result.
    fn poll(&mut self);
    /// A fresh VAD (after `prepare` succeeded).
    fn vad(&mut self) -> Result<Self::Vad, (ListenFailure, String)>;
    /// Open the microphone (after `prepare` succeeded).
    fn mic(&mut self) -> Result<Self::Mic, (ListenFailure, String)>;
    /// The engines (after `prepare` succeeded).
    fn engines(&mut self) -> &mut Self::Engines;
    /// NEW-57: start capturing `selection`'s desktop audio.
    fn apps(&mut self, selection: etalk::apps::Selection) -> Result<Box<dyn AppAudio>, String> {
        etalk::apps::open(selection).map(|capture| Box::new(capture) as Box<dyn AppAudio>)
    }
    /// NEW-57: where the debug ring is written (None = nowhere).
    fn debug_dir(&self) -> Option<PathBuf> {
        None
    }
}

/// NEW-57: a meeting as the host asked for it.
#[derive(Clone, PartialEq, Eq)]
struct MeetingWanted {
    id: String,
    language: &'static str,
    phrase: String,
    you_label: String,
    selection: Option<etalk::apps::Selection>,
    paused: bool,
}

/// NEW-57: a meeting that holds the microphone (and the desktop audio).
struct MeetingRun<H: Hearing> {
    meeting: Meeting<H::Vad>,
    you_label: String,
    mic: H::Mic,
    apps: Option<Box<dyn AppAudio>>,
}

/// NEW-57: what the host was last told about the meeting's microphone.
type MeetingReport = Option<(String, bool, Option<ListenFailure>)>;

fn epoch_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_millis() as i64)
}

/// NEW-57: the engines, then the microphone and the desktop audio (a
/// capture that fails to open leaves the microphone alone, logged).
fn open_meeting<H: Hearing>(hearing: &mut H, wanted: &MeetingWanted) -> Result<MeetingRun<H>, (ListenFailure, String)> {
    hearing.prepare(true)?;
    let mic = hearing.mic()?;
    let apps = wanted.selection.clone().and_then(|selection| match hearing.apps(selection) {
        Ok(apps) => Some(apps),
        Err(err) => {
            eprintln!("meeting: no desktop audio ({err}); the microphone only");
            None
        }
    });
    let mut meeting = Meeting::new(wanted.id.clone(), wanted.language, wanted.phrase.clone(), crate::meeting::ring_enabled());
    meeting.set_paused(wanted.paused);
    Ok(MeetingRun { meeting, you_label: wanted.you_label.clone(), mic, apps })
}

/// NEW-57: the meeting let the microphone go (stopped, replaced, failed):
/// the debug ring, when on, is written now.
fn close_meeting<H: Hearing>(run: MeetingRun<H>, hearing: &H) {
    let MeetingRun { meeting, mic, apps, .. } = run;
    drop(apps);
    drop(mic);
    let segments = meeting.segments();
    let written = meeting.stop(hearing.debug_dir().as_deref());
    eprintln!("meeting: stopped after {segments} segment(s){}", if written.is_empty() { String::new() } else { format!("; debug ring: {} file(s)", written.len()) });
}

/// NEW-57: a draining stop: what the microphone and the desktop audio
/// already hold, then every open segment, transcribed and emitted.
fn drain_meeting<H: Hearing>(run: &mut MeetingRun<H>, hearing: &mut H, links: &Links) -> Result<(), (ListenFailure, String)> {
    let mut emit = |event: Event| (links.emit)(&event);
    let mut pending: Vec<(String, String, Vec<f32>, i64)> = Vec::new();
    while let Ok(samples) = run.mic.recv_timeout(Duration::ZERO) {
        pending.push((etalk::YOU_CHANNEL.to_string(), run.you_label.clone(), samples, epoch_ms()));
    }
    if let Some(apps) = run.apps.as_ref() {
        while let Ok(chunk) = apps.recv_timeout(Duration::ZERO) {
            let channel = if chunk.channel == etalk::YOU_CHANNEL { "app-you".to_string() } else { chunk.channel };
            pending.push((channel, chunk.label, chunk.samples, chunk.captured_ms as i64));
        }
    }
    for (channel, label, samples, captured_ms) in pending {
        if run.meeting.needs(&channel) {
            let vad = hearing.vad()?;
            run.meeting.add(&channel, &label, vad);
        }
        run.meeting.feed(&channel, &label, &samples, captured_ms, hearing.engines(), &mut emit)?;
    }
    run.meeting.finish(hearing.engines(), &mut emit)
}

/// NEW-57: one pass of a running meeting: the microphone (waiting up to
/// 20 ms), then every app chunk already there, then the idle channels.
fn hear_meeting<H: Hearing>(run: &mut MeetingRun<H>, hearing: &mut H, links: &Links) -> Result<(), (ListenFailure, String)> {
    let mut emit = |event: Event| (links.emit)(&event);
    // `captured_ms`: when the samples' last one was captured. The
    // microphone (a bounded queue read every 20 ms) is dated as it is
    // read; app chunks carry their capture time (meeting.json CHANNELS).
    let mut feed = |run: &mut MeetingRun<H>, hearing: &mut H, channel: &str, label: &str, samples: &[f32], captured_ms: i64| {
        if run.meeting.needs(channel) {
            let vad = hearing.vad()?;
            run.meeting.add(channel, label, vad);
        }
        run.meeting.feed(channel, label, samples, captured_ms, hearing.engines(), &mut emit)
    };
    match run.mic.recv_timeout(Duration::from_millis(20)) {
        Ok(samples) => {
            let label = run.you_label.clone();
            feed(run, hearing, etalk::YOU_CHANNEL, &label, &samples, epoch_ms())?;
        }
        Err(RecvTimeoutError::Timeout) => {}
        Err(RecvTimeoutError::Disconnected) => return Err((ListenFailure::NoMicrophone, "the microphone closed".into())),
    }
    loop {
        let Some(apps) = run.apps.as_ref() else { break };
        match apps.recv_timeout(Duration::ZERO) {
            Ok(chunk) => {
                // Never the microphone's channel, whatever an app calls itself.
                let channel = if chunk.channel == etalk::YOU_CHANNEL { "app-you".to_string() } else { chunk.channel };
                feed(run, hearing, &channel, &chunk.label, &chunk.samples, chunk.captured_ms as i64)?;
            }
            Err(RecvTimeoutError::Timeout) => break,
            Err(RecvTimeoutError::Disconnected) => {
                eprintln!("meeting: the desktop audio ended; the microphone goes on");
                run.apps = None;
            }
        }
    }
    let mut emit = |event: Event| (links.emit)(&event);
    run.meeting.idle(epoch_ms(), hearing.engines(), &mut emit)
}

/// The Listen or Call running now.
enum Active<H: Hearing> {
    Listen(Held<H>),
    Call(Calling<H>),
}

impl<H: Hearing> Active<H> {
    fn id(&self) -> &str {
        match self {
            Active::Listen(held) => held.utterance.id(),
            Active::Call(calling) => calling.call.id(),
        }
    }

    /// Whether the loop reads a microphone (else it waits on jobs).
    fn hears_a_mic(&self) -> bool {
        match self {
            Active::Listen(_) => true,
            Active::Call(calling) => calling.ears.as_ref().is_some_and(|ears| ears.mic.is_some()),
        }
    }
}

/// A push-to-talk Listen: the key is held (or #NEW-56 a wake turn).
struct Held<H: Hearing> {
    utterance: Utterance,
    mic: H::Mic,
    vad: H::Vad,
    started: Instant,
    /// A wake turn: the thread ends it after the quiet ([`turn_ended`]).
    woken: bool,
}

/// #NEW-56: the wake word's microphone between turns.
struct Scan<H: Hearing> {
    mic: H::Mic,
    vad: H::Vad,
    scanner: WakeScanner,
}

/// #NEW-56: the wake word as the host asked for it.
#[derive(Clone, PartialEq, Eq)]
struct WakeWord {
    phrase: String,
    language: &'static str,
}

/// #NEW-56: what the host was last told about the wake word's microphone.
type WakeReport = (bool, Option<ListenFailure>);

/// NEW-55: a call.
struct Calling<H: Hearing> {
    call: Call,
    remote: bool,
    /// S-3: `None` while the call warms up (the engines load or download).
    ears: Option<CallEars<H>>,
    muted: bool,
    /// S-3: `CallListening` went out: only from then on is anything heard.
    listening: bool,
    /// S-3: the input's peak level (dBFS) this second and over the call.
    level: InputLevel,
}

/// S-3: a call's VAD and, local, its microphone and echo canceller.
struct CallEars<H: Hearing> {
    vad: H::Vad,
    /// `None` = remote: the audio comes as [`Job::Audio`].
    mic: Option<H::Mic>,
    /// Local calls only.
    echo: Option<Echo>,
}

/// S-3 ("it does not detect my speech"): the raw input's peak per second
/// (logged at debug) and over the whole call (logged once at its end).
struct InputLevel {
    second: Instant,
    peak: f32,
    max: f32,
}

impl Default for InputLevel {
    fn default() -> Self {
        InputLevel { second: Instant::now(), peak: 0.0, max: 0.0 }
    }
}

impl InputLevel {
    fn hear(&mut self, id: &str, samples: &[f32]) {
        let peak = samples.iter().fold(0.0f32, |peak, sample| peak.max(sample.abs()));
        self.peak = self.peak.max(peak);
        self.max = self.max.max(peak);
        if self.second.elapsed() >= Duration::from_secs(1) {
            crate::debug(&format!("call {id}: input peak {:.1} dBFS", dbfs(self.peak)));
            self.peak = 0.0;
            self.second = Instant::now();
        }
    }
}

/// A sample's magnitude in dBFS (-120 for silence).
fn dbfs(peak: f32) -> f32 {
    if peak > 0.0 { (20.0 * peak.log10()).max(-120.0) } else { -120.0 }
}

impl<H: Hearing> Calling<H> {
    fn new(id: String, language: &str, remote: bool) -> Self {
        Calling { call: Call::new(id, language), remote, ears: None, muted: false, listening: false, level: InputLevel::default() }
    }

    /// Hear `samples` (echo-cancelled when local; dropped while muted, and
    /// S-3 before `CallListening`).
    fn hear(&mut self, samples: &[f32], engines: &mut H::Engines, links: &Links) -> Result<(), (ListenFailure, String)> {
        let Some(ears) = self.ears.as_mut() else { return Ok(()) };
        if self.listening && !self.muted {
            self.level.hear(self.call.id(), samples);
        }
        let cleaned;
        let samples = match &mut ears.echo {
            // Also while muted: the reference keeps pace with the microphone.
            Some(echo) => {
                cleaned = echo.cancel(samples);
                &cleaned[..]
            }
            None => samples,
        };
        if self.muted || !self.listening {
            return Ok(());
        }
        if let Some(echo) = &ears.echo {
            // #NEW-92: the barge-in gate hears what the canceller says.
            self.call.set_echo(echo.state());
            // #NEW-92: the reply's own words are its echo (unless the
            // utterance barged in): dropped, never a turn.
            self.call.set_spoken((links.spoken)());
        }
        self.call.feed(samples, &mut ears.vad, engines, &mut |event| {
            if matches!(event, Event::SpeechStarted { .. }) {
                // Barge-in: the reply stops (its Finished goes out) BEFORE
                // the host hears that the person talks.
                (links.barge)();
            }
            (links.emit)(&event);
        })
    }

    /// S-3: warm the call up: the engines (a download keeps it pending:
    /// `Ok`), then its VAD and microphone (unless a Hangup or a newer
    /// Listen/Call came meanwhile), then, once the voice is settled too,
    /// `CallListening`. `Err` fails the call.
    fn warm_up(
        &mut self,
        hearing: &mut H,
        rx: &Receiver<Job>,
        backlog: &mut VecDeque<Job>,
        links: &Links,
    ) -> Result<(), (ListenFailure, String)> {
        if self.ears.is_none() {
            match hearing.prepare(!self.remote) {
                Ok(()) => {}
                // The ring goes on; the download's progress reaches the host.
                Err((ListenFailure::Downloading, _)) => return Ok(()),
                Err(failed) => return Err(failed),
            }
            // A load takes seconds: what arrived meanwhile comes first.
            backlog.extend(rx.try_iter());
            let id = self.call.id();
            if backlog.iter().any(|job| job.starts() || matches!(job, Job::Hangup { id: other } if other == id)) {
                return Ok(());
            }
            let vad = hearing.vad()?;
            let mic = if self.remote { None } else { Some(hearing.mic()?) };
            let echo = (!self.remote).then(|| Echo::new(links.far.clone()));
            self.ears = Some(CallEars { vad, mic, echo });
        }
        if !self.listening && (links.voice_ready)() {
            self.listening = true;
            self.level = InputLevel::default();
            eprintln!(
                "call {}: listening ({})",
                self.call.id(),
                if self.remote { "the caller's stream" } else { "the microphone, echo-cancelled" }
            );
            (links.emit)(&Event::CallListening { id: self.call.id().to_string() });
        }
        Ok(())
    }
}

impl<H: Hearing> Drop for Calling<H> {
    /// S-3: one line per call: how loud the member's input ever got.
    fn drop(&mut self) {
        if self.listening {
            eprintln!("call {}: ended; loudest input {:.1} dBFS", self.call.id(), dbfs(self.level.max));
        }
    }
}

/// Where the listener's events go and what it reaches in the speaker.
#[derive(Clone)]
pub struct Links {
    pub listening: Arc<Listening>,
    /// NEW-55: stop any reply now (barge-in), its Finished emitted first.
    pub barge: Arc<dyn Fn() + Send + Sync>,
    /// NEW-55: the far-end reference a local call's canceller reads.
    pub far: Arc<FarEnd>,
    pub emit: Arc<dyn Fn(&Event) + Send + Sync>,
    /// #1263 review: a reply is being spoken (the wake word ignores it).
    pub speaking: Arc<dyn Fn() -> bool + Send + Sync>,
    /// #NEW-92: the sentences of the reply a local call may still hear
    /// (empty = none): an utterance made of their words is the echo.
    pub spoken: Arc<dyn Fn() -> Vec<String> + Send + Sync>,
    /// S-3: the voice is settled (loaded, or known unavailable): a call
    /// says `CallListening` only then, so its greeting can be spoken.
    pub voice_ready: Arc<dyn Fn() -> bool + Send + Sync>,
}

impl Links {
    /// Events to stdout, no speaker (the loop's tests, the examples).
    pub fn new(listening: Arc<Listening>) -> Self {
        Links {
            listening,
            barge: Arc::new(|| {}),
            far: Arc::default(),
            emit: Arc::new(emit),
            speaking: Arc::new(|| false),
            spoken: Arc::new(Vec::new),
            voice_ready: Arc::new(|| true),
        }
    }
}

enum Fetch {
    Idle { failures: u32, retry_at: Option<Instant> },
    /// The download thread answers on its OWN channel, so the listening
    /// thread holds no sender to its request channel and sees EOF.
    /// `failures` carries the count across the run (#1238 review: the
    /// backoff grows with every failed attempt).
    Running { result: Receiver<Result<(), String>>, failures: u32 },
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
        self.running(result);
        Ok(())
    }

    /// A download runs; the failures so far ride along.
    fn running(&mut self, result: Receiver<Result<(), String>>) {
        self.fetch = Fetch::Running { result, failures: self.failures() };
    }

    /// Failed downloads in a row (through a running one too).
    fn failures(&self) -> u32 {
        match &self.fetch {
            Fetch::Idle { failures, .. } | Fetch::Running { failures, .. } => *failures,
        }
    }

    fn downloaded(&mut self, result: Result<(), String>) {
        match result {
            Ok(()) => {
                self.fetch = Fetch::Idle { failures: 0, retry_at: None };
                emit(&Event::ModelReady);
            }
            Err(err) => {
                let failures = self.failures() + 1;
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

    fn prepare(&mut self, microphone: bool) -> Result<(), (ListenFailure, String)> {
        let refused = || if microphone { microphone_refused() } else { None };
        if self.engines.is_some() {
            return refused().map_or(Ok(()), |reason| Err((reason, "the microphone is not allowed".into())));
        }
        if !Engines::files_ready(&self.data_dir) {
            match self.fetch {
                Fetch::Running { .. } => {}
                Fetch::Idle { retry_at: Some(at), .. } if Instant::now() < at => {
                    return Err((ListenFailure::Unavailable, "the speech model failed to download; retrying later".into()));
                }
                Fetch::Idle { .. } => {
                    self.start_download().map_err(|err| (ListenFailure::Unavailable, err))?;
                }
            }
            return Err((ListenFailure::Downloading, "the speech model is downloading".into()));
        }
        if let Some(reason) = refused() {
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
        matches!(self.fetch, Fetch::Running { .. })
    }

    fn poll(&mut self) {
        let Fetch::Running { result, .. } = &self.fetch else { return };
        match result.try_recv() {
            Ok(downloaded) => self.downloaded(downloaded),
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => self.downloaded(Err("the download stopped".into())),
        }
    }

    fn vad(&mut self) -> Result<SileroVad, (ListenFailure, String)> {
        self.engines().vad().map_err(|err| (ListenFailure::Unavailable, err))
    }

    fn mic(&mut self) -> Result<Capture, (ListenFailure, String)> {
        capture::open()
    }

    fn engines(&mut self) -> &mut Engines {
        self.engines.as_mut().expect("prepared")
    }

    fn debug_dir(&self) -> Option<PathBuf> {
        Some(self.data_dir.join(crate::meeting::DEBUG_DIR))
    }
}

/// Start the listening thread on the [`Hearing`] `make` builds there; it
/// ends when the returned sender is dropped (EOF), closing any microphone.
pub fn spawn<H: Hearing>(make: impl FnOnce() -> H + Send + 'static, links: Links) -> (Sender<Job>, std::thread::JoinHandle<()>) {
    let (tx, rx) = mpsc::channel();
    let thread = std::thread::Builder::new()
        .name("voice-listen".into())
        .spawn(move || listen_loop(&rx, &mut make(), &links))
        .expect("spawn the listening thread");
    (tx, thread)
}

/// The language Parakeet is told: `user_voice_settings.language`.
fn language_of(language: &str) -> &'static str {
    if language == "de" { "de" } else { "en" }
}

fn listen_loop<H: Hearing>(rx: &Receiver<Job>, hearing: &mut H, links: &Links) {
    let mut active: Option<Active<H>> = None;
    // Jobs read while the engines loaded, handled before new ones.
    let mut backlog: VecDeque<Job> = VecDeque::new();
    let emit = |event: Event| (links.emit)(&event);
    // #NEW-56: the wake word, its microphone between turns, and when it may
    // try to open that again.
    let mut wake: Option<WakeWord> = None;
    let mut scan: Option<Scan<H>> = None;
    let mut wake_retry: Option<Instant> = None;
    let mut reported: WakeReport = (false, None);
    let mut turns = 0u64;
    // #1263 review: when the voice last played (its echo tail).
    let mut spoke: Option<Instant> = None;
    // #NEW-86: whether a wake turn ran on the last pass, and when one last
    // ended (its cooldown).
    let mut woken = false;
    let mut last_turn: Option<Instant> = None;
    let mut report = |now: WakeReport| {
        if now != reported {
            reported = now;
            emit(Event::WakeListening { listening: now.0, reason: now.1 });
        }
    };
    // NEW-57: the meeting asked for, the one holding the microphone, when
    // it may try to open again, and what the host was told.
    let mut meeting_wanted: Option<MeetingWanted> = None;
    let mut meeting: Option<MeetingRun<H>> = None;
    let mut meeting_retry: Option<Instant> = None;
    let mut meeting_reported: MeetingReport = None;
    let mut report_meeting = |now: MeetingReport| {
        if now != meeting_reported {
            if let Some((id, listening, reason)) = now.clone().or_else(|| meeting_reported.clone().map(|(id, ..)| (id, false, None))) {
                emit(Event::MeetingListening { id, listening, reason });
            }
            meeting_reported = now;
        }
    };
    loop {
        hearing.poll();
        // S-3: a call warming up advances (its download, the load, its
        // microphone, CallListening).
        if let Some(Active::Call(calling)) = active.as_mut() {
            if let Err((reason, detail)) = calling.warm_up(hearing, rx, &mut backlog, links) {
                let id = calling.call.id().to_string();
                eprintln!("call {id}: could not listen ({reason:?}): {detail}");
                active = None;
                links.listening.end(&id);
                emit(Event::ListenFailed { id, reason, detail });
            }
        }
        // NEW-57: a wanted meeting takes the microphone once no Listen or
        // Call runs (the wake scan lets it go).
        let meeting_due = meeting_retry.is_none_or(|at| Instant::now() >= at);
        if let (Some(wanted), None, None, true) = (meeting_wanted.as_ref(), meeting.as_ref(), active.as_ref(), meeting_due) {
            if scan.take().is_some() {
                report((false, None));
            }
            match open_meeting(hearing, wanted) {
                Ok(run) => {
                    eprintln!("meeting: listening ({})", if run.apps.is_some() { "microphone and desktop audio" } else { "microphone" });
                    meeting = Some(run);
                    meeting_retry = None;
                    report_meeting(Some((wanted.id.clone(), true, None)));
                }
                Err((reason, detail)) => {
                    eprintln!("meeting: the microphone did not open ({reason:?}): {detail}");
                    meeting_retry = Some(Instant::now() + WAKE_RETRY);
                    report_meeting(Some((wanted.id.clone(), false, Some(reason))));
                }
            }
        }
        let woken_now = matches!(&active, Some(Active::Listen(held)) if held.woken);
        if woken && !woken_now {
            last_turn = Some(Instant::now());
        }
        woken = woken_now;
        let due = wake_retry.is_none_or(|at| Instant::now() >= at);
        if let (Some(word), None, None, true, None) = (wake.as_ref(), active.as_ref(), scan.as_ref(), due, meeting_wanted.as_ref()) {
            match open_scan(hearing, &word.phrase) {
                Ok(opened) => {
                    scan = Some(opened);
                    wake_retry = None;
                    report((true, None));
                }
                Err((reason, detail)) => {
                    eprintln!("wake word: the microphone did not open ({reason:?}): {detail}");
                    wake_retry = Some(Instant::now() + WAKE_RETRY);
                    report((false, Some(reason)));
                }
            }
        }
        let job = if let Some(job) = backlog.pop_front() {
            Some(job)
        } else if active.as_ref().is_some_and(Active::hears_a_mic) || scan.is_some() || meeting.is_some() {
            match rx.try_recv() {
                Ok(job) => Some(job),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => break,
            }
        } else if hearing.downloading() || active.is_some() || wake.is_some() || meeting_wanted.is_some() {
            // A remote call hears its audio as jobs; S-3: a call warming up
            // looks again soon (the voice may settle any moment).
            let warming = matches!(&active, Some(Active::Call(calling)) if !calling.listening);
            match rx.recv_timeout(Duration::from_millis(if warming { 50 } else { 250 })) {
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
        match job {
            Some(Job::Wake { phrase, language }) => {
                let wanted = (!phrase.trim().is_empty()).then(|| WakeWord { phrase, language: language_of(&language) });
                if wanted.is_none() {
                    scan = None;
                    // #1263 review: a wake turn in flight ends with it,
                    // unanswered: nothing it heard is ever sent.
                    if matches!(&active, Some(Active::Listen(held)) if held.woken) {
                        if let Some(ended) = active.take() {
                            links.listening.end(ended.id());
                        }
                    }
                    report((false, None));
                }
                if wanted != wake {
                    // A new phrase or language: the next segment hears it
                    // (through a window sized to the new phrase).
                    wake_retry = None;
                    if let (Some(scanning), Some(word)) = (scan.as_mut(), wanted.as_ref()) {
                        scanning.scanner = WakeScanner::new(crate::wake::window_ms(&word.phrase));
                    }
                }
                wake = wanted;
                continue;
            }
            Some(Job::Meeting { id, language, phrase, you_label, selection }) => {
                let paused = meeting_wanted.as_ref().is_some_and(|wanted| wanted.id == id && wanted.paused);
                let wanted = MeetingWanted { id, language: language_of(&language), phrase, you_label, selection, paused };
                if meeting_wanted.as_ref() != Some(&wanted) {
                    // A new meeting, or new apps: it opens again.
                    if let Some(run) = meeting.take() {
                        close_meeting(run, hearing);
                    }
                    if meeting_wanted.as_ref().is_some_and(|old| old.id != wanted.id) {
                        report_meeting(None);
                    }
                    meeting_retry = None;
                    meeting_wanted = Some(wanted);
                }
                continue;
            }
            Some(Job::MeetingStop { id, drain }) => {
                if meeting_wanted.as_ref().is_some_and(|wanted| wanted.id == id) {
                    meeting_wanted = None;
                    meeting_retry = None;
                    if let Some(mut run) = meeting.take() {
                        if drain {
                            if let Err((reason, detail)) = drain_meeting(&mut run, hearing, links) {
                                eprintln!("meeting: the last segments failed ({reason:?}): {detail}");
                            }
                        }
                        close_meeting(run, hearing);
                    }
                    report_meeting(None);
                    // The wake word takes the microphone back on the next pass.
                    wake_retry = None;
                }
                if drain {
                    // Everything this meeting had is out: the host's final batch.
                    emit(Event::MeetingStopped { id });
                }
                continue;
            }
            // NEW-57: a meeting owns the microphone (talk-listen.json
            // `micOwnerCases`): a Listen is its flush trigger (answered
            // Empty), a Call is refused (voice-call.json `failed.meeting`).
            Some(Job::Start { id, .. }) if meeting_wanted.is_some() => {
                if let Some(wanted) = meeting_wanted.as_ref() {
                    emit(Event::MeetingFlushNow { id: wanted.id.clone() });
                }
                links.listening.end(&id);
                emit(Event::Empty { id });
                continue;
            }
            Some(Job::Call { id, .. }) if meeting_wanted.is_some() => {
                links.listening.end(&id);
                emit(Event::ListenFailed { id, reason: ListenFailure::Meeting, detail: "a meeting holds the microphone".into() });
                continue;
            }
            Some(Job::MeetingPause { id, paused }) => {
                if let Some(wanted) = meeting_wanted.as_mut().filter(|wanted| wanted.id == id) {
                    wanted.paused = paused;
                    if let Some(run) = meeting.as_mut() {
                        run.meeting.set_paused(paused);
                    }
                }
                continue;
            }
            Some(job) => {
                // #NEW-56: a Listen borrows the wake word's open microphone;
                // a call opens its own (behind its echo canceller).
                let lent = match &job {
                    Job::Start { .. } => scan.take().map(|scan| (scan.vad, scan.mic)),
                    Job::Call { .. } => {
                        // A call takes precedence: the scan pauses until
                        // Hangup (the tray's dot goes out, also when the
                        // call replaces a wake turn).
                        scan = None;
                        report((false, None));
                        None
                    }
                    _ => None,
                };
                handle(job, &mut active, hearing, rx, &mut backlog, links, lent);
                continue;
            }
            None => {}
        }
        if let Some(running) = active.as_mut() {
            let engines = hearing.engines();
            match running {
                Active::Listen(held) => match held.mic.recv_timeout(Duration::from_millis(20)) {
                    Ok(samples) => {
                        let mut sink = emit;
                        if let Err((reason, detail)) = held.utterance.feed(&samples, &mut held.vad, engines, &mut sink) {
                            let id = held.utterance.id().to_string();
                            active = None;
                            links.listening.end(&id);
                            emit(Event::ListenFailed { id, reason, detail });
                        } else if held.started.elapsed() >= MAX_HOLD || held.woken && turn_ended(&held.utterance) {
                            let Some(Active::Listen(held)) = active.take() else { unreachable!("a listen") };
                            let id = held.utterance.id().to_string();
                            finish(held, engines, links);
                            links.listening.end(&id);
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => {
                        let Some(Active::Listen(held)) = active.take() else { unreachable!("a listen") };
                        let id = held.utterance.id().to_string();
                        if held.utterance.seconds() == 0.0 {
                            emit(Event::ListenFailed {
                                id: id.clone(),
                                reason: ListenFailure::NoMicrophone,
                                detail: "the microphone closed".into(),
                            });
                        } else {
                            // The device went away mid-utterance: answer what was heard.
                            finish(held, engines, links);
                        }
                        links.listening.end(&id);
                    }
                },
                Active::Call(calling) => {
                    let mic = calling.ears.as_ref().and_then(|ears| ears.mic.as_ref());
                    let received = mic.map(|mic| mic.recv_timeout(Duration::from_millis(20)));
                    let failed = match received {
                        Some(Ok(samples)) => calling.hear(&samples, engines, links).err(),
                        Some(Err(RecvTimeoutError::Timeout)) | None => None,
                        Some(Err(RecvTimeoutError::Disconnected)) => {
                            Some((ListenFailure::NoMicrophone, "the microphone closed".to_string()))
                        }
                    };
                    if let Some((reason, detail)) = failed {
                        let id = calling.call.id().to_string();
                        active = None;
                        links.listening.end(&id);
                        emit(Event::ListenFailed { id, reason, detail });
                    }
                }
            }
            continue;
        }
        if let Some(run) = meeting.as_mut() {
            if let Err((reason, detail)) = hear_meeting(run, hearing, links) {
                eprintln!("meeting: listening failed ({reason:?}): {detail}");
                if let Some(run) = meeting.take() {
                    close_meeting(run, hearing);
                }
                meeting_retry = Some(Instant::now() + WAKE_RETRY);
                if let Some(wanted) = meeting_wanted.as_ref() {
                    report_meeting(Some((wanted.id.clone(), false, Some(reason))));
                }
            }
            continue;
        }
        let (Some(scanning), Some(word)) = (scan.as_mut(), wake.as_ref()) else { continue };
        match scanning.mic.recv_timeout(Duration::from_millis(20)) {
            Ok(samples) => match crate::wake::scan(
                &mut scanning.scanner,
                &mut scanning.vad,
                hearing.engines(),
                &samples,
                &word.phrase,
                word.language,
                {
                    // #1263 review: the voice (and its echo tail) never wakes itself.
                    if (links.speaking)() {
                        spoke = Some(Instant::now());
                    }
                    // #NEW-86: nor does a wake turn's tail open a second one.
                    spoke.is_some_and(|at| at.elapsed() < Duration::from_millis(crate::wake::ECHO_TAIL_MS as u64))
                        || crate::wake::cooling(last_turn.map(|at| at.elapsed().as_millis() as usize))
                },
            ) {
                Ok(None) => {}
                Ok(Some(matched)) => {
                    let Scan { mic, vad, scanner } = scan.take().expect("scanning");
                    turns += 1;
                    let id = format!("wake-{turns}");
                    let utterance = match crate::wake::turn(&id, word.language, &word.phrase, &scanner, matched, hearing.engines()) {
                        Ok(utterance) => utterance,
                        Err((reason, detail)) => {
                            eprintln!("wake word: the turn failed to start ({reason:?}): {detail}");
                            continue;
                        }
                    };
                    if !links.listening.begin(&id) {
                        // A call was asked for meanwhile: it takes the
                        // microphone (the turn and its scan close here).
                        continue;
                    }
                    // Like a Listen: the voice stops for the person.
                    (links.barge)();
                    emit(Event::Woke { id });
                    active = Some(Active::Listen(Held { utterance, mic, vad, started: Instant::now(), woken: true }));
                }
                Err(detail) => {
                    eprintln!("wake word: {detail}");
                    scan = None;
                    wake_retry = Some(Instant::now() + WAKE_RETRY);
                    report((false, Some(ListenFailure::Failed)));
                }
            },
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                scan = None;
                wake_retry = Some(Instant::now() + WAKE_RETRY);
                report((false, Some(ListenFailure::NoMicrophone)));
            }
        }
    }
    // EOF: `active` and `scan` drop here, closing the microphone.
    if let Some(run) = meeting.take() {
        close_meeting(run, hearing);
    }
}

/// #NEW-56: the engines, then the wake word's own VAD and microphone.
fn open_scan<H: Hearing>(hearing: &mut H, phrase: &str) -> Result<Scan<H>, (ListenFailure, String)> {
    hearing.prepare(true)?;
    let vad = hearing.vad()?;
    let mic = hearing.mic()?;
    Ok(Scan { mic, vad, scanner: WakeScanner::new(crate::wake::window_ms(phrase)) })
}

/// One job from the reader (or the backlog).
fn handle<H: Hearing>(
    job: Job,
    active: &mut Option<Active<H>>,
    hearing: &mut H,
    rx: &Receiver<Job>,
    backlog: &mut VecDeque<Job>,
    links: &Links,
    lent: Option<(H::Vad, H::Mic)>,
) {
    let calling = |active: &mut Option<Active<H>>, id: &str| -> bool {
        matches!(active, Some(Active::Call(calling)) if calling.call.id() == id)
    };
    match job {
        Job::Start { id, language } => {
            // A call owns the microphone until Hangup: a Listen during it
            // is refused, nothing heard (talk-listen.json `micOwnerCases`).
            if matches!(active, Some(Active::Call(_))) {
                (links.emit)(&Event::Empty { id });
                return;
            }
            // A new Listen replaces the running Listen (no answer).
            if let Some(replaced) = active.take() {
                links.listening.end(replaced.id());
            }
            *active = start(hearing, rx, backlog, id.clone(), language_of(&language), links, lent).map(Active::Listen);
            if active.is_none() {
                links.listening.end(&id);
            }
        }
        Job::Call { id, language, remote } => {
            if let Some(replaced) = active.take() {
                links.listening.end(replaced.id());
            }
            // S-3: it warms up on the loop's next passes (Calling::warm_up).
            *active = Some(Active::Call(Calling::new(id, language_of(&language), remote)));
        }
        Job::End { id } => {
            if matches!(active, Some(Active::Listen(held)) if held.utterance.id() == id) {
                let Some(Active::Listen(held)) = active.take() else { unreachable!("checked") };
                finish(held, hearing.engines(), links);
            }
            links.listening.end(&id);
        }
        Job::Cancel { id } => {
            if matches!(active, Some(Active::Listen(held)) if held.utterance.id() == id) {
                *active = None;
            }
            links.listening.end(&id);
        }
        Job::Audio { id, samples } => {
            if !calling(active, &id) {
                return;
            }
            let Some(Active::Call(running)) = active.as_mut() else { unreachable!("checked") };
            // Only a remote call takes streamed audio.
            if !running.remote {
                return;
            }
            if let Err((reason, detail)) = running.hear(&samples, hearing.engines(), links) {
                *active = None;
                links.listening.end(&id);
                (links.emit)(&Event::ListenFailed { id, reason, detail });
            }
        }
        Job::Mute { id, muted } => {
            if !calling(active, &id) {
                return;
            }
            let Some(Active::Call(running)) = active.as_mut() else { unreachable!("checked") };
            if muted && !running.muted {
                // The open utterance is dropped, never half said.
                running.call.reset();
            } else if !muted && running.muted && running.ears.is_some() {
                // A fresh VAD: its stream starts with the unmuted audio.
                match hearing.vad() {
                    Ok(vad) => running.ears.as_mut().expect("checked").vad = vad,
                    Err((reason, detail)) => {
                        *active = None;
                        links.listening.end(&id);
                        (links.emit)(&Event::ListenFailed { id, reason, detail });
                        return;
                    }
                }
            }
            running.muted = muted;
        }
        Job::Hangup { id } => {
            if calling(active, &id) {
                // Drops the microphone and the canceller.
                *active = None;
            }
            links.listening.end(&id);
        }
        // The loop keeps the wake word and the meeting.
        Job::Wake { .. } | Job::Meeting { .. } | Job::MeetingStop { .. } | Job::MeetingPause { .. } => {}
    }
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
    links: &Links,
    lent: Option<(H::Vad, H::Mic)>,
) -> Option<Held<H>> {
    let fail = |reason: ListenFailure, detail: String| {
        (links.emit)(&Event::ListenFailed { id: id.clone(), reason, detail });
        None
    };
    if let Err((reason, detail)) = hearing.prepare(true) {
        return fail(reason, detail);
    }
    backlog.extend(rx.try_iter());
    let ends = |job: &Job| matches!(job, Job::End { id: other } | Job::Cancel { id: other } if *other == id);
    if let Some(index) = backlog.iter().position(ends) {
        if let Some(Job::End { .. }) = backlog.remove(index) {
            // Released before the microphone opened: nothing was heard.
            (links.emit)(&Event::Empty { id: id.clone() });
        }
        return None;
    }
    if backlog.iter().any(Job::starts) {
        // Replaced before it began (no answer, as for a running one).
        return None;
    }
    // #NEW-56: the wake word's open microphone, when it lent it.
    let (vad, mic) = match lent {
        Some(lent) => lent,
        None => {
            let vad = match hearing.vad() {
                Ok(vad) => vad,
                Err((reason, detail)) => return fail(reason, detail),
            };
            match hearing.mic() {
                Ok(mic) => (vad, mic),
                Err((reason, detail)) => return fail(reason, detail),
            }
        }
    };
    Some(Held { utterance: Utterance::new(id, language), mic, vad, started: Instant::now(), woken: false })
}

/// The key was released: take what the microphone still holds, close it,
/// transcribe the rest.
fn finish<H: Hearing>(held: Held<H>, engines: &mut H::Engines, links: &Links) {
    let Held { mut utterance, mic, mut vad, .. } = held;
    let mut sink = |event: Event| (links.emit)(&event);
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
        (links.emit)(&Event::ListenFailed { id, reason, detail });
    }
}

/// macOS answers a refused microphone with silence, so ask first; the
/// first Listen (not yet decided) opens the device and macOS prompts.
fn microphone_refused() -> Option<ListenFailure> {
    etalk::microphone::refused().then_some(ListenFailure::NotGranted)
}

/// A [`Hearing`] without models or a device, for the loop's tests.
#[cfg(test)]
pub(crate) mod fake {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use etalk::ListenFailure;

    use super::{AppAudio, Hearing, Mic};
    use crate::listen::{measure, Transcriber, Vad, FRAME};

    /// A VAD on loudness alone (speech = a frame above -40 dBFS); silence
    /// never speaks.
    #[derive(Default)]
    pub struct EnergyVad(Vec<f32>);

    impl Vad for EnergyVad {
        fn push(&mut self, samples: &[f32]) -> Result<Vec<f32>, String> {
            self.0.extend_from_slice(samples);
            let whole = self.0.len() / FRAME * FRAME;
            let out = self.0[..whole].chunks(FRAME).map(|frame| if measure(frame).0 > 0.4 { 0.9 } else { 0.05 }).collect();
            self.0.drain(..whole);
            Ok(out)
        }
    }

    /// Names each clip by its count.
    #[derive(Default)]
    pub struct Counter(usize);

    impl Transcriber for Counter {
        fn transcribe(&mut self, _pcm: &[f32], _language: &str) -> Result<String, String> {
            self.0 += 1;
            Ok(format!("clip{}", self.0))
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

    /// What the test sees: each opened microphone's closed flag and its
    /// sender, and the gate `prepare` waits on (a slow engine load).
    #[derive(Clone)]
    pub struct Probe {
        pub opened: Arc<Mutex<Vec<Arc<AtomicBool>>>>,
        pub mics: Arc<Mutex<Vec<Sender<Vec<f32>>>>>,
        pub opened_tx: Sender<()>,
        pub loads: Arc<Mutex<Option<Receiver<()>>>>,
        /// NEW-57: each desktop capture opened (its sender) and the
        /// selection it was opened for.
        pub apps: Arc<Mutex<Vec<(etalk::apps::Selection, Sender<etalk::apps::Chunk>)>>>,
        /// S-3: `prepare` answers `Downloading` this many more times; then
        /// `failed` decides whether the download failed.
        pub downloads: Arc<Mutex<(usize, bool)>>,
    }

    /// NEW-57: desktop audio the test plays.
    pub struct FakeApps(Receiver<etalk::apps::Chunk>);

    impl AppAudio for FakeApps {
        fn recv_timeout(&self, timeout: Duration) -> Result<etalk::apps::Chunk, RecvTimeoutError> {
            self.0.recv_timeout(timeout)
        }
    }

    impl Probe {
        /// A probe whose engine load waits for `release` when `slow`.
        pub fn new(slow: bool) -> (Probe, Receiver<()>, Option<Sender<()>>) {
            let (opened_tx, opened) = mpsc::channel();
            let (release, loads) = mpsc::channel();
            let probe = Probe {
                opened: Arc::default(),
                mics: Arc::default(),
                opened_tx,
                loads: Arc::new(Mutex::new(slow.then_some(loads))),
                apps: Arc::default(),
                downloads: Arc::default(),
            };
            (probe, opened, slow.then_some(release))
        }

        pub fn hearing(&self) -> FakeHearing {
            FakeHearing { probe: self.clone(), engines: Counter::default() }
        }

        /// NEW-57: play `samples` as app `channel` into the newest capture.
        pub fn play(&self, channel: &str, label: &str, samples: Vec<f32>) {
            let apps = self.apps.lock().unwrap();
            let chunk = etalk::apps::Chunk { channel: channel.into(), label: label.into(), samples, captured_ms: etalk::apps::now_ms() };
            apps.last().expect("a capture is open").1.send(chunk).unwrap();
        }

        /// NEW-57: [`Probe::play`] with an explicit capture time.
        pub fn play_at(&self, channel: &str, label: &str, samples: Vec<f32>, captured_ms: u64) {
            let apps = self.apps.lock().unwrap();
            let chunk = etalk::apps::Chunk { channel: channel.into(), label: label.into(), samples, captured_ms };
            apps.last().expect("a capture is open").1.send(chunk).unwrap();
        }

        /// Speak into the newest microphone.
        pub fn say(&self, samples: Vec<f32>) {
            let mics = self.mics.lock().unwrap();
            mics.last().expect("a microphone is open").send(samples).unwrap();
        }
    }

    pub struct FakeHearing {
        probe: Probe,
        engines: Counter,
    }

    impl Hearing for FakeHearing {
        type Engines = Counter;
        type Vad = EnergyVad;
        type Mic = FakeMic;

        fn prepare(&mut self, _microphone: bool) -> Result<(), (ListenFailure, String)> {
            {
                let mut downloads = self.probe.downloads.lock().unwrap();
                if downloads.0 > 0 {
                    downloads.0 -= 1;
                    return Err((ListenFailure::Downloading, "downloading".into()));
                }
                if downloads.1 {
                    return Err((ListenFailure::Unavailable, "the download failed".into()));
                }
            }
            if let Some(loads) = self.probe.loads.lock().unwrap().take() {
                let _ = loads.recv();
            }
            Ok(())
        }

        fn downloading(&self) -> bool {
            false
        }

        fn poll(&mut self) {}

        fn vad(&mut self) -> Result<EnergyVad, (ListenFailure, String)> {
            Ok(EnergyVad::default())
        }

        fn mic(&mut self) -> Result<FakeMic, (ListenFailure, String)> {
            let (open, samples) = mpsc::channel();
            let closed = Arc::new(AtomicBool::new(false));
            self.probe.opened.lock().unwrap().push(closed.clone());
            self.probe.mics.lock().unwrap().push(open.clone());
            let _ = self.probe.opened_tx.send(());
            Ok(FakeMic { samples, _open: open, closed })
        }

        fn engines(&mut self) -> &mut Counter {
            &mut self.engines
        }

        fn apps(&mut self, selection: etalk::apps::Selection) -> Result<Box<dyn AppAudio>, String> {
            let (tx, rx) = mpsc::channel();
            self.probe.apps.lock().unwrap().push((selection, tx));
            Ok(Box::new(FakeApps(rx)))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;
    use std::sync::Arc;

    use super::fake::Probe;
    use super::*;

    /// #1238 review: the download backoff grows with every failed attempt.
    #[test]
    fn a_failed_model_download_backs_off_further_each_time() {
        let mut ears = Ears::new(std::env::temp_dir().join("voice-listen-backoff"));
        for attempt in 1..=3u32 {
            let (_keep, result) = mpsc::channel();
            ears.running(result);
            assert_eq!(ears.failures(), attempt - 1, "the count rides along the run");
            let before = Instant::now();
            ears.downloaded(Err("offline".into()));
            let Fetch::Idle { failures, retry_at: Some(at) } = &ears.fetch else { panic!("idle with a retry time") };
            assert_eq!(*failures, attempt);
            assert!(*at >= before + retry_after(attempt), "attempt {attempt} waits {:?}", retry_after(attempt));
        }
        let (_keep, result) = mpsc::channel();
        ears.running(result);
        ears.downloaded(Ok(()));
        assert_eq!(ears.failures(), 0, "a success resets the count");
    }

    #[test]
    fn the_listening_gate_ends_only_its_own_listen() {
        let listening = Listening::default();
        assert!(!listening.active());
        assert!(listening.begin("u1"));
        assert!(listening.begin("u2"));
        listening.end("u1");
        assert!(listening.active(), "u2 replaced u1 and still runs");
        assert!(listening.refuses_speech());
        listening.end("u2");
        assert!(!listening.active());
        // NEW-55: a call takes replies; a remote one streams them.
        listening.begin_call("c1", false);
        assert!(listening.active() && !listening.refuses_speech() && !listening.remote_call());
        listening.begin_call("c2", true);
        assert!(listening.remote_call());
        // A call owns the microphone: push to talk is refused.
        assert!(!listening.begin("u3"));
        assert!(listening.remote_call(), "the call goes on");
        listening.end("c2");
        assert!(!listening.active() && !listening.remote_call());
    }

    /// #NEW-56: the wake word opens the microphone between Listens, and a
    /// Listen borrows it instead of opening a second one.
    #[test]
    fn a_listen_borrows_the_wake_words_microphone() {
        let (probe, opened, _) = Probe::new(false);
        let listening = Arc::new(Listening::default());
        let hearing = probe.hearing();
        let (tx, thread) = spawn(move || hearing, Links::new(listening.clone()));
        tx.send(Job::Wake { phrase: "Hey Studio".into(), language: "en".into() }).unwrap();
        opened.recv_timeout(Duration::from_secs(5)).expect("the wake word opens the microphone");
        assert!(listening.begin("u1"));
        tx.send(Job::Start { id: "u1".into(), language: "en".into() }).unwrap();
        // Let the Listen start before anything that would end it is queued.
        std::thread::sleep(Duration::from_millis(200));
        tx.send(Job::Wake { phrase: String::new(), language: "en".into() }).unwrap();
        tx.send(Job::End { id: "u1".into() }).unwrap();
        drop(tx);
        thread.join().unwrap();
        let mics = probe.opened.lock().unwrap();
        assert_eq!(mics.len(), 1, "the Listen borrowed the open microphone");
        assert!(mics[0].load(Ordering::SeqCst), "closed at the end");
        assert!(!listening.active());
    }

    #[test]
    fn a_wake_word_off_closes_its_microphone() {
        let (probe, opened, _) = Probe::new(false);
        let hearing = probe.hearing();
        let (tx, thread) = spawn(move || hearing, Links::new(Arc::new(Listening::default())));
        tx.send(Job::Wake { phrase: "Hey Studio".into(), language: "en".into() }).unwrap();
        opened.recv_timeout(Duration::from_secs(5)).expect("opened");
        tx.send(Job::Wake { phrase: " ".into(), language: "en".into() }).unwrap();
        let closed = probe.opened.lock().unwrap()[0].clone();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !closed.load(Ordering::SeqCst) {
            assert!(Instant::now() < deadline, "a blank phrase closes the microphone");
            std::thread::sleep(Duration::from_millis(5));
        }
        drop(tx);
        thread.join().unwrap();
        assert_eq!(probe.opened.lock().unwrap().len(), 1, "never reopened while off");
    }

    #[test]
    fn a_release_during_the_engine_load_never_opens_the_microphone() {
        let (probe, _opened, release) = Probe::new(true);
        let listening = Arc::new(Listening::default());
        let hearing = probe.hearing();
        let (tx, thread) = spawn(move || hearing, Links::new(listening.clone()));
        assert!(listening.begin("u1"));
        tx.send(Job::Start { id: "u1".into(), language: "en".into() }).unwrap();
        tx.send(Job::End { id: "u1".into() }).unwrap();
        release.unwrap().send(()).unwrap();
        drop(tx);
        thread.join().unwrap();
        assert!(probe.opened.lock().unwrap().is_empty(), "the microphone never opened");
        assert!(!listening.active());
    }

    fn tone(seconds: f32, amplitude: f32) -> Vec<f32> {
        (0..(seconds * RATE as f32) as usize)
            .map(|index| amplitude * (std::f32::consts::TAU * 440.0 * index as f32 / RATE as f32).sin())
            .collect()
    }

    /// One phrase: 0.2 s quiet, `seconds` of speech, 0.5 s quiet.
    /// The pause after it closes a call's utterance too (S-3: 700 ms).
    fn phrase(seconds: f32) -> Vec<f32> {
        [tone(0.2, 0.0), tone(seconds, 0.3), tone(0.9, 0.0)].concat()
    }

    /// A call's listener with its events (the barge marked as `barge`).
    struct Harness {
        probe: Probe,
        opened: Receiver<()>,
        listening: Arc<Listening>,
        events: Arc<Mutex<Vec<Event>>>,
        /// What `Links::speaking` answers (a reply plays).
        speaking: Arc<std::sync::atomic::AtomicBool>,
        /// What `Links::spoken` answers (the reply's sentences).
        spoken: Arc<Mutex<Vec<String>>>,
        /// S-3: the voice is settled (CallListening waits for it).
        voice: Arc<std::sync::atomic::AtomicBool>,
        tx: Option<Sender<Job>>,
        thread: Option<std::thread::JoinHandle<()>>,
    }

    const BARGE: &str = "barge";

    impl Harness {
        fn new() -> Self {
            let (probe, opened, _) = Probe::new(false);
            let listening = Arc::new(Listening::default());
            let events: Arc<Mutex<Vec<Event>>> = Arc::default();
            let (sink, barged) = (events.clone(), events.clone());
            let speaking: Arc<std::sync::atomic::AtomicBool> = Arc::default();
            let plays = speaking.clone();
            let spoken: Arc<Mutex<Vec<String>>> = Arc::default();
            let replying = spoken.clone();
            let voice = Arc::new(std::sync::atomic::AtomicBool::new(true));
            let settled = voice.clone();
            let links = Links {
                listening: listening.clone(),
                barge: Arc::new(move || barged.lock().unwrap().push(Event::Finished { id: BARGE.into(), stopped: true })),
                far: Arc::default(),
                emit: Arc::new(move |event| sink.lock().unwrap().push(event.clone())),
                speaking: Arc::new(move || plays.load(Ordering::SeqCst)),
                spoken: Arc::new(move || replying.lock().unwrap().clone()),
                voice_ready: Arc::new(move || settled.load(Ordering::SeqCst)),
            };
            let hearing = probe.hearing();
            let (tx, thread) = spawn(move || hearing, links);
            Harness { probe, opened, listening, events, speaking, spoken, voice, tx: Some(tx), thread: Some(thread) }
        }

        fn send(&self, job: Job) {
            self.tx.as_ref().unwrap().send(job).unwrap();
        }

        fn call(&self, id: &str, remote: bool) {
            // The reader refuses a call during a meeting before it marks one.
            if self.listening.meeting().is_none() {
                self.listening.begin_call(id, remote);
            }
            self.send(Job::Call { id: id.into(), language: "en".into(), remote });
        }

        /// The events once `done` holds (10 s at most).
        fn until(&self, done: impl Fn(&[Event]) -> bool) -> Vec<Event> {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                let events = self.events.lock().unwrap().clone();
                if done(&events) || Instant::now() > deadline {
                    return events;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }

        fn said(&self, count: usize) -> Vec<Event> {
            let events = self.until(|events| said(events).len() >= count);
            assert_eq!(said(&events).len(), count, "{:?}", without_levels(&events));
            events
        }

        /// EOF: the thread ends.
        fn close(mut self) -> Vec<Event> {
            drop(self.tx.take());
            self.thread.take().unwrap().join().unwrap();
            self.events.lock().unwrap().clone()
        }
    }

    fn said(events: &[Event]) -> Vec<String> {
        events.iter().filter_map(|event| if let Event::Said { text, .. } = event { Some(text.clone()) } else { None }).collect()
    }

    fn without_levels(events: &[Event]) -> Vec<&Event> {
        events.iter().filter(|event| !matches!(event, Event::Level { .. })).collect()
    }

    fn woke(events: &[Event]) -> Vec<String> {
        events.iter().filter_map(|event| if let Event::Woke { id } = event { Some(id.clone()) } else { None }).collect()
    }

    /// The fake transcriber's first clip is `clip1`: the phrase that wakes.
    fn say_the_phrase(harness: &Harness) {
        for chunk in phrase(0.8).chunks(320) {
            harness.probe.say(chunk.to_vec());
        }
    }

    /// #1263 review: while a reply plays (and its echo tail after) the wake
    /// word never checks what it hears, so the agent can't wake itself or
    /// barge in on itself; the person's phrase after the reply wakes.
    #[test]
    fn the_voice_never_wakes_itself() {
        let harness = Harness::new();
        harness.speaking.store(true, Ordering::SeqCst);
        harness.send(Job::Wake { phrase: "Clip1".into(), language: "en".into() });
        harness.opened.recv_timeout(Duration::from_secs(10)).expect("the wake word opened the microphone");
        say_the_phrase(&harness);
        say_the_phrase(&harness);
        std::thread::sleep(Duration::from_millis(1_000));
        assert!(woke(&harness.events.lock().unwrap()).is_empty(), "the reply's own words never wake");
        harness.speaking.store(false, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(crate::wake::ECHO_TAIL_MS as u64 + 200));
        say_the_phrase(&harness);
        let events = harness.until(|events| !woke(events).is_empty());
        assert_eq!(woke(&events), ["wake-1"], "never transcribed before: the first clip wakes");
        harness.close();
    }

    /// #NEW-86: for the cooldown after a wake turn ends the scan checks
    /// nothing it hears (the phrase said again at once is not transcribed);
    /// after it the phrase wakes again.
    #[test]
    fn a_wake_turn_cools_down_before_the_next() {
        let harness = Harness::new();
        harness.send(Job::Wake { phrase: "Clip1".into(), language: "en".into() });
        harness.opened.recv_timeout(Duration::from_secs(10)).expect("opened");
        say_the_phrase(&harness);
        harness.until(|events| !woke(events).is_empty());
        // The turn ends after the quiet (its answer does not matter here).
        for chunk in tone(crate::wake::NO_COMMAND_MS as f32 / 1000.0 + 0.5, 0.0).chunks(320) {
            harness.probe.say(chunk.to_vec());
        }
        let ended = |events: &[Event]| events.iter().any(|event| matches!(event, Event::Heard { id, .. } | Event::Empty { id } if id == "wake-1"));
        let events = harness.until(ended);
        assert!(ended(&events), "{:?}", without_levels(&events));
        let deadline = Instant::now() + Duration::from_secs(5);
        while harness.probe.opened.lock().unwrap().len() < 2 {
            assert!(Instant::now() < deadline, "the scan reopened");
            std::thread::sleep(Duration::from_millis(5));
        }
        // The next transcription would be clip3: listen for that.
        harness.send(Job::Wake { phrase: "Clip3".into(), language: "en".into() });
        say_the_phrase(&harness);
        std::thread::sleep(Duration::from_millis(600));
        assert_eq!(woke(&harness.events.lock().unwrap()), ["wake-1"], "inside the cooldown: never transcribed");
        std::thread::sleep(Duration::from_millis(crate::wake::COOLDOWN_MS as u64));
        say_the_phrase(&harness);
        let events = harness.until(|events| woke(events).len() == 2);
        assert_eq!(woke(&events), ["wake-1", "wake-2"], "after the cooldown the phrase wakes");
        harness.close();
    }

    /// #1263 review: Wake word off (or Mute) ends a wake turn in flight in
    /// the worker, unanswered: nothing it heard is sent.
    #[test]
    fn wake_word_off_ends_a_wake_turn_unanswered() {
        let harness = Harness::new();
        harness.send(Job::Wake { phrase: "Clip1".into(), language: "en".into() });
        harness.opened.recv_timeout(Duration::from_secs(10)).expect("opened");
        say_the_phrase(&harness);
        let events = harness.until(|events| !woke(events).is_empty());
        assert_eq!(woke(&events), ["wake-1"]);
        assert!(harness.listening.active(), "the turn runs");
        harness.send(Job::Wake { phrase: String::new(), language: "en".into() });
        let deadline = Instant::now() + Duration::from_secs(5);
        while harness.listening.active() {
            assert!(Instant::now() < deadline, "the turn ended");
            std::thread::sleep(Duration::from_millis(5));
        }
        let events = harness.close();
        assert!(!events.iter().any(|event| matches!(event, Event::Heard { .. } | Event::Empty { .. } | Event::ListenFailed { .. })), "{:?}", without_levels(&events));
    }

    /// NEW-55: a local call opens the microphone and answers Said per
    /// utterance, never Heard or Empty; barge-in runs before SpeechStarted.
    #[test]
    fn a_local_call_says_each_utterance_and_barges_in_first() {
        let harness = Harness::new();
        harness.call("c1", false);
        harness.opened.recv_timeout(Duration::from_secs(10)).expect("the microphone opened");
        for _ in 0..2 {
            for chunk in phrase(0.8).chunks(320) {
                harness.probe.say(chunk.to_vec());
            }
        }
        let events = harness.said(2);
        let events = without_levels(&events);
        let kinds: Vec<String> = events
            .iter()
            .map(|event| match event {
                Event::Finished { id, .. } if id == BARGE => "barge".to_string(),
                Event::SpeechStarted { id } => format!("started {id}"),
                Event::Said { id, text } => format!("said {id} {text}"),
                Event::CallListening { id } => format!("listening {id}"),
                other => format!("{other:?}"),
            })
            .collect();
        assert_eq!(kinds, ["listening c1", "barge", "started c1", "said c1 clip1", "barge", "started c1", "said c1 clip2"]);
        assert!(harness.listening.active(), "a call runs until Hangup");
        let events = harness.close();
        assert!(!events.iter().any(|event| matches!(event, Event::Heard { .. } | Event::Empty { .. })));
    }

    /// #NEW-92 (#1281 review): a local call hands the reply's sentences to
    /// the echo text rule, and a short answer that repeats them goes out.
    #[test]
    fn a_local_call_says_a_short_answer_that_repeats_the_reply() {
        let harness = Harness::new();
        *harness.spoken.lock().unwrap() = vec!["Should I merge clip1 or close it?".into()];
        harness.call("c1", false);
        harness.opened.recv_timeout(Duration::from_secs(10)).expect("the microphone opened");
        for chunk in phrase(0.8).chunks(320) {
            harness.probe.say(chunk.to_vec());
        }
        let events = harness.said(1);
        let said: Vec<&Event> = events.iter().filter(|event| matches!(event, Event::Said { .. })).collect();
        assert_eq!(said, [&Event::Said { id: "c1".into(), text: "clip1".into() }]);
        harness.close();
    }

    /// NEW-55: Hangup closes the microphone without an answer.
    #[test]
    fn hangup_ends_the_call_without_an_answer() {
        let harness = Harness::new();
        harness.call("c1", false);
        harness.opened.recv_timeout(Duration::from_secs(10)).unwrap();
        // Mid-utterance.
        for chunk in tone(0.6, 0.3).chunks(320) {
            harness.probe.say(chunk.to_vec());
        }
        harness.until(|events| events.iter().any(|event| matches!(event, Event::SpeechStarted { .. })));
        harness.send(Job::Hangup { id: "c1".into() });
        let closed = harness.probe.opened.lock().unwrap()[0].clone();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !closed.load(Ordering::SeqCst) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(closed.load(Ordering::SeqCst), "the microphone closed");
        assert!(!harness.listening.active());
        let events = harness.close();
        assert!(said(&events).is_empty(), "the open utterance is dropped");
        assert!(!events.iter().any(|event| matches!(event, Event::ListenFailed { .. } | Event::Heard { .. } | Event::Empty { .. })));
    }

    /// NEW-55: muted, the audio is dropped (no Level, no utterance).
    #[test]
    fn a_muted_call_hears_nothing() {
        let harness = Harness::new();
        harness.call("c1", false);
        harness.opened.recv_timeout(Duration::from_secs(10)).unwrap();
        harness.until(|events| events.contains(&Event::CallListening { id: "c1".into() }));
        harness.events.lock().unwrap().clear();
        harness.send(Job::Mute { id: "c1".into(), muted: true });
        // The mute is handled before the next chunk (jobs come first).
        harness.until(|_| true);
        std::thread::sleep(Duration::from_millis(50));
        for chunk in phrase(0.8).chunks(320) {
            harness.probe.say(chunk.to_vec());
        }
        std::thread::sleep(Duration::from_millis(200));
        assert!(harness.events.lock().unwrap().is_empty(), "nothing while muted");
        harness.send(Job::Mute { id: "c1".into(), muted: false });
        std::thread::sleep(Duration::from_millis(50));
        for chunk in phrase(0.8).chunks(320) {
            harness.probe.say(chunk.to_vec());
        }
        let events = harness.said(1);
        assert_eq!(said(&events), ["clip1"]);
        harness.close();
    }

    /// NEW-55: a remote call never opens the microphone; its audio comes as
    /// jobs, and a Listen during it is refused.
    #[test]
    fn a_remote_call_hears_streamed_audio() {
        let harness = Harness::new();
        harness.call("c1", true);
        for chunk in phrase(0.8).chunks(320) {
            harness.send(Job::Audio { id: "c1".into(), samples: chunk.to_vec() });
        }
        // Audio of another call is dropped.
        for chunk in phrase(0.8).chunks(320) {
            harness.send(Job::Audio { id: "other".into(), samples: chunk.to_vec() });
        }
        let events = harness.said(1);
        assert!(events.iter().any(|event| matches!(event, Event::Level { id, .. } if id == "c1")));
        assert!(harness.probe.opened.lock().unwrap().is_empty(), "no microphone");
        assert!(harness.listening.remote_call());
        // Push to talk is refused: the call goes on (micOwnerCases).
        harness.send(Job::Start { id: "u1".into(), language: "en".into() });
        harness.until(|events| events.iter().any(|event| matches!(event, Event::Empty { id } if id == "u1")));
        assert!(harness.probe.opened.lock().unwrap().is_empty(), "no microphone opened");
        assert!(harness.listening.remote_call());
        for chunk in phrase(0.8).chunks(320) {
            harness.send(Job::Audio { id: "c1".into(), samples: chunk.to_vec() });
        }
        harness.said(2);
        harness.close();
    }

    /// S-3: a call whose recognizer is still downloading stays pending (no
    /// ListenFailed): its microphone opens after the download, and
    /// CallListening waits for the voice too; nothing said before it is heard.
    #[test]
    fn a_call_warms_up_through_a_download_and_the_voice() {
        let harness = Harness::new();
        harness.voice.store(false, Ordering::SeqCst);
        *harness.probe.downloads.lock().unwrap() = (3, false);
        harness.call("c1", false);
        harness.opened.recv_timeout(Duration::from_secs(10)).expect("the microphone opened after the download");
        assert_eq!(harness.probe.downloads.lock().unwrap().0, 0, "it waited the download out");
        // The voice still loads: what the microphone hears is nobody's.
        for chunk in phrase(0.8).chunks(320) {
            harness.probe.say(chunk.to_vec());
        }
        std::thread::sleep(Duration::from_millis(300));
        {
            let events = harness.events.lock().unwrap();
            assert!(events.is_empty(), "nothing before CallListening: {:?}", without_levels(&events));
        }
        harness.voice.store(true, Ordering::SeqCst);
        let events = harness.until(|events| !events.is_empty());
        assert_eq!(events.first(), Some(&Event::CallListening { id: "c1".into() }));
        for chunk in phrase(0.8).chunks(320) {
            harness.probe.say(chunk.to_vec());
        }
        let events = harness.said(1);
        assert_eq!(events.iter().filter(|event| matches!(event, Event::CallListening { .. })).count(), 1, "once per call");
        assert!(!events.iter().any(|event| matches!(event, Event::ListenFailed { .. })));
        harness.close();
    }

    /// S-3: a download (or load) that fails fails the call `unavailable`,
    /// never `downloading`; a Hangup while it downloads ends it unanswered.
    #[test]
    fn a_failed_download_fails_the_call_and_a_hangup_ends_a_pending_one() {
        let harness = Harness::new();
        *harness.probe.downloads.lock().unwrap() = (2, true);
        harness.call("c1", true);
        let events = harness.until(|events| events.iter().any(|event| matches!(event, Event::ListenFailed { .. })));
        assert_eq!(
            without_levels(&events),
            [&Event::ListenFailed { id: "c1".into(), reason: ListenFailure::Unavailable, detail: "the download failed".into() }]
        );
        assert!(!harness.listening.active());
        *harness.probe.downloads.lock().unwrap() = (usize::MAX, false);
        harness.call("c2", false);
        std::thread::sleep(Duration::from_millis(200));
        harness.listening.end("c2");
        harness.send(Job::Hangup { id: "c2".into() });
        std::thread::sleep(Duration::from_millis(200));
        let events = harness.close();
        assert_eq!(events.len(), 1, "nothing for the hung-up call: {:?}", without_levels(&events));
        assert!(!events.iter().any(|event| matches!(event, Event::CallListening { .. })));
    }

    /// #1265 review: talk-listen.json `micOwnerCases`. A call takes
    /// precedence: it closes the wake word's microphone, the scan pauses for
    /// the whole call and comes back after Hangup; a Listen during a call is
    /// refused. Never two microphones open at once.
    #[test]
    fn mic_owner_cases() {
        const FIXTURE: &str = include_str!("../../../../../packages/domain-contract/fixtures/talk-listen.json");
        let fixture: serde_json::Value = serde_json::from_str(FIXTURE).unwrap();
        for case in fixture["micOwnerCases"]["cases"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let harness = Harness::new();
            let mut listens = 0;
            for (index, step) in case["steps"].as_array().unwrap().iter().enumerate() {
                let at = format!("{name}, step {index}");
                let before = harness.events.lock().unwrap().len();
                let answer = step["answer"].as_str();
                match step["do"].as_str().unwrap() {
                    "wake" => harness.send(Job::Wake { phrase: "Clip1".into(), language: "en".into() }),
                    "call" => harness.call("c1", false),
                    "remoteCall" => harness.call("c1", true),
                    "hangup" => {
                        harness.listening.end("c1");
                        harness.send(Job::Hangup { id: "c1".into() });
                    }
                    "listen" => {
                        listens += 1;
                        let id = format!("u{listens}");
                        // The reader's gate refuses it; the loop does too.
                        assert!(!harness.listening.begin(&id), "{at}: the gate refuses a Listen during a call or a meeting");
                        harness.send(Job::Start { id, language: "en".into() });
                    }
                    "phrase" => say_the_phrase(&harness),
                    // NEW-57.
                    "meeting" => meeting(&harness, "m1", None),
                    "meetingStop" => {
                        harness.listening.end_meeting("m1");
                        harness.send(Job::MeetingStop { id: "m1".into(), drain: true });
                    }
                    other => panic!("{at}: unknown step {other}"),
                }
                // NEW-57: a meeting's flush trigger (the phrase opening a
                // `you` segment, a Listen during it).
                let flushed = |events: &[Event]| {
                    events[before..].iter().any(|event| matches!(event, Event::MeetingWake { .. } | Event::MeetingFlushNow { .. }))
                };
                let produced = |events: &[Event]| -> Option<&'static str> {
                    events[before..]
                        .iter()
                        .find_map(|event| match event {
                            Event::Woke { .. } => Some("woke"),
                            Event::Said { .. } => Some("said"),
                            Event::Empty { .. } | Event::Heard { .. } => Some("empty"),
                            Event::ListenFailed { reason: ListenFailure::Meeting, .. } => Some("refused"),
                            _ => None,
                        })
                        .or_else(|| flushed(events).then_some("flush"))
                };
                let flush = answer == Some("flush") || step["flush"].as_bool() == Some(true);
                // The tray's dot: lit while the wake word holds the microphone.
                let dot = |events: &[Event]| {
                    let last = events.iter().rev().find_map(|event| match event {
                        Event::WakeListening { listening, .. } => Some(*listening),
                        _ => None,
                    });
                    last.unwrap_or(false)
                };
                let scanning = matches!(step["mic"].as_str(), Some("wake" | "listen"));
                let events = harness.until(|events| {
                    produced(events) == answer
                        && flushed(events) == flush
                        && dot(events) == scanning
                        && mic_owner(&harness) == step["mic"].as_str()
                });
                assert_eq!(flushed(&events), flush, "{at}: a flush trigger: {:?}", without_levels(&events[before..]));
                assert_eq!(produced(&events), answer, "{at}: {:?}", without_levels(&events[before..]));
                assert_eq!(mic_owner(&harness), step["mic"].as_str(), "{at}: the microphone's owner");
                if matches!(answer, Some("said" | "flush")) {
                    assert!(woke(&events[before..]).is_empty(), "{at}: no wake match during a call or a meeting");
                }
                assert_eq!(dot(&events), scanning, "{at}: the tray's dot");
            }
            let events = harness.close();
            // A replaced wake turn is never answered.
            assert!(!events.iter().any(|event| matches!(event, Event::Heard { id, .. } | Event::Empty { id } if id.starts_with("wake-"))), "{name}");
        }
    }

    fn meeting_segments(events: &[Event]) -> Vec<(String, String, String)> {
        events
            .iter()
            .filter_map(|event| match event {
                Event::MeetingSegment { channel, label, text, .. } => Some((channel.clone(), label.clone(), text.clone())),
                _ => None,
            })
            .collect()
    }

    fn open_mics(harness: &Harness) -> usize {
        harness.probe.opened.lock().unwrap().iter().filter(|closed| !closed.load(Ordering::SeqCst)).count()
    }

    fn wait(what: &str, done: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !done() {
            assert!(Instant::now() < deadline, "{what}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn meeting(harness: &Harness, id: &str, selection: Option<etalk::apps::Selection>) {
        harness.listening.begin_meeting(id);
        harness.send(Job::Meeting { id: id.into(), language: "en".into(), phrase: "Clip1".into(), you_label: "You".into(), selection });
    }

    /// NEW-57 (meeting.json MICROPHONE, CHANNELS): the meeting takes the
    /// wake word's microphone (ONE open), hears `you` and every app on its
    /// own, carries the wake phrase itself, and hands the microphone back
    /// to the wake word when it stops.
    #[test]
    fn a_meeting_owns_the_microphone_and_hears_each_channel_apart() {
        let harness = Harness::new();
        harness.send(Job::Wake { phrase: "Clip1".into(), language: "en".into() });
        harness.opened.recv_timeout(Duration::from_secs(10)).expect("the wake word opens the microphone");
        meeting(&harness, "m1", Some(etalk::apps::Selection::All));
        harness.opened.recv_timeout(Duration::from_secs(10)).expect("the meeting opens the microphone");
        wait("the meeting's desktop audio opens", || !harness.probe.apps.lock().unwrap().is_empty());
        assert_eq!(open_mics(&harness), 1, "the wake word let it go");
        assert!(!harness.listening.begin("u1"), "push to talk never takes it");
        // The phrase into the microphone: a segment and the flush trigger,
        // never a wake turn.
        say_the_phrase(&harness);
        harness.until(|events| !meeting_segments(events).is_empty());
        // The app plays (its own VAD stream, interleaved with silence on
        // the microphone).
        for chunk in phrase(0.8).chunks(320) {
            harness.probe.play("us.zoom.xos", "Zoom", chunk.to_vec());
            harness.probe.say(vec![0.0; 320]);
        }
        let events = harness.until(|events| meeting_segments(events).len() >= 2);
        assert_eq!(
            meeting_segments(&events),
            [("you".into(), "You".into(), "clip1".into()), ("us.zoom.xos".into(), "Zoom".into(), "clip2".into())]
        );
        let you_at = events.iter().position(|event| matches!(event, Event::MeetingSegment { channel, .. } if channel == "you")).unwrap();
        assert!(matches!(events.get(you_at + 1), Some(Event::MeetingWake { id }) if id == "m1"), "{:?}", without_levels(&events));
        assert!(woke(&events).is_empty(), "the wake scan is paused");
        assert!(events.iter().any(|event| matches!(event, Event::MeetingListening { listening: true, .. })));
        assert!(!events.iter().any(|event| matches!(event, Event::Level { .. })), "nothing draws a wave");

        // Paused: nothing is heard.
        harness.send(Job::MeetingPause { id: "m1".into(), paused: true });
        std::thread::sleep(Duration::from_millis(100));
        say_the_phrase(&harness);
        std::thread::sleep(Duration::from_millis(200));
        harness.send(Job::MeetingPause { id: "m1".into(), paused: false });
        std::thread::sleep(Duration::from_millis(100));
        for chunk in phrase(0.8).chunks(320) {
            harness.probe.say(chunk.to_vec());
        }
        let events = harness.until(|events| meeting_segments(events).len() >= 3);
        assert_eq!(meeting_segments(&events)[2], ("you".into(), "You".into(), "clip3".into()), "the paused phrase was dropped");

        harness.listening.end_meeting("m1");
        harness.send(Job::MeetingStop { id: "m1".into(), drain: true });
        harness.opened.recv_timeout(Duration::from_secs(10)).expect("the wake word takes the microphone back");
        wait("one microphone open", || open_mics(&harness) == 1);
        let events = harness.close();
        assert!(events.iter().any(|event| matches!(event, Event::MeetingListening { listening: false, reason: None, .. })));
    }

    /// NEW-57 (meeting.json CHANNELS): an app segment is dated by its
    /// chunks' CAPTURE time, never by when the worker read them (the feed
    /// site in `hear_meeting`; `epoch_ms()` there fails this).
    #[test]
    fn an_app_segment_is_dated_by_its_capture_time() {
        let harness = Harness::new();
        meeting(&harness, "m1", Some(etalk::apps::Selection::All));
        harness.opened.recv_timeout(Duration::from_secs(10)).expect("the meeting opens the microphone");
        wait("the meeting's desktop audio opens", || !harness.probe.apps.lock().unwrap().is_empty());
        // Captured an hour ago (a stalled queue), 20 ms per chunk.
        let base = etalk::apps::now_ms() - 3_600_000;
        for (n, chunk) in phrase(0.8).chunks(320).enumerate() {
            harness.probe.play_at("us.zoom.xos", "Zoom", chunk.to_vec(), base + 20 * n as u64);
            harness.probe.say(vec![0.0; 320]);
        }
        let events = harness.until(|events| meeting_segments(events).iter().any(|(channel, ..)| channel == "us.zoom.xos"));
        let start = events
            .iter()
            .find_map(|event| match event {
                Event::MeetingSegment { channel, start_ms, .. } if channel == "us.zoom.xos" => Some(*start_ms),
                _ => None,
            })
            .unwrap();
        assert!(start >= base as i64 && start < base as i64 + 60_000, "dated {start}, captured from {base}");
        harness.listening.end_meeting("m1");
        harness.send(Job::MeetingStop { id: "m1".into(), drain: false });
        harness.close();
    }

    /// NEW-57: a meeting asked for during a call waits for its Hangup (one
    /// owner at a time); a meeting without desktop audio opens none.
    #[test]
    fn a_meeting_waits_for_a_running_call() {
        let harness = Harness::new();
        harness.call("c1", false);
        harness.opened.recv_timeout(Duration::from_secs(10)).expect("the call opens the microphone");
        meeting(&harness, "m1", None);
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(harness.probe.opened.lock().unwrap().len(), 1, "the call keeps it");
        harness.listening.end("c1");
        harness.send(Job::Hangup { id: "c1".into() });
        harness.opened.recv_timeout(Duration::from_secs(10)).expect("the meeting opens it after Hangup");
        wait("one microphone open", || open_mics(&harness) == 1);
        assert!(harness.probe.apps.lock().unwrap().is_empty(), "no desktop audio asked for");
        for chunk in phrase(0.8).chunks(320) {
            harness.probe.say(chunk.to_vec());
        }
        let events = harness.until(|events| !meeting_segments(events).is_empty());
        assert_eq!(meeting_segments(&events), [("you".into(), "You".into(), "clip1".into())]);
        harness.close();
    }

    /// NEW-57 (meeting.json FLUSH): a draining stop closes the open
    /// segment mid-phrase, emits it, then MeetingStopped; a plain stop
    /// drops it.
    #[test]
    fn a_draining_stop_emits_the_open_segment_then_stopped() {
        for drain in [true, false] {
            let harness = Harness::new();
            meeting(&harness, "m1", None);
            harness.opened.recv_timeout(Duration::from_secs(10)).expect("the meeting opens the microphone");
            // Mid-phrase: no silence closed it yet.
            harness.probe.say([tone(0.2, 0.0), tone(0.8, 0.3)].concat());
            std::thread::sleep(Duration::from_millis(300));
            harness.listening.end_meeting("m1");
            harness.send(Job::MeetingStop { id: "m1".into(), drain });
            let events = harness.close();
            let tail: Vec<&str> = events
                .iter()
                .filter_map(|event| match event {
                    Event::MeetingSegment { .. } => Some("segment"),
                    Event::MeetingStopped { .. } => Some("stopped"),
                    _ => None,
                })
                .collect();
            let expected: &[&str] = if drain { &["segment", "stopped"] } else { &[] };
            assert_eq!(tail, expected, "drain {drain}");
        }
    }

    #[test]
    fn the_gate_refuses_push_to_talk_during_a_meeting() {
        let listening = Listening::default();
        listening.begin_meeting("m1");
        assert_eq!(listening.meeting().as_deref(), Some("m1"));
        assert!(!listening.begin("u1"));
        listening.end_meeting("m0");
        assert!(listening.meeting().is_some(), "only its own stop ends it");
        listening.end_meeting("m1");
        assert!(listening.begin("u1"));
    }

    /// Who holds the device's microphone: `wake`, `listen`, `call` or none
    /// (never two at once).
    fn mic_owner(harness: &Harness) -> Option<&'static str> {
        let open = harness.probe.opened.lock().unwrap().iter().filter(|closed| !closed.load(Ordering::SeqCst)).count();
        assert!(open <= 1, "{open} microphones open");
        if open == 0 {
            None
        } else if harness.listening.meeting().is_some() && !harness.listening.active() {
            Some("meeting")
        } else if harness.listening.in_call() {
            Some("call")
        } else if harness.listening.refuses_speech() {
            Some("listen")
        } else {
            Some("wake")
        }
    }
}
