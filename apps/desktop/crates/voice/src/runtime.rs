//! NEW-78: the ONE CrispASR runtime of the worker, shared by the speaker and
//! the transcriber (both builds name their libraries alike, so a process
//! loads one). Downloaded and verified on first use, then kept until exit.

use std::path::Path;
use std::sync::Mutex;

use crispasr::{artifact, Build, Runtime};

static LOADED: Mutex<Option<&'static Runtime>> = Mutex::new(None);

/// The runtime already loaded, if any.
pub fn loaded() -> Option<&'static Runtime> {
    *LOADED.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Whether `build`'s files are on disk and verified by their manifests.
pub fn is_ready(data_dir: &Path, build: Build) -> bool {
    let dir = crispasr::runtime_dir_of(data_dir, build);
    build.artifacts().iter().all(|artifact| artifact::is_ready(&dir, artifact))
}

/// Download (`progress(received, total)` over the runtime's bytes) and
/// load `build`, or return the runtime already loaded when it IS `build`.
pub fn get(data_dir: &Path, build: Build, progress: impl FnMut(u64, u64)) -> Result<&'static Runtime, String> {
    let mut loaded = LOADED.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(runtime) = *loaded {
        return if runtime.build() == build {
            Ok(runtime)
        } else {
            Err(format!("the {:?} runtime is loaded, not the {build:?} one", runtime.build()))
        };
    }
    let dir = crispasr::runtime_dir_of(data_dir, build);
    artifact::ensure_all(&dir, build.artifacts(), progress).map_err(|err| err.to_string())?;
    // Verified again inside `load_build` (every file re-hashed before dlopen).
    let runtime: &'static Runtime = Box::leak(Box::new(Runtime::load_build(&dir, build)?));
    *loaded = Some(runtime);
    Ok(runtime)
}
