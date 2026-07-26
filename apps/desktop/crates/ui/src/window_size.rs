//! Last-used window size persistence (EXP-210) — strictly LOCAL (a plain
//! JSON file in the app data dir, never synced to the server).
//!
//! The main shell window (ordinal 0) records its size on every resize
//! (debounced in [`crate::shell::Shell`], flushed on quit); the next launch
//! opens at that size instead of the fixed default. Sizes are clamped to
//! [`MIN_SIZE`] — the floor below which page layouts break — which is also
//! the `window_min_size` every shell window is opened with.

use std::{path::PathBuf, sync::OnceLock};

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

/// The outer size the main window was opened with (`open_shell_window`), so
/// the size observer can tell a launch echo from a real user resize. Set
/// once per process, before the first window exists.
static LAUNCH_SIZE: OnceLock<Size<Pixels>> = OnceLock::new();

/// Record the size handed to `WindowOptions.window_bounds` for the MAIN
/// window (EXP-276/EXP-278). Later windows are ignored — only ordinal 0
/// persists its size, and every window opens at the same computed size
/// anyway.
pub fn note_launch_size(requested: Size<Pixels>) {
    let _ = LAUNCH_SIZE.set(requested);
}

/// The size the main window was opened at, if it was recorded.
pub fn launch_size() -> Option<Size<Pixels>> {
    LAUNCH_SIZE.get().copied()
}

/// Tolerance for "is this frame the size we asked for?". Scale-factor round
/// trips (logical → device → logical) can land a hair off an integral value;
/// the smallest thing we must still tell apart is the 24px client inset, so
/// half a pixel is comfortably inside the noise floor.
const ECHO_EPSILON: Pixels = px(0.5);

fn approx_eq(a: Size<Pixels>, b: Size<Pixels>) -> bool {
    (f32::from(a.width) - f32::from(b.width)).abs() <= f32::from(ECHO_EPSILON)
        && (f32::from(a.height) - f32::from(b.height)).abs() <= f32::from(ECHO_EPSILON)
}

/// What [`WindowSizeTracker::observe`] wants done with one observed frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SizeFrame {
    /// Nothing to do — a degenerate frame, or a launch echo (the platform
    /// merely reporting back the size we opened the window with).
    Ignore,
    /// A genuine size (user resize / WM resize): persist it as the next
    /// launch's size. Always the VISIBLE frame size, never the outer surface.
    Persist(Size<Pixels>),
    /// Linux X11 CSD only: ask the platform for this OUTER size so the
    /// VISIBLE frame ends up at the size we were asked to restore. Nothing to
    /// persist — this is still the launch echo.
    ResizeTo(Size<Pixels>),
}

