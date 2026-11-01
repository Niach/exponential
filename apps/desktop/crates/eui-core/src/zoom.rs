//! EXP-1586: the ZoomableImage native's maths (catalog/style.json
//! `zoomableImage`, whose $comment is the spec). Pure: every painter feeds
//! its pointer, wheel, pinch and key input through these functions, so a
//! zoom, a pan or a double tap lands on the same view everywhere. Mirrors
//! `src/zoom.ts`; `fixtures/zoom.json` locks it.

use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

use crate::generated::catalog as g;

/// The pill's geometry (style.json `zoomableImage.pill`).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoomPillSpec {
    pub height: f64,
    pub inset: f64,
    pub padding: f64,
    pub gap: f64,
    pub percent_min_width: f64,
    pub divider_height: f64,
}

/// style.json `zoomableImage`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoomSpec {
    pub tall_aspect: f64,
    pub max_scale: f64,
    pub presets: Vec<f64>,
    pub menu_presets: Vec<f64>,
    pub double_tap_factor: f64,
    pub wheel_rate: f64,
    pub wheel_line_px: f64,
    pub rubber: f64,
    pub momentum_tau_ms: f64,
    pub auto_hide_ms: f64,
    pub pill: ZoomPillSpec,
}

pub static ZOOM: LazyLock<ZoomSpec> = LazyLock::new(|| {
    let style: serde_json::Value = serde_json::from_str(g::STYLE_JSON).expect("style.json");
    serde_json::from_value(style["zoomableImage"].clone()).expect("style.json zoomableImage")
});

/// The `initial` prop (catalog enum `zoomInitial`); anything else = `Auto`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", from = "String")]
pub enum ZoomInitial {
    #[default]
    Auto,
    Fit,
    FitWidth,
    Actual,
}

impl From<String> for ZoomInitial {
    fn from(s: String) -> Self {
        Self::parse(&s)
    }
}

