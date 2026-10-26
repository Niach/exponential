//! NEW-53: exponentialTalk, the host side.
//!
//! [`SpeechClient`] owns ONE speech worker: the host's own executable re-run
//! as `<exe> __speech-worker --data-dir <dir>` ([`ARGV_MARKER`]), spawned on
//! the first request and again after it dies. Requests go down as JSON
//! lines ([`Request`]), events come back the same way ([`Event`]) and reach
//! the host through the callback it registered. The worker (`crates/voice`)
//! holds the model download, the synthesis and the audio device; nothing of
//! that runs in the host process.
//!
//! Slice (b) speaks only: [`SpeechClient::prepare`] fetches the voice ahead
//! of the first reply (and, NEW-74, hands it the member's language and
//! voice file), [`SpeechClient::speak`] replaces whatever is being said,
//! [`SpeechClient::stop`] ends it. The worker answers with
//! [`Event::Voices`]: the languages it can speak on THIS machine (none
//! without a usable GPU).
//!
//! NEW-78 (protocol 3) listens: [`SpeechClient::listen`] opens the
//! microphone and stops any speech (barge-in), the worker streams
//! [`Event::Level`] frames for the wave and, on a GPU, [`Event::Partial`]
//! transcripts; [`SpeechClient::end_listen`] closes the microphone and the
//! worker answers [`Event::Heard`] (or [`Event::Empty`] when nobody spoke);
//! [`SpeechClient::cancel_listen`] drops the utterance without an answer.

pub mod microphone;

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

/// The argv marker a host checks FIRST in `main` (before its logger).
pub const ARGV_MARKER: &str = "__speech-worker";
/// The flag carrying the data dir the worker keeps its models under.
pub const DATA_DIR_FLAG: &str = "--data-dir";
/// The wire's version; a worker that answers another is replaced.
/// NEW-78: 3 = listening.
pub const PROTOCOL: u32 = 3;

/// NEW-78: the bands of a [`Event::Level`] frame (low to high pitch).
pub const LEVEL_BANDS: usize = 8;

/// Host → worker, one JSON object per line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "camelCase")]
pub enum Request {
    /// Download and load the voice now (the switch just turned on, or the
    /// language or voice file changed). Repeating it is cheap.
    Prepare { voice: VoiceChoice },
    /// Say `text` (already the spoken form); replaces anything playing.
    Speak { id: String, text: String },
    /// End whatever is being said.
    Stop,
    /// NEW-78: open the microphone and listen for one utterance `id`
    /// spoken in `language` (`user_voice_settings.language`, en | de).
    /// Stops any speech (barge-in) and replaces a running Listen.
    Listen { id: String, language: String },
    /// NEW-78: the key was released: close the microphone and transcribe.
    EndListen { id: String },
    /// NEW-78: drop utterance `id` (no Heard, no Empty).
    CancelListen { id: String },
}

/// Worker → host, one JSON object per line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "camelCase")]
pub enum Event {
    /// The worker is up and speaks this protocol.
    Ready { protocol: u32 },
    /// The voice model is downloading (`total` 0 = unknown).
    Downloading { received: u64, total: u64 },
    /// The download or its verification failed; the next request retries.
    DownloadFailed { error: String },
    /// The voice is on disk, verified and loaded.
    ModelReady,
    /// NEW-74: the languages this machine can speak now; empty = no usable
    /// GPU (replies are shown, never spoken). Sent after every Prepare.
    Voices { voices: Vec<String> },
    /// The first audio of `id` is playing.
    Started { id: String },
    /// `id` ended: played to the end, or `stopped`.
    Finished { id: String, stopped: bool },
    /// `id` could not be spoken.
    Failed { id: String, error: String },
    /// NEW-78: the microphone level while `id` listens, one frame per
    /// 32 ms (~31 Hz): `level` and each of the [`LEVEL_BANDS`] `bands` in
    /// 0..=1, already smoothed for drawing.
    Level { id: String, level: f32, bands: [f32; LEVEL_BANDS] },
    /// NEW-78: what `id` has heard so far (only where the transcriber runs
    /// on the GPU); replaced by the next Partial and finally by Heard.
    Partial { id: String, text: String },
    /// NEW-78: `id`'s final transcript.
    Heard { id: String, text: String },
    /// NEW-78: `id` ended without speech (a tap, or only noise).
    Empty { id: String },
    /// NEW-78: `id` could not listen or transcribe.
    ListenFailed { id: String, reason: ListenFailure, detail: String },
    /// Host-side: the worker process ended (crash, kill, EOF). Never sent
    /// by the worker itself.
    Exited,
}

