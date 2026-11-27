//! #EDE-237: eDE mode's page windows off unix. The session bus
//! (`api::bus`) is unix-only and eDE runs only on Linux, so here every
//! entry point is a no-op: no page windows, nothing to route or restore.

use gpui::{AnyWindowHandle, App, Window, WindowId};

use crate::navigation::Screen;

pub fn set_app_id(_app_id: &str) {}

pub(crate) fn register(_window: &mut Window, _cx: &mut App) {}

pub(crate) fn unregister(_window_id: WindowId, _cx: &mut App) {}

pub fn quitting(_cx: &mut App) {}

pub(crate) fn any_page_window(_cx: &App) -> Option<AnyWindowHandle> {
    None
}

pub(crate) fn route(_window: &Window, _screen: &Screen, _cx: &mut App) -> bool {
    false
}

pub(crate) fn publish(_window: &mut Window, _cx: &mut App) {}

pub fn restore(_cx: &mut App) -> bool {
    false
}
