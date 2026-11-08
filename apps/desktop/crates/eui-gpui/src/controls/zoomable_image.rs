//! EXP-1586 — THE ZoomableImage: a picture that zooms and pans, the gpui
//! painter of the catalog `ZoomableImage` (catalog/style.json
//! `zoomableImage`, whose $comment is the spec) and a view the desktop app
//! embeds on its own (the lightbox).
//!
//! Every view change goes through the core's maths (`eui_core::zoom`, locked
//! by `fixtures/zoom.json`); this file only feeds it gpui's input:
//!
//! - ⌘/ctrl + wheel and a trackpad pinch zoom about the pointer (a pinch past
//!   the min/max rubber-bands, released = sprung back to the limits);
//! - a plain wheel / two-finger scroll PANS while the picture has room on
//!   that axis (a tall picture scrolls), else it reaches the host;
//! - a mouse drag pans (rubber band past the edges, momentum on release,
//!   sprung back with the theme's standard motion; reduced motion snaps);
//! - a double click toggles rest ↔ max(100%, 2 × rest) about the point; a
//!   single click at rest fires [`ZoomableImageEvent::Tap`] (after the
//!   double-click window, so a double click never taps);
//! - `+ = − 0 1` while focused (arrows are never consumed: they page a
//!   host's carousel);
//! - THE PILL (`controls`): one capsule bottom-centre `pill.inset` above the
//!   picture's bottom edge: − / the percent (opening a Menu of
//!   `zoom_menu`: Fit, 50%, 100%, 200%, 400%) / + / a hairline / fit (drawn
//!   `selected` while at rest). `auto` shows it on appear and hides it
//!   `autoHideMs` after the last pointer move / zoom / focus, kept while
//!   hovered or focused.
//!
//! Colours, radii and type come from the theme's `ZoomableImage` recipe
//! parts (`image`, `pill`, `zoomOut`, `percent`, `zoomIn`, `divider`, `fit`)
//! and the `Menu` parts for the percent menu.
//!
//! # Embedding
//!
//! ```ignore
//! let zoom = cx.new(|cx| {
//!     ZoomableImage::new("attachment-42", Some(image_source), Some((1440.0, 900.0)), ZoomableImageOptions::default(), cx)
//! });
//! cx.subscribe(&zoom, |this, _, ev: &ZoomableImageEvent, cx| match ev {
//!     ZoomableImageEvent::Zoom { scale, at_fit } => { /* … */ }
//!     ZoomableImageEvent::Tap => { /* toggle the lightbox chrome */ }
//!     ZoomableImageEvent::Focus => {}
//! })
//! .detach();
//! // render: div().size_full().child(zoom.clone())
//! ```
//!
//! The view fills its parent (`size_full`) and measures its own box; a host
//! that knows the size up front hands it over with
//! [`ZoomableImage::set_viewport_hint`] so the first frame is right. A new
//! source key ([`ZoomableImage::set_source`]) returns to the rest view
//! without animation; a resize keeps the rest view at rest, else the scale
//! about the centre (`zoom_resize`).

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use eui_core::theme::{Mode, ResolvedTheme};
use eui_core::zoom::{self as z, WheelDeltaMode, ZoomFrame, ZoomInitial, ZoomPoint, ZoomSize, ZoomView, ZOOM};
use gpui::{
    anchored, canvas, deferred, div, point, prelude::*, px, Anchor, AnyElement, App, Bounds, Context, Corners, CursorStyle, DispatchPhase, ElementId, EventEmitter, FocusHandle, Focusable, Font, FontWeight,
    Hsla, ImageSource, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PinchEvent, Pixels, Point, ScrollDelta, ScrollEvent, SharedString, Task, TouchPhase, Window,
};
use serde_json::{json, Map, Value};

use crate::chrome::ease;
use crate::host::{HostPlugin, NoHost};
use crate::paint::icons;
use crate::paint::natives::{render_image, styled_box};
use crate::paint::parts::{part_props, px_prop};
use crate::paint::PaintStyle;

/// The `controls` prop (catalog enum `zoomControls`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ZoomControls {
    /// Shown on appear, hidden `autoHideMs` after the last activity.
    #[default]
    Auto,
    Always,
    None,
}

impl ZoomControls {
    /// Anything else = `Auto`.
    pub fn parse(s: &str) -> Self {
        match s {
            "always" => Self::Always,
            "none" => Self::None,
            _ => Self::Auto,
        }
    }
}

/// The pill's words (catalog/strings.json `zoomIn`, `zoomOut`, `zoomToFit`,
/// `zoomLevel`, `zoomFit`), localised by the host.
#[derive(Debug, Clone, PartialEq)]
pub struct ZoomStrings {
    pub zoom_in: SharedString,
    pub zoom_out: SharedString,
    pub zoom_to_fit: SharedString,
    pub zoom_level: SharedString,
    /// The Menu's first row (the rest view).
    pub zoom_fit: SharedString,
}

impl Default for ZoomStrings {
    fn default() -> Self {
        let s = |id: &str| SharedString::from(eui_core::strings::default_string(id).unwrap_or(id).to_string());
        ZoomStrings { zoom_in: s("zoomIn"), zoom_out: s("zoomOut"), zoom_to_fit: s("zoomToFit"), zoom_level: s("zoomLevel"), zoom_fit: s("zoomFit") }
    }
}

/// How a [`ZoomableImage`] behaves and paints.
#[derive(Clone)]
pub struct ZoomableImageOptions {
    /// The accessibility name (the catalog's `alt`).
    pub alt: SharedString,
    pub initial: ZoomInitial,
    /// `None` = `ZOOM.max_scale` (8).
    pub max_scale: Option<f64>,
    pub controls: ZoomControls,
    /// The theme the recipe parts resolve from (`None` = geometry only: no
    /// pill paint, the picture still zooms).
    pub theme: Option<Arc<ResolvedTheme>>,
    pub mode: Mode,
    /// Icons (`ui-minus`, `ui-add`, `ui-fullscreen`, `ui-check`) resolve
    /// through the host's registry, else the painter's own glyphs.
    pub host: Rc<dyn HostPlugin>,
    pub strings: ZoomStrings,
    /// Springs and steps snap; momentum stops at once.
    pub reduced_motion: bool,
    /// The pill reads right to left.
    pub rtl: bool,
    /// The family the percent label sets in (`None` = inherited).
    pub font_family: Option<SharedString>,
}