/// NEW-78: why a Listen failed (`detail` carries the specifics for the log).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListenFailure {
    /// No input device, or the capture would not open.
    NoMicrophone,
    /// The OS refused the microphone (macOS privacy).
    NotGranted,
    /// The speech model is still downloading (progress = `Downloading`).
    Downloading,
    /// The model or its runtime failed to download, verify or load.
    Unavailable,
    /// Transcribing failed.
    Failed,
}

/// NEW-74: the voice the member picked (`user_voice_settings`): the
/// language replies are spoken in and, optionally, the reference clip the
/// voice is cloned from. Empty `file` = the built-in voice of `language`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceChoice {
    pub language: String,
    pub file: String,
    pub transcript: String,
}

/// The largest reference clip read (a calm 8-15 s clip is ~0.5-1.5 MB).
pub const MAX_VOICE_FILE_BYTES: u64 = 10 * 1024 * 1024;

/// NEW-74: the member's `voiceFile` is set remotely (web and phones write
/// it), so the voice device accepts only an absolute local path to a
/// regular `.wav` of at most [`MAX_VOICE_FILE_BYTES`], with a transcript:
/// no URL scheme, no `..`. `Ok(None)` = no file (the built-in voice);
/// `Err` = rejected (the caller logs it once and uses the built-in voice).
pub fn custom_voice_file(file: &str, transcript: &str) -> Result<Option<PathBuf>, String> {
    let file = file.trim();
    if file.is_empty() {
        return Ok(None);
    }
    let lower = file.to_ascii_lowercase();
    if lower.contains("://") || lower.starts_with("file:") || lower.starts_with("http:") || lower.starts_with("https:") {
        return Err("the voice file must be a local path, not a URL".into());
    }
    // Windows: a UNC or device path would open SMB (and hand the share the
    // member's NTLM hash) the moment it is stat'ed; checked on the string
    // BEFORE anything touches the file system.
    #[cfg(windows)]
    windows_local_disk_path(file)?;
    let path = PathBuf::from(file);
    if !path.is_absolute() {
        return Err("the voice file must be an absolute path".into());
    }
    if path.components().any(|component| matches!(component, std::path::Component::ParentDir)) {
        return Err("the voice file path must not contain `..`".into());
    }
    if !path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("wav")) {
        return Err("the voice file must be a .wav".into());
    }
    if transcript.trim().is_empty() {
        return Err("the voice file needs its transcript".into());
    }
    let metadata = std::fs::metadata(&path).map_err(|err| format!("the voice file is unreadable ({err})"))?;
    if !metadata.is_file() {
        return Err("the voice file is not a regular file".into());
    }
    if metadata.len() > MAX_VOICE_FILE_BYTES {
        return Err(format!("the voice file is larger than {} MB", MAX_VOICE_FILE_BYTES / (1024 * 1024)));
    }
    Ok(Some(path))
}

/// How Windows reads a path's prefix (`std::path::Prefix`), decided on the
/// string so it is testable on every platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowsPrefix {
    /// `C:\…` / `C:/…`
    Disk,
    /// `\\?\C:\…`
    VerbatimDisk,
    /// `\\server\share\…` (also with forward slashes)
    Unc,
    /// `\\?\UNC\server\share\…`
    VerbatimUnc,
    /// `\\.\…` (a device namespace path, `\\.\pipe\…`, `\\.\UNC\…`)
    DeviceNs,
    /// `\\?\anything-else`
    Verbatim,
    /// No prefix: relative, or rooted on the current drive (`\foo`).
    None,
}

pub fn windows_prefix(path: &str) -> WindowsPrefix {
    let bytes = path.as_bytes();
    let sep = |byte: u8| byte == b'\\' || byte == b'/';
    if bytes.len() >= 2 && sep(bytes[0]) && sep(bytes[1]) {
        let rest = &path[2..];
        if let Some(verbatim) = rest.strip_prefix("?\\").or_else(|| rest.strip_prefix("?/")) {
            let upper = verbatim.to_ascii_uppercase();
            if upper.starts_with("UNC\\") || upper.starts_with("UNC/") {
                return WindowsPrefix::VerbatimUnc;
            }
            let v = verbatim.as_bytes();
            if v.len() >= 2 && v[0].is_ascii_alphabetic() && v[1] == b':' {
                return WindowsPrefix::VerbatimDisk;
            }
            return WindowsPrefix::Verbatim;
        }
        if rest.starts_with(".\\") || rest.starts_with("./") || rest == "." {
            return WindowsPrefix::DeviceNs;
        }
        return WindowsPrefix::Unc;
    }
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return WindowsPrefix::Disk;
    }
    WindowsPrefix::None
}

