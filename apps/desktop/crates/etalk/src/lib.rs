//! NEW-53: exponentialTalk, the host side.
//!
//! [`SpeechClient`] owns ONE speech worker: the host's own executable re-run
//! as `<exe> __speech-worker --data-dir <dir>` ([`ARGV_MARKER`]), spawned on
//! the first request and again after it dies. Requests go down as JSON
//! lines ([`Request`]), events come back the same way ([`Event`]) and reach
//! the host through the callback it registered. The worker (`crates/etalk-engine`)
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
//!
//! NEW-56 (protocol 4) wakes: [`SpeechClient::wake`] keeps the microphone
//! open for the member's wake phrase (empty = off); the worker answers
//! [`Event::WakeListening`] as the microphone opens or closes for it, and a
//! heard phrase starts a turn of its own, [`Event::Woke`], followed by the
//! same Level, Partial and Heard or Empty as a Listen.
//!
//! NEW-55 (protocol 5) calls (`packages/etalk/fixtures/voice-call.json`):
//! [`SpeechClient::call`] listens continuously, from the microphone or (a
//! `remote` call, a phone or the web streaming to the voice device) from the
//! [`SpeechClient::call_audio`] the host decoded. Every finished utterance
//! comes back as [`Event::Said`]; [`Event::SpeechStarted`] marks speech of
//! [`BARGE_IN_MS`] (the worker has already stopped any reply: barge-in).
//! Speak is allowed during a call; a remote call's replies stream back as
//! [`Event::SpeechAudio`] instead of playing here. [`SpeechClient::hangup`]
//! ends it without an answer.
//!
//! S-3 (protocol 7) rings: a Call carries `ring_ms`; a LOCAL call opens the
//! call's one output stream at once and plays a ring tone on it (EXP-1423:
//! the shared cadence, looped until the first Speak, at most that long)
//! while the worker warms up (the recognizer and the voice loaded, a
//! download included, the microphone opened). [`Event::CallListening`]
//! confirms the call hears (the host's `ready`); nothing of the call is
//! heard before it. A worker of protocol 6 ignores `ringMs` and never
//! sends CallListening (the host reads [`Event::Ready`]'s protocol).
//!
//! NEW-57 (protocol 6) meets (`packages/etalk/fixtures/meeting.json`):
//! [`SpeechClient::meeting`] owns the microphone (channel `you`) and, per
//! [`MeetingApps`], the desktop audio ([`apps`]) until
//! [`SpeechClient::meeting_stop`]; every channel runs the push-to-talk
//! segment rules on its own (never a mix) and every closed segment comes
//! back as [`Event::MeetingSegment`]. The wake phrase opening a `you`
//! segment adds [`Event::MeetingWake`]; a push-to-talk Listen during a
//! meeting is answered [`Event::MeetingFlushNow`] then `Empty` (a flush
//! trigger, the microphone stays the meeting's); a Call is refused
//! (`ListenFailed { reason: meeting }`). [`SpeechClient::meeting_pause`]
//! drops every channel's audio until resumed.

pub mod apps;
/// The call state machine (`fixtures/voice-call.json`): availability, ring, greeting, controls.
pub mod call;
/// Meeting mode (`fixtures/meeting.json`): settings rows, channels, flush, message.
pub mod meeting;
pub mod microphone;
/// Voice settings, push to talk, wake word, tray and spoken-reply rules (`fixtures/voice.json`).
pub mod voice;

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

/// The argv marker a host checks FIRST in `main` (before its logger).
pub const ARGV_MARKER: &str = "__speech-worker";
/// The flag carrying the data dir the worker keeps its models under.
pub const DATA_DIR_FLAG: &str = "--data-dir";
/// The wire's version, announced in [`Event::Ready`]. The worker is the
/// host's OWN executable (`ARGV_MARKER`), so the two sides agree by
/// construction; nothing here replaces a worker on a mismatch, the host
/// only reads the number.
/// NEW-78: 3 = listening. NEW-56 (#1263): 4 = the wake word. NEW-55: 5 = calls.
/// NEW-57: 6 = meetings. S-3: 7 = the ring and CallListening.
pub const PROTOCOL: u32 = 7;

/// S-3: a worker stderr line starting with this is logged at debug (the
/// rest at info): the call's per-second input level.
pub const DEBUG_PREFIX: &str = "debug: ";

/// NEW-57: the microphone's meeting channel (`meeting.json` `you`).
pub const YOU_CHANNEL: &str = "you";

/// NEW-55: a call closes an utterance after this much silence (push to talk
/// keeps talk-listen.json's 300 ms); `voice-call.json` `minSilenceMs`
/// (S-3: 700, a breath between two clauses no longer ends the turn).
pub const CALL_MIN_SILENCE_MS: usize = 700;
/// NEW-55: speech this long in the open utterance is a barge-in
/// ([`Event::SpeechStarted`]); `voice-call.json` `bargeInMs`.
pub const BARGE_IN_MS: usize = 250;
/// NEW-55: the rate of call audio coming in ([`Request::CallAudio`]).
pub const CALL_RATE: u32 = 16_000;
/// NEW-55: one [`Event::SpeechAudio`] carries at most this much audio.
pub const SPEECH_AUDIO_CHUNK_MS: u32 = 100;

