//! #NEW-54: the push-to-talk overlay window: THE capsule
//! (`voice_overlay::capsule`) in a transparent, always-on-top window that
//! never takes focus (the `computer_indicator` banner's pattern), at the
//! bottom centre of the display under the pointer, [`EDGE`] px above its
//! edge. Wayland places it itself. The window is the capsule's measured
//! size ([`capsule_size`], at most the display less [`EDGE`] each side): it
//! opens again, wider, when a longer line no longer fits.

use exponential_ui::voice_wave::Activity;
use gpui::{
    div, point, px, size, App, AppContext as _, Bounds, Context, IntoElement, ParentElement as _,
    Pixels, Point, Render, SharedString, Size, StatefulInteractiveElement as _, Styled as _, Window, WindowBounds, WindowHandle, WindowKind, WindowOptions,
};
use gpui_component::ActiveTheme as _;

use domain::voice::PttOverlay;

use crate::voice_overlay::{capsule, capsule_size, Capsule};

/// The air between the capsule and the display's edge.
const EDGE: f32 = 24.;
/// The window's air around the capsule (its shadow).
const AIR: f32 = 8.;

/// The window, open while the capsule shows.
#[derive(Default)]
pub(super) struct Overlay {
    window: Option<WindowHandle<OverlayView>>,
    /// The open window's width: a line that needs more reopens it.
    width: f32,
}

impl Overlay {
    /// Show `view` (None = close the window).
    pub(super) fn show(&mut self, view: Option<PttOverlay>, cx: &mut App) {
        let Some(view) = view else {
            if let Some(window) = self.window.take() {
                let _ = window.update(cx, |_, window, _| window.remove_window());
            }
            return;
        };
        let (display, area) = placement(cx);
        let window_size = window_size(&specimen(&view), area.map(|area| f32::from(area.size.width)), cx);
        if let Some(window) = self.window.take() {
            // The window only grows while it shows: a shorter line sits
            // centred in it, a longer one opens it again.
            if f32::from(window_size.width) <= self.width
                && window
                    .update(cx, |this, _, cx| {
                        this.view = view.clone();
                        cx.notify();
                    })
                    .is_ok()
            {
                self.window = Some(window);
                return;
            }
            let _ = window.update(cx, |_, window, _| window.remove_window());
        }
        let origin = area.map(|area| bottom_centre(area, window_size)).unwrap_or_else(|| point(px(0.), px(0.)));
        match cx.open_window(options(display, origin, window_size), |_, cx| cx.new(|_| OverlayView { view })) {
            Ok(window) => {
                self.window = Some(window);
                self.width = f32::from(window_size.width);
            }
            Err(err) => log::warn!("[voice] the push-to-talk overlay did not open: {err}"),
        }
    }

    /// A new level frame: redraw the wave.
    pub(super) fn refresh(&self, cx: &mut App) {
        if let Some(window) = self.window.as_ref() {
            let _ = window.update(cx, |_, _, cx| cx.notify());
        }
    }
}

pub(super) struct OverlayView {
    view: PttOverlay,
}

/// The fixture's activity key as the wave's.
pub(crate) fn activity(key: &str) -> Activity {
    match key {
        "listening" => Activity::Listening,
        "thinking" => Activity::Thinking,
        "speaking" => Activity::Speaking,
        "muted" => Activity::Muted,
        _ => Activity::Idle,
    }
}

/// The capsule for `view`, without the live level (the words size it).
fn specimen(view: &PttOverlay) -> Capsule {
    Capsule {
        activity: activity(view.activity),
        level: 0.,
        bands: Vec::new(),
        state: SharedString::from(view.state.clone()),
        line: SharedString::from(view.line.clone()),
        pill: SharedString::from(view.pill.clone()),
        primary: view.primary,
    }
}

impl Render for OverlayView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // No `Root` here: the rem and the font are set like the banner's.
        window.set_rem_size(cx.theme().font_size);
        let (level, bands) = if self.view.activity == "listening" { super::level(cx) } else { (0., Vec::new()) };
        let c = Capsule { level, bands, ..specimen(&self.view) };
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .font_family(cx.theme().font_family.clone())
            // The window never takes focus; a click still lands: the pill's
            // action (Stop, Unmute).
            .child(capsule("push-to-talk-overlay", &c, cx).on_click(|_, _, cx| super::on_pill(cx)))
    }
}

/// The window around the capsule `c`: its measured size plus the air,
/// never wider than `room` (the display's width) less [`EDGE`] each side.
fn window_size(c: &Capsule, room: Option<f32>, cx: &App) -> Size<Pixels> {
    let max_width = room.map_or(f32::MAX, |room| room - 2. * EDGE);
    let capsule = capsule_size(c, max_width - 2. * AIR, cx);
    size(capsule.width + px(2. * AIR), capsule.height + px(2. * AIR))
}