/// The launch ↔ persist round trip for the main window's size
/// (EXP-210 persistence, EXP-269 CSD insets, EXP-276/EXP-278 the asymmetry).
///
/// # Why this is not just `bounds - padding`
///
/// `WindowOptions.window_bounds` is an OUTER size, and under Linux
/// client-side decorations the outer surface is the visible frame plus a
/// 12px shadow margin per side. The two Linux backends then disagree about
/// what the value we passed in ends up meaning, so the same subtraction
/// cannot be right on both:
///
/// - **Wayland CSD**: gpui shrinks the xdg window geometry by the inset and
///   the compositor echoes that geometry back in its configure;
///   `compute_outer_size` re-adds the inset, so `bounds` settles at
///   `requested + inset` and the VISIBLE frame is exactly `requested`.
///   Persisting `bounds` raw would grow the window +24px per launch — the
///   unbounded creep EXP-269 fixed by persisting the visible size.
/// - **X11 CSD**: `bounds` is the raw `ConfigureNotify` size and
///   `set_client_inset` only publishes `_GTK_FRAME_EXTENTS` (gpui's own
///   `inner_window_bounds` subtracts them from `bounds`, which is the proof
///   that `bounds` is the outer surface). Nothing re-adds the inset, so the
///   VISIBLE frame comes out `requested - 24` and persisting it shrank the
///   window 24px per launch until it hit [`MIN_SIZE`] (EXP-276/EXP-278).
/// - **Server decorations / macOS / Windows**: the padding is zero, so outer
///   and visible are the same number and neither drift exists.
///
/// # How this fixes it without picking a backend
///
/// Both drifts are launch echoes being mistaken for user resizes, so the
/// tracker never persists a frame that merely reports back the size we
/// opened with — under EITHER convention (`bounds == launched`, the X11
/// reading, or `bounds - padding == launched`, the Wayland one). That alone
/// removes the per-launch recurrence on every platform.
///
/// On top of that, an X11-shaped echo (the visible frame came out short
/// because the platform took our number as the outer surface) gets ONE
/// compensating resize to `launched + padding`, so the window actually opens
/// at the remembered size instead of 24px under it. It is a no-op wherever
/// the visible frame already matches. If the WM refuses it, the echo is still
/// suppressed, so the worst case is a one-time 24px discrepancy rather than a
/// drift. (Even if a Wayland compositor ever emitted an X11-shaped frame
/// before `compute_outer_size` kicked in, `launched + padding` is exactly the
/// outer size that backend settles at anyway.)
///
/// # Maximized and fullscreen frames (EXP-292)
///
/// No frame observed while maximized or fullscreen is acted on at all —
/// neither persisted nor fought (the compositor owns that size, same rule as
/// [`MinSizeClamp`]). It cannot be treated as a windowed size, because
/// `window_bounds()` reports something else entirely there, differently per
/// backend: X11 hands back `Maximized(state.bounds)`, the CURRENT maximized
/// surface, so quitting maximized replaced the remembered windowed size with
/// the screen size; Wayland hands back `Maximized(state.window_bounds)`, the
/// PRE-MAXIMIZE OUTER bounds, while `window_paddings` is zero because
/// maximizing tiles every edge — so `outer - pad` was the windowed size plus
/// the shadow, growing it 24px per maximize-and-quit cycle. What survives a
/// maximized session is the last WINDOWED frame, persisted before the
/// maximize.
///
/// Measured on X11 + CSD (Cinnamon/muffin, scale 2, EXP-276): a window
/// restored to a 1000×700 visible frame reports `bounds` 1024×724 with
/// `_GTK_FRAME_EXTENTS` 12 per side, i.e. X11 geometry INCLUDES the shadow —
/// so the persist side was right and the restore side was the broken half.
pub struct WindowSizeTracker {
    /// The size the window was opened at; `None` once a genuine frame has
    /// landed (or if the launch size was never recorded), after which every
    /// frame is a real resize.
    launched: Option<Size<Pixels>>,
    /// One compensating resize per window — a WM that refuses ours must not
    /// be asked again every frame.
    compensated: bool,
}

impl WindowSizeTracker {
    /// `launched` is [`launch_size`] — `None` degrades to "persist every
    /// frame", i.e. the pre-EXP-276 behavior.
    pub fn new(launched: Option<Size<Pixels>>) -> Self {
        Self {
            launched,
            compensated: false,
        }
    }