/// NEW-78: the bands of a [`Event::Level`] frame (low to high pitch).
pub const LEVEL_BANDS: usize = 8;

/// Host → worker, one JSON object per line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "camelCase", rename_all_fields = "camelCase")]
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
    /// NEW-55: call `id`: listen continuously in `language` until Hangup,
    /// answering Said per utterance. `remote` = the audio comes ONLY as
    /// CallAudio (a caller streams it; the microphone stays closed) and
    /// replies stream back as SpeechAudio. Replaces a running Listen or Call.
    /// S-3: `ring_ms` = how long a LOCAL call rings on its output stream at
    /// most while the worker warms up (EXP-1423: its first Speak, the
    /// greeting, ends it sooner; 0 = no ring; a remote call never rings
    /// here); [`Event::CallListening`] follows once it hears.
    Call {
        id: String,
        language: String,
        remote: bool,
        #[serde(default)]
        ring_ms: u32,
    },
    /// NEW-55: caller audio of remote call `id`: base64 of i16 little-endian
    /// mono [`CALL_RATE`] samples ([`encode_pcm`]).
    CallAudio { id: String, pcm: String },
    /// NEW-55: `muted` drops call `id`'s incoming audio (no Level, no
    /// utterances); replies still play.
    CallMute { id: String, muted: bool },
    /// NEW-55: end call `id` (no answer events).
    Hangup { id: String },
    /// #NEW-56: listen for `phrase` (already resolved: `Hey {agent}` when
    /// the member left it empty) in `language`; an empty phrase = off.
    Wake { phrase: String, language: String },
    /// NEW-57: meeting `id`: the microphone (channel [`YOU_CHANNEL`],
    /// labelled `you_label`) and `selection`'s desktop audio, each channel
    /// segmented and transcribed in `language` until MeetingStop. `phrase`
    /// = the wake phrase a `you` segment may open with (empty = none);
    /// `capability` = what the voice device's doctor reported (`none` =
    /// the microphone only, `system` = one `desktop` channel whatever the
    /// selection). Waits for a running Listen or Call to end; replaces a
    /// running meeting.
    Meeting {
        id: String,
        language: String,
        phrase: String,
        you_label: String,
        selection: MeetingApps,
        capability: apps::Capability,
    },
    /// NEW-57: end meeting `id`. `drain` (a stop, never a sign-out): every
    /// channel's open segment is closed and transcribed first, then
    /// [`Event::MeetingStopped`]; else open segments are dropped and nothing
    /// more is emitted for it.
    MeetingStop {
        id: String,
        #[serde(default)]
        drain: bool,
    },
    /// NEW-57: hold every channel of meeting `id` (its audio is dropped,
    /// open segments with it) or resume it.
    MeetingPause { id: String, paused: bool },
}

/// NEW-57: which desktop audio a meeting hears beside the microphone
/// (`desktopAudio` with its `desktopAudioApps` ids).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", content = "apps", rename_all = "camelCase")]
pub enum MeetingApps {
    /// The microphone only.
    #[default]
    Off,
    All,
    Include(Vec<String>),
    Exclude(Vec<String>),
}

impl MeetingApps {
    /// What [`apps::open`] is asked for (None = no desktop audio): with
    /// `system` capability `include`/`exclude` behave as `all`.
    pub fn selection(&self, capability: apps::Capability) -> Option<apps::Selection> {
        match (self, capability) {
            (MeetingApps::Off, _) | (_, apps::Capability::None) => None,
            (_, apps::Capability::System) | (MeetingApps::All, _) => Some(apps::Selection::All),
            (MeetingApps::Include(ids), _) => Some(apps::Selection::Include(ids.clone())),
            (MeetingApps::Exclude(ids), _) => Some(apps::Selection::Exclude(ids.clone())),
        }
    }
}

/// NEW-57: one word of a meeting segment (epoch ms).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingWord {
    pub text: String,
    pub start_ms: i64,
    pub end_ms: i64,
}

