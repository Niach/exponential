//! NEW-57: per-app desktop audio for meeting mode (`fixtures/meeting.json`
//! CAPABILITY + CHANNELS). The host reads [`capability`] and [`live`] for
//! the device doctor (`doctor.desktopAudio`); the speech worker (`crates/voice`)
//! [`open`]s the channels. Never a mix: one [`Chunk`] stream per app (or one
//! `desktop` channel where the OS only offers system loopback).
//!
//! One implementation per OS: macOS Core Audio process taps (14.2+), Linux
//! PipeWire app nodes (PulseAudio's monitor source as `system`), Windows
//! WASAPI process loopback (build 20348+, else system loopback).

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// What this machine can capture (`doctor.desktopAudio.capability`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    None,
    System,
    PerApp,
}

/// One app that plays audio now (`doctor.desktopAudio.apps`): `id` is stable
/// across its processes (macOS bundle id, Linux `application.process.binary`
/// or `application.name`, Windows the exe's file stem lowercased).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AudioApp {
    pub id: String,
    pub name: String,
}

/// Which apps a meeting listens to (`desktopAudio` minus `off`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Selection {
    All,
    Include(Vec<String>),
    Exclude(Vec<String>),
}

impl Selection {
    /// Whether `app` is listened to (never this process itself: the caller
    /// filters its own pid).
    pub fn wants(&self, app: &str) -> bool {
        match self {
            Selection::All => true,
            Selection::Include(ids) => ids.iter().any(|id| id == app),
            Selection::Exclude(ids) => !ids.iter().any(|id| id == app),
        }
    }
}

/// The `system` channel's id (`meeting.json` `desktop`).
pub const DESKTOP_CHANNEL: &str = "desktop";
/// The rate every chunk arrives at: mono f32, the speech worker's rate.
pub const SAMPLE_RATE: u32 = 16_000;

/// Audio of ONE channel: an app's id (or [`DESKTOP_CHANNEL`]) and its name.
#[derive(Clone, Debug, PartialEq)]
pub struct Chunk {
    pub channel: String,
    pub label: String,
    /// Mono f32 at [`SAMPLE_RATE`].
    pub samples: Vec<f32>,
    /// When its LAST sample was captured, epoch ms (the OS callback that
    /// delivered it, read on the wall clock there): the worker dates
    /// segments by this, never by when it read the chunk.
    pub captured_ms: u64,
}

/// How many chunks a capture holds for the worker (meeting.json CHANNELS:
/// BOUNDED). Chunks are 20 ms (macOS, per app) to 100 ms (Linux, Windows),
/// so this is ~10 s of audio for a few apps; when full, a chunk is dropped.
pub const CAPTURE_QUEUE: usize = 1024;
/// Dropped chunks are logged at most this often.
pub const DROP_LOG_EVERY_MS: u64 = 10_000;

/// The wall clock, epoch ms (a chunk's `captured_ms` where the OS gives no
/// better time).
pub fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_millis() as u64)
}

/// Counts dropped chunks and says when a log line is due (pure).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct DropLog {
    pub(crate) total: u64,
    unlogged: u64,
    last_log_ms: Option<u64>,
}

impl DropLog {
    /// One chunk dropped at `now_ms`: `Some((since the last line, total))`
    /// when a log line is due (the first drop, then every
    /// [`DROP_LOG_EVERY_MS`]).
    pub(crate) fn record(&mut self, now_ms: u64) -> Option<(u64, u64)> {
        self.total += 1;
        self.unlogged += 1;
        if self.last_log_ms.is_some_and(|last| now_ms.saturating_sub(last) < DROP_LOG_EVERY_MS) {
            return None;
        }
        self.last_log_ms = Some(now_ms);
        Some((std::mem::take(&mut self.unlogged), self.total))
    }
}

/// The capture side of a bounded chunk queue: never blocks the OS thread.
#[derive(Clone)]
pub(crate) struct ChunkSender {
    tx: SyncSender<Chunk>,
    drops: Arc<Mutex<DropLog>>,
}

impl ChunkSender {
    /// Hand `chunk` to the worker; a full queue drops it (counted, logged
    /// rate-limited). `false` = the worker is gone (stop capturing).
    pub(crate) fn send(&self, chunk: Chunk) -> bool {
        match self.tx.try_send(chunk) {
            Ok(()) => true,
            Err(TrySendError::Full(chunk)) => {
                let due = self.drops.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).record(now_ms());
                if let Some((dropped, total)) = due {
                    log::warn!("talk: desktop audio queue full; dropped {dropped} chunk(s) (latest {}, {total} in all)", chunk.channel);
                }
                true
            }
            Err(TrySendError::Disconnected(_)) => false,
        }
    }

    #[cfg(test)]
    fn dropped(&self) -> u64 {
        self.drops.lock().unwrap().total
    }
}

