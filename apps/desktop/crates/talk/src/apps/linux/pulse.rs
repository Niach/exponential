//! The `system` fallback without PipeWire: PulseAudio's default sink
//! monitor (`@DEFAULT_MONITOR@`) recorded by `parec` (pulseaudio-utils, on
//! every PulseAudio desktop) as raw mono f32 at 16 kHz: the server does the
//! downmix and the resampling, no libpulse link. ONE `desktop` channel.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;

use super::super::{AppCapture, Chunk, DESKTOP_CHANNEL, SAMPLE_RATE};
use super::pcm;

/// The `desktop` channel's label (meeting.json `copy.desktop`).
const DESKTOP_LABEL: &str = "Desktop";

/// A PulseAudio server to talk to (its socket, or `PULSE_SERVER`) and the
/// `parec` to record it with.
pub(super) fn available() -> bool {
    server_reachable() && parec().is_some()
}

fn server_reachable() -> bool {
    if std::env::var_os("PULSE_SERVER").is_some_and(|server| !server.is_empty()) {
        return true;
    }
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
    runtime.is_some_and(|dir| dir.join("pulse").join("native").exists())
}

fn parec() -> Option<PathBuf> {
    find_in_path("parec", &std::env::var_os("PATH")?)
}

pub(super) fn find_in_path(program: &str, path: &std::ffi::OsStr) -> Option<PathBuf> {
    std::env::split_paths(path).map(|dir| dir.join(program)).find(|candidate| is_executable(candidate))
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata().is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

/// The `parec` command line: raw float32le mono at [`SAMPLE_RATE`] from
/// the default sink's monitor, 100 ms fragments.
pub(super) fn arguments() -> Vec<String> {
    vec![
        "--device=@DEFAULT_MONITOR@".to_string(),
        "--raw".to_string(),
        "--format=float32le".to_string(),
        format!("--rate={SAMPLE_RATE}"),
        "--channels=1".to_string(),
        "--latency-msec=100".to_string(),
        "--client-name=Exponential".to_string(),
        "--stream-name=Meeting desktop audio".to_string(),
    ]
}

struct Guard {
    child: Child,
}

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub(super) fn open() -> Result<AppCapture, String> {
    let program = parec().ok_or("parec (pulseaudio-utils) is not installed.")?;
    let mut child = Command::new(program)
        .args(arguments())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("parec: {error}"))?;
    let mut stdout = child.stdout.take().ok_or("parec: no output")?;
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("talk-apps-parec".to_string())
        .spawn(move || {
            let mut chunker = pcm::Chunker::default();
            let mut buffer = vec![0u8; pcm::CHUNK * 4];
            let mut carry: Vec<u8> = Vec::new();
            loop {
                let read = match stdout.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(read) => read,
                };
                carry.extend_from_slice(&buffer[..read]);
                let whole = carry.len() / 4 * 4;
                let samples = pcm::f32_le(&carry[..whole]);
                carry.drain(..whole);
                for samples in chunker.push(&samples) {
                    let chunk =
                        Chunk { channel: DESKTOP_CHANNEL.to_string(), label: DESKTOP_LABEL.to_string(), samples };
                    if tx.send(chunk).is_err() {
                        return;
                    }
                }
            }
        })
        .map_err(|error| format!("parec reader: {error}"))?;
    Ok(AppCapture::new(rx, Box::new(Guard { child })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parec_records_the_default_monitor_as_mono_16k_float() {
        let args = arguments();
        assert!(args.contains(&"--device=@DEFAULT_MONITOR@".to_string()));
        assert!(args.contains(&"--format=float32le".to_string()));
        assert!(args.contains(&"--rate=16000".to_string()));
        assert!(args.contains(&"--channels=1".to_string()));
    }

    #[test]
    fn finds_programs_on_the_path() {
        assert!(find_in_path("sh", std::ffi::OsStr::new("/nonexistent:/bin:/usr/bin")).is_some());
        assert!(find_in_path("no-such-program-here", std::ffi::OsStr::new("/bin:/usr/bin")).is_none());
    }
}