/// Worker → host, one JSON object per line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "camelCase", rename_all_fields = "camelCase")]
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
    /// NEW-55: call `id` heard one finished utterance (normalised like
    /// Heard, never empty).
    Said { id: String, text: String },
    /// NEW-55: call `id`'s open utterance reached [`BARGE_IN_MS`] of speech;
    /// a reply that was playing has already ended (`Finished { stopped:
    /// true }` came first).
    SpeechStarted { id: String },
    /// S-3: call `id` hears: the recognizer and the voice are loaded (a
    /// download waited for) and the microphone (local) or the stream
    /// intake (remote) is open. Once per call; its Level, SpeechStarted
    /// and Said only follow it.
    CallListening { id: String },
    /// NEW-55: reply `id`'s audio during a remote call (never played here):
    /// base64 of i16 LE mono samples at `rate` Hz (the voice's, 24000), at
    /// most [`SPEECH_AUDIO_CHUNK_MS`] per event, between Started and Finished.
    SpeechAudio { id: String, pcm: String, rate: u32 },
    /// #NEW-56: the wake phrase was heard: turn `id` runs like a Listen
    /// (Level, Partial) and the worker ends it itself (Heard or Empty).
    Woke { id: String },
    /// #NEW-56: the microphone is open for the wake word (`false`: off,
    /// muted, or it could not open; `reason` says why when it failed).
    WakeListening { listening: bool, reason: Option<ListenFailure> },
    /// NEW-57: meeting `id` finalized a segment of `channel` (`you`, an
    /// app's id or `desktop`), labelled `label`: normalised text (never
    /// empty), from `start_ms` (epoch ms, the first word) to `end_ms`.
    /// `words` = per-word timestamps where the runtime gives them (empty:
    /// the segment's own start and end stand for them).
    MeetingSegment {
        id: String,
        channel: String,
        label: String,
        text: String,
        start_ms: i64,
        end_ms: i64,
        words: Vec<MeetingWord>,
    },
    /// NEW-57: the wake phrase opened the `you` segment that just came
    /// (a flush trigger, `wake`).
    MeetingWake { id: String },
    /// NEW-57: push to talk was pressed during meeting `id` (a flush
    /// trigger, `ptt`); the Listen itself is answered Empty.
    MeetingFlushNow { id: String },
    /// NEW-57: meeting `id` listens (`true`, the microphone is open) or
    /// stopped listening (`false`: stopped, or it could not open; `reason`
    /// says why, and the worker retries on its own while it is wanted).
    MeetingListening { id: String, listening: bool, reason: Option<ListenFailure> },
    /// NEW-57: a draining MeetingStop of `id` is done: every segment it had
    /// has been emitted (the host sends its final batch now).
    MeetingStopped { id: String },
    /// Host-side: the worker process ended on its own (a crash, a failed
    /// write). Never sent by the worker itself, nor (#NEW-66) after
    /// [`SpeechClient::shutdown`].
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
    /// NEW-57: a meeting owns the microphone (a Call is refused,
    /// voice-call.json `failed.meeting`).
    Meeting,
}

/// NEW-55: i16 samples as the wire's base64 of little-endian bytes.
pub fn encode_pcm(samples: &[i16]) -> String {
    use base64::Engine as _;
    let bytes: Vec<u8> = samples.iter().flat_map(|sample| sample.to_le_bytes()).collect();
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// NEW-55: the wire's base64 back to i16 samples (an odd trailing byte is
/// refused).
pub fn decode_pcm(pcm: &str) -> Result<Vec<i16>, String> {
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD.decode(pcm).map_err(|err| err.to_string())?;
    if bytes.len() % 2 != 0 {
        return Err("odd number of PCM bytes".into());
    }
    Ok(bytes.chunks_exact(2).map(|pair| i16::from_le_bytes([pair[0], pair[1]])).collect())
}

/// NEW-55: an f32 sample (-1..=1) as i16, clipped.
pub fn to_i16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * 32767.0).round() as i16
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

/// #1238 review: where the audio tools (`pw-record`, `parec`, `pactl`,
/// `pw-play`, `paplay`) are run from: the system directories, never the
/// inherited `PATH`, so a planted binary earlier on it never gets the
/// microphone. NixOS keeps its system profile under `/run/current-system`.
pub const SYSTEM_BIN_DIRS: [&str; 4] = ["/usr/bin", "/bin", "/usr/local/bin", "/run/current-system/sw/bin"];

/// `name` in [`SYSTEM_BIN_DIRS`] (the first that has it), absolute.
pub fn system_binary(name: &str) -> Option<PathBuf> {
    binary_in(&SYSTEM_BIN_DIRS.map(std::path::Path::new), name)
}

