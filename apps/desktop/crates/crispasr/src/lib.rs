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
//!   12 Hz codec).

pub mod artifact;
pub mod models;
pub mod platform;

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::{Path, PathBuf};

/// Where the runtime lives: inside the app bundle where it ships there
/// (macOS), else the download dir `{data_dir}/talk/crispasr-<version>/`.
pub fn runtime_dir(data_dir: &Path) -> PathBuf {
    platform::bundled_dir().unwrap_or_else(|| data_dir.join("talk").join(platform::RUNTIME_DIR))
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
}

/// The loaded CrispASR library. Every session borrows it.
pub struct Runtime {
    fns: Fns,
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
        if !platform::supported() {
            return Err("no GPU runtime for this platform".into());
        }
        for artifact in platform::ARTIFACTS {
            artifact::verify(dir, artifact).map_err(|err| err.to_string())?;
        }
        let library = open_library(&dir.join(platform::LIBRARY))?;
        let ggml = open_library(&dir.join(platform::GGML))?;
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
        };
        Ok(Runtime { fns, _ggml: ggml, _library: library })
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
        let backend_name = CString::new(match device {
            Device::Gpu => platform::GPU_BACKEND,
            Device::Cpu => "cpu",
        })
        .expect("static");
        let model_path = path_c(model)?;
        let backend_c = CString::new(backend).map_err(|err| err.to_string())?;
        let threads = std::thread::available_parallelism().map(|n| n.get().clamp(1, 8) as c_int).unwrap_or(4);
        // SAFETY: valid C strings; the session pointer is checked.
        let session = unsafe {
            (self.fns.set_gpu_backend)(backend_name.as_ptr());
            (self.fns.open_explicit)(model_path.as_ptr(), backend_c.as_ptr(), threads)
        };
        if session.is_null() {
            return Err(format!("{backend} failed to open {}", model.display()));
        }
        let session = TtsSession { runtime: self, session };
        let codec_path = path_c(codec)?;
        // SAFETY: a live session and a valid C string.
        if unsafe { (self.fns.set_codec_path)(session.session, codec_path.as_ptr()) } != 0 {
            return Err(format!("{backend} failed to load the codec {}", codec.display()));
        }
        Ok(session)
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
