//! Window management (masterplan-v3 §3.6 "Multi-window").
//!
//! `open_shell_window` is callable N times; every window gets its own
//! `Root → Shell → DockArea` but they all read the same global `Store`
//! and `Theme`. Window close drops that window's entities (the Shell
//! decrements the shared window counter on release).

use std::sync::atomic::{AtomicUsize, Ordering};

use gpui::{px, App, AppContext as _, Bounds, WindowBounds, WindowKind, WindowOptions};
use gpui_component::Root;

/// Per-window layout slot (window 0 = main). Monotonic within a run; each
/// slot persists its own `DockAreaState` file (§3.3).
static WINDOW_ORDINAL: AtomicUsize = AtomicUsize::new(0);

/// Cascade offset so a second window doesn't open exactly over the first.
const CASCADE_STEP: f32 = 24.;

pub fn open_shell_window(cx: &mut App) {
    let ordinal = WINDOW_ORDINAL.fetch_add(1, Ordering::SeqCst);

    // X11 taskbar-icon fallback (EXP-79): stamp _NET_WM_ICON on the window
    // once it maps — the `.desktop` Icon= association (set up below via
    // app_id) only renders after the shell re-indexes its desktop files,
    // which a first launch always loses the race against.
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    crate::x11_window_icon::install();

    // EXP-210: open at the last used size (main-window resizes persist it —
    // `ui::window_size`, local file only), falling back to the §3.6 default;
    // the 800×600 min size is the floor below which page layouts break (and
    // still floors the §3.9 zero-size guard).
    let default_size =
        ui::window_size::load_last_size().unwrap_or(ui::window_size::DEFAULT_SIZE);
    let min_size = ui::window_size::MIN_SIZE;
    // EXP-276/EXP-278: the size observer needs the number we asked for so it
    // can tell a launch echo (which Linux CSD reports back off by the 24px
    // shadow, in opposite directions on X11 and Wayland) from a real resize.
    ui::window_size::note_launch_size(default_size);

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
            // Associate the window with our `.desktop` entry so Linux
            // taskbars/docks show the app icon (EXP-68). gpui maps this to
            // the Wayland toplevel `app_id` and the X11 `WM_CLASS`; without
            // it the X11 window has NO WM_CLASS at all and neither protocol
            // can match the window to `<APP_ID>.desktop` (the file
            // `desktop_integration` installs, whose `Icon=` carries the
            // taskbar icon). No-op on macOS/Windows (gpui ignores it there).
            app_id: Some(crate::channel::APP_ID.to_string()),
            // EXP-269: the app draws its own chrome on every platform — the
            // in-app titlebar (`ui`'s AppTitleBar). macOS: transparent native
            // titlebar with the traffic lights floating over the 34px bar at
            // (9,9); Windows: `appears_transparent` suppresses the native
            // caption and the bar's WindowControlArea hitboxes drive
            // min/max/close + Snap Layouts.
            titlebar: Some(gpui_component::TitleBar::title_bar_options()),
            // EXP-290 glass: a non-opaque window with behind-window blur, so
            // the translucent page gradient (`theme::glass_sidebar_alpha` /
            // `glass_content_alpha`) lets the desktop show through. macOS gets a
            // real `NSVisualEffectView` backdrop; Wayland asks the compositor's
            // blur manager (KDE protocol) and degrades to plain transparency
            // where it is absent, which is also what X11 does. That degrade is
            // still worth asking for here — it is harmless (Linux CSD already
            // REQUIRES a non-opaque window: the shadow margins and rounded
            // corners of `ui::window_frame` can only composite against
            // transparency; X11 without a compositor falls back to Server
            // decorations, where the in-app bar hides itself —
            // `app_title_bar::client_chrome`) — but EXP-293 stopped the PAGE
            // from being translucent when no blur backdrop actually exists:
            // unsmeared transparency was a legible ghost of the desktop, so
            // `theme::blur_backdrop_available()` pins those platforms opaque.
            // Windows is left at the default Opaque on purpose: its Blurred
            // path is the legacy undocumented `ACCENT_ENABLE_BLURBEHIND`, and
            // any non-opaque window there loses ClearType subpixel AA
            // (`Window::should_use_subpixel_rendering`).
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            window_background: gpui::WindowBackgroundAppearance::Blurred,
            #[cfg(target_os = "linux")]
            window_decorations: Some(gpui::WindowDecorations::Client),
            ..Default::default()
        };

        let window = cx.open_window(options, move |window, cx| {
            let shell = cx.new(|cx| ui::Shell::new(ordinal, window, cx));
            // Root MUST be the first view of every window (§3.3) — it hosts
            // the Dialog/Sheet/Popover/Notification overlay layers.
            cx.new(|cx| {
                let root = Root::new(shell, window, cx);
                // EXP-269 Linux CSD: the stock Root frame is square-cornered —
                // disable it (ui's rounded window_frame renders inside the
                // Shell instead) and clear Root's opaque background so the
                // shadow margins around the frame stay transparent.
                #[cfg(target_os = "linux")]
                let root = {
                    use gpui::Styled as _;
                    root.bordered(false).bg(gpui::transparent_black())
                };
                // EXP-290 macOS glass: stock `Root` paints an opaque
                // full-window `tokens.background` fill UNDER everything, which
                // would sit between the blurred backdrop and the translucent
                // page gradient and hide the effect entirely. Clear it (the
                // stock frame stays — off Linux CSD it is a pass-through).
                #[cfg(target_os = "macos")]
                let root = {
                    use gpui::Styled as _;
                    root.bg(gpui::transparent_black())
                };
                root
            })
        })?;

        // EXP-303: give the Blurred shell window a working frosted backdrop —
        // gpui's own BlurredView renders no backdrop on current macOS
        // (`ui::macos_blur` inserts a stock NSGlassEffectView /
        // NSVisualEffectView above it).
        #[cfg(target_os = "macos")]
        ui::macos_blur::ensure_window_blur();

        window.update(cx, |_, window, cx| {
            window.set_window_title(crate::channel::APP_NAME);
            window.activate_window();
            let _ = cx;
        })?;

        anyhow::Ok(())
    })
    .detach();
}