fn options(display: Option<gpui::DisplayId>, origin: Point<Pixels>, window: Size<Pixels>) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds { origin, size: window })),
        titlebar: None,
        focus: false,
        show: true,
        // Above every window, never activated (X11 override-redirect, a
        // floating panel on macOS).
        kind: WindowKind::PopUp,
        is_movable: false,
        is_resizable: false,
        is_minimizable: false,
        display_id: display,
        window_background: gpui::WindowBackgroundAppearance::Transparent,
        app_id: Some(crate::undock::CHANNEL_APP_ID.to_string()),
        #[cfg(target_os = "linux")]
        window_decorations: Some(gpui::WindowDecorations::Client),
        ..Default::default()
    }
}

/// The bottom centre of `area`, [`EDGE`] above its bottom (less the air).
pub(crate) fn bottom_centre(area: Bounds<Pixels>, window: Size<Pixels>) -> Point<Pixels> {
    point(
        area.origin.x + (area.size.width - window.width) / 2.,
        area.origin.y + area.size.height - window.height - px(EDGE - AIR),
    )
}

/// The display under the pointer and the area the window centres in.
fn placement(cx: &App) -> (Option<gpui::DisplayId>, Option<Bounds<Pixels>>) {
    #[cfg(target_os = "linux")]
    if super::hotkey::session() == Some(domain::voice::VoiceSession::X11) {
        if let Some(area) = x11_area(cx) {
            return (cx.primary_display().map(|display| display.id()), Some(area));
        }
    }
    let displays = cx.displays();
    let under = pointer(cx).and_then(|pointer| displays.iter().find(|display| display.bounds().contains(&pointer)).cloned());
    let display = under.or_else(|| cx.primary_display());
    (display.as_ref().map(|display| display.id()), display.map(|display| display.visible_bounds()))
}

/// X11: the toolkit's display is the whole X screen, so the monitor under
/// the pointer comes from RandR, in physical pixels, scaled by the root's
/// logical width (the banner's EXP-1273 rule).
#[cfg(target_os = "linux")]
fn x11_area(cx: &App) -> Option<Bounds<Pixels>> {
    use x11rb::connection::Connection as _;
    use x11rb::protocol::randr::ConnectionExt as _;
    use x11rb::protocol::xproto::ConnectionExt as _;

    let logical_root_width = f32::from(cx.primary_display()?.bounds().size.width);
    let (conn, screen) = x11rb::connect(None).ok()?;
    let root = conn.setup().roots.get(screen)?;
    let pointer = conn.query_pointer(root.root).ok()?.reply().ok()?;
    let monitors = conn.randr_get_monitors(root.root, true).ok()?.reply().ok()?.monitors;
    let (px_x, px_y) = (i32::from(pointer.root_x), i32::from(pointer.root_y));
    let monitor = monitors
        .iter()
        .find(|m| {
            px_x >= i32::from(m.x) && px_x < i32::from(m.x) + i32::from(m.width) && px_y >= i32::from(m.y) && px_y < i32::from(m.y) + i32::from(m.height)
        })
        .or_else(|| monitors.iter().find(|m| m.primary))?;
    let scale = f32::from(root.width_in_pixels) / logical_root_width;
    if scale <= 0. {
        return None;
    }
    Some(Bounds {
        origin: point(px(f32::from(monitor.x) / scale), px(f32::from(monitor.y) / scale)),
        size: size(px(f32::from(monitor.width) / scale), px(f32::from(monitor.height) / scale)),
    })
}

/// The pointer in the toolkit's display coordinates.
#[cfg(target_os = "macos")]
fn pointer(cx: &App) -> Option<Point<Pixels>> {
    use cocoa::foundation::NSPoint;
    use objc::{class, msg_send, sel, sel_impl};
    // SAFETY: a class method returning a plain struct.
    let location: NSPoint = unsafe { msg_send![class!(NSEvent), mouseLocation] };
    // Cocoa counts from the primary screen's bottom; gpui from its top.
    let height = f32::from(cx.primary_display()?.bounds().size.height);
    Some(point(px(location.x as f32), px(height - location.y as f32)))
}

#[cfg(target_os = "windows")]
fn pointer(_cx: &App) -> Option<Point<Pixels>> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let mut at = POINT { x: 0, y: 0 };
    // SAFETY: a plain out-parameter.
    (unsafe { GetCursorPos(&mut at) } != 0).then(|| point(px(at.x as f32), px(at.y as f32)))
}

/// Wayland places the window itself (and X11 answered above).
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn pointer(_cx: &App) -> Option<Point<Pixels>> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_capsule_sits_24_px_above_the_bottom_centre() {
        let area = Bounds { origin: point(px(1920.), px(0.)), size: size(px(1920.), px(1080.)) };
        let window = size(px(436.), px(74. + 2. * AIR));
        let origin = bottom_centre(area, window);
        // The capsule's own bottom (the window less its air) is 24 above.
        assert_eq!(f32::from(origin.y) + f32::from(window.height) - AIR, 1080. - EDGE);
        assert_eq!(f32::from(origin.x) + f32::from(window.width) / 2., 1920. + 960.);
    }

    #[test]
    fn every_fixture_activity_maps() {
        assert_eq!(activity("listening"), Activity::Listening);
        assert_eq!(activity("thinking"), Activity::Thinking);
        assert_eq!(activity("speaking"), Activity::Speaking);
        assert_eq!(activity("muted"), Activity::Muted);
        assert_eq!(activity("idle"), Activity::Idle);
    }
}