/// Windows: only a path on a local drive letter (`C:\…`, `\\?\C:\…`),
/// never UNC, verbatim UNC or a device namespace.
pub fn windows_local_disk_path(path: &str) -> Result<(), String> {
    match windows_prefix(path) {
        WindowsPrefix::Disk | WindowsPrefix::VerbatimDisk => Ok(()),
        WindowsPrefix::Unc | WindowsPrefix::VerbatimUnc => Err("the voice file must be on this computer, not a network share".into()),
        WindowsPrefix::DeviceNs | WindowsPrefix::Verbatim => Err("the voice file must be a file on a local drive".into()),
        WindowsPrefix::None => Err("the voice file must be an absolute path".into()),
    }
}

/// Whether THIS process was started as the speech worker.
pub fn requested() -> bool {
    std::env::args_os().nth(1).is_some_and(|arg| arg == ARGV_MARKER)
}

/// The worker's `--data-dir` (its own argv), if given.
pub fn data_dir_arg() -> Option<PathBuf> {
    let args: Vec<_> = std::env::args_os().collect();
    args.iter()
        .position(|arg| arg == DATA_DIR_FLAG)
        .and_then(|index| args.get(index + 1))
        .map(PathBuf::from)
}

type Callback = Arc<dyn Fn(Event) + Send + Sync>;

struct Worker {
    child: Child,
    stdin: ChildStdin,
}

/// The host's handle on the speech worker. Cheap to clone; every clone
/// drives the same worker.
#[derive(Clone)]
pub struct SpeechClient {
    exe: PathBuf,
    data_dir: PathBuf,
    on_event: Callback,
    worker: Arc<Mutex<Option<Worker>>>,
}

impl SpeechClient {
    /// `exe` = the host's own executable, `data_dir` = where the voice
    /// lives (`{data_dir}/talk/…`), `on_event` runs on the worker's reader
    /// thread (keep it short; hand off to the UI thread).
    pub fn new(exe: PathBuf, data_dir: PathBuf, on_event: impl Fn(Event) + Send + Sync + 'static) -> Self {
        Self { exe, data_dir, on_event: Arc::new(on_event), worker: Arc::new(Mutex::new(None)) }
    }

    pub fn prepare(&self, voice: VoiceChoice) {
        self.send(&Request::Prepare { voice });
    }

    pub fn speak(&self, id: impl Into<String>, text: impl Into<String>) {
        self.send(&Request::Speak { id: id.into(), text: text.into() });
    }

    /// NEW-78: listen for utterance `id` in `language` (stops speech).
    pub fn listen(&self, id: impl Into<String>, language: impl Into<String>) {
        self.send(&Request::Listen { id: id.into(), language: language.into() });
    }

    /// NEW-78: the key was released; Heard or Empty follows.
    pub fn end_listen(&self, id: impl Into<String>) {
        self.send_if_running(&Request::EndListen { id: id.into() });
    }

    /// NEW-78: drop utterance `id`.
    pub fn cancel_listen(&self, id: impl Into<String>) {
        self.send_if_running(&Request::CancelListen { id: id.into() });
    }

    pub fn stop(&self) {
        self.send_if_running(&Request::Stop);
    }

    /// Requests that mean nothing to a worker that is not running never
    /// spawn one.
    fn send_if_running(&self, request: &Request) {
        // Never spawn a worker for these: there is nothing to end.
        let Ok(mut guard) = self.worker.lock() else { return };
        if let Some(worker) = guard.as_mut() {
            if write_line(&mut worker.stdin, request).is_err() {
                *guard = None;
            }
        }
    }

    /// Kill the worker (the switch turned off, the app quits).
    pub fn shutdown(&self) {
        if let Ok(mut guard) = self.worker.lock() {
            if let Some(mut worker) = guard.take() {
                let _ = worker.child.kill();
                let _ = worker.child.wait();
            }
        }
    }

