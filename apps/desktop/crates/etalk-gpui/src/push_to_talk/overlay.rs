//! #NEW-54: the push-to-talk overlay window: THE capsule
//! (`voice_overlay::capsule`) in a transparent, always-on-top window that
//! never takes focus (Exponential's computer-use banner's pattern), at the
//! bottom centre of the display under the pointer, [`EDGE`] px above its
//! edge. Wayland places it itself. The window is the capsule's measured
//! size ([`capsule_size`], at most the display less [`EDGE`] each side): it
//! opens again, wider, when a longer line no longer fits. S-3: a call
//! (`voice_call`) opens its own window of the same kind with the FLOATING
//! CALL PILL ([`Face::Call`], `voice_overlay::call_pill`) while no IDE
//! window is focused: FIXED at `pill.width` × `pill.height` (never resized
//! or reopened while its words change), draggable by any point outside its
//! buttons (X11 override-redirect windows get no WM move: [`Drag`] tracks
//! the pointer itself), its position remembered on this device
//! ([`PILL_FILE`]).
//!
//! NEW-59 (`fixtures/work-screen.json` OVERLAY): never on a display with a
//! fullscreen app. When the window opens, the computer crate's screen state
//! is read ONCE and `epilot::work_screen::overlay_display` picks the
//! display: the pointer's without one, else the work screen, else the first
//! free one; none = the window stays hidden (the turn still runs, the reply
//! is still spoken).
//!
//! EXP-1912: none of these windows takes the keyboard. On Wayland they are
//! layer-shell surfaces on the overlay layer with keyboard interactivity
//! None ([`overlay_kind`]), anchored to the bottom centre of their display;
//! a compositor without wlr-layer-shell gets the window it had before
//! ([`open_overlay`]). A layer surface cannot be moved, so the pill's surface covers
//! what the panel leaves free of its display and the pill moves inside it
//! ([`Float`]): only the pill takes clicks (the window's input region).

use eui_core::voice_wave::Activity;
use gpui::{
    div, point, px, size, App, AppContext as _, Bounds, Context, InteractiveElement as _, IntoElement, MouseButton, ParentElement as _,
    Pixels, Point, Render, SharedString, Size, StatefulInteractiveElement as _, Styled as _, Window, WindowBounds, WindowHandle, WindowKind,
    WindowOptions,
};
use gpui_component::ActiveTheme as _;

use etalk::voice::PttOverlay;
use etalk::call::VOICE_CALL;

use crate::voice_overlay::{call_pill, capsule, capsule_size, CallActions, CallFace, Capsule};

/// The air between the capsule and the display's edge.
const EDGE: f32 = 24.;
/// The window's air around the capsule (its shadow).
const AIR: f32 = 8.;

/// EXP-1912: the capsule's layer-shell namespace (a compositor rule can
/// tell the overlays apart).
const CAPSULE_NAMESPACE: &str = "exponential-overlay:push-to-talk";
const PILL_NAMESPACE: &str = "exponential-overlay:call-pill";

/// EXP-1912: whether this app's windows are Wayland surfaces.
pub fn on_wayland(cx: &App) -> bool {
    cx.compositor_name() == "Wayland"
}

/// EXP-1912: the kind of a window that floats above every other and never
/// takes the keyboard. On Wayland a layer-shell surface on the overlay
/// layer, at the bottom centre of its display, `bottom` px above the edge
/// of what the panel leaves free, keyboard interactivity None (an ordinary
/// toplevel there takes the keyboard from the window being typed into, or
/// opens behind it). Elsewhere the PopUp: X11 override-redirect, a floating
/// panel on macOS.
pub fn overlay_kind(wayland: bool, namespace: &str, bottom: f32) -> WindowKind {
    #[cfg(target_os = "linux")]
    if wayland {
        use gpui::layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions};
        return WindowKind::LayerShell(LayerShellOptions {
            namespace: namespace.into(),
            layer: Layer::Overlay,
            anchor: Anchor::BOTTOM,
            margin: Some((px(0.), px(0.), px(bottom), px(0.))),
            keyboard_interactivity: KeyboardInteractivity::None,
            ..Default::default()
        });
    }
    let _ = (wayland, namespace, bottom);
    WindowKind::PopUp
}

