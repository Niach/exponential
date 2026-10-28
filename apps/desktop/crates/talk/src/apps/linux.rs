//! NEW-57: linux desktop audio (meeting.json CAPABILITY). Filled by its lane.

use super::{AppCapture, AudioApp, Capability, Selection};

pub(super) fn capability() -> Capability {
    Capability::None
}

pub(super) fn live() -> Vec<AudioApp> {
    Vec::new()
}

pub(super) fn open(_selection: Selection) -> Result<AppCapture, String> {
    Err("Desktop audio is not available here yet.".to_string())
}