    /// Classify one observed frame. `outer` is
    /// `window.window_bounds().get_bounds().size`, `pad` the TOTAL client
    /// inset per axis (`window_paddings` left+right / top+bottom — zero
    /// under server decorations, and zero while maximized because every edge
    /// tiles), and `wm_sized` whether the frame is fullscreen or maximized —
    /// which is ignored outright (EXP-292).
    pub fn observe(
        &mut self,
        outer: Size<Pixels>,
        pad: Size<Pixels>,
        wm_sized: bool,
    ) -> SizeFrame {
        let visible = size(outer.width - pad.width, outer.height - pad.height);
        // Minimize/teardown can report a zero (or sub-pixel) frame.
        if visible.width < px(1.) || visible.height < px(1.) {
            return SizeFrame::Ignore;
        }

        // EXP-292: a fullscreen/maximized frame belongs to the compositor, not
        // to the user's windowed size — never persist it and never fight it
        // (same rule as [`MinSizeClamp`]). This has to sit above BOTH the
        // `launched` check and the echo branches: `window_bounds()` is not the
        // restore size on either Linux backend while maximized, so any frame
        // that did not happen to match an echo used to fall through and
        // persist. `launched` is deliberately left intact — un-maximizing
        // hands back the size we opened with, which is still an echo.
        if wm_sized {
            return SizeFrame::Ignore;
        }

        let Some(launched) = self.launched else {
            return SizeFrame::Persist(visible);
        };

        // Wayland CSD / server decorations / macOS / Windows: the window
        // opened at exactly the size we asked for. Nothing to persist, and
        // nothing to fix.
        if approx_eq(visible, launched) {
            return SizeFrame::Ignore;
        }

        // X11 CSD: our number became the OUTER surface, so the visible frame
        // is `pad` short of what we were asked to restore. Still an echo —
        // never persist it — but ask for the size that makes it right.
        if approx_eq(outer, launched) {
            if self.compensated {
                return SizeFrame::Ignore;
            }
            self.compensated = true;
            return SizeFrame::ResizeTo(size(
                launched.width + pad.width,
                launched.height + pad.height,
            ));
        }

        // Neither convention matches: a genuine resize. The launch is over.
        self.launched = None;
        SizeFrame::Persist(visible)
    }
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

    // ---- EXP-276/EXP-278: the launch ↔ persist round trip ------------------

    /// The 12px-per-side gpui-component shadow, as a per-axis total.
    const CSD_PAD: Size<Pixels> = Size {
        width: px(24.),
        height: px(24.),
    };
    const NO_PAD: Size<Pixels> = Size {
        width: px(0.),
        height: px(0.),
    };
    const LAUNCHED: Size<Pixels> = Size {
        width: px(1280.),
        height: px(820.),
    };
    /// A maximized window's surface — what X11 reports as `window_bounds()`
    /// while maximized (EXP-292).
    const MAXIMIZED_SURFACE: Size<Pixels> = Size {
        width: px(1920.),
        height: px(1080.),
    };

    #[test]
    fn wayland_csd_launch_echo_is_never_persisted() {
        // The compositor echoes the geometry we set and `compute_outer_size`
        // re-adds the inset, so `bounds` settles at launched + 24 and the
        // visible frame is exactly what we asked for. Persisting the raw
        // outer size here is the +24px-per-launch creep EXP-269 fixed.
        let mut tracker = WindowSizeTracker::new(Some(LAUNCHED));
        let outer = size(LAUNCHED.width + px(24.), LAUNCHED.height + px(24.));
        assert_eq!(tracker.observe(outer, CSD_PAD, false), SizeFrame::Ignore);
        // Repeat configures (a window move re-fires the observer) stay quiet.
        assert_eq!(tracker.observe(outer, CSD_PAD, false), SizeFrame::Ignore);
    }

    #[test]
    fn x11_csd_launch_echo_is_compensated_not_persisted() {
        // X11 takes our number as the outer surface and never re-adds the
        // inset, so the visible frame lands 24px short. Persisting that is
        // the -24px-per-launch shrink (EXP-276/EXP-278).
        let mut tracker = WindowSizeTracker::new(Some(LAUNCHED));
        assert_eq!(
            tracker.observe(LAUNCHED, CSD_PAD, false),
            SizeFrame::ResizeTo(size(px(1304.), px(844.)))
        );
        // The WM honors it: the visible frame is now the remembered size,
        // and there is still nothing to persist.
        let grown = size(px(1304.), px(844.));
        assert_eq!(tracker.observe(grown, CSD_PAD, false), SizeFrame::Ignore);
    }

    #[test]
    fn a_refused_compensation_is_asked_for_only_once_and_never_persisted() {
        // A WM that ignores the resize keeps re-reporting the short frame.
        // Suppressing the echo is what actually stops the drift — the
        // compensation is only the cosmetic half.
        let mut tracker = WindowSizeTracker::new(Some(LAUNCHED));
        assert!(matches!(
            tracker.observe(LAUNCHED, CSD_PAD, false),
            SizeFrame::ResizeTo(_)
        ));
        for _ in 0..5 {
            assert_eq!(tracker.observe(LAUNCHED, CSD_PAD, false), SizeFrame::Ignore);
        }
    }