fn binary_in(dirs: &[&std::path::Path], name: &str) -> Option<PathBuf> {
    dirs.iter().map(|dir| dir.join(name)).find(|path| path.is_file())
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

/// The worker's whole speech data under `data_dir` (#NEW-63): the CrispASR
/// runtime, `models/`, `voices/`, `no-gpu.txt` and any partial download.
pub fn voice_data_dir(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("talk")
}

/// #NEW-63: this device stopped being the voice device of every account
/// signed in here: free its speech data (GBs). Call only with no worker
/// running ([`SpeechClient::remove_voice_data`] makes sure). `Ok(true)` = it
/// was there and is gone.
pub fn remove_voice_data(data_dir: &std::path::Path) -> std::io::Result<bool> {
    let Some(aside) = set_voice_data_aside(data_dir)? else { return Ok(false) };
    remove_set_aside(data_dir, &aside);
    Ok(true)
}

/// The prefix of a speech data directory renamed aside for deletion.
const ASIDE_PREFIX: &str = "talk.removing-";

/// #1278 review: `{data_dir}/talk` renamed aside (one instant rename), so a
/// worker started right after begins on a fresh `talk/` and never loads from
/// or downloads into the tree being deleted. `None` = nothing there (a
/// non-directory goes at once).
fn set_voice_data_aside(data_dir: &std::path::Path) -> std::io::Result<Option<PathBuf>> {
    let dir = voice_data_dir(data_dir);
    // Never through a symlink: only our own directory goes.
    match std::fs::symlink_metadata(&dir) {
        Ok(meta) if meta.is_dir() => {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or_default();
            let aside = data_dir.join(format!("{ASIDE_PREFIX}{}-{nanos}", std::process::id()));
            std::fs::rename(&dir, &aside)?;
            Ok(Some(aside))
        }
        Ok(_) => std::fs::remove_file(&dir).map(|()| None),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

/// Delete `aside` and any tree an earlier removal left aside (the app quit
/// mid-delete).
fn remove_set_aside(data_dir: &std::path::Path, aside: &std::path::Path) {
    if let Err(err) = std::fs::remove_dir_all(aside) {
        log::warn!("talk: removing the speech data failed: {err}");
    }
    let Ok(entries) = std::fs::read_dir(data_dir) else { return };
    for entry in entries.flatten() {
        let leftover = entry.file_name().to_string_lossy().starts_with(ASIDE_PREFIX)
            && entry.file_type().is_ok_and(|kind| kind.is_dir());
        if leftover {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// #NEW-63: the voice turned off here: drop what a killed download left
/// (`*.part`, up to 1.8 GB, and `*.tmp`) anywhere under the speech data; the
/// verified downloads stay. Call only with no worker running
/// ([`SpeechClient::remove_partial_downloads`] makes sure). Answers how many
/// files went.
pub fn remove_partial_downloads(data_dir: &std::path::Path) -> usize {
    fn sweep(dir: &std::path::Path, removed: &mut usize) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else { continue };
            let path = entry.path();
            if kind.is_dir() {
                sweep(&path, removed);
            } else if kind.is_file()
                && path.extension().is_some_and(|extension| extension == "part" || extension == "tmp")
                && std::fs::remove_file(&path).is_ok()
            {
                *removed += 1;
            }
        }
    }
    let mut removed = 0;
    sweep(&voice_data_dir(data_dir), &mut removed);
    removed
}

type Callback = Arc<dyn Fn(Event) + Send + Sync>;

struct Worker {
    child: Child,
    stdin: ChildStdin,
    /// #NEW-66: set by [`SpeechClient::shutdown`] before the kill: the
    /// exit was asked for, so no `Exited` goes out (a quick off/on never
    /// counts the old worker's end as a crash of the new setting).
    ended: Arc<AtomicBool>,
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

    /// #NEW-56: listen for the wake `phrase` (empty = stop listening; that
    /// never spawns a worker).
    pub fn wake(&self, phrase: impl Into<String>, language: impl Into<String>) {
        let request = Request::Wake { phrase: phrase.into(), language: language.into() };
        if matches!(&request, Request::Wake { phrase, .. } if phrase.is_empty()) {
            self.send_if_running(&request);
        } else {
            self.send(&request);
        }
    }

    /// NEW-57: start (or re-hand) meeting `id`; see [`Request::Meeting`].
    pub fn meeting(
        &self,
        id: impl Into<String>,
        language: impl Into<String>,
        phrase: impl Into<String>,
        you_label: impl Into<String>,
        selection: MeetingApps,
        capability: apps::Capability,
    ) {
        self.send(&Request::Meeting {
            id: id.into(),
            language: language.into(),
            phrase: phrase.into(),
            you_label: you_label.into(),
            selection,
            capability,
        });
    }

    /// NEW-57: end meeting `id`; `drain` = emit its open segments first,
    /// then [`Event::MeetingStopped`].
    pub fn meeting_stop(&self, id: impl Into<String>, drain: bool) {
        self.send_if_running(&Request::MeetingStop { id: id.into(), drain });
    }

    /// NEW-57: pause or resume meeting `id`.
    pub fn meeting_pause(&self, id: impl Into<String>, paused: bool) {
        self.send_if_running(&Request::MeetingPause { id: id.into(), paused });
    }

    /// #NEW-54: bring the worker up without a request (push to talk wants
    /// it resident and its `Ready` known before the first key-down).
    pub fn start(&self) {
        let failed = {
            let Ok(mut guard) = self.worker.lock() else { return };
            if guard.is_some() {
                return;
            }
            match self.spawn() {
                Ok(worker) => {
                    *guard = Some(worker);
                    false
                }
                Err(err) => {
                    log::warn!("talk: speech worker failed to start: {err}");
                    true
                }
            }
        };
        // Wave review of #1263/#1278: a worker that never came up is an
        // `Exited` like one that died (off the lock: the host may call
        // back in), so the Devices speech row reads unavailable rather
        // than vanishing.
        if failed {
            (self.on_event)(Event::Exited);
        }
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

    /// NEW-55: start call `id` in `language`; `remote` = the audio comes
    /// through [`Self::call_audio`] (a phone or the web streams it) and
    /// replies stream back as [`Event::SpeechAudio`]. S-3: a local call
    /// rings on this device (until the greeting, at most `ring_ms`) while
    /// the worker warms up.
    pub fn call(&self, id: impl Into<String>, language: impl Into<String>, remote: bool, ring_ms: u32) {
        self.send(&Request::Call { id: id.into(), language: language.into(), remote, ring_ms });
    }

    /// NEW-55: the caller's audio of remote call `id`: mono [`CALL_RATE`] Hz
    /// samples (the host decoded the caller's Opus).
    pub fn call_audio(&self, id: impl Into<String>, pcm: &[i16]) {
        self.send_if_running(&Request::CallAudio { id: id.into(), pcm: encode_pcm(pcm) });
    }

    /// NEW-55: mute or unmute call `id`.
    pub fn call_mute(&self, id: impl Into<String>, muted: bool) {
        self.send_if_running(&Request::CallMute { id: id.into(), muted });
    }

    /// NEW-55: end call `id`.
    pub fn hangup(&self, id: impl Into<String>) {
        self.send_if_running(&Request::Hangup { id: id.into() });
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
                worker.ended.store(true, Ordering::SeqCst);
                let _ = worker.child.kill();
                let _ = worker.child.wait();
            }
        }
    }

    /// #NEW-63 / #1278 review: free the speech data, on the worker's own
    /// lock, so no worker starts while it is set aside. `false` = skipped: a
    /// worker runs (it may be downloading into it); ask again once it is
    /// shut down. The tree is renamed aside at once and deleted on a thread
    /// of its own.
    pub fn remove_voice_data(&self) -> bool {
        let Ok(guard) = self.worker.lock() else { return false };
        if guard.is_some() {
            return false;
        }
        match set_voice_data_aside(&self.data_dir) {
            Ok(Some(aside)) => {
                drop(guard);
                let data_dir = self.data_dir.clone();
                let _ = std::thread::Builder::new()
                    .name("talk-cleanup".into())
                    .spawn(move || remove_set_aside(&data_dir, &aside));
                log::info!("talk: not the voice device of any account here; removed the speech data");
                true
            }
            Ok(None) => true,
            Err(err) => {
                log::warn!("talk: setting the speech data aside failed: {err}");
                false
            }
        }
    }

    /// #NEW-63 / #1278 review: [`remove_partial_downloads`] on the worker's
    /// lock: skipped (0) while a worker runs, since its `.part` is a download
    /// in progress; a worker starting meanwhile waits for the sweep.
    pub fn remove_partial_downloads(&self) -> usize {
        let Ok(guard) = self.worker.lock() else { return 0 };
        if guard.is_some() {
            return 0;
        }
        remove_partial_downloads(&self.data_dir)
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
                        // As in [`Self::start`]: reported as an exit, off the lock.
                        drop(guard);
                        (self.on_event)(Event::Exited);
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
        let mut command = Command::new(&self.exe);
        // #1238 review: CrispASR's debug switches (`CRISPASR_PARAKEET_DEBUG`
        // prints decoded token ids) never reach the worker, whose stderr
        // is this process's log.
        for (key, _) in std::env::vars_os() {
            if strips_env(&key.to_string_lossy(), cfg!(windows)) {
                command.env_remove(key);
            }
        }
        let mut child = command
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
        let ended = Arc::new(AtomicBool::new(false));
        let asked = ended.clone();
        std::thread::Builder::new().name("talk-events".into()).spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                match serde_json::from_str::<Event>(&line) {
                    Ok(Event::Exited) => {}
                    Ok(event) => on_event(event),
                    Err(err) => log::debug!("talk: unreadable worker line ({err}): {line}"),
                }
            }
            if !asked.load(Ordering::SeqCst) {
                on_event(Event::Exited);
            }
        })?;
        std::thread::Builder::new().name("talk-stderr".into()).spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                match line.strip_prefix(DEBUG_PREFIX) {
                    Some(line) => log::debug!("speech-worker: {line}"),
                    None => log::info!("speech-worker: {line}"),
                }
            }
        })?;
        log::info!("talk: speech worker started (pid {})", child.id());
        Ok(Worker { child, stdin, ended })
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Whether `key` is one of CrispASR's switches the worker never inherits.
/// #NEW-80: Windows env names are case-insensitive, so `crispasr_…` counts
/// there too.
fn strips_env(key: &str, case_insensitive: bool) -> bool {
    const PREFIX: &str = "CRISPASR_";
    if case_insensitive {
        key.get(..PREFIX.len()).is_some_and(|head| head.eq_ignore_ascii_case(PREFIX))
    } else {
        key.starts_with(PREFIX)
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
    fn audio_tools_resolve_in_the_system_dirs_only() {
        let root = std::env::temp_dir().join(format!("talk-bin-{}", std::process::id()));
        let (first, second) = (root.join("usr-bin"), root.join("bin"));
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        std::fs::write(second.join("pw-record"), "").unwrap();
        let dirs = [first.as_path(), second.as_path()];
        assert_eq!(binary_in(&dirs, "pw-record"), Some(second.join("pw-record")));
        std::fs::write(first.join("pw-record"), "").unwrap();
        assert_eq!(binary_in(&dirs, "pw-record"), Some(first.join("pw-record")), "the first dir wins");
        assert_eq!(binary_in(&dirs, "parec"), None);
        assert!(SYSTEM_BIN_DIRS.iter().all(|dir| std::path::Path::new(dir).is_absolute()));
        let _ = std::fs::remove_dir_all(&root);
    }

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
    fn protocol_6_meets() {
        assert!(PROTOCOL >= 6);
        let start = Request::Meeting {
            id: "m1".into(),
            language: "en".into(),
            phrase: "Hey Studio".into(),
            you_label: "You".into(),
            selection: MeetingApps::Include(vec!["us.zoom.xos".into()]),
            capability: apps::Capability::PerApp,
        };
        let json = r#"{"t":"meeting","id":"m1","language":"en","phrase":"Hey Studio","youLabel":"You","selection":{"mode":"include","apps":["us.zoom.xos"]},"capability":"per_app"}"#;
        assert_eq!(serde_json::to_string(&start).unwrap(), json);
        assert_eq!(serde_json::from_str::<Request>(json).unwrap(), start);
        assert_eq!(serde_json::to_string(&MeetingApps::Off).unwrap(), r#"{"mode":"off"}"#);
        assert_eq!(serde_json::from_str::<MeetingApps>(r#"{"mode":"all"}"#).unwrap(), MeetingApps::All);
        for (request, json) in [
            (Request::MeetingStop { id: "m1".into(), drain: true }, r#"{"t":"meetingStop","id":"m1","drain":true}"#),
            (Request::MeetingPause { id: "m1".into(), paused: true }, r#"{"t":"meetingPause","id":"m1","paused":true}"#),
        ] {
            assert_eq!(serde_json::to_string(&request).unwrap(), json);
            assert_eq!(serde_json::from_str::<Request>(json).unwrap(), request);
        }
        let segment = Event::MeetingSegment {
            id: "m1".into(),
            channel: "you".into(),
            label: "You".into(),
            text: "Ship it.".into(),
            start_ms: 1_000,
            end_ms: 2_000,
            words: vec![MeetingWord { text: "Ship".into(), start_ms: 1_000, end_ms: 1_400 }],
        };
        let json = r#"{"t":"meetingSegment","id":"m1","channel":"you","label":"You","text":"Ship it.","startMs":1000,"endMs":2000,"words":[{"text":"Ship","startMs":1000,"endMs":1400}]}"#;
        assert_eq!(serde_json::to_string(&segment).unwrap(), json);
        assert_eq!(serde_json::from_str::<Event>(json).unwrap(), segment);
        assert_eq!(serde_json::to_string(&Event::MeetingWake { id: "m1".into() }).unwrap(), r#"{"t":"meetingWake","id":"m1"}"#);
        assert_eq!(serde_json::to_string(&Event::MeetingFlushNow { id: "m1".into() }).unwrap(), r#"{"t":"meetingFlushNow","id":"m1"}"#);
        assert_eq!(
            serde_json::to_string(&Event::MeetingListening { id: "m1".into(), listening: false, reason: Some(ListenFailure::Downloading) }).unwrap(),
            r#"{"t":"meetingListening","id":"m1","listening":false,"reason":"downloading"}"#
        );
        assert_eq!(serde_json::to_string(&ListenFailure::Meeting).unwrap(), r#""meeting""#);
        assert_eq!(serde_json::to_string(&Event::MeetingStopped { id: "m1".into() }).unwrap(), r#"{"t":"meetingStopped","id":"m1"}"#);
        let bare: Request = serde_json::from_str(r#"{"t":"meetingStop","id":"m1"}"#).unwrap();
        assert_eq!(bare, Request::MeetingStop { id: "m1".into(), drain: false }, "no drain = drop");
    }

    #[test]
    fn a_meetings_desktop_audio_follows_the_capability() {
        use apps::{Capability, Selection};
        let include = MeetingApps::Include(vec!["a".into()]);
        assert_eq!(MeetingApps::Off.selection(Capability::PerApp), None);
        assert_eq!(MeetingApps::All.selection(Capability::None), None, "nothing to capture: the microphone only");
        assert_eq!(include.selection(Capability::PerApp), Some(Selection::Include(vec!["a".into()])));
        assert_eq!(include.selection(Capability::System), Some(Selection::All), "system: one desktop channel");
        assert_eq!(MeetingApps::Exclude(vec!["b".into()]).selection(Capability::PerApp), Some(Selection::Exclude(vec!["b".into()])));
    }

    #[test]
    fn protocol_5_calls() {
        let requests = [
            (
                Request::Call { id: "c1".into(), language: "de".into(), remote: true, ring_ms: 0 },
                r#"{"t":"call","id":"c1","language":"de","remote":true,"ringMs":0}"#,
            ),
            (Request::CallAudio { id: "c1".into(), pcm: encode_pcm(&[1, -2, i16::MAX, i16::MIN]) }, r#"{"t":"callAudio","id":"c1","pcm":"AQD+//9/AIA="}"#),
            (Request::CallMute { id: "c1".into(), muted: true }, r#"{"t":"callMute","id":"c1","muted":true}"#),
            (Request::Hangup { id: "c1".into() }, r#"{"t":"hangup","id":"c1"}"#),
        ];
        for (request, json) in requests {
            assert_eq!(serde_json::to_string(&request).unwrap(), json);
            assert_eq!(serde_json::from_str::<Request>(json).unwrap(), request);
        }
        let events = [
            (Event::Said { id: "c1".into(), text: "What is still open?".into() }, r#"{"t":"said","id":"c1","text":"What is still open?"}"#),
            (Event::SpeechStarted { id: "c1".into() }, r#"{"t":"speechStarted","id":"c1"}"#),
            (Event::SpeechAudio { id: "m1".into(), pcm: "AQA=".into(), rate: 24_000 }, r#"{"t":"speechAudio","id":"m1","pcm":"AQA=","rate":24000}"#),
        ];
        for (event, json) in events {
            assert_eq!(serde_json::to_string(&event).unwrap(), json);
            assert_eq!(serde_json::from_str::<Event>(json).unwrap(), event);
        }
        assert_eq!(decode_pcm("AQD+//9/AIA=").unwrap(), [1, -2, i16::MAX, i16::MIN]);
        assert!(decode_pcm("AQ==").is_err(), "an odd byte");
        assert!(decode_pcm("not base64!").is_err());
        assert_eq!((to_i16(1.5), to_i16(-1.0), to_i16(0.0)), (i16::MAX, -i16::MAX, 0));
    }

    #[test]
    fn protocol_7_rings() {
        assert_eq!(PROTOCOL, 7);
        let call = Request::Call { id: "c1".into(), language: "en".into(), remote: false, ring_ms: 3000 };
        let json = r#"{"t":"call","id":"c1","language":"en","remote":false,"ringMs":3000}"#;
        assert_eq!(serde_json::to_string(&call).unwrap(), json);
        assert_eq!(serde_json::from_str::<Request>(json).unwrap(), call);
        let old: Request = serde_json::from_str(r#"{"t":"call","id":"c1","language":"en","remote":false}"#).unwrap();
        assert_eq!(old, Request::Call { id: "c1".into(), language: "en".into(), remote: false, ring_ms: 0 }, "an old host rings nothing");
        let listening = Event::CallListening { id: "c1".into() };
        assert_eq!(serde_json::to_string(&listening).unwrap(), r#"{"t":"callListening","id":"c1"}"#);
        assert_eq!(serde_json::from_str::<Event>(r#"{"t":"callListening","id":"c1"}"#).unwrap(), listening);
    }

    #[test]
    fn the_call_constants_are_the_fixture() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../../../../packages/etalk/fixtures/voice-call.json")).unwrap();
        let constants = &fixture["constants"];
        assert_eq!(constants["minSilenceMs"], CALL_MIN_SILENCE_MS);
        assert_eq!(constants["bargeInMs"], BARGE_IN_MS);
        assert_eq!(constants["upRate"], CALL_RATE);
    }

    #[test]
    fn protocol_4_wakes() {
        assert_eq!(
            serde_json::to_string(&Request::Wake { phrase: "Hey Studio".into(), language: "en".into() }).unwrap(),
            r#"{"t":"wake","phrase":"Hey Studio","language":"en"}"#
        );
        let event: Event = serde_json::from_str(r#"{"t":"woke","id":"wake-1"}"#).unwrap();
        assert_eq!(event, Event::Woke { id: "wake-1".into() });
        assert_eq!(
            serde_json::to_string(&Event::WakeListening { listening: false, reason: Some(ListenFailure::Downloading) }).unwrap(),
            r#"{"t":"wakeListening","listening":false,"reason":"downloading"}"#
        );
        let event: Event = serde_json::from_str(r#"{"t":"wakeListening","listening":true,"reason":null}"#).unwrap();
        assert_eq!(event, Event::WakeListening { listening: true, reason: None });
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
    fn crispasr_switches_are_stripped_case_insensitively_on_windows() {
        assert!(strips_env("CRISPASR_PARAKEET_DEBUG", false));
        assert!(!strips_env("crispasr_parakeet_debug", false), "unix names are case-sensitive");
        assert!(strips_env("crispasr_parakeet_debug", true));
        assert!(strips_env("CrispAsr_X", true));
        assert!(!strips_env("CRISPASR", true));
        assert!(!strips_env("MY_CRISPASR_X", true));
        assert!(!strips_env("ü", true), "a short non-ASCII name never panics");
    }

    #[test]
    fn a_missing_executable_fails_quietly() {
        let client = SpeechClient::new(PathBuf::from("/nonexistent/exe"), std::env::temp_dir(), |_| {});
        client.start();
        client.speak("m1", "Hello.");
        client.stop();
        client.end_listen("u1");
        client.cancel_listen("u1");
        client.call("c1", "en", true, 0);
        client.call_audio("c1", &[0, 1, -1]);
        client.call_mute("c1", true);
        client.hangup("c1");
        client.wake("", "en");
        client.meeting_stop("m1", true);
        client.meeting_pause("m1", true);
        client.shutdown();
    }

    #[test]
    fn voice_data_goes_partials_first_then_all_of_it() {
        let data = std::env::temp_dir().join(format!("talk-data-{}", std::process::id()));
        let talk = voice_data_dir(&data);
        std::fs::create_dir_all(talk.join("models")).unwrap();
        std::fs::create_dir_all(talk.join("crispasr-0.8.45")).unwrap();
        std::fs::write(talk.join("models").join("qwen3-tts.gguf"), "model").unwrap();
        std::fs::write(talk.join("models").join("qwen3-tts.part"), "half").unwrap();
        std::fs::write(talk.join("crispasr-0.8.45").join("lib.part"), "half").unwrap();
        std::fs::write(talk.join("crispasr-0.8.45").join("x.tmp"), "half").unwrap();
        std::fs::write(talk.join("no-gpu.txt"), "").unwrap();
        // Outside the speech data: never touched.
        std::fs::write(data.join("other.part"), "mine").unwrap();

        assert_eq!(remove_partial_downloads(&data), 3);
        assert!(talk.join("models").join("qwen3-tts.gguf").is_file(), "verified downloads stay");
        assert!(talk.join("no-gpu.txt").is_file());
        assert!(data.join("other.part").is_file());

        assert!(remove_voice_data(&data).unwrap());
        assert!(!talk.exists());
        assert!(data.join("other.part").is_file());
        assert!(!remove_voice_data(&data).unwrap(), "nothing left to remove");
        assert_eq!(remove_partial_downloads(&data), 0);
        let _ = std::fs::remove_dir_all(&data);
    }

    #[cfg(unix)]
    #[test]
    fn a_shutdown_is_no_crash_but_a_death_is() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("talk-exit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = |name: &str, body: &str| {
            let path = dir.join(name);
            std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path
        };
        let exits = |exe: PathBuf| {
            let seen = Arc::new(Mutex::new(0usize));
            let counter = seen.clone();
            let client = SpeechClient::new(exe, dir.clone(), move |event| {
                if event == Event::Exited {
                    *counter.lock().unwrap() += 1;
                }
            });
            (client, seen)
        };
        let (client, seen) = exits(script("lives", "exec sleep 30"));
        client.start();
        std::thread::sleep(std::time::Duration::from_millis(100));
        client.shutdown();
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert_eq!(*seen.lock().unwrap(), 0, "an asked-for exit sends no Exited");

        let (client, seen) = exits(script("dies", "exit 1"));
        client.start();
        std::thread::sleep(std::time::Duration::from_millis(500));
        assert_eq!(*seen.lock().unwrap(), 1, "a worker that dies on its own is reported");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Wave review of #1263/#1278: a worker that never SPAWNS is reported
    /// like one that died — once per attempt, from `start` and from a
    /// request that would have spawned it.
    #[test]
    fn a_failed_spawn_is_reported_as_an_exit() {
        let dir = std::env::temp_dir().join(format!("talk-nospawn-{}", std::process::id()));
        let seen = Arc::new(Mutex::new(0usize));
        let counter = seen.clone();
        let client = SpeechClient::new(dir.join("no-such-executable"), dir.clone(), move |event| {
            if event == Event::Exited {
                *counter.lock().unwrap() += 1;
            }
        });
        client.start();
        assert_eq!(*seen.lock().unwrap(), 1, "a failed start is an exit");
        client.call("c1", "en", false, 3_000);
        assert_eq!(*seen.lock().unwrap(), 2, "a request that cannot spawn is an exit");
        // Requests that never spawn a worker say nothing.
        client.stop();
        client.hangup("c1");
        assert_eq!(*seen.lock().unwrap(), 2);
    }

    /// #1278 review: the cleanup is ordered against the worker: nothing goes
    /// while one runs (a download in progress), and a worker started right
    /// after a removal never sees the tree being deleted.
    #[cfg(unix)]
    #[test]
    fn the_cleanup_waits_for_the_worker_and_a_new_one_starts_fresh() {
        use std::os::unix::fs::PermissionsExt;
        let data = std::env::temp_dir().join(format!("talk-order-{}", std::process::id()));
        let talk = voice_data_dir(&data);
        std::fs::create_dir_all(talk.join("models")).unwrap();
        std::fs::write(talk.join("models").join("qwen3-tts.gguf"), "model").unwrap();
        std::fs::write(talk.join("models").join("qwen3-tts.part"), "downloading").unwrap();
        let exe = data.join("worker");
        std::fs::write(&exe, "#!/bin/sh\nexec sleep 30\n").unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        let client = SpeechClient::new(exe, data.clone(), |_| {});

        client.start();
        assert_eq!(client.remove_partial_downloads(), 0, "a running worker's .part is a download in progress");
        assert!(!client.remove_voice_data(), "skipped while a worker runs");
        assert!(talk.join("models").join("qwen3-tts.part").is_file());
        assert!(talk.join("models").join("qwen3-tts.gguf").is_file());

        client.shutdown();
        assert_eq!(client.remove_partial_downloads(), 1);
        assert!(client.remove_voice_data());
        assert!(!talk.exists(), "set aside before the call returns");
        // The worker comes back at once and downloads into a fresh talk/.
        client.start();
        std::fs::create_dir_all(talk.join("models")).unwrap();
        std::fs::write(talk.join("models").join("qwen3-tts.part"), "again").unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::fs::read_dir(&data).unwrap().flatten().any(|e| e.file_name().to_string_lossy().starts_with(ASIDE_PREFIX)) {
            assert!(std::time::Instant::now() < deadline, "the set-aside tree is deleted");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(talk.join("models").join("qwen3-tts.part").is_file(), "the new download is untouched");
        client.shutdown();
        assert!(client.remove_voice_data());
        assert!(client.remove_voice_data(), "nothing left is done too");
        let _ = std::fs::remove_dir_all(&data);
    }
}