    fn send(&self, request: &Request) {
        let Ok(mut guard) = self.worker.lock() else { return };
        // One retry: a worker that died since the last request is replaced.
        for _ in 0..2 {
            if guard.as_ref().is_none() {
                match self.spawn() {
                    Ok(worker) => *guard = Some(worker),
                    Err(err) => {
                        log::warn!("talk: speech worker failed to start: {err}");
                        return;
                    }
                }
            }
            let worker = guard.as_mut().expect("spawned above");
            match write_line(&mut worker.stdin, request) {
                Ok(()) => return,
                Err(err) => {
                    log::info!("talk: speech worker gone ({err}); restarting");
                    if let Some(mut dead) = guard.take() {
                        let _ = dead.child.kill();
                        let _ = dead.child.wait();
                    }
                }
            }
        }
    }

    fn spawn(&self) -> std::io::Result<Worker> {
        let mut child = Command::new(&self.exe)
            .arg(ARGV_MARKER)
            .arg(DATA_DIR_FLAG)
            .arg(&self.data_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let stdin = child.stdin.take().expect("piped");
        let stdout = child.stdout.take().expect("piped");
        let stderr = child.stderr.take().expect("piped");
        let on_event = self.on_event.clone();
        std::thread::Builder::new().name("talk-events".into()).spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                match serde_json::from_str::<Event>(&line) {
                    Ok(Event::Exited) => {}
                    Ok(event) => on_event(event),
                    Err(err) => log::debug!("talk: unreadable worker line ({err}): {line}"),
                }
            }
            on_event(Event::Exited);
        })?;
        std::thread::Builder::new().name("talk-stderr".into()).spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                log::info!("speech-worker: {line}");
            }
        })?;
        log::info!("talk: speech worker started (pid {})", child.id());
        Ok(Worker { child, stdin })
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn write_line(stdin: &mut ChildStdin, request: &Request) -> std::io::Result<()> {
    let mut line = serde_json::to_string(request).map_err(std::io::Error::other)?;
    line.push('\n');
    stdin.write_all(line.as_bytes())?;
    stdin.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wire_is_tagged_camel_case() {
        assert_eq!(
            serde_json::to_string(&Request::Speak { id: "m1".into(), text: "Hi.".into() }).unwrap(),
            r#"{"t":"speak","id":"m1","text":"Hi."}"#
        );
        assert_eq!(serde_json::to_string(&Request::Stop).unwrap(), r#"{"t":"stop"}"#);
        let event: Event = serde_json::from_str(r#"{"t":"finished","id":"m1","stopped":true}"#).unwrap();
        assert_eq!(event, Event::Finished { id: "m1".into(), stopped: true });
        let event: Event = serde_json::from_str(r#"{"t":"downloading","received":5,"total":10}"#).unwrap();
        assert_eq!(event, Event::Downloading { received: 5, total: 10 });
        let prepare = Request::Prepare { voice: VoiceChoice { language: "de".into(), ..Default::default() } };
        assert_eq!(
            serde_json::to_string(&prepare).unwrap(),
            r#"{"t":"prepare","voice":{"language":"de","file":"","transcript":""}}"#
        );
        let event: Event = serde_json::from_str(r#"{"t":"voices","voices":["en","de"]}"#).unwrap();
        assert_eq!(event, Event::Voices { voices: vec!["en".into(), "de".into()] });
    }

    #[test]
    fn windows_voice_files_must_sit_on_a_local_drive() {
        for (path, prefix) in [
            (r"C:\Users\me\clip.wav", WindowsPrefix::Disk),
            ("c:/Users/me/clip.wav", WindowsPrefix::Disk),
            (r"\\?\C:\Users\me\clip.wav", WindowsPrefix::VerbatimDisk),
            (r"\\host\share\v.wav", WindowsPrefix::Unc),
            ("//host/share/v.wav", WindowsPrefix::Unc),
            (r"\/host\share\v.wav", WindowsPrefix::Unc),
            (r"\\?\UNC\host\share\v.wav", WindowsPrefix::VerbatimUnc),
            (r"\\?\unc\host\share\v.wav", WindowsPrefix::VerbatimUnc),
            (r"\\.\pipe\x.wav", WindowsPrefix::DeviceNs),
            (r"\\.\UNC\host\share\v.wav", WindowsPrefix::DeviceNs),
            (r"\\?\Volume{1234}\v.wav", WindowsPrefix::Verbatim),
            (r"\Users\me\clip.wav", WindowsPrefix::None),
            ("clip.wav", WindowsPrefix::None),
        ] {
            assert_eq!(windows_prefix(path), prefix, "{path}");
            let accepted = matches!(prefix, WindowsPrefix::Disk | WindowsPrefix::VerbatimDisk);
            assert_eq!(windows_local_disk_path(path).is_ok(), accepted, "{path}");
        }
    }

    #[test]
    fn protocol_3_listens() {
        assert_eq!(
            serde_json::to_string(&Request::Listen { id: "u1".into(), language: "de".into() }).unwrap(),
            r#"{"t":"listen","id":"u1","language":"de"}"#
        );
        assert_eq!(serde_json::to_string(&Request::EndListen { id: "u1".into() }).unwrap(), r#"{"t":"endListen","id":"u1"}"#);
        assert_eq!(
            serde_json::to_string(&Request::CancelListen { id: "u1".into() }).unwrap(),
            r#"{"t":"cancelListen","id":"u1"}"#
        );
        let level = Event::Level { id: "u1".into(), level: 0.5, bands: [0.0, 0.25, 0.5, 0.75, 1.0, 0.5, 0.25, 0.0] };
        assert_eq!(
            serde_json::to_string(&level).unwrap(),
            r#"{"t":"level","id":"u1","level":0.5,"bands":[0.0,0.25,0.5,0.75,1.0,0.5,0.25,0.0]}"#
        );
        let event: Event = serde_json::from_str(r#"{"t":"heard","id":"u1","text":"Move NEW-78 to done."}"#).unwrap();
        assert_eq!(event, Event::Heard { id: "u1".into(), text: "Move NEW-78 to done.".into() });
        let event: Event = serde_json::from_str(r#"{"t":"partial","id":"u1","text":"Move"}"#).unwrap();
        assert_eq!(event, Event::Partial { id: "u1".into(), text: "Move".into() });
        let event: Event = serde_json::from_str(r#"{"t":"empty","id":"u1"}"#).unwrap();
        assert_eq!(event, Event::Empty { id: "u1".into() });
        let failed = Event::ListenFailed { id: "u1".into(), reason: ListenFailure::NoMicrophone, detail: "pw-record: not found".into() };
        assert_eq!(
            serde_json::to_string(&failed).unwrap(),
            r#"{"t":"listenFailed","id":"u1","reason":"no_microphone","detail":"pw-record: not found"}"#
        );
    }

    #[test]
    fn only_a_local_absolute_wav_with_a_transcript_is_a_voice_file() {
        let dir = std::env::temp_dir().join(format!("talk-voice-file-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        let clip = dir.join("me.WAV");
        std::fs::write(&clip, b"RIFF").unwrap();
        let clip_text = clip.to_string_lossy().into_owned();
        assert_eq!(custom_voice_file("", "x"), Ok(None), "empty = the built-in voice");
        assert_eq!(custom_voice_file(&clip_text, "Hello there."), Ok(Some(clip.clone())));
        for rejected in [
            "file:///tmp/me.wav".to_string(),
            "https://example.com/me.wav".to_string(),
            "me.wav".to_string(),
            dir.join("sub").join("..").join("me.WAV").to_string_lossy().into_owned(),
            dir.join("me.mp3").to_string_lossy().into_owned(),
            dir.join("missing.wav").to_string_lossy().into_owned(),
            dir.join("sub.wav").to_string_lossy().into_owned(),
        ] {
            if rejected.ends_with("sub.wav") {
                std::fs::create_dir_all(&rejected).unwrap();
            }
            assert!(custom_voice_file(&rejected, "Hello.").is_err(), "{rejected}");
        }
        assert!(custom_voice_file(&clip_text, "  ").is_err(), "a clip needs its transcript");
        let big = dir.join("big.wav");
        std::fs::File::create(&big).unwrap().set_len(MAX_VOICE_FILE_BYTES + 1).unwrap();
        assert!(custom_voice_file(&big.to_string_lossy(), "Hello.").is_err(), "over 10 MB");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_executable_fails_quietly() {
        let client = SpeechClient::new(PathBuf::from("/nonexistent/exe"), std::env::temp_dir(), |_| {});
        client.speak("m1", "Hello.");
        client.stop();
        client.end_listen("u1");
        client.cancel_listen("u1");
        client.shutdown();
    }
}
