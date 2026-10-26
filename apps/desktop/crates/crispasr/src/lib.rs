//! NEW-74: CrispASR for exponentialTalk, shared by the speaker (NEW-74) and
//! the transcriber (NEW-54). Runs ONLY inside the speech worker.
//!
//! - [`artifact`]: pinned downloads, verified before they are unpacked or
//!   loaded.
//! - [`platform`]: the pinned prebuilt runtime per platform and the cheap
//!   GPU-driver check that runs before any download.
//! - [`Runtime`]: the loaded library (`libloading`, by absolute path after
//!   [`artifact::verify`]), its GPU devices, and sessions on the GPU or the
//!   CPU ([`Device`]).
//! - [`models`]: the pinned model files (Qwen3-TTS 0.6B Base F16 and its
//!   12 Hz codec; NEW-78: Parakeet TDT 0.6B v3 and Silero VAD).
//! - NEW-78: [`AsrSession`] (speech to text) and [`VadStream`] (speech
//!   probability per 32 ms frame, streaming) for the transcriber.

pub mod artifact;
pub mod models;
pub mod platform;

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use artifact::Artifact;

/// Where the runtime lives: inside the app bundle where it ships there
/// (macOS), else the download dir `{data_dir}/talk/crispasr-<version>/`.
pub fn runtime_dir(data_dir: &Path) -> PathBuf {
    runtime_dir_of(data_dir, Build::Gpu)
}

/// NEW-78: where `build` lives: the GPU build in the app bundle where it
/// ships there (macOS), else under `{data_dir}/talk/`.
pub fn runtime_dir_of(data_dir: &Path, build: Build) -> PathBuf {
    match build {
        Build::Gpu => platform::bundled_dir().unwrap_or_else(|| data_dir.join("talk").join(build.dir_name())),
        Build::Cpu => data_dir.join("talk").join(build.dir_name()),
    }
}

/// NEW-78: which pinned runtime build a process loads (ONE per process:
/// both builds name their libraries alike).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Build {
    /// The GPU build ([`platform::ARTIFACTS`]); its sessions also run on
    /// the CPU. Needs the platform's GPU driver.
    Gpu,
    /// The CPU-only build ([`platform::CPU_ARTIFACTS`]).
    Cpu,
}

impl Build {
    /// The build this machine can load: the GPU build where the driver
    /// exists, else the CPU build; `None` = no pinned runtime here.
    pub fn for_this_machine() -> Option<Build> {
        if platform::supported() && platform::gpu_driver_present() {
            Some(Build::Gpu)
        } else if platform::cpu_supported() {
            Some(Build::Cpu)
        } else {
            None
        }
    }

    pub fn artifacts(self) -> &'static [Artifact] {
        match self {
            Build::Gpu => platform::ARTIFACTS,
            Build::Cpu => platform::CPU_ARTIFACTS,
        }
    }

    fn dir_name(self) -> &'static str {
        match self {
            Build::Gpu => platform::RUNTIME_DIR,
            Build::Cpu => platform::CPU_RUNTIME_DIR,
        }
    }

    fn libraries(self) -> (&'static str, &'static str) {
        match self {
            Build::Gpu => (platform::LIBRARY, platform::GGML),
            Build::Cpu => (platform::CPU_LIBRARY, platform::CPU_GGML),
        }
    }
}

/// Where a session runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Device {
    /// The platform's GPU backend ([`platform::GPU_BACKEND`]).
    Gpu,
    Cpu,
}

/// One GPU ggml sees (`ggml_backend_dev_*`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gpu {
    pub description: String,
    /// Shares system memory (an integrated GPU).
    pub integrated: bool,
    pub total_bytes: u64,
    pub free_bytes: u64,
}

type SessionPtr = *mut c_void;
type ResultPtr = *mut c_void;
type VadPtr = *mut c_void;

