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

/// EXP-638: "open a shell window" — the OS-notification click path needs
/// one when every shell window is closed (macOS keeps the app alive in the
/// dock; a click on a banner must still land somewhere). The host owns
/// `open_shell_window` (per-window layout slots, cascade, platform chrome),
/// so `ui` calls back into it exactly like the window-opened hook above.
struct OpenShellWindowHook(Rc<dyn Fn(&mut App)>);

impl Global for OpenShellWindowHook {}

/// Install the host's "open a shell window" callback. Called once from `app`.
pub fn set_open_shell_window_hook(f: impl Fn(&mut App) + 'static, cx: &mut App) {
    cx.set_global(OpenShellWindowHook(Rc::new(f)));
}

/// Ask the host for a new shell window. `false` when no hook is installed
/// (headless/test harnesses) — the caller then simply has nowhere to route.
pub(crate) fn request_shell_window(cx: &mut App) -> bool {
    let Some(hook) = cx.try_global::<OpenShellWindowHook>().map(|hook| hook.0.clone()) else {
        return false;
    };
    (hook)(cx);
    true
}

/// #EDE-64: the IDE runs as the eDE session's device
/// (`exponential-device.service` sets `EXPONENTIAL_SESSION_DEVICE`): it
/// starts with no window and closing the last window keeps it running, so
/// the device stays online and its coding sessions live on. A launch from
/// the panel (a second launch, forwarded) brings a window up.
struct ResidentDevice;

impl Global for ResidentDevice {}

/// Mark this process as the session's resident device. Called once from `app`.
pub fn set_resident_device(cx: &mut App) {
    cx.set_global(ResidentDevice);
}

/// This process is the session's resident device.
pub fn resident_device(cx: &App) -> bool {
    cx.has_global::<ResidentDevice>()
}

/// #EXP-1938: gpui never quits the app by itself. Its default quits on
/// Linux and Windows the moment the last window is gone, before anything
/// here is asked: the tray's "closing the last window keeps the IDE
/// running" (`etalk_gpui::voice_tray`) and the resident device (#EDE-64)
/// could not hold. The quit is the shell's own (its release guard), the
/// tray's and [`init`]'s observer.
pub const QUIT_MODE: gpui::QuitMode = gpui::QuitMode::Explicit;

/// Nothing keeps the app running: no window, no tray, not the resident
/// device.
fn nothing_keeps_running(cx: &App) -> bool {
    cx.windows().is_empty() && !etalk_gpui::voice_tray::keeps_running(cx) && !resident_device(cx)
}

/// #EXP-1938: the windows that are no shell (the push-to-talk capsule, the
/// call pill, the computer-use banner) can be the last to close, after the
/// tray that kept the IDE went: the app ends then, as gpui's default did.
/// macOS keeps running without a window, as always.
pub(crate) fn init(cx: &mut App) {
    #[cfg(not(target_os = "macos"))]
    cx.on_window_closed(|cx, _| {
        if nothing_keeps_running(cx) {
            cx.quit();
        }
    })
    .detach();
    #[cfg(target_os = "macos")]
    let _ = (cx, nothing_keeps_running);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #EXP-1938: what the last window's close asks before it quits.
    #[gpui::test]
    async fn the_last_window_closing_quits_only_when_nothing_keeps_the_app(cx: &mut gpui::TestAppContext) {
        assert!(cx.update(|cx| nothing_keeps_running(cx)), "no window, no tray, no resident device");
        let window = cx.add_window(|_, _| gpui::Empty);
        assert!(!cx.update(|cx| nothing_keeps_running(cx)), "a window is open");
        cx.update(|cx| window.update(cx, |_, window, _| window.remove_window())).unwrap();
        cx.run_until_parked();
        assert!(cx.update(|cx| nothing_keeps_running(cx)));
        cx.update(set_resident_device);
        assert!(!cx.update(|cx| nothing_keeps_running(cx)), "#EDE-64: the session's device stays");
    }

    /// #EXP-1938: a real gpui loop (headless, in a child process) under the
    /// app's quit mode: the last window closes and the app is still there
    /// for whoever keeps it (the tray, the resident device). gpui's default
    /// ends the loop inside the close.
    #[cfg(target_os = "linux")]
    #[test]
    fn gpui_leaves_the_quit_to_the_app_when_the_last_window_closes() {
        use gpui::AppContext as _;
        use std::{cell::Cell, process::Command, time::Duration};

        const CHILD: &str = "EXP_QUIT_MODE_CHILD";
        if std::env::var_os(CHILD).is_some() {
            let outlived = Rc::new(Cell::new(false));
            gpui_platform::headless().with_quit_mode(QUIT_MODE).run({
                let outlived = outlived.clone();
                move |cx| {
                    cx.spawn(async move |cx| {
                        let window = cx
                            .update(|cx| cx.open_window(gpui::WindowOptions::default(), |_, cx| cx.new(|_| gpui::Empty)))
                            .expect("a headless window opens");
                        cx.update(|cx| window.update(cx, |_, window, _| window.remove_window())).expect("it closes");
                        cx.background_executor().timer(Duration::from_millis(200)).await;
                        outlived.set(true);
                        cx.update(|cx| cx.quit());
                    })
                    .detach();
                }
            });
            assert!(outlived.get(), "gpui quit with the last window");
            return;
        }
        // Test names don't include the crate's name.
        let module = module_path!().split_once("::").map_or(module_path!(), |(_, module)| module);
        let name = format!("{module}::gpui_leaves_the_quit_to_the_app_when_the_last_window_closes");
        let output = Command::new(std::env::current_exe().expect("the test binary"))
            .args(["--exact", &name, "--nocapture"])
            .env(CHILD, "1")
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .output()
            .expect("the child runs");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success() && stdout.contains("1 passed"),
            "the app did not outlive its last window\nstdout:\n{stdout}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
