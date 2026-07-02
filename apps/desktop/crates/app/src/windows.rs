//! Window management (masterplan-v3 §3.6 "Multi-window").
//!
//! `open_workspace_window` is callable N times; every window gets its own
//! `Root → Workspace → DockArea` but they all read the same global `Store`
//! and `Theme`. Window close drops that window's entities (the Workspace
//! decrements the shared window counter on release).

use std::sync::atomic::{AtomicUsize, Ordering};

use gpui::{px, size, App, AppContext as _, Bounds, WindowBounds, WindowKind, WindowOptions};
use gpui_component::Root;

/// Per-window layout slot (window 0 = main). Monotonic within a run; each
/// slot persists its own `DockAreaState` file (§3.3).
static WINDOW_ORDINAL: AtomicUsize = AtomicUsize::new(0);

/// Cascade offset so a second window doesn't open exactly over the first.
const CASCADE_STEP: f32 = 24.;

pub fn open_workspace_window(cx: &mut App) {
    let ordinal = WINDOW_ORDINAL.fetch_add(1, Ordering::SeqCst);

    // §3.6 default bounds; min size floors the §3.9 zero-size guard.
    let default_size = size(px(1280.), px(820.));
    let min_size = size(px(760.), px(480.));

    let mut bounds = Bounds::centered(None, default_size, cx);
    let cascade = px(CASCADE_STEP * (ordinal.min(8)) as f32);
    bounds.origin.x += cascade;
    bounds.origin.y += cascade;

    // The gpui-component-sanctioned pattern opens windows inside a foreground
    // spawn (§3.6; hello_world main.rs, story lib.rs).
    cx.spawn(async move |cx| {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(min_size),
            kind: WindowKind::Normal,
            // Linux CSD: Root draws the client-side window border/shadow.
            #[cfg(target_os = "linux")]
            window_background: gpui::WindowBackgroundAppearance::Transparent,
            #[cfg(target_os = "linux")]
            window_decorations: Some(gpui::WindowDecorations::Client),
            ..Default::default()
        };

        let window = cx.open_window(options, move |window, cx| {
            let workspace = cx.new(|cx| ui::Workspace::new(ordinal, window, cx));
            // Root MUST be the first view of every window (§3.3) — it hosts
            // the Dialog/Sheet/Popover/Notification overlay layers.
            cx.new(|cx| Root::new(workspace, window, cx))
        })?;

        window.update(cx, |_, window, cx| {
            window.set_window_title("Exponential");
            window.activate_window();
            let _ = cx;
        })?;

        anyhow::Ok(())
    })
    .detach();
}
