//! Last-used window size persistence (EXP-210) — strictly LOCAL (a plain
//! JSON file in the app data dir, never synced to the server).
//!
//! The main shell window (ordinal 0) records its size on every resize
//! (debounced in [`crate::shell::Shell`], flushed on quit); the next launch
//! opens at that size instead of the fixed default. Sizes are clamped to
//! [`MIN_SIZE`] — the floor below which page layouts break — which is also
//! the `window_min_size` every shell window is opened with.

use std::path::PathBuf;

use anyhow::{anyhow, Context as _, Result};
use gpui::{px, size, App, Pixels, Size, Window};
use serde::{Deserialize, Serialize};

/// Minimum shell-window content size (EXP-210: layouts break below 800×600).
pub const MIN_SIZE: Size<Pixels> = Size {
    width: px(800.),
    height: px(600.),
};

/// First-launch default (no persisted size yet) — §3.6.
pub const DEFAULT_SIZE: Size<Pixels> = Size {
    width: px(1280.),
    height: px(820.),
};

/// Logical (scale-independent) pixels, the unit gpui bounds already use.
#[derive(Serialize, Deserialize)]
struct SavedSize {
    width: f32,
    height: f32,
}

/// The app-local data dir shared with the per-window layout files. macOS:
/// `~/Library/Application Support/Exponential/…`; Linux:
/// `~/.local/share/exponential/…`.
pub(crate) fn app_data_dir() -> Option<PathBuf> {
    Some(dirs::data_local_dir()?.join(if cfg!(target_os = "macos") {
        "Exponential"
    } else {
        "exponential"
    }))
}

fn size_file() -> Option<PathBuf> {
    Some(app_data_dir()?.join("window-size.json"))
}

/// How many consecutive sub-floor frames we will answer with a resize
/// request before conceding the window manager owns the size. A tiling WM
/// (i3, xmonad, …) refuses the request and, per ICCCM, replies with a
/// synthetic ConfigureNotify at the UNCHANGED size — which gpui dispatches
/// as another bounds change, so an unbounded clamp would spin at frame rate
/// for as long as the tile stays below the floor. The budget refills the
/// moment a frame lands at or above the floor, so an ordinary WM that
/// honors the request never runs it down.
const MAX_CLAMP_ATTEMPTS: u8 = 3;

/// Per-window state for [`enforce_min_size`]. Owned by the shell view, so
/// two windows can never share (or exhaust) each other's budget.
#[derive(Default)]
pub struct MinSizeClamp {
    attempts: u8,
}

impl MinSizeClamp {
    /// The size to resize to for `current`, or `None` to leave the window
    /// alone (already at/above the floor, compositor-owned, or the WM has
    /// refused [`MAX_CLAMP_ATTEMPTS`] times in a row).
    fn next(
        &mut self,
        current: Size<Pixels>,
        is_fullscreen: bool,
        is_maximized: bool,
    ) -> Option<Size<Pixels>> {
        // Fullscreen/maximized frames are the compositor's size, never
        // fought — and they refill the budget like any healthy frame.
        if is_fullscreen || is_maximized {
            self.attempts = 0;
            return None;
        }
        if current.width >= MIN_SIZE.width && current.height >= MIN_SIZE.height {
            self.attempts = 0;
            return None;
        }
        if self.attempts >= MAX_CLAMP_ATTEMPTS {
            return None;
        }
        self.attempts += 1;
        Some(size(
            current.width.max(MIN_SIZE.width),
            current.height.max(MIN_SIZE.height),
        ))
    }
}