impl Default for ZoomableImageOptions {
    fn default() -> Self {
        ZoomableImageOptions {
            alt: SharedString::default(),
            initial: ZoomInitial::Auto,
            max_scale: None,
            controls: ZoomControls::Auto,
            theme: Some(eui_core::themes::default_theme()),
            mode: Mode::Dark,
            host: Rc::new(NoHost),
            strings: ZoomStrings::default(),
            reduced_motion: false,
            rtl: false,
            font_family: None,
        }
    }
}

impl ZoomableImageOptions {
    /// Every field equal (the theme and host by identity).
    fn same_as(&self, o: &ZoomableImageOptions) -> bool {
        let theme = match (&self.theme, &o.theme) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        theme
            && Rc::ptr_eq(&self.host, &o.host)
            && self.alt == o.alt
            && self.initial == o.initial
            && self.max_scale == o.max_scale
            && self.controls == o.controls
            && self.mode == o.mode
            && self.strings == o.strings
            && self.reduced_motion == o.reduced_motion
            && self.rtl == o.rtl
            && self.font_family == o.font_family
    }
}

/// What a [`ZoomableImage`] reports (`cx.subscribe`).
#[derive(Debug, Clone, PartialEq)]
pub enum ZoomableImageEvent {
    /// After each SETTLED zoom change (a step, a menu pick, a double click,
    /// the end of a wheel burst, a released pinch): the catalog `zoom`
    /// event. `at_fit` = the rest view (the fit button's `selected`).
    Zoom { scale: f64, at_fit: bool },
    /// A single click at rest (the catalog `tap`).
    Tap,
    /// The picture took keyboard focus (a click or a programmatic focus).
    Focus,
}

/// The pill's pressable parts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ZoomPart {
    ZoomOut,
    Percent,
    ZoomIn,
    Fit,
}

impl ZoomPart {
    /// The recipe part name (and the element id suffix).
    pub fn name(self) -> &'static str {
        match self {
            ZoomPart::ZoomOut => "zoomOut",
            ZoomPart::Percent => "percent",
            ZoomPart::ZoomIn => "zoomIn",
            ZoomPart::Fit => "fit",
        }
    }
}

/// A single click waits this long for a second one before it taps.
pub const TAP_DELAY: Duration = Duration::from_millis(250);
/// A pointer that moved less than this (px) between down and up clicked.
const CLICK_SLOP: f32 = 3.0;
/// The end of a wheel burst (the `zoom` event fires once it settled).
const WHEEL_SETTLE: Duration = Duration::from_millis(150);
/// The release velocity averages the pointer over this window.
const VELOCITY_WINDOW_MS: f64 = 80.0;
/// `motion.standard` / `ease.standard` when the theme has none.
const STANDARD_MS: f64 = 180.0;
const STANDARD_EASE: [f32; 4] = [0.2, 0.0, 0.0, 1.0];

/// A view moving on its own.
#[derive(Debug, Clone, Copy)]
enum Anim {
    /// Eased from one view to another (a step, a spring back).
    Tween { from: ZoomView, to: ZoomView, start: Instant, ms: f64 },
    /// Mouse-drag momentum from `from` at (vx, vy) px/ms (`zoom_momentum`).
    Momentum { from: ZoomView, vx: f64, vy: f64, start: Instant },
}

/// A left button held on the picture.
#[derive(Debug, Clone)]
struct Press {
    origin: Point<Pixels>,
    start: ZoomView,
    /// It moved past the click slop (a drag, never a click).
    dragging: bool,
    /// The picture pans (it has room on an axis).
    pans: bool,
    /// (time, pointer) for the release velocity.
    samples: Vec<(Instant, Point<Pixels>)>,
}

/// THE ZoomableImage view. See the module doc.
pub struct ZoomableImage {
    key: SharedString,
    image: Option<ImageSource>,
    /// The author's natural size, else the decoded picture's.
    given: Option<ZoomSize>,
    decoded: Option<ZoomSize>,
    options: ZoomableImageOptions,
    view: ZoomView,
    viewport: ZoomSize,
    /// The box measured at the last paint (window px).
    measured: Rc<Cell<Option<Bounds<Pixels>>>>,
    anim: Option<Anim>,
    press: Option<Press>,
    /// The pinch in flight: the view it started from, the factor so far.
    pinch: Option<(ZoomView, f64)>,
    focus: FocusHandle,
    was_focused: bool,
    /// The scale the last `zoom` event reported.
    reported: f64,
    /// The last `scale` the host bound in ([`Self::set_scale`]).
    external_scale: Option<f64>,
    pill_shown: bool,
    pill_hovered: bool,
    hovered_part: Option<ZoomPart>,
    menu_open: bool,
    hide_task: Option<Task<()>>,
    tap_task: Option<Task<()>>,
    settle_task: Option<Task<()>>,
}

impl EventEmitter<ZoomableImageEvent> for ZoomableImage {}