/// EXP-1912: the pill's kind: on Wayland a layer surface anchored to all
/// four edges (what the panel leaves free of the display), keyboard
/// interactivity None; the pill floats inside it ([`Float`]).
fn pill_kind(wayland: bool) -> WindowKind {
    #[cfg(target_os = "linux")]
    if wayland {
        use gpui::layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions};
        return WindowKind::LayerShell(LayerShellOptions {
            namespace: PILL_NAMESPACE.into(),
            layer: Layer::Overlay,
            anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
            keyboard_interactivity: KeyboardInteractivity::None,
            ..Default::default()
        });
    }
    let _ = wayland;
    WindowKind::PopUp
}

/// EXP-1912: open the overlay window `options` describes for `kind`; `build`
/// hears whether it is a layer surface. A Wayland compositor without
/// wlr-layer-shell refuses one: the window then opens as the PopUp it was
/// before.
pub fn open_overlay<V: Render + 'static>(
    kind: WindowKind,
    cx: &mut App,
    options: impl Fn(WindowKind, &App) -> WindowOptions,
    build: impl Fn(bool, &mut Window, &mut App) -> gpui::Entity<V>,
) -> gpui::Result<WindowHandle<V>> {
    let layer = kind != WindowKind::PopUp;
    let first = options(kind, cx);
    match cx.open_window(first, |window, cx| build(layer, window, cx)) {
        Err(err) if layer => {
            log::warn!("[overlay] no layer surface ({err}): opening a plain window");
            let plain = options(WindowKind::PopUp, cx);
            cx.open_window(plain, |window, cx| build(false, window, cx))
        }
        opened => opened,
    }
}

/// EXP-1912: the pill inside its Wayland layer surface.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Float {
    /// The pill's origin in the window (`None` = [`pill_default`]).
    offset: Option<Point<Pixels>>,
    /// The display's origin: a remembered position is a desktop position.
    display: Point<Pixels>,
    /// A drag: the pointer's distance from the pill's origin at the press.
    grab: Option<Point<Pixels>>,
    /// Where the pill was drawn last: the window's input region.
    region: Option<Bounds<Pixels>>,
}

/// EXP-1912: the floating pill's origin in a window of `room`: `offset`, or
/// the default place, kept whole inside the window.
fn float_origin(offset: Option<Point<Pixels>>, room: Size<Pixels>, pill: Size<Pixels>) -> Point<Pixels> {
    let at = offset.unwrap_or_else(|| pill_default(Bounds { origin: point(px(0.), px(0.)), size: room }, pill));
    let inside = |at: Pixels, room: Pixels, pill: Pixels| px(f32::from(at).min(f32::from(room - pill)).max(0.));
    point(inside(at.x, room.width, pill.width), inside(at.y, room.height, pill.height))
}

/// What the window shows.
#[derive(Clone, Debug, PartialEq)]
pub enum Face {
    Ptt(PttOverlay),
    /// S-3: the floating call pill (`voice_call` hands it every change:
    /// a step, the level, the timer's second).
    Call(Box<CallFace>),
}

/// The window, open while the capsule shows.
#[derive(Default)]
pub struct Overlay {
    window: Option<WindowHandle<OverlayView>>,
    /// The open window's width: a line that needs more reopens it.
    width: f32,
    /// NEW-59: where this showing went, read once when it opened and kept
    /// until the face closes (a wider reopen stays on the same display).
    place: Option<Place>,
    /// EXP-1901: the windows this overlay closed last; the watcher's
    /// window-close observer asks once ([`Overlay::closed`]).
    closed: Vec<gpui::WindowId>,
}

/// EXP-1901: how many closed windows an overlay remembers (the pill's are
/// never asked for).
const CLOSED_KEPT: usize = 4;

