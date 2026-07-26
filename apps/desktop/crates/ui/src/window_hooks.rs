//! Host callbacks `ui` needs but cannot implement itself (EXP-287).
//!
//! `ui` may not depend on `app` (the dependency runs the other way), so the
//! binary installs a closure here and `ui` calls it. Today there is exactly
//! one: "a window this crate opened just came up".
//!
//! Why that matters — `app::x11_window_icon` stamps `_NET_WM_ICON` on our X11
//! windows from a SHORT-LIVED thread started at launch (it expires ~16s in,
//! once the desktop shell has re-indexed our `.desktop` file). Every window
//! that opens later — a native dialog, an undocked terminal — maps with no
//! icon property, and since EXP-287 a dialog is a `WindowKind::Normal` window
//! with its own taskbar button, so that gap is now a visible generic gear
//! sitting next to the app's own button. Re-running the (idempotent) stamping
//! pass per window closes it. No-op on Wayland, macOS and Windows, where the
//! `app_id`/bundle association is the whole story.

use gpui::{App, Global};
use std::rc::Rc;

struct WindowOpenedHook(Rc<dyn Fn()>);

impl Global for WindowOpenedHook {}

/// Install the host's "window opened" callback. Called once from `app`.
pub fn set_window_opened_hook(f: impl Fn() + 'static, cx: &mut App) {
    cx.set_global(WindowOpenedHook(Rc::new(f)));
}

/// Fire the host hook, if one was installed.
pub(crate) fn notify_window_opened(cx: &App) {
    if let Some(hook) = cx.try_global::<WindowOpenedHook>() {
        (hook.0)();
    }
}
