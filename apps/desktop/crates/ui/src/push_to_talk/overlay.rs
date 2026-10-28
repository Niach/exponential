//! #NEW-54: the push-to-talk overlay window: THE capsule
//! (`voice_overlay::capsule`) in a transparent, always-on-top window that
//! never takes focus (the `computer_indicator` banner's pattern), at the
//! bottom centre of the display under the pointer, [`EDGE`] px above its
//! edge. Wayland places it itself. The window is the capsule's measured
//! size ([`capsule_size`], at most the display less [`EDGE`] each side): it
//! opens again, wider, when a longer line no longer fits. NEW-55: a call
//! (`voice_call`) opens its own window of the same kind with the call's
//! face ([`Face::Call`]).
//!
//! NEW-59 (`fixtures/work-screen.json` OVERLAY): never on a display with a
//! fullscreen app. When the window opens, the computer crate's screen state
//! is read ONCE and `computer::work_screen::overlay_display` picks the
//! display: the pointer's without one, else the work screen, else the first
//! free one; none = the window stays hidden (the turn still runs, the reply
//! is still spoken).

use exponential_ui::voice_wave::Activity;
use gpui::{
    div, point, px, size, App, AppContext as _, Bounds, Context, IntoElement, ParentElement as _,
    Pixels, Point, Render, SharedString, Size, StatefulInteractiveElement as _, Styled as _, Window, WindowBounds, WindowHandle, WindowKind, WindowOptions,
};
use gpui_component::ActiveTheme as _;

use domain::voice::PttOverlay;
use domain::voice_call::{CallView, VOICE_CALL};

use crate::voice_overlay::{call_capsule, capsule, capsule_size, Capsule};

/// The air between the capsule and the display's edge.
const EDGE: f32 = 24.;
/// The window's air around the capsule (its shadow).
const AIR: f32 = 8.;

/// What the window shows.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Face {
    Ptt(PttOverlay),
    /// NEW-55: a call: the lead pill (Mute / Unmute) and End call.
    Call(CallView),
}

/// The window, open while the capsule shows.
#[derive(Default)]
pub(crate) struct Overlay {
    window: Option<WindowHandle<OverlayView>>,
    /// The open window's width: a line that needs more reopens it.
    width: f32,
    /// NEW-59: where this showing went, read once when it opened and kept
    /// until the face closes (a wider reopen stays on the same display).
    place: Option<Place>,
}

/// NEW-59: where the overlay shows.
#[derive(Clone, Debug, PartialEq)]
enum Place {
    /// Every display holds a fullscreen app: no window.
    Hidden,
    At(Option<gpui::DisplayId>, Option<Bounds<Pixels>>),
}