/// EXP-1901: run `update` in the overlay's own window. A click on the pill
/// or the capsule arrives here from inside that window's update, where gpui
/// refuses a nested one: `update` then runs right after it. `false` = the
/// window is gone.
fn update_window(
    window: WindowHandle<OverlayView>,
    cx: &mut App,
    update: impl FnOnce(&mut OverlayView, &mut Window, &mut Context<OverlayView>) + 'static,
) -> bool {
    let mut update = Some(update);
    let _ = window.update(cx, |this, window, cx| {
        if let Some(update) = update.take() {
            update(this, window, cx);
        }
    });
    let Some(update) = update else { return true };
    if !cx.windows().iter().any(|open| open.window_id() == window.window_id()) {
        return false;
    }
    cx.defer(move |cx| {
        let _ = window.update(cx, update);
    });
    true
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
    pub fn show(&mut self, view: Option<Face>, cx: &mut App) {
        let Some(view) = view else {
            self.place = None;
            if let Some(window) = self.window.take() {
                self.close(window, cx);
            }
            return;
        };
        if let Face::Call(face) = view {
            self.show_pill(face, cx);
            return;
        }
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
            if f32::from(window_size.width) <= self.width && {
                let view = view.clone();
                update_window(window, cx, move |this, _, cx| {
                    this.view = view;
                    cx.notify();
                })
            } {
                self.window = Some(window);
                return;
            }
            self.close(window, cx);
        }
        let origin = area.map(|area| bottom_centre(area, window_size)).unwrap_or_else(|| point(px(0.), px(0.)));
        // EXP-1912: on Wayland the layer surface's anchor and margin are
        // this very place ([`bottom_centre`]).
        let kind = overlay_kind(on_wayland(cx), CAPSULE_NAMESPACE, EDGE - AIR);
        let opened = open_overlay(
            kind,
            cx,
            |kind, cx| options(kind, display, origin, window_size, cx),
            |_, _, cx| cx.new(|_| OverlayView { view: view.clone(), drag: None, float: None, level: 0., bands: Vec::new() }),
        );
        match opened {
            Ok(window) => {
                self.window = Some(window);
                self.width = f32::from(window_size.width);
            }
            Err(err) => log::warn!("[voice] the push-to-talk overlay did not open: {err}"),
        }
    }

    /// S-3: the pill's window, FIXED at `pill.width` × `pill.height`: an
    /// open one only repaints; a new one opens where it was last dropped
    /// (or at the bottom centre of the display under the pointer).
    fn show_pill(&mut self, face: Box<CallFace>, cx: &mut App) {
        if let Some(window) = self.window {
            let next = Face::Call(face.clone());
            let updated = update_window(window, cx, move |this, _, cx| {
                if this.view != next {
                    this.view = next;
                    cx.notify();
                }
            });
            if updated {
                return;
            }
            self.window = None;
        }
        let pill = size(px(VOICE_CALL.pill.width), px(VOICE_CALL.pill.height));
        let remembered = load_pill_origin(cx).filter(|origin| on_a_display(*origin, pill, cx));
        let (display, origin) = match remembered {
            Some(origin) => (None, origin),
            None => {
                let (display, area) = match placement(cx) {
                    Place::At(display, area) => (display, area),
                    Place::Hidden => (None, None),
                };
                (display, area.map(|area| pill_default(area, pill)).unwrap_or_else(|| point(px(0.), px(0.))))
            }
        };
        // EXP-1912: on Wayland the pill floats inside a layer surface over
        // its display: the remembered one's, else the placement's.
        let displays = cx.displays();
        let home = remembered.and_then(|origin| {
            let centre = point(origin.x + pill.width / 2., origin.y + pill.height / 2.);
            displays.iter().find(|display| display.bounds().contains(&centre)).map(|display| (display.id(), display.bounds()))
        });
        let float = match (remembered, home) {
            (Some(origin), Some((_, bounds))) => {
                Float { offset: Some(point(origin.x - bounds.origin.x, origin.y - bounds.origin.y)), display: bounds.origin, ..Float::default() }
            }
            _ => {
                let on = displays.iter().find(|shown| Some(shown.id()) == display).or(displays.first());
                Float { display: on.map(|display| display.bounds().origin).unwrap_or_default(), ..Float::default() }
            }
        };
        let room = home.map(|(_, bounds)| bounds.size).or(displays.first().map(|display| display.bounds().size)).unwrap_or(pill);
        let layer_display = home.map(|(id, _)| id).or(display);
        let opened = open_overlay(
            pill_kind(on_wayland(cx)),
            cx,
            |kind, cx| match kind {
                WindowKind::PopUp => options(kind, display, origin, pill, cx),
                kind => options(kind, layer_display, point(px(0.), px(0.)), room, cx),
            },
            |layer, _, cx| {
                cx.new(|_| OverlayView { view: Face::Call(face.clone()), drag: None, float: layer.then_some(float), level: 0., bands: Vec::new() })
            },
        );
        match opened {
            Ok(window) => {
                self.window = Some(window);
                self.width = f32::from(pill.width);
            }
            Err(err) => log::warn!("[voice] the call pill did not open: {err}"),
        }
    }

    fn close(&mut self, window: WindowHandle<OverlayView>, cx: &mut App) {
        if self.closed.len() == CLOSED_KEPT {
            self.closed.remove(0);
        }
        self.closed.push(window.window_id());
        update_window(window, cx, |_, window, _| window.remove_window());
    }

    /// EXP-1901: whether `id` is a window this overlay closed (asked once:
    /// by then [`Overlay::owns`] no longer knows it).
    pub fn closed(&mut self, id: gpui::WindowId) -> bool {
        let at = self.closed.iter().position(|closed| *closed == id);
        at.map(|at| self.closed.remove(at)).is_some()
    }

    /// Whether `id` is the overlay's own window (it opens again, wider, for
    /// a longer line mid-hold: never the IDE window the key is held in).
    pub fn owns(&self, id: gpui::WindowId) -> bool {
        self.window.as_ref().is_some_and(|window| window.window_id() == id)
    }

    /// The face the open window shows.
    #[cfg(test)]
    pub(crate) fn face(&self, cx: &mut App) -> Option<Face> {
        self.window.as_ref()?.update(cx, |this, _, _| this.view.clone()).ok()
    }

    /// A new level frame: redraw the wave. EXP-1901: the window holds the
    /// level itself; its first draw runs inside the watcher's update, where
    /// reading the watcher is a panic.
    pub fn refresh(&self, level: f32, bands: &[f32], cx: &mut App) {
        if let Some(window) = self.window.as_ref() {
            let _ = window.update(cx, |this, _, cx| {
                this.level = level;
                this.bands = bands.to_vec();
                cx.notify();
            });
        }
    }
}

