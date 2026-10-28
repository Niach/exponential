//! NEW-57: MEETING MODE in the IDE on the member's voice device
//! (`packages/domain-contract/fixtures/meeting.json`). The host side: it
//! follows the voice settings (`meetingStartedAt`, `desktopAudio`,
//! `desktopAudioApps`) handed over on every heartbeat, asks the speech
//! worker to listen per channel, folds the finalized segments through
//! `domain::meeting::flush_step`, sends each batch to the member's
//! conversation with the voice agent (`sessions.send` with `meeting: true`),
//! keeps the raw segments in the IDE's SQLite for 30 days and owns the pause.
//!
//! The rest of the IDE reads [`MeetingState`] (a gpui Global; observe it
//! with `cx.observe_global::<MeetingState>`) and flips the pause with
//! [`set_paused`].

use gpui::{App, Global};

/// What the tray, the rail, Settings → Voice and the call host read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MeetingState {
    /// A meeting runs on THIS device (the voice device, `meetingStartedAt` set).
    pub running: bool,
    /// The pause hotkey holds every channel (and screen memory) paused.
    pub paused: bool,
    /// `domain::meeting::who(...)` for the running meeting (the tray label).
    pub who: String,
}

impl Global for MeetingState {}

/// Install the Global and start following the voice settings.
pub fn init(cx: &mut App) {
    cx.set_global(MeetingState::default());
}

/// The current state (default before [`init`]).
pub fn state(cx: &App) -> MeetingState {
    cx.try_global::<MeetingState>().cloned().unwrap_or_default()
}

/// Pause or resume listening (the hotkey and the tray's Pause listening):
/// every meeting channel and screen memory (`screen::set_hotkey_paused`).
pub fn set_paused(paused: bool, cx: &mut App) {
    screen::set_hotkey_paused(paused);
    let mut next = state(cx);
    next.paused = paused;
    cx.set_global(next);
}