impl Focusable for ZoomableImage {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

fn size_of(natural: Option<(f64, f64)>) -> Option<ZoomSize> {
    natural.filter(|(w, h)| w.is_finite() && h.is_finite() && *w > 0.0 && *h > 0.0).map(|(width, height)| ZoomSize { width, height })
}

fn lerp_view(a: ZoomView, b: ZoomView, t: f64) -> ZoomView {
    ZoomView { scale: a.scale + (b.scale - a.scale) * t, x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t }
}

fn same_view(a: ZoomView, b: ZoomView) -> bool {
    (a.scale - b.scale).abs() < 1e-6 && (a.x - b.x).abs() < 1e-3 && (a.y - b.y).abs() < 1e-3
}

impl ZoomableImage {
    /// A picture identified by `key` (a new key = a new picture: back to
    /// rest), drawn from `image` (`None` = nothing to load: the broken-image
    /// glyph), `natural` = its pixel size (`None` = the decoded size once it
    /// loads).
    pub fn new(key: impl Into<SharedString>, image: Option<ImageSource>, natural: Option<(f64, f64)>, options: ZoomableImageOptions, cx: &mut Context<Self>) -> Self {
        let mut this = ZoomableImage {
            key: key.into(),
            image,
            given: size_of(natural),
            decoded: None,
            pill_shown: options.controls != ZoomControls::None,
            options,
            view: ZoomView { scale: 1.0, x: 0.0, y: 0.0 },
            viewport: ZoomSize::default(),
            measured: Rc::new(Cell::new(None)),
            anim: None,
            press: None,
            pinch: None,
            focus: cx.focus_handle(),
            was_focused: false,
            reported: 1.0,
            external_scale: None,
            pill_hovered: false,
            hovered_part: None,
            menu_open: false,
            hide_task: None,
            tap_task: None,
            settle_task: None,
        };
        this.reset_view();
        this.schedule_hide(cx);
        this
    }

    // -- The host's API -----------------------------------------------------

    /// A (possibly) new picture. Another `key` returns to the rest view
    /// without animation; the same key only refreshes the source and size.
    pub fn set_source(&mut self, key: impl Into<SharedString>, image: Option<ImageSource>, natural: Option<(f64, f64)>, cx: &mut Context<Self>) {
        let key = key.into();
        let given = size_of(natural);
        let fresh = key != self.key;
        let resized = given != self.given;
        self.key = key;
        self.image = image;
        self.given = given;
        if fresh {
            self.decoded = None;
            self.external_scale = None;
        }
        if fresh || resized {
            self.reset_view();
            cx.notify();
        }
    }

    /// New options. A change of `initial` / `maxScale` re-rests a picture
    /// at rest (a zoomed one is clamped into the new limits).
    pub fn set_options(&mut self, options: ZoomableImageOptions, cx: &mut Context<Self>) {
        if self.options.same_as(&options) {
            return;
        }
        let limits_changed = options.initial != self.options.initial || options.max_scale != self.options.max_scale;
        let was_rest = self.at_rest();
        let controls_changed = options.controls != self.options.controls;
        self.options = options;
        if limits_changed {
            let f = self.frame();
            self.view = if was_rest { z::zoom_rest_view(&f) } else { z::zoom_clamp(self.view, &f) };
            self.reported = self.view.scale;
        }
        if controls_changed {
            self.pill_shown = self.options.controls != ZoomControls::None;
            self.schedule_hide(cx);
        }
        cx.notify();
    }

    /// The box size when the host knows it before the first paint (the
    /// surface's laid-out frame); the measured box wins once painted.
    pub fn set_viewport_hint(&mut self, width: f32, height: f32) {
        if self.measured.get().is_none() {
            self.resize(ZoomSize { width: width as f64, height: height as f64 });
        }
    }

    /// The host's bound `scale` (1 = 100%): a value other than the last one
    /// bound zooms there about the centre, without an event.
    pub fn set_scale(&mut self, scale: Option<f64>, cx: &mut Context<Self>) {
        let scale = scale.filter(|s| s.is_finite() && *s > 0.0);
        if scale == self.external_scale {
            return;
        }
        self.external_scale = scale;
        let Some(s) = scale else { return };
        if (s - self.view.scale).abs() > 1e-4 && self.has_frame() {
            let f = self.frame();
            self.view = z::zoom_to(self.view, s, z::zoom_centre(&f), &f);
            self.anim = None;
            self.reported = self.view.scale;
            cx.notify();
        }
    }

    /// The view now (scale = displayed px per image px; x/y = the picture's
    /// top-left in the box).
    pub fn view(&self) -> ZoomView {
        self.view
    }

    /// The view the current motion ends at (= [`Self::view`] at rest).
    pub fn target_view(&self) -> ZoomView {
        match self.anim {
            Some(Anim::Tween { to, .. }) => to,
            Some(Anim::Momentum { from, vx, vy, .. }) => z::zoom_momentum(from, vx, vy, 1e9, &self.frame()),
            None => self.view,
        }
    }

    /// The scale (1 = 100%).
    pub fn scale(&self) -> f64 {
        self.view.scale
    }

    /// At the rest view's scale (the fit button is `selected`).
    pub fn at_rest(&self) -> bool {
        z::zoom_at_rest(self.target_view(), &self.frame())
    }

    /// The pill is drawn now.
    pub fn pill_visible(&self) -> bool {
        self.options.controls == ZoomControls::Always || (self.options.controls == ZoomControls::Auto && self.pill_shown)
    }

    /// The percent Menu is open.
    pub fn menu_open(&self) -> bool {
        self.menu_open
    }

    /// The core frame the maths runs on.
    pub fn frame(&self) -> ZoomFrame {
        ZoomFrame { natural: self.natural().unwrap_or_default(), viewport: self.viewport, initial: self.options.initial, max_scale: self.options.max_scale }
    }

    /// One step up (`1`) or down (`-1`) the ladder about the centre.
    pub fn step(&mut self, direction: i32, window: &mut Window, cx: &mut Context<Self>) {
        let f = self.frame();
        let to = z::zoom_step(self.target_view(), direction, &f);
        self.zoom_settled(to, window, cx);
    }

    /// Back to the rest view (the fit button, `0`).
    pub fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let to = z::zoom_rest_view(&self.frame());
        self.zoom_settled(to, window, cx);
    }

    /// Zoom to `scale` about the centre (a Menu pick, `1`).
    pub fn zoom_to_scale(&mut self, scale: f64, window: &mut Window, cx: &mut Context<Self>) {
        let f = self.frame();
        let to = z::zoom_to(self.target_view(), scale, z::zoom_centre(&f), &f);
        self.zoom_settled(to, window, cx);
    }

    /// A key while focused (`+`, `=`, `-`, `−`, `_`, `0`, `1`); `true` =
    /// taken. Everything else (arrows, Tab, Escape) stays the host's.
    pub fn handle_key(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let key = match key {
            "plus" => "+",
            "equal" => "=",
            "minus" => "-",
            k => k,
        };
        if !self.has_frame() {
            return false;
        }
        let Some(to) = z::zoom_key(self.target_view(), key, &self.frame()) else { return false };
        self.zoom_settled(to, window, cx);
        true
    }

