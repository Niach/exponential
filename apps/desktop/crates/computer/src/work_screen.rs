//! NEW-59: THE work screen (`fixtures/work-screen.json`): which displays
//! agents may drive, the `wrong_display` refusals the hub answers with,
//! the run's current display and where the voice overlay may open. The
//! pure rules replay the fixture; the OS reads go through the backend.

use crate::{Display, ScreenState};

pub(crate) fn set_picked(_number: Option<u32>) {
    // Filled by the backend lane.
}

pub(crate) fn displays() -> Vec<Display> {
    Vec::new()
}

pub(crate) fn screen_state() -> Option<ScreenState> {
    None
}

pub(crate) fn banner_display() -> Option<Display> {
    None
}

/// The display the voice overlay opens on (0-based), `None` = hidden
/// (`overlayCases`).
pub fn overlay_display(count: usize, pointer: usize, fullscreen: &[usize], work: &[usize]) -> Option<usize> {
    let free = |index: usize| !fullscreen.contains(&index);
    if pointer < count && free(pointer) {
        return Some(pointer);
    }
    if let Some(&index) = work.iter().find(|&&index| free(index)) {
        return Some(index);
    }
    (0..count).find(|&index| free(index))
}
