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

use std::sync::mpsc::{Receiver, RecvTimeoutError};
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