    /// Press a pill part (what a click on it does; for hosts and tests).
    pub fn press_part(&mut self, part: ZoomPart, window: &mut Window, cx: &mut Context<Self>) {
        let b = z::zoom_bounds(self.target_view(), &self.frame());
        self.activity(cx);
        match part {
            ZoomPart::ZoomOut if !b.at_min => self.step(-1, window, cx),
            ZoomPart::ZoomIn if !b.at_max => self.step(1, window, cx),
            ZoomPart::Percent => {
                self.menu_open = !self.menu_open;
                cx.notify();
            }
            ZoomPart::Fit => self.reset(window, cx),
            _ => {}
        }
    }

    /// Pick Menu row `index` of `zoom_menu` (0 = Fit).
    pub fn pick_menu(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let items = z::zoom_menu(self.target_view(), &self.frame(), &self.options.strings.zoom_fit);
        self.menu_open = false;
        match items.get(index).map(|i| i.scale) {
            Some(Some(s)) => self.zoom_to_scale(s, window, cx),
            Some(None) => self.reset(window, cx),
            None => cx.notify(),
        }
    }

    // -- State ----------------------------------------------------------------

    fn natural(&self) -> Option<ZoomSize> {
        self.given.or(self.decoded)
    }

    fn has_frame(&self) -> bool {
        self.natural().is_some() && self.viewport.width > 0.0 && self.viewport.height > 0.0
    }

    fn reset_view(&mut self) {
        self.anim = None;
        self.press = None;
        self.pinch = None;
        self.menu_open = false;
        self.view = z::zoom_rest_view(&self.frame());
        self.reported = self.view.scale;
    }

    fn resize(&mut self, to: ZoomSize) {
        if (to.width - self.viewport.width).abs() < 0.01 && (to.height - self.viewport.height).abs() < 0.01 {
            return;
        }
        let from = self.viewport;
        let had = self.has_frame();
        let target = self.target_view();
        self.viewport = to;
        self.anim = None;
        self.view = if had && from.width > 0.0 && from.height > 0.0 { z::zoom_resize(target, from, &self.frame()) } else { z::zoom_rest_view(&self.frame()) };
        self.reported = self.view.scale;
    }

    fn now(cx: &App) -> Instant {
        cx.background_executor().now()
    }

    /// The theme's standard motion (ms, cubic-bezier).
    fn standard_motion(&self) -> (f64, [f32; 4]) {
        let t = self.options.theme.as_deref();
        let ms = t.and_then(|t| t.tokens.motion.get("standard").copied()).unwrap_or(STANDARD_MS);
        let curve = t.and_then(|t| t.tokens.ease.get("standard")).filter(|e| e.len() == 4).map_or(STANDARD_EASE, |e| [e[0] as f32, e[1] as f32, e[2] as f32, e[3] as f32]);
        (ms, curve)
    }

    /// The view at `now`, finishing a motion that ended (and reporting it).
    fn advance(&mut self, now: Instant, cx: &mut Context<Self>) -> bool {
        let Some(anim) = self.anim else { return false };
        let f = self.frame();
        match anim {
            Anim::Tween { from, to, start, ms } => {
                let t = (now.saturating_duration_since(start).as_secs_f64() * 1000.0 / ms.max(1.0)).clamp(0.0, 1.0);
                if t >= 1.0 {
                    self.view = to;
                    self.anim = None;
                } else {
                    let (_, curve) = self.standard_motion();
                    self.view = lerp_view(from, to, ease(curve)(t as f32) as f64);
                }
            }
            Anim::Momentum { from, vx, vy, start } => {
                let t = now.saturating_duration_since(start).as_secs_f64() * 1000.0;
                let next = z::zoom_momentum(from, vx, vy, t, &f);
                let stuck = same_view(next, self.view) && t > 16.0;
                self.view = next;
                if !z::zoom_momentum_active(vx, vy, t) || stuck {
                    self.anim = None;
                }
            }
        }
        if self.anim.is_none() {
            self.report(cx);
        }
        self.anim.is_some()
    }

    /// Move to `to` with the standard motion (snapped under reduced motion)
    /// and report the zoom once it settles.
    fn zoom_settled(&mut self, to: ZoomView, _window: &mut Window, cx: &mut Context<Self>) {
        self.tween_to(to, cx);
        self.activity(cx);
        if self.anim.is_none() {
            self.report(cx);
        }
        cx.notify();
    }

    fn tween_to(&mut self, to: ZoomView, cx: &mut Context<Self>) {
        let now = Self::now(cx);
        self.advance(now, cx);
        let (ms, _) = self.standard_motion();
        if self.options.reduced_motion || same_view(self.view, to) {
            self.view = to;
            self.anim = None;
        } else {
            self.anim = Some(Anim::Tween { from: self.view, to, start: now, ms });
        }
    }

    /// The `zoom` event when the settled scale moved.
    fn report(&mut self, cx: &mut Context<Self>) {
        if (self.view.scale - self.reported).abs() < 1e-6 {
            return;
        }
        self.reported = self.view.scale;
        let at_fit = z::zoom_at_rest(self.view, &self.frame());
        cx.emit(ZoomableImageEvent::Zoom { scale: self.view.scale, at_fit });
    }

    /// Activity inside (a pointer move, a zoom, focus): the `auto` pill
    /// shows and its hide timer restarts.
    fn activity(&mut self, cx: &mut Context<Self>) {
        if self.options.controls != ZoomControls::Auto {
            return;
        }
        if !self.pill_shown {
            self.pill_shown = true;
            cx.notify();
        }
        self.schedule_hide(cx);
    }