/// `whisper_vad_context_params` (crispasr.h).
#[repr(C)]
#[derive(Clone, Copy)]
struct VadContextParams {
    n_threads: c_int,
    use_gpu: bool,
    gpu_device: c_int,
}

type LogCallback = unsafe extern "C" fn(c_int, *const c_char, *mut c_void);

struct Fns {
    set_gpu_backend: unsafe extern "C" fn(*const c_char),
    open_explicit: unsafe extern "C" fn(*const c_char, *const c_char, c_int) -> SessionPtr,
    close: unsafe extern "C" fn(SessionPtr),
    set_codec_path: unsafe extern "C" fn(SessionPtr, *const c_char) -> c_int,
    set_voice: unsafe extern "C" fn(SessionPtr, *const c_char, *const c_char) -> c_int,
    set_target_language: unsafe extern "C" fn(SessionPtr, *const c_char) -> c_int,
    set_reference_language: unsafe extern "C" fn(SessionPtr, *const c_char) -> c_int,
    synthesize: unsafe extern "C" fn(SessionPtr, *const c_char, *mut c_int) -> *mut f32,
    pcm_free: unsafe extern "C" fn(*mut f32),
    output_sample_rate: unsafe extern "C" fn(SessionPtr) -> c_int,
    last_synth_error: unsafe extern "C" fn(SessionPtr) -> *const c_char,
    dev_count: unsafe extern "C" fn() -> usize,
    dev_get: unsafe extern "C" fn(usize) -> *mut c_void,
    dev_type: unsafe extern "C" fn(*mut c_void) -> c_int,
    dev_memory: unsafe extern "C" fn(*mut c_void, *mut usize, *mut usize),
    dev_description: unsafe extern "C" fn(*mut c_void) -> *const c_char,
    // NEW-78: speech to text and the VAD.
    transcribe_lang: unsafe extern "C" fn(SessionPtr, *const f32, c_int, *const c_char) -> ResultPtr,
    result_n_segments: unsafe extern "C" fn(ResultPtr) -> c_int,
    result_segment_text: unsafe extern "C" fn(ResultPtr, c_int) -> *const c_char,
    result_free: unsafe extern "C" fn(ResultPtr),
    vad_default_context_params: unsafe extern "C" fn() -> VadContextParams,
    vad_init: unsafe extern "C" fn(*const c_char, VadContextParams) -> VadPtr,
    vad_detect: unsafe extern "C" fn(VadPtr, *const f32, c_int) -> bool,
    vad_detect_continue: unsafe extern "C" fn(VadPtr, *const f32, c_int) -> bool,
    vad_n_probs: unsafe extern "C" fn(VadPtr) -> c_int,
    vad_probs: unsafe extern "C" fn(VadPtr) -> *mut f32,
    vad_free: unsafe extern "C" fn(VadPtr),
}

/// The loaded CrispASR library. Every session borrows it.
pub struct Runtime {
    fns: Fns,
    build: Build,
    /// CrispASR's GPU preference is process-global and read when a session
    /// opens: setting it and opening happen under this lock (the speaker
    /// and the transcriber open from different threads).
    open_lock: Mutex<()>,
    // Dropped last (after `fns`, whose pointers point into them).
    _ggml: libloading::Library,
    _library: libloading::Library,
}

const GGML_DEVICE_GPU: c_int = 1;
const GGML_DEVICE_IGPU: c_int = 2;

fn open_library(path: &Path) -> Result<libloading::Library, String> {
    // Right after `artifact::verify`: still the regular file it hashed, never
    // a symlink swapped in since.
    if !std::fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_file()) {
        return Err(format!("{} is not a regular file", path.display()));
    }
    #[cfg(windows)]
    let library = {
        // The DLL's own directory resolves its dependencies (ggml*.dll).
        use libloading::os::windows::{Library, LOAD_WITH_ALTERED_SEARCH_PATH};
        // SAFETY: a verified CrispASR/ggml library; its initialisers are inert.
        unsafe { Library::load_with_flags(path, LOAD_WITH_ALTERED_SEARCH_PATH) }.map(libloading::Library::from)
    };
    #[cfg(not(windows))]
    // SAFETY: a verified CrispASR/ggml library; its initialisers are inert.
    let library = unsafe { libloading::Library::new(path) };
    library.map_err(|err| format!("load {}: {err}", path.display()))
}