/// Re-assert [`MIN_SIZE`] on a live window (EXP-263). macOS enforces the
/// `window_min_size` option natively, but on Linux it is advisory only —
/// X11 window managers may ignore (or mis-scale) the WM_NORMAL_HINTS and
/// Wayland compositors are free to configure any size, which gpui applies
/// blindly — so shell windows clamp themselves back whenever a resize
/// lands below the floor. Fullscreen/maximized frames are the
/// compositor's size, never fought, and a WM that keeps refusing wins
/// after [`MAX_CLAMP_ATTEMPTS`] (see [`MinSizeClamp`]).
pub fn enforce_min_size(clamp: &mut MinSizeClamp, window: &mut Window, cx: &mut App) {
    let Some(clamped) = clamp.next(
        window.viewport_size(),
        window.is_fullscreen(),
        window.is_maximized(),
    ) else {
        return;
    };
    // Deferred: resizing from inside a bounds observer would re-enter the
    // platform resize path mid-dispatch.
    window.defer(cx, move |window, _cx| window.resize(clamped));
}

/// The persisted last-used size, clamped to [`MIN_SIZE`]; `None` on first
/// launch or an unreadable/garbled file (callers fall back to
/// [`DEFAULT_SIZE`]).
pub fn load_last_size() -> Option<Size<Pixels>> {
    let json = std::fs::read_to_string(size_file()?).ok()?;
    let saved: SavedSize = serde_json::from_str(&json).ok()?;
    if !saved.width.is_finite() || !saved.height.is_finite() {
        return None;
    }
    Some(size(
        px(saved.width).max(MIN_SIZE.width),
        px(saved.height).max(MIN_SIZE.height),
    ))
}

/// Persist `last` as the next launch's window size (best-effort at call
/// sites — a failed write only costs the remembered size).
pub fn save_last_size(last: Size<Pixels>) -> Result<()> {
    let path = size_file().ok_or_else(|| anyhow!("no data dir"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let saved = SavedSize {
        width: f32::from(last.width),
        height: f32::from(last.height),
    };
    let json = serde_json::to_string(&saved).context("serialize window size")?;
    std::fs::write(&path, json).context("write window size")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const BELOW: Size<Pixels> = Size {
        width: px(640.),
        height: px(480.),
    };

    #[test]
    fn clamps_a_sub_floor_frame_up_to_the_floor() {
        let mut clamp = MinSizeClamp::default();
        assert_eq!(clamp.next(BELOW, false, false), Some(MIN_SIZE));
    }

    #[test]
    fn clamps_per_axis_only() {
        let mut clamp = MinSizeClamp::default();
        let wide_but_short = size(px(1600.), px(480.));
        assert_eq!(
            clamp.next(wide_but_short, false, false),
            Some(size(px(1600.), MIN_SIZE.height))
        );
    }

    #[test]
    fn leaves_healthy_fullscreen_and_maximized_frames_alone() {
        let mut clamp = MinSizeClamp::default();
        assert_eq!(clamp.next(size(px(1280.), px(820.)), false, false), None);
        assert_eq!(clamp.next(BELOW, true, false), None);
        assert_eq!(clamp.next(BELOW, false, true), None);
    }

    #[test]
    fn gives_up_after_a_wm_refuses_the_budget() {
        // A tiling WM replies to every request with a synthetic
        // ConfigureNotify at the unchanged sub-floor size — without the
        // budget this is an infinite resize/redraw loop (EXP-263 fix).
        let mut clamp = MinSizeClamp::default();
        for _ in 0..MAX_CLAMP_ATTEMPTS {
            assert_eq!(clamp.next(BELOW, false, false), Some(MIN_SIZE));
        }
        assert_eq!(clamp.next(BELOW, false, false), None);
        assert_eq!(clamp.next(BELOW, false, false), None);
    }

    #[test]
    fn a_healthy_frame_refills_the_budget() {
        let mut clamp = MinSizeClamp::default();
        for _ in 0..MAX_CLAMP_ATTEMPTS {
            clamp.next(BELOW, false, false);
        }
        assert_eq!(clamp.next(BELOW, false, false), None);
        // The WM honored a resize (or the user dragged back up) — the next
        // dip below the floor gets clamped again.
        assert_eq!(clamp.next(size(px(1280.), px(820.)), false, false), None);
        assert_eq!(clamp.next(BELOW, false, false), Some(MIN_SIZE));
    }
}