    #[test]
    fn server_decorations_are_untouched() {
        // macOS/Windows/X11-without-a-compositor: padding is zero, outer ==
        // visible, and the launch echo is simply ignored.
        let mut tracker = WindowSizeTracker::new(Some(LAUNCHED));
        assert_eq!(tracker.observe(LAUNCHED, NO_PAD, false), SizeFrame::Ignore);
        assert_eq!(
            tracker.observe(size(px(1000.), px(700.)), NO_PAD, false),
            SizeFrame::Persist(size(px(1000.), px(700.)))
        );
    }

    #[test]
    fn a_maximized_launch_frame_is_never_fought() {
        // The compositor owns a maximized/fullscreen size — same rule as
        // `MinSizeClamp`. (Padding is zero there in practice; the guard is
        // what keeps us from requesting a resize if it is not.)
        let mut tracker = WindowSizeTracker::new(Some(LAUNCHED));
        assert_eq!(tracker.observe(LAUNCHED, CSD_PAD, true), SizeFrame::Ignore);
    }

    #[test]
    fn a_maximized_frame_is_never_persisted() {
        // EXP-292 (1): the guard used to sit only inside the X11 echo branch,
        // so a maximized frame at any OTHER size fell through to `Persist` —
        // and `window_bounds()` reports something unusable as a windowed size
        // on both Linux backends. `pad` is zero in both readings: maximizing
        // tiles every edge, which zeroes `window_paddings`.
        //
        // X11 reports `Maximized(state.bounds)`, the CURRENT maximized
        // surface, so the remembered windowed size became the screen size.
        let mut x11 = WindowSizeTracker::new(Some(LAUNCHED));
        assert_eq!(
            x11.observe(MAXIMIZED_SURFACE, NO_PAD, true),
            SizeFrame::Ignore
        );
        // Wayland reports `Maximized(state.window_bounds)`, the PRE-MAXIMIZE
        // OUTER bounds — the windowed size plus the shadow that is no longer
        // subtracted, i.e. +24px per maximize-and-quit cycle.
        let mut wayland = WindowSizeTracker::new(Some(LAUNCHED));
        let pre_maximize_outer = size(LAUNCHED.width + px(24.), LAUNCHED.height + px(24.));
        assert_eq!(
            wayland.observe(pre_maximize_outer, NO_PAD, true),
            SizeFrame::Ignore
        );
    }

    #[test]
    fn a_maximized_frame_is_ignored_after_the_launch_is_over() {
        // The guard has to sit ABOVE the `launched` check: once a genuine
        // resize has landed, `launched` is `None` and every later frame —
        // maximized ones included — would otherwise persist.
        let mut tracker = WindowSizeTracker::new(None);
        assert_eq!(
            tracker.observe(MAXIMIZED_SURFACE, NO_PAD, true),
            SizeFrame::Ignore
        );
    }

    #[test]
    fn unmaximizing_back_to_the_launch_size_is_still_an_echo() {
        // A maximized frame must not consume the launch: restoring the window
        // hands back the size we opened with, which is still an echo — and a
        // real drag after that still persists.
        let mut tracker = WindowSizeTracker::new(Some(LAUNCHED));
        assert_eq!(
            tracker.observe(MAXIMIZED_SURFACE, NO_PAD, true),
            SizeFrame::Ignore
        );
        let restored = size(LAUNCHED.width + px(24.), LAUNCHED.height + px(24.));
        assert_eq!(tracker.observe(restored, CSD_PAD, false), SizeFrame::Ignore);
        assert_eq!(
            tracker.observe(size(px(1024.), px(724.)), CSD_PAD, false),
            SizeFrame::Persist(size(px(1000.), px(700.)))
        );
    }

    #[test]
    fn a_genuine_resize_persists_the_visible_size() {
        let mut tracker = WindowSizeTracker::new(Some(LAUNCHED));
        let outer = size(px(1024.), px(724.));
        assert_eq!(
            tracker.observe(outer, CSD_PAD, false),
            SizeFrame::Persist(size(px(1000.), px(700.)))
        );
        // The launch is over: every later frame persists, including one that
        // happens to land back on the launch size.
        assert_eq!(
            tracker.observe(LAUNCHED, CSD_PAD, false),
            SizeFrame::Persist(size(px(1256.), px(796.)))
        );
    }