/// A bounded chunk queue of `capacity` ([`CAPTURE_QUEUE`] in production).
pub(crate) fn chunk_queue(capacity: usize) -> (ChunkSender, Receiver<Chunk>) {
    let (tx, rx) = sync_channel(capacity);
    (ChunkSender { tx, drops: Arc::default() }, rx)
}

/// A running capture: chunks of every listened app, apps joining as they
/// start playing and leaving as they stop. Dropping it stops everything.
pub struct AppCapture {
    rx: Receiver<Chunk>,
    _guard: Box<dyn Send>,
}

impl AppCapture {
    pub fn new(rx: Receiver<Chunk>, guard: Box<dyn Send>) -> Self {
        AppCapture { rx, _guard: guard }
    }

    pub fn recv_timeout(&self, timeout: Duration) -> Result<Chunk, RecvTimeoutError> {
        self.rx.recv_timeout(timeout)
    }
}

/// What this machine can capture, read without prompting.
pub fn capability() -> Capability {
    #[cfg(target_os = "linux")]
    return linux::capability();
    #[cfg(target_os = "macos")]
    return macos::capability();
    #[cfg(target_os = "windows")]
    return windows::capability();
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    Capability::None
}

/// The apps that play audio right now (never this process), by name.
pub fn live() -> Vec<AudioApp> {
    #[cfg(target_os = "linux")]
    return linux::live();
    #[cfg(target_os = "macos")]
    return macos::live();
    #[cfg(target_os = "windows")]
    return windows::live();
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    Vec::new()
}

/// macOS: whether the Audio Recording permission process taps need is
/// granted (`Some(false)` = the doctor's `grant`); `None` where the OS asks
/// for none.
pub fn permission() -> Option<bool> {
    #[cfg(target_os = "macos")]
    return macos::permission();
    #[cfg(not(target_os = "macos"))]
    None
}

/// macOS: ask for that permission now (the doctor's `grant`); returns the
/// state afterwards.
pub fn request_permission() -> Option<bool> {
    #[cfg(target_os = "macos")]
    return macos::request_permission();
    #[cfg(not(target_os = "macos"))]
    None
}

/// Start capturing `selection` (per app where the OS can; one
/// [`DESKTOP_CHANNEL`] where it can only loop the whole system back).
pub fn open(selection: Selection) -> Result<AppCapture, String> {
    #[cfg(target_os = "linux")]
    return linux::open(selection);
    #[cfg(target_os = "macos")]
    return macos::open(selection);
    #[cfg(target_os = "windows")]
    return windows::open(selection);
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = selection;
        Err("Desktop audio is not available on this OS.".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(n: u64) -> Chunk {
        Chunk { channel: "z".into(), label: "Zoom".into(), samples: vec![0.0; 4], captured_ms: n }
    }

    #[test]
    fn a_full_queue_drops_and_counts_without_blocking() {
        let (tx, rx) = chunk_queue(2);
        assert!(tx.send(chunk(1)) && tx.send(chunk(2)));
        assert!(tx.send(chunk(3)), "full: dropped, the capture goes on");
        assert!(tx.send(chunk(4)));
        assert_eq!(tx.dropped(), 2);
        let got: Vec<u64> = rx.try_iter().map(|chunk| chunk.captured_ms).collect();
        assert_eq!(got, [1, 2], "the oldest stay, the newest are dropped");
        assert!(tx.send(chunk(5)), "room again");
        assert_eq!(rx.try_recv().unwrap().captured_ms, 5);
        drop(rx);
        assert!(!tx.send(chunk(6)), "the worker is gone");
    }

    #[test]
    fn drops_are_logged_at_most_every_ten_seconds() {
        let mut log = DropLog::default();
        assert_eq!(log.record(1_000), Some((1, 1)), "the first drop says so at once");
        assert_eq!(log.record(2_000), None);
        assert_eq!(log.record(10_999), None);
        assert_eq!(log.record(11_000), Some((3, 4)), "then a line with the drops since");
        assert_eq!(log.record(11_001), None);
        assert_eq!(log.total, 5);
    }
}