macro_rules! symbol {
    ($library:expr, $name:literal) => {
        // SAFETY: the signature matches CrispASR v0.8.45's header
        // (crispasr_session.h / crispasr.h / ggml-backend.h).
        *unsafe { $library.get(concat!($name, "\0").as_bytes()) }
            .map_err(|err| format!("{}: {err}", $name))?
    };
}

impl Runtime {
    /// Load the runtime from `dir` ([`runtime_dir`]), by absolute path. Every
    /// downloaded file is re-hashed first; a mismatch deletes it and fails
    /// (the next download replaces it). A bundled runtime is the signed
    /// bundle's (the OS checks its signature on load).
    pub fn load(dir: &Path) -> Result<Runtime, String> {
        Runtime::load_build(dir, Build::Gpu)
    }

    /// NEW-78: load `build` from `dir` (verified like [`Runtime::load`]).
    pub fn load_build(dir: &Path, build: Build) -> Result<Runtime, String> {
        let available = match build {
            Build::Gpu => platform::supported(),
            Build::Cpu => platform::cpu_supported(),
        };
        if !available {
            return Err(format!("no {build:?} runtime for this platform"));
        }
        for artifact in build.artifacts() {
            artifact::verify(dir, artifact).map_err(|err| err.to_string())?;
        }
        let (library_name, ggml_name) = build.libraries();
        let library = open_library(&dir.join(library_name))?;
        let ggml = open_library(&dir.join(ggml_name))?;
        let fns = Fns {
            set_gpu_backend: symbol!(library, "crispasr_set_gpu_backend"),
            open_explicit: symbol!(library, "crispasr_session_open_explicit"),
            close: symbol!(library, "crispasr_session_close"),
            set_codec_path: symbol!(library, "crispasr_session_set_codec_path"),
            set_voice: symbol!(library, "crispasr_session_set_voice"),
            set_target_language: symbol!(library, "crispasr_session_set_target_language"),
            set_reference_language: symbol!(library, "crispasr_session_set_tts_reference_language"),
            synthesize: symbol!(library, "crispasr_session_synthesize"),
            pcm_free: symbol!(library, "crispasr_pcm_free"),
            output_sample_rate: symbol!(library, "crispasr_session_output_sample_rate"),
            last_synth_error: symbol!(library, "crispasr_session_last_synth_error"),
            dev_count: symbol!(ggml, "ggml_backend_dev_count"),
            dev_get: symbol!(ggml, "ggml_backend_dev_get"),
            dev_type: symbol!(ggml, "ggml_backend_dev_type"),
            dev_memory: symbol!(ggml, "ggml_backend_dev_memory"),
            dev_description: symbol!(ggml, "ggml_backend_dev_description"),
            transcribe_lang: symbol!(library, "crispasr_session_transcribe_lang"),
            result_n_segments: symbol!(library, "crispasr_session_result_n_segments"),
            result_segment_text: symbol!(library, "crispasr_session_result_segment_text"),
            result_free: symbol!(library, "crispasr_session_result_free"),
            vad_default_context_params: symbol!(library, "whisper_vad_default_context_params"),
            vad_init: symbol!(library, "whisper_vad_init_from_file_with_params"),
            vad_detect: symbol!(library, "whisper_vad_detect_speech"),
            vad_detect_continue: symbol!(library, "whisper_vad_detect_speech_continue"),
            vad_n_probs: symbol!(library, "whisper_vad_n_probs"),
            vad_probs: symbol!(library, "whisper_vad_probs"),
            vad_free: symbol!(library, "whisper_vad_free"),
        };
        // NEW-78: CrispASR logs every VAD call and every decode at INFO; the
        // worker's stderr is the IDE's log, so only warnings and errors pass.
        // SAFETY: `whisper_log_set(ggml_log_callback, void*)` (crispasr.h);
        // a static callback that only reads the message.
        unsafe {
            if let Ok(log_set) = library.get::<LogSet>(b"whisper_log_set\0") {
                log_set(Some(log_warnings), std::ptr::null_mut());
            }
        }
        Ok(Runtime { fns, build, open_lock: Mutex::new(()), _ggml: ggml, _library: library })
    }