impl Overlay {
    /// Show `view` (None = close the window).
    pub(crate) fn show(&mut self, view: Option<Face>, cx: &mut App) {
        let Some(view) = view else {
            self.place = None;
            if let Some(window) = self.window.take() {
                let _ = window.update(cx, |_, window, _| window.remove_window());
            }
            return;
        };
        let place = match self.place.clone() {
            Some(place) => place,
            None => {
                let place = placement(cx);
                self.place = Some(place.clone());
                place
            }
        };
        let Place::At(display, area) = place else {
            // NEW-59: no display without a fullscreen app.
            return;
        };
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

    /// Whether `id` is the overlay's own window (it opens again, wider, for
    /// a longer line mid-hold: never the IDE window the key is held in).
    pub(crate) fn owns(&self, id: gpui::WindowId) -> bool {
        self.window.as_ref().is_some_and(|window| window.window_id() == id)
    }

    /// A new level frame: redraw the wave.
    pub(crate) fn refresh(&self, cx: &mut App) {
        if let Some(window) = self.window.as_ref() {
            let _ = window.update(cx, |_, _, cx| cx.notify());
        }
    }
}

pub(crate) struct OverlayView {
    view: Face,
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
fn specimen(view: &Face) -> Capsule {
    match view {
        Face::Ptt(view) => Capsule {
            activity: activity(view.activity),
            level: 0.,
            bands: Vec::new(),
            state: SharedString::from(view.state.clone()),
            line: SharedString::from(view.line.clone()),
            pill: SharedString::from(view.pill.clone()),
            primary: view.primary,
            lead: None,
        },
        Face::Call(view) => Capsule {
            activity: activity(view.activity),
            level: 0.,
            bands: Vec::new(),
            state: SharedString::from(view.state.clone()),
            line: SharedString::from(view.line.clone()),
            pill: SharedString::from(VOICE_CALL.copy.end_call.clone()),
            primary: true,
            lead: Some(SharedString::from(view.mute.clone())),
        },
    }
}

impl Render for OverlayView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // No `Root` here: the rem and the font are set like the banner's.
        window.set_rem_size(cx.theme().font_size);
        let c = specimen(&self.view);
        let (level, bands) = match &self.view {
            Face::Ptt(view) if view.activity == "listening" => super::level(cx),
            Face::Call(view) if view.activity == "listening" => crate::voice_call::level(cx),
            _ => (0., Vec::new()),
        };
        let c = Capsule { level, bands, ..c };
        let face = match &self.view {
            Face::Ptt(_) => capsule("push-to-talk-overlay", &c, cx).on_click(|_, _, cx| super::on_pill(cx)),
            Face::Call(_) => call_capsule(
                "voice-call-overlay",
                &c,
                std::rc::Rc::new(crate::voice_call::toggle_mute),
                std::rc::Rc::new(crate::voice_call::end),
                cx,
            ),
        };
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .font_family(cx.theme().font_family.clone())
            // The window never takes focus; a click still lands: the pill's
            // action (Stop, Unmute).
            .child(face)
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

/// The display under the pointer and the area the window centres in; NEW-59
/// moved off (or hidden from) a display with a fullscreen app.
fn placement(cx: &App) -> Place {
    #[cfg(target_os = "linux")]
    if super::hotkey::session() == Some(domain::voice::VoiceSession::X11) {
        if let Some(place) = x11_place(cx) {
            return place;
        }
    }
    let displays = cx.displays();
    let primary = cx.primary_display().and_then(|primary| displays.iter().position(|display| display.id() == primary.id()));
    let under = pointer(cx).and_then(|pointer| displays.iter().position(|display| display.bounds().contains(&pointer)));
    let at = |display: &std::rc::Rc<dyn gpui::PlatformDisplay>| Place::At(Some(display.id()), Some(display.visible_bounds()));
    if let (Some(pointer_ix), Some(state)) = (under.or(primary), coding::computer::screen_state()) {
        let map = display_map(&displays, &state.displays);
        if let Some(Some(pointer)) = map.get(pointer_ix).copied() {
            let target = coding::computer::work_screen::overlay_display(state.displays.len(), pointer, &state.fullscreen, &state.work);
            match target {
                None => return Place::Hidden,
                Some(target) => {
                    if let Some(display) = map.iter().position(|mapped| *mapped == Some(target)).and_then(|ix| displays.get(ix)) {
                        return at(display);
                    }
                }
            }
        }
    }
    match under.or(primary).and_then(|ix| displays.get(ix)) {
        Some(display) => at(display),
        None => Place::At(None, None),
    }
}

/// A display rectangle: the toolkit's (logical) or the computer crate's (the
/// OS's screen units).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Rect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl From<&coding::computer::Display> for Rect {
    fn from(display: &coding::computer::Display) -> Self {
        Rect { x: display.x as f32, y: display.y as f32, width: display.width as f32, height: display.height as f32 }
    }
}

/// NEW-59: `displays` (the toolkit's) mapped to `screens` (the computer
/// crate's), index by index ([`map_displays`]). The banner places by it too.
pub(crate) fn display_map(
    displays: &[std::rc::Rc<dyn gpui::PlatformDisplay>],
    screens: &[coding::computer::Display],
) -> Vec<Option<usize>> {
    let rects: Vec<Rect> = displays
        .iter()
        .map(|display| {
            let bounds = display.bounds();
            Rect {
                x: f32::from(bounds.origin.x),
                y: f32::from(bounds.origin.y),
                width: f32::from(bounds.size.width),
                height: f32::from(bounds.size.height),
            }
        })
        .collect();
    let screens: Vec<Rect> = screens.iter().map(Rect::from).collect();
    map_displays(&rects, &screens)
}

/// NEW-59: the toolkit's displays mapped to the computer crate's (index by
/// index; `None` = no counterpart). macOS counts both in points, so the
/// origins agree; where the units differ (Windows scaling) and the counts
/// agree, the displays pair up by their order on the desktop (left to right,
/// then top to bottom), which a uniform scale keeps.
fn map_displays(toolkit: &[Rect], screens: &[Rect]) -> Vec<Option<usize>> {
    let near = |a: &Rect, b: &Rect| (a.x - b.x).abs() < 2. && (a.y - b.y).abs() < 2.;
    let exact: Vec<Option<usize>> =
        toolkit.iter().map(|rect| screens.iter().position(|screen| near(rect, screen))).collect();
    if exact.iter().all(Option::is_some) || toolkit.len() != screens.len() {
        return exact;
    }
    let order = |rects: &[Rect]| {
        let mut ix: Vec<usize> = (0..rects.len()).collect();
        ix.sort_by(|&a, &b| {
            (rects[a].x, rects[a].y).partial_cmp(&(rects[b].x, rects[b].y)).unwrap_or(std::cmp::Ordering::Equal)
        });
        ix
    };
    let mut out = vec![None; toolkit.len()];
    for (t, s) in order(toolkit).into_iter().zip(order(screens)) {
        out[t] = Some(s);
    }
    out
}

/// X11 (NEW-59): RandR's monitors and the computer crate's displays are
/// both in physical root pixels, so the pointer's monitor maps directly;
/// the chosen display is scaled into the toolkit's logical pixels.
#[cfg(target_os = "linux")]
fn x11_place(cx: &App) -> Option<Place> {
    let primary = cx.primary_display().map(|display| display.id());
    if let Some(state) = coding::computer::screen_state() {
        if let Some((pointer, scale)) = x11_pointer(cx) {
            let contains = |display: &coding::computer::Display| {
                pointer.0 >= display.x
                    && pointer.0 < display.x + display.width as i32
                    && pointer.1 >= display.y
                    && pointer.1 < display.y + display.height as i32
            };
            let under = state
                .displays
                .iter()
                .position(contains)
                .or_else(|| state.displays.iter().position(|display| display.primary));
            if let Some(under) = under {
                return Some(
                    match coding::computer::work_screen::overlay_display(state.displays.len(), under, &state.fullscreen, &state.work) {
                        None => Place::Hidden,
                        Some(target) => {
                            let display = &state.displays[target];
                            Place::At(
                                primary,
                                Some(Bounds {
                                    origin: point(px(display.x as f32 / scale), px(display.y as f32 / scale)),
                                    size: size(px(display.width as f32 / scale), px(display.height as f32 / scale)),
                                }),
                            )
                        }
                    },
                );
            }
        }
    }
    x11_area(cx).map(|area| Place::At(primary, Some(area)))
}

/// X11: the pointer in physical root pixels and the root's physical over
/// logical scale.
#[cfg(target_os = "linux")]
fn x11_pointer(cx: &App) -> Option<((i32, i32), f32)> {
    use x11rb::connection::Connection as _;
    use x11rb::protocol::xproto::ConnectionExt as _;

    let logical_root_width = f32::from(cx.primary_display()?.bounds().size.width);
    let (conn, screen) = x11rb::connect(None).ok()?;
    let root = conn.setup().roots.get(screen)?;
    let pointer = conn.query_pointer(root.root).ok()?.reply().ok()?;
    let scale = f32::from(root.width_in_pixels) / logical_root_width;
    (scale > 0.).then_some(((i32::from(pointer.root_x), i32::from(pointer.root_y)), scale))
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

    /// NEW-59: same units pair by origin; scaled units (equal counts) pair
    /// by the desktop order; a toolkit display without a counterpart maps
    /// to none.
    #[test]
    fn displays_map_by_origin_then_by_order() {
        let rect = |x: f32, y: f32, width: f32, height: f32| Rect { x, y, width, height };
        let toolkit = [rect(0., 0., 1512., 982.), rect(-2560., -200., 2560., 1440.)];
        let points = [rect(-2560., -200., 2560., 1440.), rect(0., 0., 1512., 982.)];
        assert_eq!(map_displays(&toolkit, &points), vec![Some(1), Some(0)]);
        let pixels = [rect(-3840., -300., 3840., 2160.), rect(0., 0., 3024., 1964.)];
        assert_eq!(map_displays(&toolkit, &pixels), vec![Some(1), Some(0)]);
        assert_eq!(map_displays(&toolkit, &points[..1]), vec![None, Some(0)]);
    }
}