/// S-3: the pill's default place: the bottom centre of `area`,
/// `pill.marginPx` above its edge.
fn pill_default(area: Bounds<Pixels>, pill: Size<Pixels>) -> Point<Pixels> {
    point(
        area.origin.x + (area.size.width - pill.width) / 2.,
        area.origin.y + area.size.height - pill.height - px(VOICE_CALL.pill.margin_px),
    )
}

/// S-3: a remembered position still shows the whole pill's centre on one
/// of the displays (else it falls back to the default).
fn on_a_display(origin: Point<Pixels>, pill: Size<Pixels>, cx: &App) -> bool {
    let centre = point(origin.x + pill.width / 2., origin.y + pill.height / 2.);
    cx.displays().iter().any(|display| display.bounds().contains(&centre))
}

/// S-3: the device-local file the pill's last position persists in (beside
/// the tray's `voice-tray.json`).
const PILL_FILE: &str = "voice-call-pill.json";

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
struct PillOrigin {
    x: f32,
    y: f32,
}

fn pill_file(cx: &App) -> Option<std::path::PathBuf> {
    // The host's data dir (the tray keeps its switches there).
    crate::host::data_dir(cx).map(|data_dir| data_dir.join(PILL_FILE))
}

fn load_pill_origin(cx: &App) -> Option<Point<Pixels>> {
    let bytes = std::fs::read(pill_file(cx)?).ok()?;
    let origin: PillOrigin = serde_json::from_slice(&bytes).ok()?;
    (origin.x.is_finite() && origin.y.is_finite()).then(|| point(px(origin.x), px(origin.y)))
}

fn save_pill_origin(origin: Point<Pixels>, cx: &App) {
    let Some(path) = pill_file(cx) else { return };
    let origin = PillOrigin { x: f32::from(origin.x), y: f32::from(origin.y) };
    let written = serde_json::to_vec(&origin).map_err(std::io::Error::other).and_then(|bytes| std::fs::write(path, bytes));
    if let Err(err) = written {
        log::warn!("[voice] the call pill's position was not kept: {err}");
    }
}