    /// NEW-78: the build this runtime was loaded from.
    pub fn build(&self) -> Build {
        self.build
    }

    /// Open `backend` on `model` on `device` (the preference is set and the
    /// session opened under [`Runtime::open_lock`]).
    fn open(&self, backend: &str, model: &Path, device: Device) -> Result<SessionPtr, String> {
        let backend_name = CString::new(match device {
            Device::Gpu => platform::GPU_BACKEND,
            Device::Cpu => "cpu",
        })
        .expect("static");
        let model_path = path_c(model)?;
        let backend_c = CString::new(backend).map_err(|err| err.to_string())?;
        let threads = std::thread::available_parallelism().map(|n| n.get().clamp(1, 8) as c_int).unwrap_or(4);
        let _open = self.open_lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        // SAFETY: valid C strings; the session pointer is checked.
        let session = unsafe {
            (self.fns.set_gpu_backend)(backend_name.as_ptr());
            (self.fns.open_explicit)(model_path.as_ptr(), backend_c.as_ptr(), threads)
        };
        if session.is_null() {
            return Err(format!("{backend} failed to open {}", model.display()));
        }
        Ok(session)
    }

    /// NEW-78: open a speech-to-text session: `backend` (e.g. `parakeet`)
    /// on `model`, on `device`.
    pub fn open_asr(&self, backend: &str, model: &Path, device: Device) -> Result<AsrSession<'_>, String> {
        let session = self.open(backend, model, device)?;
        Ok(AsrSession { runtime: self, session })
    }

    /// NEW-78: open a streaming Silero VAD on `model` (one thread, on the
    /// CPU: the graph is tiny and CrispASR runs it there regardless).
    pub fn open_vad(&self, model: &Path) -> Result<VadStream<'_>, String> {
        let model_path = path_c(model)?;
        // SAFETY: a by-value struct; a valid C string; the pointer is checked.
        let vad = unsafe {
            let mut params = (self.fns.vad_default_context_params)();
            params.n_threads = 1;
            params.use_gpu = false;
            (self.fns.vad_init)(model_path.as_ptr(), params)
        };
        if vad.is_null() {
            return Err(format!("the VAD failed to open {}", model.display()));
        }
        Ok(VadStream { runtime: self, vad, pending: Vec::new(), started: false })
    }

    /// The GPUs ggml's backends found (dedicated and integrated).
    pub fn gpus(&self) -> Vec<Gpu> {
        let mut gpus = Vec::new();
        // SAFETY: ggml's device registry; indices below its count are valid.
        unsafe {
            for index in 0..(self.fns.dev_count)() {
                let device = (self.fns.dev_get)(index);
                if device.is_null() {
                    continue;
                }
                let kind = (self.fns.dev_type)(device);
                if kind != GGML_DEVICE_GPU && kind != GGML_DEVICE_IGPU {
                    continue;
                }
                let (mut free, mut total) = (0usize, 0usize);
                (self.fns.dev_memory)(device, &mut free, &mut total);
                gpus.push(Gpu {
                    description: c_text((self.fns.dev_description)(device)),
                    integrated: kind == GGML_DEVICE_IGPU,
                    total_bytes: total as u64,
                    free_bytes: free as u64,
                });
            }
        }
        gpus
    }

    /// Open a text-to-speech session: `backend` (e.g. `qwen3-tts`) on
    /// `model`, with its `codec` companion, on `device`.
    pub fn open_tts(&self, backend: &str, model: &Path, codec: &Path, device: Device) -> Result<TtsSession<'_>, String> {
        let session = TtsSession { runtime: self, session: self.open(backend, model, device)? };
        let codec_path = path_c(codec)?;
        // SAFETY: a live session and a valid C string.
        if unsafe { (self.fns.set_codec_path)(session.session, codec_path.as_ptr()) } != 0 {
            return Err(format!("{backend} failed to load the codec {}", codec.display()));
        }
        Ok(session)
    }
}

