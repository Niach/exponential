//! NEW-57: Linux desktop audio (meeting.json CAPABILITY + CHANNELS).
//!
//! PipeWire (the `pipewire` feature, on by default: only the IDE executable
//! links `talk`, and it already loads libpipewire through gpui) = per_app:
//! one capture stream per `Stream/Output/Audio` node, the nodes of one app
//! (`application.process.binary`, else `application.name`) summed into its
//! channel. No PipeWire daemon (or the feature off) but a PulseAudio server
//! = system: the default sink's monitor (`@DEFAULT_MONITOR@`) through
//! `parec` (pulseaudio-utils), ONE `desktop` channel. Neither = none.

#[path = "pcm.rs"]
mod pcm;

mod nodes;
#[cfg(feature = "pipewire")]
mod pipewire;
mod pulse;

use super::{AppCapture, AudioApp, Capability, Selection};

pub(super) fn capability() -> Capability {
    #[cfg(feature = "pipewire")]
    if pipewire::available() {
        return Capability::PerApp;
    }
    if pulse::available() {
        return Capability::System;
    }
    Capability::None
}

pub(super) fn live() -> Vec<AudioApp> {
    #[cfg(feature = "pipewire")]
    match pipewire::live() {
        Ok(apps) => return apps,
        Err(error) => log::debug!("[talk] PipeWire apps: {error}"),
    }
    Vec::new()
}

pub(super) fn open(selection: Selection) -> Result<AppCapture, String> {
    #[cfg(feature = "pipewire")]
    if pipewire::available() {
        return pipewire::open(selection);
    }
    if pulse::available() {
        // System loopback only: `include` / `exclude` behave as `all`.
        let _ = selection;
        return pulse::open();
    }
    Err("Desktop audio needs PipeWire or PulseAudio.".to_string())
}