    fn schedule_hide(&mut self, cx: &mut Context<Self>) {
        if self.options.controls != ZoomControls::Auto {
            self.hide_task = None;
            return;
        }
        let after = Duration::from_millis(ZOOM.auto_hide_ms as u64);
        self.hide_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(after).await;
            let _ = this.update(cx, |this, cx| {
                this.hide_task = None;
                // Kept while hovered, focused or its Menu is open; the next
                // activity re-arms the timer.
                if this.pill_hovered || this.menu_open || this.was_focused {
                    return;
                }
                if this.pill_shown {
                    this.pill_shown = false;
                    cx.notify();
                }
            });
        }));
    }

    /// The pointer's place in the box (`None` before the first paint).
    fn local(&self, at: Point<Pixels>) -> Option<ZoomPoint> {
        let b = self.measured.get()?;
        Some(ZoomPoint { x: f64::from(f32::from(at.x - b.origin.x)), y: f64::from(f32::from(at.y - b.origin.y)) })
    }

    // -- Input ----------------------------------------------------------------

    fn on_wheel(&mut self, ev: &ScrollEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.has_frame() || self.press.is_some() {
            return;
        }
        let f = self.frame();
        let now = Self::now(cx);
        self.advance(now, cx);
        self.anim = None;
        // gpui's deltas point the content's way (a wheel up = +y); the core
        // takes the web's (deltaY > 0 = down).
        let zoom = ev.modifiers.platform || ev.modifiers.control;
        if zoom {
            let Some(anchor) = self.local(ev.position) else { return };
            let (dy, mode) = match ev.delta {
                ScrollDelta::Pixels(p) => (-f64::from(f32::from(p.y)), WheelDeltaMode::Pixel),
                ScrollDelta::Lines(l) => (-(l.y as f64), WheelDeltaMode::Line),
            };
            cx.stop_propagation();
            self.view = z::zoom_wheel(self.view, dy, mode, anchor, &f);
            self.activity(cx);
            self.settle_later(cx);
            cx.notify();
            return;
        }
        let d = ev.delta.pixel_delta(px(ZOOM.wheel_line_px as f32));
        let next = z::zoom_scroll(self.view, -f64::from(f32::from(d.x)), -f64::from(f32::from(d.y)), &f);
        if same_view(next, self.view) {
            // No room (or at the edge): the host scrolls / pages.
            return;
        }
        cx.stop_propagation();
        self.view = next;
        self.activity(cx);
        cx.notify();
    }

    /// The `zoom` event once a wheel burst stopped.
    fn settle_later(&mut self, cx: &mut Context<Self>) {
        self.settle_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(WHEEL_SETTLE).await;
            let _ = this.update(cx, |this, cx| {
                this.settle_task = None;
                this.report(cx);
            });
        }));
    }

    fn on_pinch(&mut self, ev: &PinchEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.has_frame() {
            return;
        }
        cx.stop_propagation();
        let f = self.frame();
        let anchor = self.local(ev.position).unwrap_or_else(|| z::zoom_centre(&f));
        let now = Self::now(cx);
        match ev.phase {
            TouchPhase::Ended | TouchPhase::Cancelled => {
                self.pinch = None;
                let to = z::zoom_clamp(self.view, &f);
                self.tween_to(to, cx);
                if self.anim.is_none() {
                    self.report(cx);
                }
            }
            phase => {
                if phase == TouchPhase::Started || self.pinch.is_none() {
                    self.advance(now, cx);
                    self.anim = None;
                    self.pinch = Some((self.view, 1.0));
                }
                if let Some((start, factor)) = self.pinch.as_mut() {
                    *factor *= (1.0 + ev.delta as f64).max(0.01);
                    let (start, factor) = (*start, *factor);
                    self.view = z::zoom_pinch(start, factor, anchor, &f);
                }
            }
        }
        self.activity(cx);
        cx.notify();
    }

    fn on_down(&mut self, ev: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        self.focus.focus(window, cx);
        if self.menu_open {
            self.menu_open = false;
        }
        if ev.click_count >= 2 {
            self.tap_task = None;
        }
        if !self.has_frame() {
            return;
        }
        let now = Self::now(cx);
        self.advance(now, cx);
        self.anim = None;
        let room = z::zoom_pan_room(self.view, &self.frame());
        self.press = Some(Press { origin: ev.position, start: self.view, dragging: false, pans: room.x || room.y, samples: vec![(now, ev.position)] });
        cx.notify();
    }

    fn on_move(&mut self, ev: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some(press) = self.press.as_mut() else { return };
        if ev.pressed_button != Some(MouseButton::Left) {
            return;
        }
        let now = cx.background_executor().now();
        let d = ev.position - press.origin;
        let (dx, dy) = (f32::from(d.x), f32::from(d.y));
        if !press.dragging && dx.hypot(dy) >= CLICK_SLOP {
            press.dragging = true;
            self.tap_task = None;
        }
        press.samples.push((now, ev.position));
        press.samples.retain(|(t, _)| now.saturating_duration_since(*t).as_secs_f64() * 1000.0 <= VELOCITY_WINDOW_MS * 2.0);
        if press.dragging && press.pans {
            let (start, pdx, pdy) = (press.start, dx as f64, dy as f64);
            self.view = z::zoom_drag(start, pdx, pdy, &self.frame());
            cx.notify();
        }
    }

    fn on_up(&mut self, ev: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(press) = self.press.take() else { return };
        let f = self.frame();
        let now = Self::now(cx);
        if press.dragging {
            if press.pans {
                let clamped = z::zoom_clamp(self.view, &f);
                if !same_view(clamped, self.view) {
                    self.tween_to(clamped, cx);
                } else if let Some((vx, vy)) = Self::velocity(&press.samples, now) {
                    if !self.options.reduced_motion && z::zoom_momentum_active(vx, vy, 0.0) {
                        self.anim = Some(Anim::Momentum { from: self.view, vx, vy, start: now });
                    }
                }
            }
            cx.notify();
            return;
        }
        // A click.
        let Some(at) = self.local(ev.position) else { return };
        if ev.click_count >= 2 {
            self.tap_task = None;
            let to = z::zoom_double_tap(self.view, at, &f);
            self.zoom_settled(to, window, cx);
            return;
        }
        if z::zoom_at_rest(self.view, &f) {
            self.tap_task = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(TAP_DELAY).await;
                let _ = this.update(cx, |this, cx| {
                    this.tap_task = None;
                    cx.emit(ZoomableImageEvent::Tap);
                });
            }));
        }
    }

    /// The pointer's velocity (px/ms) over the last [`VELOCITY_WINDOW_MS`].
    fn velocity(samples: &[(Instant, Point<Pixels>)], now: Instant) -> Option<(f64, f64)> {
        let recent: Vec<&(Instant, Point<Pixels>)> = samples.iter().filter(|(t, _)| now.saturating_duration_since(*t).as_secs_f64() * 1000.0 <= VELOCITY_WINDOW_MS).collect();
        let (first, last) = (recent.first()?, recent.last()?);
        let dt = last.0.saturating_duration_since(first.0).as_secs_f64() * 1000.0;
        if dt < 1.0 {
            return None;
        }
        let d = last.1 - first.1;
        Some((f64::from(f32::from(d.x)) / dt, f64::from(f32::from(d.y)) / dt))
    }

    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let m = ev.keystroke.modifiers;
        if m.platform || m.control || m.alt {
            return;
        }
        if self.menu_open && ev.keystroke.key == "escape" {
            self.menu_open = false;
            cx.stop_propagation();
            cx.notify();
            return;
        }
        let key = ev.keystroke.key_char.as_deref().filter(|k| !k.is_empty()).unwrap_or(ev.keystroke.key.as_str()).to_string();
        if self.handle_key(&key, window, cx) {
            cx.stop_propagation();
        }
    }

    // -- Paint ----------------------------------------------------------------

    fn part(&self, part: &str, states: &[&str]) -> (PaintStyle, Map<String, Value>) {
        let mut owner = Map::new();
        let controls = match self.options.controls {
            ZoomControls::Auto => "auto",
            ZoomControls::Always => "always",
            ZoomControls::None => "none",
        };
        owner.insert("controls".into(), json!(controls));
        let states: Vec<String> = states.iter().map(|s| s.to_string()).collect();
        let props = part_props(self.options.theme.as_deref(), self.options.mode, "ZoomableImage", part, &owner, &states);
        (PaintStyle::from_visual(&eui_core::style::visual(&props, eui_core::style::BoxKind::Leaf)), props)
    }

    fn menu_part(&self, part: &str, states: &[&str]) -> (PaintStyle, Map<String, Value>) {
        let mut owner = Map::new();
        owner.insert("openOn".into(), json!("press"));
        let states: Vec<String> = states.iter().map(|s| s.to_string()).collect();
        let props = part_props(self.options.theme.as_deref(), self.options.mode, "Menu", part, &owner, &states);
        (PaintStyle::from_visual(&eui_core::style::visual(&props, eui_core::style::BoxKind::Leaf)), props)
    }

    fn ink(&self, s: &PaintStyle) -> Hsla {
        s.color.unwrap_or_else(|| crate::paint::parts::default_ink(self.options.theme.as_deref(), self.options.mode))
    }

    fn text_font(&self, props: &Map<String, Value>) -> Option<Font> {
        let family = props.get("fontFamily").and_then(Value::as_str).map(|f| SharedString::from(f.to_string())).or_else(|| self.options.font_family.clone())?;
        let weight = props.get("fontWeight").and_then(Value::as_f64).unwrap_or(400.0) as u16;
        Some(crate::measure::with_tabular_nums(crate::measure::make_font(family, weight)))
    }

    /// One pill button (`zoomOut`, `zoomIn`, `fit`, `percent`).
    fn pill_button(&self, part: ZoomPart, disabled: bool, selected: bool, content: impl FnOnce(Hsla, &Map<String, Value>) -> AnyElement, extra: Option<AnyElement>, cx: &mut Context<Self>) -> AnyElement {
        let mut states = Vec::new();
        if disabled {
            states.push("disabled");
        } else if self.hovered_part == Some(part) {
            states.push("hover");
        }
        if selected {
            states.push("selected");
        }
        let (s, props) = self.part(part.name(), &states);
        let ink = self.ink(&s);
        let h = px_prop(&props, "height").unwrap_or(28.0);
        let w = px_prop(&props, "width");
        let label = match part {
            ZoomPart::ZoomOut => self.options.strings.zoom_out.clone(),
            ZoomPart::ZoomIn => self.options.strings.zoom_in.clone(),
            ZoomPart::Fit => self.options.strings.zoom_to_fit.clone(),
            ZoomPart::Percent => self.options.strings.zoom_level.clone(),
        };
        let mut el = div()
            .id(ElementId::Name(SharedString::from(format!("zoomable-image.{}", part.name()))))
            .debug_selector(|| format!("zoomable-image.{}", part.name()))
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .h(px(h))
            .role(gpui::Role::Button)
            .aria_label(label);
        el = match w {
            Some(w) => el.w(px(w)),
            None => el.min_w(px(ZOOM.pill.percent_min_width as f32)).px(px(s.padding[1])),
        };
        el = styled_box(el, &s, w.unwrap_or(ZOOM.pill.percent_min_width as f32), h);
        if part == ZoomPart::Fit {
            el = el.aria_toggled(if selected { gpui::Toggled::True } else { gpui::Toggled::False });
        }
        if part == ZoomPart::Percent {
            el = el.aria_expanded(self.menu_open).aria_value(SharedString::from(z::zoom_percent(self.target_view().scale)));
        }
        let el = el.child(content(ink, &props)).children(extra).on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
            let next = if *hovered { Some(part) } else if this.hovered_part == Some(part) { None } else { this.hovered_part };
            if next != this.hovered_part {
                this.hovered_part = next;
                cx.notify();
            }
        }));
        if disabled {
            return el.into_any_element();
        }
        el.cursor(CursorStyle::PointingHand)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                this.press_part(part, window, cx);
            }))
            .into_any_element()
    }

    fn icon(&self, name: &str, ink: Hsla) -> AnyElement {
        icons::concept(self.options.host.as_ref(), name, 16.0, ink)
    }

    fn pill(&self, cx: &mut Context<Self>) -> AnyElement {
        let view = self.target_view();
        let f = self.frame();
        let b = z::zoom_bounds(view, &f);
        let at_rest = z::zoom_at_rest(view, &f);
        let (s, _) = self.part("pill", &[]);
        let spec = &ZOOM.pill;
        let (divider_style, divider) = self.part("divider", &[]);
        let dw = px_prop(&divider, "width").unwrap_or(1.0);
        let dh = px_prop(&divider, "height").unwrap_or(spec.divider_height as f32);
        let percent = SharedString::from(z::zoom_percent(view.scale));
        let font_size = |p: &Map<String, Value>| px_prop(p, "fontSize").unwrap_or(14.0);
        let minus = self.pill_button(ZoomPart::ZoomOut, b.at_min, false, |ink, _| self.icon("ui-minus", ink), None, cx);
        let label = {
            let font = self.text_font(&self.part("percent", &[]).1);
            let menu = self.menu_open.then(|| self.menu(cx));
            self.pill_button(
                ZoomPart::Percent,
                false,
                false,
                move |ink, props| {
                    let weight = props.get("fontWeight").and_then(Value::as_f64).unwrap_or(500.0) as f32;
                    let mut t = div().text_size(px(font_size(props))).font_weight(FontWeight(weight)).text_color(ink).whitespace_nowrap().child(percent);
                    if let Some(lh) = px_prop(props, "lineHeight") {
                        t = t.line_height(px(lh));
                    }
                    if let Some(font) = font {
                        t = t.font(font);
                    }
                    t.into_any_element()
                },
                menu,
                cx,
            )
        };
        let plus = self.pill_button(ZoomPart::ZoomIn, b.at_max, false, |ink, _| self.icon("ui-add", ink), None, cx);
        let line = styled_box(div().flex_none().w(px(dw)).h(px(dh)).mx(px(spec.gap as f32)), &divider_style, dw, dh);
        let fit = self.pill_button(ZoomPart::Fit, false, at_rest, |ink, _| self.icon("ui-fullscreen", ink), None, cx);
        let pad = if s.padding.iter().any(|p| *p > 0.0) { s.padding[0] } else { spec.padding as f32 };
        let gap = if s.gap > 0.0 { s.gap } else { spec.gap as f32 };
        let mut row = div().id("zoomable-image.pill").debug_selector(|| "zoomable-image.pill".into()).flex().items_center().h(px(spec.height as f32)).p(px(pad)).gap(px(gap));
        row = if self.options.rtl { row.flex_row_reverse() } else { row.flex_row() };
        let row = styled_box(row, &s, 200.0, spec.height as f32)
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                this.pill_hovered = *hovered;
                if !*hovered {
                    this.hovered_part = None;
                    this.schedule_hide(cx);
                }
                cx.notify();
            }))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(minus)
            .child(label)
            .child(plus)
            .child(line)
            .child(fit);
        row.into_any_element()
    }

    /// THE Menu of `zoom_menu` above the percent button, painted from the
    /// theme's `Menu` parts (`content`, `item`, `check`).
    fn menu(&self, cx: &mut Context<Self>) -> AnyElement {
        let items = z::zoom_menu(self.target_view(), &self.frame(), &self.options.strings.zoom_fit);
        let (content, _) = self.menu_part("content", &[]);
        let mut list = div().id("zoomable-image.menu").flex().flex_col().min_w(px(120.0)).role(gpui::Role::Menu).occlude();
        let pad = content.padding[0];
        list = styled_box(list.p(px(pad)), &content, 120.0, 200.0).on_mouse_down_out(cx.listener(|this, _, _, cx| {
            this.menu_open = false;
            cx.notify();
        }));
        for (i, item) in items.into_iter().enumerate() {
            let (s, props) = self.menu_part("item", if item.checked { &["selected"] } else { &[] });
            let (_, hover_props) = self.menu_part("item", &["hover"]);
            let hover_bg = PaintStyle::from_visual(&eui_core::style::visual(&hover_props, eui_core::style::BoxKind::Leaf)).bg;
            let ink = self.ink(&s);
            let h = px_prop(&props, "height").unwrap_or(24.0).max(28.0);
            let (check, _) = self.menu_part("check", &[]);
            let mut row = div()
                .id(ElementId::Name(SharedString::from(format!("zoomable-image.menu.{i}"))))
                .debug_selector(|| format!("zoomable-image.menu.{i}"))
                .flex()
                .flex_row()
                .items_center()
                .gap(px(s.gap))
                .h(px(h))
                .px(px(s.padding[1]))
                .text_size(px(px_prop(&props, "fontSize").unwrap_or(14.0)))
                .text_color(ink)
                .cursor(CursorStyle::PointingHand)
                .role(gpui::Role::MenuItemCheckBox)
                .aria_toggled(if item.checked { gpui::Toggled::True } else { gpui::Toggled::False })
                .aria_label(SharedString::from(item.label.clone()));
            row = styled_box(row, &s, 120.0, h);
            if let Some(bg) = hover_bg {
                row = row.hover(move |st| st.bg(bg));
            }
            let check_ink = check.color.unwrap_or(ink);
            let mark = if item.checked { self.icon("ui-check", check_ink) } else { div().size(px(16.0)).into_any_element() };
            let row = row
                .child(mark)
                .child(div().flex_1().whitespace_nowrap().child(SharedString::from(item.label)))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.pick_menu(i, window, cx);
                }));
            list = list.child(row);
        }
        div().absolute().top_0().left_0().child(deferred(anchored().anchor(Anchor::BottomLeft).child(div().mb(px(8.0)).child(list))).with_priority(1)).into_any_element()
    }
}