type LogSet = unsafe extern "C" fn(Option<LogCallback>, *mut c_void);

/// ggml's log levels (`GGML_LOG_LEVEL_WARN` / `_ERROR`).
const GGML_LOG_WARN: c_int = 3;
const GGML_LOG_ERROR: c_int = 4;

unsafe extern "C" fn log_warnings(level: c_int, text: *const c_char, _user: *mut c_void) {
    if level == GGML_LOG_WARN || level == GGML_LOG_ERROR {
        let line = c_text(text);
        let line = line.trim_end();
        if !line.is_empty() {
            eprintln!("crispasr: {line}");
        }
    }
}

fn c_text(text: *const c_char) -> String {
    if text.is_null() {
        return String::new();
    }
    // SAFETY: CrispASR/ggml return NUL-terminated strings they own.
    unsafe { CStr::from_ptr(text) }.to_string_lossy().into_owned()
}

fn path_c(path: &Path) -> Result<CString, String> {
    CString::new(path.to_string_lossy().as_bytes()).map_err(|_| format!("{} is not a valid path", path.display()))
}

/// One open TTS session (a resident model). Not shared between threads.
pub struct TtsSession<'a> {
    runtime: &'a Runtime,
    session: SessionPtr,
}

impl TtsSession<'_> {
    /// Clone the voice of `wav` (its transcript `ref_text`), spoken in
    /// `language`; the next utterances speak `language` in that voice.
    pub fn set_voice(&mut self, wav: &Path, ref_text: &str, language: &str) -> Result<(), String> {
        let wav_c = path_c(wav)?;
        let text_c = CString::new(ref_text).map_err(|_| "the transcript has a NUL byte".to_string())?;
        let language_c = CString::new(language).map_err(|err| err.to_string())?;
        let fns = &self.runtime.fns;
        // SAFETY: a live session and valid C strings.
        unsafe {
            (fns.set_target_language)(self.session, language_c.as_ptr());
            (fns.set_reference_language)(self.session, language_c.as_ptr());
            if (fns.set_voice)(self.session, wav_c.as_ptr(), text_c.as_ptr()) != 0 {
                return Err(format!("the voice {} failed to load", wav.display()));
            }
        }
        Ok(())
    }

    /// Synthesize `text` (one sentence or more) into mono PCM at
    /// [`TtsSession::sample_rate`]. CrispASR watermarks every clip.
    pub fn synthesize(&mut self, text: &str) -> Result<Vec<f32>, String> {
        let text_c = CString::new(text).map_err(|_| "the text has a NUL byte".to_string())?;
        let fns = &self.runtime.fns;
        let mut samples: c_int = 0;
        // SAFETY: a live session; the returned buffer is ours until pcm_free.
        unsafe {
            let pcm = (fns.synthesize)(self.session, text_c.as_ptr(), &mut samples);
            if pcm.is_null() {
                let error = c_text((fns.last_synth_error)(self.session));
                return Err(if error.is_empty() { "synthesis failed".into() } else { error });
            }
            let out = std::slice::from_raw_parts(pcm, samples.max(0) as usize).to_vec();
            (fns.pcm_free)(pcm);
            Ok(out)
        }
    }

    pub fn sample_rate(&self) -> u32 {
        // SAFETY: a live session.
        let rate = unsafe { (self.runtime.fns.output_sample_rate)(self.session) };
        if rate > 0 { rate as u32 } else { 24_000 }
    }
}