pub struct OverlayView {
    view: Face,
    /// S-3: the pill being dragged (X11's own pointer tracking).
    drag: Option<Drag>,
    /// EXP-1912: the pill floats inside this window (a Wayland layer
    /// surface); `None` = the window is the pill.
    float: Option<Float>,
    /// The wave's level and bands now ([`Overlay::refresh`]).
    level: f32,
    bands: Vec<f32>,
}

/// S-3: a pill drag on X11: the pointer and the window's origin at the
/// press, in root pixels; each move puts the window at the origin plus how
/// far the pointer went (an override-redirect window gets no WM move).
struct Drag {
    #[cfg(target_os = "linux")]
    x11: Option<X11Drag>,
}

#[cfg(target_os = "linux")]
struct X11Drag {
    conn: x11rb::rust_connection::RustConnection,
    root: u32,
    window: u32,
    pointer: (i32, i32),
    origin: (i32, i32),
    /// Root pixels per logical pixel (the position is kept logical).
    scale: f32,
    last: (i32, i32),
}

#[cfg(target_os = "linux")]
impl X11Drag {
    fn start(window: &Window, cx: &App) -> Option<Self> {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        use x11rb::connection::Connection as _;
        use x11rb::protocol::xproto::ConnectionExt as _;
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            return None;
        }
        let handle = HasWindowHandle::window_handle(window).ok()?;
        let x_window = match handle.as_raw() {
            RawWindowHandle::Xcb(xcb) => xcb.window.get(),
            RawWindowHandle::Xlib(xlib) => u32::try_from(xlib.window).ok()?,
            _ => return None,
        };
        let logical_root_width = f32::from(cx.primary_display()?.bounds().size.width);
        let (conn, screen) = x11rb::connect(None).ok()?;
        let root = conn.setup().roots.get(screen)?;
        let (root_id, scale) = (root.root, f32::from(root.width_in_pixels) / logical_root_width);
        let pointer = conn.query_pointer(root_id).ok()?.reply().ok()?;
        let at = conn.translate_coordinates(x_window, root_id, 0, 0).ok()?.reply().ok()?;
        let origin = (i32::from(at.dst_x), i32::from(at.dst_y));
        Some(X11Drag {
            conn,
            root: root_id,
            window: x_window,
            pointer: (i32::from(pointer.root_x), i32::from(pointer.root_y)),
            origin,
            scale: if scale > 0. { scale } else { 1. },
            last: origin,
        })
    }

    /// Follow the pointer.
    fn follow(&mut self) {
        use x11rb::connection::Connection as _;
        use x11rb::protocol::xproto::{ConfigureWindowAux, ConnectionExt as _};
        let Some(pointer) = self.conn.query_pointer(self.root).ok().and_then(|cookie| cookie.reply().ok()) else { return };
        let at = drag_origin(self.origin, self.pointer, (i32::from(pointer.root_x), i32::from(pointer.root_y)));
        if at == self.last {
            return;
        }
        self.last = at;
        let _ = self.conn.configure_window(self.window, &ConfigureWindowAux::new().x(at.0).y(at.1));
        let _ = self.conn.flush();
    }

    /// Where the pill was dropped, in logical pixels.
    fn dropped(&self) -> Point<Pixels> {
        point(px(self.last.0 as f32 / self.scale), px(self.last.1 as f32 / self.scale))
    }
}

/// S-3: the window's origin for a pointer that moved from `start` to `now`.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn drag_origin(origin: (i32, i32), start: (i32, i32), now: (i32, i32)) -> (i32, i32) {
    (origin.0 + now.0 - start.0, origin.1 + now.1 - start.1)
}

