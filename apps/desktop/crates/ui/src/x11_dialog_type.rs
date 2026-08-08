//! Linux/X11: make a resizable dialog window maximizable (EXP-450).
//!
//! EXP-308 opens every Linux dialog as `WindowKind::Floating` because the
//! transient-parent relationship it stamps (`WM_TRANSIENT_FOR`) is the ONLY
//! parent-centered placement gpui's Linux backends offer. The cost was not
//! obvious at the time: a window with `WM_TRANSIENT_FOR` and no explicit
//! `_NET_WM_WINDOW_TYPE` is classified as a DIALOG by EWMH window managers,
//! and a dialog is not maximizable. Verified against muffin/mutter — the
//! window's `_NET_WM_ALLOWED_ACTIONS` carries neither
//! `_NET_WM_ACTION_MAXIMIZE_VERT` nor `_..._HORZ`, so the `_NET_WM_STATE`
//! toggle `Window::zoom_window` sends is dropped on the floor and the
//! titlebar strip's maximize control is dead chrome (the reported bug: "New
//! issue" maximize does nothing).
//!
//! The fix is to say out loud what the type inference guessed: stamp
//! `_NET_WM_WINDOW_TYPE = _NET_WM_WINDOW_TYPE_NORMAL`, which restores
//! `_NET_WM_ACTION_MAXIMIZE_*` (plus minimize/fullscreen) while
//! `WM_TRANSIENT_FOR` — and with it the transient's parent-centering and
//! always-above-its-opener stacking — stays exactly as it was.
//!
//! **Timing is the whole trick**: the WM reads the type ONCE, when it takes
//! the window over, and that is also when it places it. Stamping before that
//! would buy maximize back at the price of the centering EXP-308 exists for,
//! so this waits for `WM_STATE` — the property a WM sets on a window it has
//! adopted, i.e. after placement — and only then rewrites the type. Managers
//! recompute the allowed actions on the resulting `PropertyNotify`, so the
//! already-centered window simply gains the maximize it was missing.
//!
//! The one visible side effect: an ordinary window is no longer tagged
//! `_NET_WM_STATE_SKIP_TASKBAR`, so a resizable dialog earns the taskbar
//! button EXP-287 wanted and EXP-308 traded away for placement. The minimize
//! control still stays off — the rest of the Linux dialogs are unpromoted
//! transients and the strip's chrome should not differ per dialog.
//!
//! Scope: Linux/X11 only, and only for dialogs that actually DRAW a maximize
//! control (`DialogSpec::resizable`) — a fixed-size dialog would gain a WM
//! maximize its content cannot use. Off Linux a native-chrome dialog is
//! already `WindowKind::Normal`, and Wayland has no window-type protocol for
//! `xdg_toplevel.set_parent` to trip over (unverified there — this is a
//! no-op under a compositor either way). gpui exposes no window-type API at
//! the pinned rev, and we never fork it; this is plain EWMH on our own window
//! id, the same trade `app::x11_window_icon` makes for `_NET_WM_ICON` (x11rb
//! is already in the tree via gpui_linux's X11 backend).

use std::time::Duration;

use gpui::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _, PropMode};
use x11rb::wrapper::ConnectionExt as _;

/// How long to wait for a window manager to adopt the window before giving
/// up on the ordering guarantee. 2s is far past any real WM's manage pass;
/// past it we stamp anyway — with no WM running there is nothing to order
/// against, and the property is harmless.
const ADOPT_POLL: Duration = Duration::from_millis(20);
const ADOPT_ROUNDS: usize = 100;

/// Ask the window manager to treat this dialog as an ordinary window, so its
/// maximize control works. Best-effort and off the foreground thread: a
/// dialog that never gets the stamp is exactly today's behavior.
pub(crate) fn allow_maximize(window: &Window) {
    // gpui prefers the Wayland backend whenever a compositor is reachable —
    // then our window is no X11 client and there is no type to stamp.
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        return;
    }
    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    let RawWindowHandle::Xcb(xcb) = handle.as_raw() else {
        return;
    };
    let x_window = xcb.window.get();
    let _ = std::thread::Builder::new()
        .name("x11-dialog-type".into())
        .spawn(move || {
            if let Err(err) = promote(x_window) {
                log::warn!("[ui] X11 dialog window type: {err:#}");
            }
        });
}

fn promote(x_window: u32) -> anyhow::Result<()> {
    let (conn, _screen) = x11rb::connect(None)?;
    let wm_state = conn.intern_atom(false, b"WM_STATE")?.reply()?.atom;
    let window_type = conn
        .intern_atom(false, b"_NET_WM_WINDOW_TYPE")?
        .reply()?
        .atom;
    let normal = conn
        .intern_atom(false, b"_NET_WM_WINDOW_TYPE_NORMAL")?
        .reply()?
        .atom;

    for _ in 0..ADOPT_ROUNDS {
        match conn
            .get_property(false, x_window, wm_state, AtomEnum::ANY, 0, 1)?
            .reply()
        {
            // `type_` stays NONE until the WM has adopted the window.
            Ok(reply) if reply.type_ != x11rb::NONE => break,
            Ok(_) => std::thread::sleep(ADOPT_POLL),
            // An X error means the dialog was closed again — nothing to stamp.
            Err(_) => return Ok(()),
        }
    }

    conn.change_property32(
        PropMode::REPLACE,
        x_window,
        window_type,
        AtomEnum::ATOM,
        &[normal],
    )?
    .check()?;
    Ok(())
}