impl Drop for TtsSession<'_> {
    fn drop(&mut self) {
        // SAFETY: closed exactly once.
        unsafe { (self.runtime.fns.close)(self.session) };
    }
}

/// NEW-78: one open speech-to-text session (a resident model). Not shared
/// between threads.
pub struct AsrSession<'a> {
    runtime: &'a Runtime,
    session: SessionPtr,
}

impl AsrSession<'_> {
    /// Transcribe mono 16 kHz `pcm` spoken in `language` (an ISO-639-1
    /// hint); the segments' text joined by spaces, untrimmed.
    pub fn transcribe(&mut self, pcm: &[f32], language: &str) -> Result<String, String> {
        let language_c = CString::new(language).map_err(|err| err.to_string())?;
        let samples = c_int::try_from(pcm.len()).map_err(|_| "the clip is too long".to_string())?;
        let fns = &self.runtime.fns;
        // SAFETY: a live session; the result is ours until result_free.
        unsafe {
            let result = (fns.transcribe_lang)(self.session, pcm.as_ptr(), samples, language_c.as_ptr());
            if result.is_null() {
                return Err("transcription failed".into());
            }
            let mut text = String::new();
            for index in 0..(fns.result_n_segments)(result).max(0) {
                if !text.is_empty() {
                    text.push(' ');
                }
                text.push_str(&c_text((fns.result_segment_text)(result, index)));
            }
            (fns.result_free)(result);
            Ok(text)
        }
    }
}

impl Drop for AsrSession<'_> {
    fn drop(&mut self) {
        // SAFETY: closed exactly once.
        unsafe { (self.runtime.fns.close)(self.session) };
    }
}

/// NEW-78: Silero's frame: 512 samples = 32 ms at 16 kHz.
pub const VAD_FRAME: usize = 512;

/// NEW-78: a streaming Silero VAD: the recurrent state carries across
/// [`VadStream::push`] calls, so a stream is scored one chunk at a time.
pub struct VadStream<'a> {
    runtime: &'a Runtime,
    vad: VadPtr,
    /// Samples short of a whole frame, scored with the next push.
    pending: Vec<f32>,
    /// The first call RESETS the recurrent state: a fresh context's state
    /// is uncleared memory, which scored seconds of speech as silence.
    started: bool,
}

impl VadStream<'_> {
    /// Score `samples` (mono 16 kHz, any length): one speech probability
    /// per whole [`VAD_FRAME`] completed by them, in order.
    pub fn push(&mut self, samples: &[f32]) -> Result<Vec<f32>, String> {
        self.pending.extend_from_slice(samples);
        let whole = self.pending.len() / VAD_FRAME * VAD_FRAME;
        if whole == 0 {
            return Ok(Vec::new());
        }
        let fns = &self.runtime.fns;
        // SAFETY: a live VAD context; `whole` samples are initialised; the
        // probabilities are read before the next call replaces them.
        let probs = unsafe {
            let detect = if self.started { fns.vad_detect_continue } else { fns.vad_detect };
            if !detect(self.vad, self.pending.as_ptr(), whole as c_int) {
                return Err("the VAD failed".into());
            }
            let count = (fns.vad_n_probs)(self.vad).max(0) as usize;
            let probs = (fns.vad_probs)(self.vad);
            if probs.is_null() { Vec::new() } else { std::slice::from_raw_parts(probs, count).to_vec() }
        };
        self.pending.drain(..whole);
        self.started = true;
        Ok(probs)
    }
}

impl Drop for VadStream<'_> {
    fn drop(&mut self) {
        // SAFETY: freed exactly once.
        unsafe { (self.runtime.fns.vad_free)(self.vad) };
    }
}