impl OverlayView {
    /// A press outside the pill's buttons starts a drag: X11 tracks the
    /// pointer itself, everything else hands the move to the system.
    fn press(&mut self, at: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        // EXP-1912: inside a layer surface the pill follows the pointer
        // itself ([`OverlayView::float_to`]).
        if let Some(float) = self.float.as_mut() {
            let origin = float.region.map(|region| region.origin).unwrap_or_default();
            float.grab = Some(point(at.x - origin.x, at.y - origin.y));
            return;
        }
        #[cfg(target_os = "linux")]
        if let Some(x11) = X11Drag::start(window, cx) {
            self.drag = Some(Drag { x11: Some(x11) });
            return;
        }
        let _ = cx;
        window.start_window_move();
        #[cfg(not(target_os = "linux"))]
        {
            self.drag = Some(Drag {});
        }
        #[cfg(target_os = "linux")]
        {
            self.drag = Some(Drag { x11: None });
        }
    }

    fn moved(&mut self) {
        #[cfg(target_os = "linux")]
        if let Some(x11) = self.drag.as_mut().and_then(|drag| drag.x11.as_mut()) {
            x11.follow();
        }
    }

    /// EXP-1912: a floating pill being dragged follows the pointer.
    fn float_to(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(float) = self.float.as_mut() else { return };
        let Some(grab) = float.grab else { return };
        float.offset = Some(point(at.x - grab.x, at.y - grab.y));
        cx.notify();
    }

    /// The drop: remember where the pill went.
    fn release(&mut self, window: &Window, cx: &App) {
        if let Some(float) = self.float.as_mut() {
            // The place it was drawn at, as a desktop position.
            if let (Some(_), Some(region)) = (float.grab.take(), float.region) {
                save_pill_origin(point(float.display.x + region.origin.x, float.display.y + region.origin.y), cx);
            }
            return;
        }
        let Some(drag) = self.drag.take() else { return };
        #[cfg(target_os = "linux")]
        if let Some(x11) = drag.x11.as_ref() {
            save_pill_origin(x11.dropped(), cx);
            return;
        }
        let _ = drag;
        save_pill_origin(window.bounds().origin, cx);
    }
}

/// The fixture's activity key as the wave's.
pub fn activity(key: &str) -> Activity {
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
        },
        Face::Call(_) => Capsule {
            activity: Activity::Idle,
            level: 0.,
            bands: Vec::new(),
            state: SharedString::default(),
            line: SharedString::default(),
            pill: SharedString::default(),
            primary: true,
        },
    }
}

impl Render for OverlayView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // No `Root` here: the rem and the font are set like the banner's.
        window.set_rem_size(cx.theme().font_size);
        let root = div().size_full().flex().items_center().justify_center().font_family(cx.theme().font_family.clone());
        // EXP-1912: a floating pill's place; only the pill takes clicks,
        // every other point of the layer surface belongs to what is under it.
        let pill = size(px(VOICE_CALL.pill.width), px(VOICE_CALL.pill.height));
        let floating = self.float.as_mut().filter(|_| matches!(self.view, Face::Call(_))).map(|float| {
            let region = Bounds { origin: float_origin(float.offset, window.viewport_size(), pill), size: pill };
            if float.region != Some(region) {
                window.set_input_region(Some(&[region]));
                float.region = Some(region);
            }
            region
        });
        match &self.view {
            Face::Ptt(view) => {
                let c = specimen(&self.view);
                let (level, bands) = if view.activity == "listening" { (self.level, self.bands.clone()) } else { (0., Vec::new()) };
                let c = Capsule { level, bands, ..c };
                // The window never takes focus; a click still lands: the
                // pill's action (Stop, Unmute).
                root.child(capsule("push-to-talk-overlay", &c, cx).on_click(|_, _, cx| super::on_pill(cx))).into_any_element()
            }
            Face::Call(face) => {
                let actions = CallActions {
                    on_mute: std::rc::Rc::new(crate::voice_call::toggle_mute),
                    on_end: std::rc::Rc::new(crate::voice_call::end),
                };
                let pill = call_pill(SharedString::from(VOICE_CALL.test_ids.pill.clone()), face, &actions, cx)
                    // Any point outside the buttons drags it (they stop
                    // their own presses).
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| this.press(event.position, window, cx)))
                    .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, _| {
                        if event.pressed_button == Some(MouseButton::Left) {
                            this.moved();
                        }
                    }))
                    .on_mouse_up(MouseButton::Left, cx.listener(|this, _, window, cx| this.release(window, cx)))
                    .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _, window, cx| this.release(window, cx)))
                    // Esc ends the call when the pill has the focus.
                    .on_key_down(|event, _, cx| {
                        if event.keystroke.key == "escape" {
                            crate::voice_call::end(cx);
                        }
                    });
                let Some(region) = floating else { return root.child(pill).into_any_element() };
                // The whole window hears the drag: the pointer may outrun
                // the pill between two frames.
                div()
                    .size_full()
                    .font_family(cx.theme().font_family.clone())
                    .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                        if event.pressed_button == Some(MouseButton::Left) {
                            this.float_to(event.position, cx);
                        }
                    }))
                    .on_mouse_up(MouseButton::Left, cx.listener(|this, _, window, cx| this.release(window, cx)))
                    .child(div().absolute().left(region.origin.x).top(region.origin.y).w(region.size.width).h(region.size.height).child(pill))
                    .into_any_element()
            }
        }
    }
}