impl ZoomInitial {
    pub fn parse(s: &str) -> Self {
        match s {
            "fit" => Self::Fit,
            "fitWidth" => Self::FitWidth,
            "actual" => Self::Actual,
            _ => Self::Auto,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct ZoomSize {
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct ZoomPoint {
    pub x: f64,
    pub y: f64,
}

/// scale = displayed px per image px (1 = 100%); x/y = the picture's
/// top-left in the viewport.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct ZoomView {
    pub scale: f64,
    pub x: f64,
    pub y: f64,
}

/// What every function needs to know about the picture and its box.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoomFrame {
    pub natural: ZoomSize,
    pub viewport: ZoomSize,
    #[serde(default)]
    pub initial: ZoomInitial,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_scale: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ZoomLimits {
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZoomPanRoom {
    pub x: bool,
    pub y: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoomBounds {
    pub at_min: bool,
    pub at_max: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ZoomMenuItem {
    /// None = Fit (the rest view).
    pub scale: Option<f64>,
    pub label: String,
    pub checked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WheelDeltaMode {
    Pixel,
    Line,
}

const EPS: f64 = 1e-6;

/// Rounded to 1e-6 so every platform prints the same numbers.
fn tidy(v: f64) -> f64 {
    let r = (v * 1e6).round() / 1e6;
    if r == 0.0 {
        0.0
    } else {
        r
    }
}

fn tidy_view(v: ZoomView) -> ZoomView {
    ZoomView { scale: tidy(v.scale), x: tidy(v.x), y: tidy(v.y) }
}

fn clamp_n(v: f64, lo: f64, hi: f64) -> f64 {
    lo.max(hi.min(v))
}

fn positive(v: f64) -> f64 {
    if v.is_finite() && v > 0.0 {
        v
    } else {
        0.0
    }
}

struct Sizes {
    iw: f64,
    ih: f64,
    vw: f64,
    vh: f64,
}

fn sizes(f: &ZoomFrame) -> Sizes {
    Sizes {
        iw: positive(f.natural.width),
        ih: positive(f.natural.height),
        vw: positive(f.viewport.width),
        vh: positive(f.viewport.height),
    }
}

/// A picture taller than 1 / tallAspect times its width.
pub fn zoom_is_tall(natural: ZoomSize) -> bool {
    let w = positive(natural.width);
    let h = positive(natural.height);
    w > 0.0 && h > 0.0 && w / h < ZOOM.tall_aspect
}

fn fit_raw(f: &ZoomFrame) -> f64 {
    let s = sizes(f);
    if s.iw == 0.0 || s.ih == 0.0 || s.vw == 0.0 || s.vh == 0.0 {
        return 1.0;
    }
    1f64.min(s.vw / s.iw).min(s.vh / s.ih)
}

fn limits_raw(f: &ZoomFrame) -> (f64, f64) {
    let min = fit_raw(f);
    let wanted = match f.max_scale.map(positive) {
        Some(v) if v > 0.0 => v,
        _ => ZOOM.max_scale,
    };
    (min, min.max(wanted))
}

fn rest_raw(f: &ZoomFrame) -> f64 {
    let s = sizes(f);
    let (min, max) = limits_raw(f);
    let mode = match f.initial {
        ZoomInitial::Auto if zoom_is_tall(f.natural) => ZoomInitial::FitWidth,
        ZoomInitial::Auto => ZoomInitial::Fit,
        other => other,
    };
    let scale = match mode {
        ZoomInitial::FitWidth => {
            if s.iw > 0.0 && s.vw > 0.0 {
                1f64.min(s.vw / s.iw)
            } else {
                1.0
            }
        }
        ZoomInitial::Actual => 1.0,
        _ => min,
    };
    clamp_n(scale, min, max)
}

/// The whole picture inside the viewport, never upscaled: the minimum.
pub fn zoom_fit_scale(f: &ZoomFrame) -> f64 {
    tidy(fit_raw(f))
}

/// The scale range: [fit, max(fit, maxScale)].
pub fn zoom_limits(f: &ZoomFrame) -> ZoomLimits {
    let (min, max) = limits_raw(f);
    ZoomLimits { min: tidy(min), max: tidy(max) }
}

/// The scale the picture opens at (and a double tap / `0` / the fit button
/// returns to), inside the limits.
pub fn zoom_rest_scale(f: &ZoomFrame) -> f64 {
    tidy(rest_raw(f))
}

/// Clamp one axis: centred when the scaled picture fits, else no gap.
fn clamp_axis(offset: f64, scaled: f64, extent: f64) -> f64 {
    if scaled <= extent + EPS {
        (extent - scaled) / 2.0
    } else {
        clamp_n(offset, extent - scaled, 0.0)
    }
}

/// The view inside its bounds (scale within the limits, no letterbox).
pub fn zoom_clamp(view: ZoomView, f: &ZoomFrame) -> ZoomView {
    let s = sizes(f);
    let (min, max) = limits_raw(f);
    let scale = clamp_n(view.scale, min, max);
    tidy_view(ZoomView {
        scale,
        x: clamp_axis(view.x, s.iw * scale, s.vw),
        y: clamp_axis(view.y, s.ih * scale, s.vh),
    })
}

/// The opening view: the rest scale, centred; a taller-than-the-viewport
/// picture starts at its top.
pub fn zoom_rest_view(f: &ZoomFrame) -> ZoomView {
    let s = sizes(f);
    let scale = rest_raw(f);
    zoom_clamp(ZoomView { scale, x: (s.vw - s.iw * scale) / 2.0, y: 0.0 }, f)
}

/// Whether the view sits at its rest scale.
pub fn zoom_at_rest(view: ZoomView, f: &ZoomFrame) -> bool {
    (view.scale - rest_raw(f)).abs() < 1e-4
}

/// Zoom to `scale` (clamped) keeping the image point under `anchor` fixed.
pub fn zoom_to(view: ZoomView, scale: f64, anchor: ZoomPoint, f: &ZoomFrame) -> ZoomView {
    let (min, max) = limits_raw(f);
    let next = clamp_n(scale, min, max);
    let ratio = if view.scale > 0.0 { next / view.scale } else { 1.0 };
    zoom_clamp(
        ZoomView {
            scale: next,
            x: anchor.x - (anchor.x - view.x) * ratio,
            y: anchor.y - (anchor.y - view.y) * ratio,
        },
        f,
    )
}

/// The viewport's centre, the anchor of keys and the pill.
pub fn zoom_centre(f: &ZoomFrame) -> ZoomPoint {
    let s = sizes(f);
    ZoomPoint { x: s.vw / 2.0, y: s.vh / 2.0 }
}

/// ⌘/ctrl + wheel (or a trackpad pinch delivered as a wheel): the scale
/// factor of one wheel event.
pub fn zoom_wheel_factor(delta_y: f64, mode: WheelDeltaMode) -> f64 {
    let px = match mode {
        WheelDeltaMode::Line => delta_y * ZOOM.wheel_line_px,
        WheelDeltaMode::Pixel => delta_y,
    };
    (-px * ZOOM.wheel_rate).exp()
}

/// One ⌘/ctrl-wheel event about the cursor.
pub fn zoom_wheel(view: ZoomView, delta_y: f64, mode: WheelDeltaMode, anchor: ZoomPoint, f: &ZoomFrame) -> ZoomView {
    zoom_to(view, view.scale * zoom_wheel_factor(delta_y, mode), anchor, f)
}

/// A plain wheel / two-finger scroll: pans by the deltas, clamped.
pub fn zoom_scroll(view: ZoomView, delta_x: f64, delta_y: f64, f: &ZoomFrame) -> ZoomView {
    zoom_clamp(ZoomView { scale: view.scale, x: view.x - delta_x, y: view.y - delta_y }, f)
}

/// The ladder steps walk: the presets plus the rest scale, inside the
/// limits, ascending.
pub fn zoom_ladder(f: &ZoomFrame) -> Vec<f64> {
    let (min, max) = limits_raw(f);
    let rest = rest_raw(f);
    let mut all: Vec<f64> = ZOOM.presets.iter().copied().chain([min, rest, max]).filter(|s| *s >= min - EPS && *s <= max + EPS).collect();
    all.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mut out: Vec<f64> = Vec::new();
    for s in all {
        if out.last().is_none_or(|last| s - last > 1e-4) {
            out.push(tidy(s));
        }
    }
    out
}

/// The next ladder scale strictly above (`1`) or below (`-1`) the current
/// one, or the current one at the end.
pub fn zoom_step_scale(scale: f64, direction: i32, f: &ZoomFrame) -> f64 {
    let ladder = zoom_ladder(f);
    if direction > 0 {
        return ladder.iter().copied().find(|s| *s > scale + 1e-4).unwrap_or(*ladder.last().unwrap_or(&scale));
    }
    ladder.iter().rev().copied().find(|s| *s < scale - 1e-4).unwrap_or(*ladder.first().unwrap_or(&scale))
}

/// + / − (keys and the pill) about the viewport centre.
pub fn zoom_step(view: ZoomView, direction: i32, f: &ZoomFrame) -> ZoomView {
    zoom_to(view, zoom_step_scale(view.scale, direction, f), zoom_centre(f), f)
}

/// The scale a double tap at rest jumps to.
pub fn zoom_double_tap_scale(f: &ZoomFrame) -> f64 {
    let (_, max) = limits_raw(f);
    tidy(max.min(1f64.max(ZOOM.double_tap_factor * rest_raw(f))))
}

/// Double click / tap: at rest → zoom in about the point; else → rest.
pub fn zoom_double_tap(view: ZoomView, point: ZoomPoint, f: &ZoomFrame) -> ZoomView {
    if zoom_at_rest(view, f) {
        zoom_to(view, zoom_double_tap_scale(f), point, f)
    } else {
        zoom_rest_view(f)
    }
}

/// A key while the picture has focus; None = not a zoom key.
pub fn zoom_key(view: ZoomView, key: &str, f: &ZoomFrame) -> Option<ZoomView> {
    match key {
        "+" | "=" => Some(zoom_step(view, 1, f)),
        "-" | "−" | "_" => Some(zoom_step(view, -1, f)),
        "0" => Some(zoom_rest_view(f)),
        "1" => Some(zoom_to(view, 1.0, zoom_centre(f), f)),
        _ => None,
    }
}

/// The overshoot past an edge, damped: (1 − 1 / (o·c/d + 1))·d.
pub fn zoom_rubber_band(overshoot: f64, extent: f64) -> f64 {
    if extent <= 0.0 || overshoot == 0.0 {
        return 0.0;
    }
    let sign = if overshoot < 0.0 { -1.0 } else { 1.0 };
    let o = overshoot.abs();
    tidy(sign * (1.0 - 1.0 / ((o * ZOOM.rubber) / extent + 1.0)) * extent)
}

fn rubber_axis(offset: f64, scaled: f64, extent: f64) -> f64 {
    let fits = scaled <= extent + EPS;
    let lo = if fits { (extent - scaled) / 2.0 } else { extent - scaled };
    let hi = if fits { (extent - scaled) / 2.0 } else { 0.0 };
    if offset > hi {
        hi + zoom_rubber_band(offset - hi, extent)
    } else if offset < lo {
        lo + zoom_rubber_band(offset - lo, extent)
    } else {
        offset
    }
}

/// A drag from `start` by (dx, dy) while held: past an edge the overshoot
/// rubber-bands. The release is [`zoom_clamp`] of the last view.
pub fn zoom_drag(start: ZoomView, dx: f64, dy: f64, f: &ZoomFrame) -> ZoomView {
    let s = sizes(f);
    tidy_view(ZoomView {
        scale: start.scale,
        x: rubber_axis(start.x + dx, s.iw * start.scale, s.vw),
        y: rubber_axis(start.y + dy, s.ih * start.scale, s.vh),
    })
}

/// A pinch from `start` by the raw factor about `anchor` while held: past
/// the min/max the scale rubber-bands (d = the limit). The release is
/// [`zoom_clamp`] of the last view.
pub fn zoom_pinch(start: ZoomView, factor: f64, anchor: ZoomPoint, f: &ZoomFrame) -> ZoomView {
    let s = sizes(f);
    let (min, max) = limits_raw(f);
    let raw = start.scale * factor;
    let scale = if raw > max {
        max + zoom_rubber_band(raw - max, max)
    } else if raw < min {
        min + zoom_rubber_band(raw - min, min)
    } else {
        raw
    };
    let ratio = if start.scale > 0.0 { scale / start.scale } else { 1.0 };
    tidy_view(ZoomView {
        scale,
        x: rubber_axis(anchor.x - (anchor.x - start.x) * ratio, s.iw * scale, s.vw),
        y: rubber_axis(anchor.y - (anchor.y - start.y) * ratio, s.ih * scale, s.vh),
    })
}

/// Mouse-drag momentum `t` ms after a release at velocity (px/ms),
/// exponential decay, stopping at the edges.
pub fn zoom_momentum(view: ZoomView, vx: f64, vy: f64, t: f64, f: &ZoomFrame) -> ZoomView {
    let tau = ZOOM.momentum_tau_ms;
    let travel = tau * (1.0 - (-t.max(0.0) / tau).exp());
    zoom_clamp(ZoomView { scale: view.scale, x: view.x + vx * travel, y: view.y + vy * travel }, f)
}

/// Whether momentum `t` ms after a release is still moving (above 0.01 px/ms).
pub fn zoom_momentum_active(vx: f64, vy: f64, t: f64) -> bool {
    let decay = (-t.max(0.0) / ZOOM.momentum_tau_ms).exp();
    vx.hypot(vy) * decay > 0.01
}

/// The axes the picture can pan on: an axis without room hands a swipe to
/// the host (paging).
pub fn zoom_pan_room(view: ZoomView, f: &ZoomFrame) -> ZoomPanRoom {
    let s = sizes(f);
    ZoomPanRoom { x: s.iw * view.scale > s.vw + EPS, y: s.ih * view.scale > s.vh + EPS }
}

/// A new viewport: at rest = the new rest view; zoomed = the same scale
/// about the viewport centre, clamped.
pub fn zoom_resize(view: ZoomView, from: ZoomSize, f: &ZoomFrame) -> ZoomView {
    let before = ZoomFrame { viewport: from, ..*f };
    if zoom_at_rest(view, &before) {
        return zoom_rest_view(f);
    }
    let s = sizes(f);
    let cx = positive(from.width) / 2.0;
    let cy = positive(from.height) / 2.0;
    zoom_clamp(ZoomView { scale: view.scale, x: s.vw / 2.0 - (cx - view.x), y: s.vh / 2.0 - (cy - view.y) }, f)
}

/// The pill's label: round(scale × 100) + `%` (JS `Math.round`: halves up).
pub fn zoom_percent(scale: f64) -> String {
    format!("{}%", (scale * 100.0 + 0.5).floor() as i64)
}

/// The % button's Menu: Fit, then the menu presets inside the limits.
pub fn zoom_menu(view: ZoomView, f: &ZoomFrame, fit_label: &str) -> Vec<ZoomMenuItem> {
    let (min, max) = limits_raw(f);
    let at_rest = zoom_at_rest(view, f);
    let mut items = Vec::new();
    for &s in &ZOOM.menu_presets {
        if s == 0.0 {
            items.push(ZoomMenuItem { scale: None, label: fit_label.to_string(), checked: at_rest });
        } else if s >= min - EPS && s <= max + EPS {
            items.push(ZoomMenuItem { scale: Some(s), label: zoom_percent(s), checked: !at_rest && (view.scale - s).abs() < 1e-4 });
        }
    }
    items
}

/// The pill's disabled ends.
pub fn zoom_bounds(view: ZoomView, f: &ZoomFrame) -> ZoomBounds {
    let (min, max) = limits_raw(f);
    ZoomBounds { at_min: view.scale <= min + 1e-4, at_max: view.scale >= max - 1e-4 }
}

/// THE JSON entry (the FFI's `zoom_json`, the fixture replays ×4): `{op,
/// frame: {natural, viewport, initial?, maxScale?}, from?: view | "rest",
/// …}` with the op named like the fixtures/zoom.json group it answers:
/// `rest`, `clamp {view}`, `zoomTo {scale, anchor}`, `wheel {deltaY, mode,
/// anchor}`, `scroll {deltaX, deltaY}`, `ladder`, `steps {scale,
/// direction}`, `step {direction}` (a view), `doubleTap {point}`, `keys
/// {key}`, `rubberBand {overshoot, extent}`, `drag {dx, dy}`, `pinch
/// {factor, anchor}`, `momentum {vx, vy, t}`, `momentumActive {vx, vy, t}`,
/// `panRoom`, `resize {fromViewport}` (`from: "rest"` = the rest view at
/// the OLD viewport), `percent {scale}`, `menu {fitLabel?}`, `bounds`.
pub fn zoom_request(req: &serde_json::Value) -> Result<serde_json::Value, String> {
    use serde_json::{json, Value};
    let op = req.get("op").and_then(Value::as_str).ok_or("op is required")?;
    let num = |k: &str| req.get(k).and_then(Value::as_f64).unwrap_or(0.0);
    let point = |k: &str| -> ZoomPoint { req.get(k).and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default() };
    let frame: ZoomFrame = match req.get("frame") {
        Some(v) => serde_json::from_value(v.clone()).map_err(|e| format!("frame: {e}"))?,
        None => ZoomFrame::default(),
    };
    let from = |f: &ZoomFrame| -> Result<ZoomView, String> {
        match req.get("from") {
            None => Ok(zoom_rest_view(f)),
            Some(Value::String(s)) if s == "rest" => Ok(zoom_rest_view(f)),
            Some(v) => serde_json::from_value(v.clone()).map_err(|e| format!("from: {e}")),
        }
    };
    let view = |v: ZoomView| json!(v);
    Ok(match op {
        "rest" => json!({
            "fit": zoom_fit_scale(&frame),
            "limits": zoom_limits(&frame),
            "restScale": zoom_rest_scale(&frame),
            "view": zoom_rest_view(&frame),
            "tall": zoom_is_tall(frame.natural),
        }),
        "clamp" => {
            let v: ZoomView = serde_json::from_value(req.get("view").cloned().unwrap_or(Value::Null)).map_err(|e| format!("view: {e}"))?;
            view(zoom_clamp(v, &frame))
        }
        "zoomTo" => view(zoom_to(from(&frame)?, num("scale"), point("anchor"), &frame)),
        "wheel" => {
            let mode = if req.get("mode").and_then(Value::as_str) == Some("line") { WheelDeltaMode::Line } else { WheelDeltaMode::Pixel };
            view(zoom_wheel(from(&frame)?, num("deltaY"), mode, point("anchor"), &frame))
        }
        "scroll" => view(zoom_scroll(from(&frame)?, num("deltaX"), num("deltaY"), &frame)),
        "ladder" => json!(zoom_ladder(&frame)),
        "steps" => json!(zoom_step_scale(num("scale"), if num("direction") < 0.0 { -1 } else { 1 }, &frame)),
        "step" => view(zoom_step(from(&frame)?, if num("direction") < 0.0 { -1 } else { 1 }, &frame)),
        "doubleTap" => view(zoom_double_tap(from(&frame)?, point("point"), &frame)),
        "keys" => json!(zoom_key(from(&frame)?, req.get("key").and_then(Value::as_str).unwrap_or(""), &frame)),
        "rubberBand" => json!(zoom_rubber_band(num("overshoot"), num("extent"))),
        "drag" => view(zoom_drag(from(&frame)?, num("dx"), num("dy"), &frame)),
        "pinch" => view(zoom_pinch(from(&frame)?, num("factor"), point("anchor"), &frame)),
        "momentum" => view(zoom_momentum(from(&frame)?, num("vx"), num("vy"), num("t"), &frame)),
        "momentumActive" => json!(zoom_momentum_active(num("vx"), num("vy"), num("t"))),
        "panRoom" => json!(zoom_pan_room(from(&frame)?, &frame)),
        "resize" => {
            let old: ZoomSize = serde_json::from_value(req.get("fromViewport").cloned().unwrap_or(Value::Null)).map_err(|e| format!("fromViewport: {e}"))?;
            let start = match req.get("from") {
                Some(Value::String(s)) if s == "rest" => zoom_rest_view(&ZoomFrame { viewport: old, ..frame }),
                _ => from(&frame)?,
            };
            view(zoom_resize(start, old, &frame))
        }
        "percent" => json!(zoom_percent(num("scale"))),
        "menu" => json!(zoom_menu(from(&frame)?, &frame, req.get("fitLabel").and_then(Value::as_str).unwrap_or("Fit"))),
        "bounds" => json!(zoom_bounds(from(&frame)?, &frame)),
        other => return Err(format!("unknown zoom op {other:?}")),
    })
}