    #[test]
    fn an_unrecorded_launch_size_persists_every_frame() {
        // Degrades to the pre-EXP-276 behavior rather than going quiet.
        let mut tracker = WindowSizeTracker::new(None);
        assert_eq!(
            tracker.observe(LAUNCHED, CSD_PAD, false),
            SizeFrame::Persist(size(px(1256.), px(796.)))
        );
    }

    #[test]
    fn degenerate_frames_are_ignored() {
        let mut tracker = WindowSizeTracker::new(Some(LAUNCHED));
        assert_eq!(
            tracker.observe(size(px(0.), px(0.)), CSD_PAD, false),
            SizeFrame::Ignore
        );
        // A frame smaller than its own shadow (teardown) must not underflow
        // into a persisted negative size either.
        assert_eq!(
            tracker.observe(size(px(10.), px(10.)), CSD_PAD, false),
            SizeFrame::Ignore
        );
    }

    #[test]
    fn a_scale_factor_rounding_wobble_still_reads_as_an_echo() {
        let mut tracker = WindowSizeTracker::new(Some(LAUNCHED));
        let wobbly = size(LAUNCHED.width + px(24.3), LAUNCHED.height + px(23.8));
        assert_eq!(tracker.observe(wobbly, CSD_PAD, false), SizeFrame::Ignore);
    }

    // ---- Multi-launch round trip -------------------------------------------

    /// Which convention a backend applies to `WindowOptions.window_bounds`.
    #[derive(Clone, Copy, Debug)]
    enum Backend {
        /// `bounds` is the raw outer surface: visible = requested - inset.
        X11Csd,
        /// The compositor echoes the inset back: visible = requested.
        WaylandCsd,
        /// No client inset at all (macOS/Windows/server decorations).
        Server,
    }

    impl Backend {
        fn pad(self) -> Size<Pixels> {
            match self {
                Backend::X11Csd | Backend::WaylandCsd => CSD_PAD,
                Backend::Server => NO_PAD,
            }
        }

        /// The outer bounds the backend reports for a window opened at
        /// `requested`.
        fn outer_for(self, requested: Size<Pixels>) -> Size<Pixels> {
            match self {
                Backend::X11Csd | Backend::Server => requested,
                Backend::WaylandCsd => {
                    size(requested.width + px(24.), requested.height + px(24.))
                }
            }
        }

        /// What `window_bounds()` reports while the window is MAXIMIZED,
        /// given the outer bounds it had windowed. Neither Linux backend
        /// answers with the windowed size (EXP-292): X11 hands back
        /// `Maximized(state.bounds)`, the current maximized surface, and
        /// Wayland `Maximized(state.window_bounds)`, the pre-maximize OUTER
        /// bounds. `Server` follows the X11 shape here — the guard ignores
        /// the frame either way.
        fn maximized_outer(self, windowed_outer: Size<Pixels>) -> Size<Pixels> {
            match self {
                Backend::X11Csd | Backend::Server => MAXIMIZED_SURFACE,
                Backend::WaylandCsd => windowed_outer,
            }
        }
    }

    /// One launch: open at `requested`, feed the backend's frames through the
    /// tracker (honoring a compensating resize like an ordinary WM would),
    /// and report the visible size the user ends up looking at plus whatever
    /// got persisted for the next launch.
    fn simulate_launch(
        backend: Backend,
        requested: Size<Pixels>,
    ) -> (Size<Pixels>, Option<Size<Pixels>>) {
        let mut tracker = WindowSizeTracker::new(Some(requested));
        let pad = backend.pad();
        let mut outer = backend.outer_for(requested);
        let mut persisted = None;
        // A handful of frames: the launch echo, the compensation echo, and
        // some idle re-configures (X11 re-fires the observer on moves).
        for _ in 0..4 {
            match tracker.observe(outer, pad, false) {
                SizeFrame::Ignore => {}
                SizeFrame::Persist(last) => persisted = Some(last),
                SizeFrame::ResizeTo(target) => outer = backend.outer_for(target),
            }
        }
        let visible = size(outer.width - pad.width, outer.height - pad.height);
        (visible, persisted)
    }