/// The window around the capsule `c`: its measured size plus the air,
/// never wider than `room` (the display's width) less [`EDGE`] each side.
fn window_size(c: &Capsule, room: Option<f32>, cx: &App) -> Size<Pixels> {
    let max_width = room.map_or(f32::MAX, |room| room - 2. * EDGE);
    let capsule = capsule_size(c, max_width - 2. * AIR, cx);
    size(capsule.width + px(2. * AIR), capsule.height + px(2. * AIR))
}

fn options(kind: WindowKind, display: Option<gpui::DisplayId>, origin: Point<Pixels>, window: Size<Pixels>, cx: &App) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds { origin, size: window })),
        titlebar: None,
        focus: false,
        show: true,
        // Above every window, never activated (X11 override-redirect, a
        // floating panel on macOS, a layer surface on Wayland).
        kind,
        is_movable: false,
        is_resizable: false,
        is_minimizable: false,
        display_id: display,
        window_background: gpui::WindowBackgroundAppearance::Transparent,
        app_id: crate::host::host(cx).map(|host| host.brand().app_id.to_string()),
        #[cfg(target_os = "linux")]
        window_decorations: Some(gpui::WindowDecorations::Client),
        ..Default::default()
    }
}

/// The bottom centre of `area`, [`EDGE`] above its bottom (less the air).
pub fn bottom_centre(area: Bounds<Pixels>, window: Size<Pixels>) -> Point<Pixels> {
    point(
        area.origin.x + (area.size.width - window.width) / 2.,
        area.origin.y + area.size.height - window.height - px(EDGE - AIR),
    )
}