impl Render for ZoomableImage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The box measured at the last paint (a host resize, the first paint).
        if let Some(b) = self.measured.get() {
            self.resize(ZoomSize { width: f64::from(f32::from(b.size.width)), height: f64::from(f32::from(b.size.height)) });
        }
        let focused = self.focus.is_focused(window);
        if focused != self.was_focused {
            self.was_focused = focused;
            if focused {
                self.activity(cx);
                cx.emit(ZoomableImageEvent::Focus);
            } else {
                self.schedule_hide(cx);
            }
        }
        // The decoded picture (its size when the author gave none).
        let data = self.image.as_ref().map(|s| render_image(s, window, cx));
        // Drawn only once decoded (pending = `Some(None)`, failed = `Err`).
        let loaded = matches!(data, Some(Some(Ok(_))));
        if self.given.is_none() {
            if let Some(Some(Ok(d))) = &data {
                let size = d.size(0);
                let decoded = ZoomSize { width: size.width.0 as f64, height: size.height.0 as f64 };
                if self.decoded != Some(decoded) {
                    self.decoded = Some(decoded);
                    self.reset_view();
                }
            }
        }
        let now = Self::now(cx);
        if self.advance(now, cx) {
            window.request_animation_frame();
        }
        let view = self.view;
        let natural = self.natural();
        let (image_style, _) = self.part("image", &[]);
        let radii = image_style.radii;
        let zoomed = !z::zoom_at_rest(view, &self.frame());
        let room = z::zoom_pan_room(view, &self.frame());
        // gpui has no zoom-in cursor: a pointing hand at rest (a click / a
        // double click acts), the hands when the picture pans.
        let cursor = match &self.press {
            Some(p) if p.dragging && p.pans => CursorStyle::ClosedHand,
            _ if zoomed || room.x || room.y => CursorStyle::OpenHand,
            _ => CursorStyle::PointingHand,
        };
        let measured = self.measured.clone();
        let ink = crate::paint::parts::default_ink(self.options.theme.as_deref(), self.options.mode);
        let picture = canvas(
            move |bounds, _, _| {
                let before = measured.get();
                measured.set(Some(bounds));
                before
            },
            move |bounds, before, window, _| {
                if before.map(|b| b.size) != Some(bounds.size) {
                    // The maths needs the new box: lay out again.
                    window.refresh();
                }
                let (Some(Some(Ok(data))), Some(natural)) = (data, natural) else { return };
                let image_bounds = Bounds { origin: bounds.origin + point(px(view.x as f32), px(view.y as f32)), size: gpui::size(px((natural.width * view.scale) as f32), px((natural.height * view.scale) as f32)) };
                let corners = Corners { top_left: px(radii[0]), top_right: px(radii[1]), bottom_right: px(radii[2]), bottom_left: px(radii[3]) };
                let _ = window.paint_image(bounds, image_bounds, corners, data, 0, false);
            },
        )
        .absolute()
        .size_full();
        let mut root = div()
            .id(ElementId::Name(SharedString::from(format!("zoomable-image:{}", self.key))))
            .relative()
            .size_full()
            .overflow_hidden()
            .track_focus(&self.focus)
            .key_context("ZoomableImage")
            .role(gpui::Role::Image)
            .aria_label(self.options.alt.clone())
            .cursor(cursor)
            .on_key_down(cx.listener(Self::on_key))
            .on_scroll(cx.listener(Self::on_wheel))
            .on_pinch(cx.listener(Self::on_pinch))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_down))
            .on_mouse_move(cx.listener(|this, _: &MouseMoveEvent, _, cx| {
                if this.press.is_none() {
                    this.activity(cx);
                }
            }))
            .child(picture);
        // Pending or failed (style.json zoomableImage): the box keeps its
        // size; the picture area is filled (mutedForeground 8%) and centres
        // the image glyph (20, mutedForeground 60%) over the alt text
        // (12/16, 400, one line, ellipsis), gap 4. No pill: nothing to zoom.
        if !loaded {
            let muted = crate::paint::parts::theme_color(self.options.theme.as_deref(), self.options.mode, "mutedForeground").unwrap_or(ink.opacity(0.6));
            let faint = muted.opacity(0.6);
            let mut placeholder = div().absolute().size_full().flex().flex_col().items_center().justify_center().gap(px(4.0)).bg(muted.opacity(0.08)).rounded_tl(px(radii[0])).rounded_tr(px(radii[1])).rounded_br(px(radii[2])).rounded_bl(px(radii[3])).child(icons::concept(self.options.host.as_ref(), "image", 20.0, faint));
            if !self.options.alt.is_empty() {
                let mut text = div().max_w_full().min_w_0().overflow_hidden().text_ellipsis().whitespace_nowrap().text_size(px(12.0)).line_height(px(16.0)).font_weight(FontWeight(400.0)).text_color(faint);
                if let Some(family) = self.options.font_family.clone() {
                    text = text.font_family(family);
                }
                placeholder = placeholder.child(text.child(self.options.alt.clone()));
            }
            root = root.child(placeholder);
        }
        // A press in flight follows the pointer anywhere in the window.
        if self.press.is_some() {
            let this = cx.entity().downgrade();
            root = root.child(
                canvas(
                    |_, _, _| {},
                    move |_, _, window, _| {
                        let (a, b) = (this.clone(), this.clone());
                        window.on_mouse_event(move |ev: &MouseMoveEvent, phase, _, cx| {
                            if phase == DispatchPhase::Capture {
                                let _ = a.update(cx, |v, cx| v.on_move(ev, cx));
                            }
                        });
                        window.on_mouse_event(move |ev: &MouseUpEvent, phase, window, cx| {
                            if phase == DispatchPhase::Capture && ev.button == MouseButton::Left {
                                let _ = b.update(cx, |v, cx| v.on_up(ev, window, cx));
                            }
                        });
                    },
                )
                .absolute()
                .size_0(),
            );
        }
        if loaded && self.pill_visible() && self.has_frame() {
            let inset = ZOOM.pill.inset as f32;
            root = root.child(div().absolute().left_0().right_0().bottom(px(inset)).flex().justify_center().child(self.pill(cx)));
        }
        root
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controls_parse_like_the_catalog_enum() {
        assert_eq!(ZoomControls::parse("always"), ZoomControls::Always);
        assert_eq!(ZoomControls::parse("none"), ZoomControls::None);
        assert_eq!(ZoomControls::parse("auto"), ZoomControls::Auto);
        assert_eq!(ZoomControls::parse("bogus"), ZoomControls::Auto);
    }

    #[test]
    fn the_default_strings_are_the_catalogs() {
        let s = ZoomStrings::default();
        assert_eq!(s.zoom_in.as_ref(), "Zoom in");
        assert_eq!(s.zoom_fit.as_ref(), "Fit");
    }
}