    /// One session the user maximizes and then quits from: the windowed
    /// launch frames, then the maximized ones. Returns whatever the tracker
    /// persisted across it — which must be nothing, since the windowed size
    /// is already on disk from before the maximize.
    fn simulate_maximized_session(
        backend: Backend,
        stored: Size<Pixels>,
    ) -> Option<Size<Pixels>> {
        let mut tracker = WindowSizeTracker::new(Some(stored));
        let pad = backend.pad();
        let mut outer = backend.outer_for(stored);
        let mut persisted = None;
        for _ in 0..4 {
            match tracker.observe(outer, pad, false) {
                SizeFrame::Ignore => {}
                SizeFrame::Persist(last) => persisted = Some(last),
                SizeFrame::ResizeTo(target) => outer = backend.outer_for(target),
            }
        }
        // Maximize: every edge tiles, so `window_paddings` drops to zero.
        let maximized = backend.maximized_outer(outer);
        for _ in 0..3 {
            if let SizeFrame::Persist(last) = tracker.observe(maximized, NO_PAD, true) {
                persisted = Some(last);
            }
        }
        persisted
    }

    #[test]
    fn maximizing_and_quitting_never_touches_the_remembered_size() {
        // EXP-292 (1): a user who maximizes and quits maximized every session
        // grew the remembered size by the inset per cycle on Wayland, and lost
        // it to the screen size outright on X11.
        for backend in [Backend::X11Csd, Backend::WaylandCsd, Backend::Server] {
            let mut stored = LAUNCHED;
            for session in 0..5 {
                let persisted = simulate_maximized_session(backend, stored);
                assert_eq!(
                    persisted, None,
                    "{backend:?} session {session}: a maximized frame was persisted"
                );
                stored = persisted.unwrap_or(stored);
                assert_eq!(stored, LAUNCHED, "{backend:?} session {session}");
            }
        }
    }

    #[test]
    fn the_window_size_survives_repeated_launches_on_every_backend() {
        // The regression this pins: EXP-269 fixed the Wayland +24px creep by
        // subtracting the inset on the persist side, which turned into a
        // -24px-per-launch shrink on X11 (EXP-276/EXP-278) because only
        // Wayland re-adds it. Neither backend may drift.
        for backend in [Backend::X11Csd, Backend::WaylandCsd, Backend::Server] {
            let mut stored = LAUNCHED;
            for launch in 0..5 {
                let (visible, persisted) = simulate_launch(backend, stored);
                assert_eq!(
                    visible, LAUNCHED,
                    "{backend:?} launch {launch}: window opened at {visible:?}, wanted {LAUNCHED:?}"
                );
                assert_eq!(
                    persisted, None,
                    "{backend:?} launch {launch}: a launch echo was persisted"
                );
                stored = persisted.unwrap_or(stored);
            }
        }
    }

    #[test]
    fn a_user_resize_round_trips_on_every_backend() {
        // Resize to a 1000x700 VISIBLE frame, quit, relaunch: the window must
        // come back at 1000x700 visible, not 24px under it.
        for backend in [Backend::X11Csd, Backend::WaylandCsd, Backend::Server] {
            let pad = backend.pad();
            let mut tracker = WindowSizeTracker::new(Some(LAUNCHED));
            let dragged = size(px(1000.) + pad.width, px(700.) + pad.height);
            let stored = match tracker.observe(dragged, pad, false) {
                SizeFrame::Persist(last) => last,
                other => panic!("{backend:?}: a user resize was not persisted ({other:?})"),
            };
            assert_eq!(stored, size(px(1000.), px(700.)), "{backend:?}");

            let (visible, _) = simulate_launch(backend, stored);
            assert_eq!(visible, size(px(1000.), px(700.)), "{backend:?}");
        }
    }
}