/// The display under the pointer and the area the window centres in; NEW-59
/// moved off (or hidden from) a display with a fullscreen app.
fn placement(cx: &App) -> Place {
    // A test never asks the machine's real screen.
    if cfg!(test) {
        return Place::At(None, None);
    }
    #[cfg(target_os = "linux")]
    if super::hotkey::session() == Some(etalk::voice::VoiceSession::X11) {
        if let Some(place) = x11_place(cx) {
            return place;
        }
    }
    let displays = cx.displays();
    let primary = cx.primary_display().and_then(|primary| displays.iter().position(|display| display.id() == primary.id()));
    let under = pointer(cx).and_then(|pointer| displays.iter().position(|display| display.bounds().contains(&pointer)));
    let at = |display: &std::rc::Rc<dyn gpui::PlatformDisplay>| Place::At(Some(display.id()), Some(display.visible_bounds()));
    if let (Some(pointer_ix), Some(state)) = (under.or(primary), epilot::screen_state()) {
        let map = display_map(&displays, &state.displays);
        if let Some(Some(pointer)) = map.get(pointer_ix).copied() {
            let target = epilot::work_screen::overlay_display(state.displays.len(), pointer, &state.fullscreen, &state.work);
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

impl From<&epilot::Display> for Rect {
    fn from(display: &epilot::Display) -> Self {
        Rect { x: display.x as f32, y: display.y as f32, width: display.width as f32, height: display.height as f32 }
    }
}

/// NEW-59: `displays` (the toolkit's) mapped to `screens` (the computer
/// crate's), index by index ([`map_displays`]). The banner places by it too.
pub fn display_map(
    displays: &[std::rc::Rc<dyn gpui::PlatformDisplay>],
    screens: &[epilot::Display],
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
    if let Some(state) = epilot::screen_state() {
        if let Some((pointer, scale)) = x11_pointer(cx) {
            let contains = |display: &epilot::Display| {
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
                    match epilot::work_screen::overlay_display(state.displays.len(), under, &state.fullscreen, &state.work) {
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

    /// S-3: the pill sits `pill.marginPx` above the bottom centre, and a
    /// drag moves the window by exactly what the pointer moved.
    #[test]
    fn the_pill_defaults_to_the_bottom_centre_and_follows_the_pointer() {
        let area = Bounds { origin: point(px(1920.), px(0.)), size: size(px(1920.), px(1080.)) };
        let pill = size(px(VOICE_CALL.pill.width), px(VOICE_CALL.pill.height));
        let origin = pill_default(area, pill);
        assert_eq!(f32::from(origin.y) + VOICE_CALL.pill.height, 1080. - VOICE_CALL.pill.margin_px);
        assert_eq!(f32::from(origin.x) + VOICE_CALL.pill.width / 2., 1920. + 960.);
        assert_eq!(drag_origin((100, 200), (150, 220), (180, 200)), (130, 180));
    }

    /// EXP-1912: on Wayland the overlays are layer surfaces on the overlay
    /// layer that never take the keyboard, anchored where the PopUp sits
    /// elsewhere: the bottom centre, the same distance above the edge.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_wayland_overlay_is_a_layer_surface_without_the_keyboard() {
        use gpui::layer_shell::{Anchor, KeyboardInteractivity, Layer};
        let WindowKind::LayerShell(layer) = overlay_kind(true, CAPSULE_NAMESPACE, EDGE - AIR) else {
            panic!("an ordinary window on Wayland takes the keyboard");
        };
        assert_eq!(layer.keyboard_interactivity, KeyboardInteractivity::None);
        assert_eq!(layer.layer, Layer::Overlay);
        assert_eq!(layer.anchor, Anchor::BOTTOM);
        assert_eq!(layer.namespace, "exponential-overlay:push-to-talk");
        // No zone of its own: the panel's keeps it off the panel.
        assert_eq!(layer.exclusive_zone, None);
        // The margin is the PopUp's distance from the bottom edge.
        let area = Bounds { origin: point(px(0.), px(0.)), size: size(px(1920.), px(1080.)) };
        let window = size(px(436.), px(90.));
        let popup_bottom = 1080. - f32::from(bottom_centre(area, window).y) - 90.;
        assert_eq!(layer.margin, Some((px(0.), px(0.), px(popup_bottom), px(0.))));
        assert_eq!(overlay_kind(false, CAPSULE_NAMESPACE, EDGE - AIR), WindowKind::PopUp);
        // The pill's surface covers what the panel leaves free of its display.
        let WindowKind::LayerShell(pill) = pill_kind(true) else {
            panic!("the pill is a layer surface on Wayland");
        };
        assert_eq!(pill.keyboard_interactivity, KeyboardInteractivity::None);
        assert_eq!((pill.layer, pill.anchor), (Layer::Overlay, Anchor::all()));
        assert_eq!(pill_kind(false), WindowKind::PopUp);
    }

    /// EXP-1912: inside its layer surface the pill sits at the default
    /// place until it was dragged, then where it was dropped, always whole
    /// inside the window.
    #[test]
    fn the_floating_pill_keeps_its_place_inside_the_layer_surface() {
        let room = size(px(1280.), px(752.));
        let pill = size(px(VOICE_CALL.pill.width), px(VOICE_CALL.pill.height));
        let default = pill_default(Bounds { origin: point(px(0.), px(0.)), size: room }, pill);
        assert_eq!(float_origin(None, room, pill), default);
        assert_eq!(float_origin(Some(point(px(100.), px(200.))), room, pill), point(px(100.), px(200.)));
        // Dropped past an edge (or remembered from a larger display).
        assert_eq!(float_origin(Some(point(px(-40.), px(9000.))), room, pill), point(px(0.), px(752. - VOICE_CALL.pill.height)));
        assert_eq!(float_origin(Some(point(px(5000.), px(-3.))), room, pill), point(px(1280. - VOICE_CALL.pill.width), px(0.)));
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
