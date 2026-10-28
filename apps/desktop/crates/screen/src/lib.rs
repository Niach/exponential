//! NEW-58: the voice device's SCREEN MEMORY (`fixtures/screen-memory.json`
//! is the spec): the last 10 minutes of the screen as deduplicated frames,
//! and the `get_screenshots` host tool the voice agent's session reaches over
//! a loopback MCP endpoint (`screen`), like the `computer` grant.
//!
//! The IDE turns it on and off (`start` / `stop`) from the tray's Screen
//! memory switch on the member's voice device, hears paused/active through
//! [`set_status_observer`], and hands the voice agent's run a [`Grant`].

mod capture;

use std::path::PathBuf;

pub const SERVER_NAME: &str = "screen";
/// The env var the run's token rides in (`Bearer ${EXP_SCREEN_TOKEN}`).
pub const TOKEN_ENV: &str = "EXP_SCREEN_TOKEN";

/// What a run needs to reach the endpoint.
#[derive(Clone, PartialEq, Eq)]
pub struct Grant {
    pub url: String,
    pub token: String,
}

impl std::fmt::Debug for Grant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Grant({}, <redacted>)", self.url)
    }
}

/// What the tray item shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Off,
    Active,
    /// A denylisted window is focused, or the pause hotkey holds it.
    Paused,
}

/// Start capturing into `data_dir/screen` (frames) + `data_dir/screen.sqlite`
/// (index); a no-op while running. `Err` = this machine cannot capture.
pub fn start(data_dir: PathBuf) -> Result<(), String> {
    let _ = data_dir;
    Ok(())
}

/// Stop capturing; `forget` also deletes every frame and index row (sign
/// out, another voice device or account).
pub fn stop(forget: bool) {
    let _ = forget;
}

pub fn status() -> Status {
    Status::Off
}

/// Who hears Off / Active / Paused changes (the tray). Called at once.
pub fn set_status_observer(observer: impl Fn(Status) + Send + Sync + 'static) {
    observer(Status::Off);
}

/// NEW-57's pause hotkey: holds capture paused while `true`.
pub fn set_hotkey_paused(paused: bool) {
    let _ = paused;
}

/// Admit `session_id` (the voice agent's conversation run) to
/// `get_screenshots`; `Err` while screen memory is off.
pub fn grant(session_id: &str) -> Result<Grant, String> {
    let _ = session_id;
    Err("Screen memory is off on this device.".to_string())
}

/// The run ended: its token is dead.
pub fn revoke(session_id: &str) {
    let _ = session_id;
}
