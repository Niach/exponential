//! VAPP-162 (docs/round-5-contract.md §6, §7): the QrCode and QrScanner
//! natives.
//!
//! QrCode paints THE CORE'S matrix (`eui_core::qr_code::qr_matrix`,
//! never a platform QR library) on a `size` px plate inside a quiet zone of
//! 4 modules; module EDGES snap to device pixels so the outer size stays
//! exact; of the plate and module recipe colours the lighter fills the
//! plate and the darker the modules (a dark theme still scans).
//!
//! QrScanner is the viewfinder CHROME (catalog/style.json `qrScanner`); the
//! camera is the HOST's ([`crate::host::HostPlugin::qr_scanner_feed`] paints
//! the live feed, the host reports decoded strings through
//! [`crate::view::SurfaceView::qr_scanned`]). No host feed = the placeholder
//! (`ui-scan` + `cameraUnavailable`), no scan line.

use std::cell::RefCell;
use std::sync::LazyLock;

use eui_core::qr_code::{qr_layout, qr_matrix, QrLevel, QrMatrix};
use gpui::{canvas, div, point, prelude::*, px, AnyElement, Bounds, Hsla, PathBuilder, Pixels, Rgba, SharedString, Size};
use serde::Deserialize;
use serde_json::Value;

use super::color::color_of;
use super::icons;
use super::natives::LeafCx;
use super::parts::px_prop;

/// style.json `qrScanner`: the viewfinder geometry.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QrScannerSpec {
    pub max_width: f32,
    pub corner_length: f32,
    pub corner_stroke: f32,
    pub feed_alpha: f32,
    pub scan_line_height: f32,
    pub scan_line_inset: f32,
    pub sweep_ms: f64,
    pub hint_gap: f32,
    pub placeholder_icon: String,
    pub placeholder_icon_size: f32,
}

pub static QR_SCANNER: LazyLock<QrScannerSpec> = LazyLock::new(|| {
    let style: Value = serde_json::from_str(eui_core::generated::catalog::STYLE_JSON).expect("style.json");
    serde_json::from_value(style["qrScanner"].clone()).expect("style.json qrScanner")
});

/// The QrCode `size` prop (px, default 160).
pub fn qr_size(props: &serde_json::Map<String, Value>) -> f32 {
    let n = match props.get("size") {
        Some(Value::Number(n)) => n.as_f64(),
        Some(Value::String(s)) => s.trim().parse().ok(),
        _ => None,
    };
    n.filter(|n| n.is_finite()).unwrap_or(160.0).max(0.0) as f32
}

/// The QrScanner viewfinder edge inside a content box `w` wide.
pub fn viewfinder_edge(w: f32) -> f32 {
    w.max(0.0).min(QR_SCANNER.max_width)
}

/// WCAG relative luminance of a colour (alpha ignored).
pub fn luminance(c: Hsla) -> f32 {
    let Rgba { r, g, b, .. } = Rgba::from(c);
    let lin = |v: f32| if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) };
    0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
}

/// (plate, module): the lighter of the two fills the plate.
pub fn plate_and_module(plate: Hsla, module: Hsla) -> (Hsla, Hsla) {
    if luminance(module) > luminance(plate) {
        (module, plate)
    } else {
        (plate, module)
    }
}

/// One edge of the module grid snapped to device pixels (`scale` px per
/// logical px): module `i` starts at `origin + inset + i × module`.
pub fn snapped_edge(origin: f32, inset: f64, module: f64, i: usize, scale: f32) -> f32 {
    let v = origin as f64 + inset + i as f64 * module;
    ((v * scale as f64).round() / scale as f64) as f32
}

/// One painted QrCode (a test aid: [`record_qr`]).
#[derive(Debug, Clone, PartialEq)]
pub struct PaintedQr {
    pub id: String,
    /// Modules per side (0 = no matrix: the empty plate).
    pub modules: usize,
    pub dark: usize,
    pub plate: Hsla,
    pub module: Hsla,
    pub size: f32,
}

thread_local! {
    static QRS: RefCell<Option<Vec<PaintedQr>>> = const { RefCell::new(None) };
}

/// Record every QrCode painted on this thread (a test aid).
pub fn record_qr(on: bool) {
    QRS.with(|t| *t.borrow_mut() = on.then(Vec::new));
}

/// The QrCodes painted since the last call (recording stays on).
pub fn take_qr() -> Vec<PaintedQr> {
    QRS.with(|t| t.borrow_mut().as_mut().map(std::mem::take).unwrap_or_default())
}

/// The modules of a matrix as horizontal runs `(row, first column, length)`
/// (one quad per run, not per module).
pub fn dark_runs(m: &QrMatrix) -> Vec<(usize, usize, usize)> {
    let mut out = Vec::new();
    for y in 0..m.size {
        let mut x = 0;
        while x < m.size {
            if m.dark(x, y) {
                let start = x;
                while x < m.size && m.dark(x, y) {
                    x += 1;
                }
                out.push((y, start, x - start));
            } else {
                x += 1;
            }
        }
    }
    out
}

/// A caption-like text line of a part recipe (`label`, `hint`,
/// `placeholder`), centred, wrapping inside `w`.
fn part_text(lcx: &LeafCx, component: &str, part: &str, text: &str, w: f32) -> gpui::Div {
    let p = lcx.part_props(component, part, &[]);
    let size = px_prop(&p, "fontSize").unwrap_or(12.0);
    let lh = px_prop(&p, "lineHeight").unwrap_or(16.0);
    let weight = p.get("fontWeight").and_then(Value::as_u64).map(|w| w as u16).unwrap_or(400);
    let ink = color_of(p.get("color").and_then(Value::as_str)).unwrap_or(lcx.ink);
    div()
        .w(px(w))
        .text_center()
        .font(crate::measure::make_font(lcx.font.family.clone(), weight))
        .text_size(px(size))
        .line_height(px(lh))
        .text_color(ink)
        .child(SharedString::from(text.to_string()))
}

/// A part's text line height (the measure counts the same).
pub fn part_line_height(lcx: &LeafCx, component: &str, part: &str) -> f32 {
    px_prop(&lcx.part_props(component, part, &[]), "lineHeight").unwrap_or(16.0)
}

/// VAPP-162 `QrCode` (§6): the plate (`size` px square, the plate recipe's
/// radius) centred at the top of the leaf, the core's matrix inside its
/// quiet zone, then the label (a muted caption, centred) under it.
pub fn qr_code(lcx: &LeafCx) -> AnyElement {
    let props = &lcx.node.props;
    let size = qr_size(props) * lcx.scale;
    let value = props.get("value").map(|v| crate::measure::display_text(Some(v))).unwrap_or_default();
    let level = QrLevel::parse(props.get("level").and_then(Value::as_str).unwrap_or("M"));
    let matrix = qr_matrix(&value, level);
    let plate_props = lcx.part_props("QrCode", "plate", &[]);
    let module_props = lcx.part_props("QrCode", "module", &[]);
    let plate_c = color_of(plate_props.get("backgroundColor").and_then(Value::as_str)).unwrap_or(gpui::white());
    let module_c = color_of(module_props.get("color").and_then(Value::as_str)).unwrap_or(gpui::black());
    let (plate, module) = plate_and_module(plate_c, module_c);
    let radius = px_prop(&plate_props, "borderRadius").unwrap_or(0.0).min(size / 2.0).max(0.0);
    QRS.with(|t| {
        if let Some(v) = t.borrow_mut().as_mut() {
            v.push(PaintedQr {
                id: lcx.node.id.clone(),
                modules: matrix.as_ref().map_or(0, |m| m.size),
                dark: matrix.as_ref().map_or(0, |m| m.rows.iter().map(|r| r.bytes().filter(|b| *b == b'1').count()).sum()),
                plate,
                module,
                size,
            });
        }
    });
    let (x, y, w, _) = lcx.inner();
    let left = x + (w - size) / 2.0;
    let mut plate_el = div().absolute().left(px(left)).top(px(y)).size(px(size)).bg(plate).rounded(px(radius)).overflow_hidden();
    if let Some(m) = matrix {
        let (step, inset) = qr_layout(size as f64, m.size);
        let runs = dark_runs(&m);
        plate_el = plate_el.child(
            canvas(
                |_, _, _| {},
                move |bounds: Bounds<Pixels>, _, window, _| {
                    let scale = window.scale_factor();
                    let (ox, oy) = (f32::from(bounds.origin.x), f32::from(bounds.origin.y));
                    for (row, col, len) in &runs {
                        let x0 = snapped_edge(ox, inset, step, *col, scale);
                        let x1 = snapped_edge(ox, inset, step, col + len, scale);
                        let y0 = snapped_edge(oy, inset, step, *row, scale);
                        let y1 = snapped_edge(oy, inset, step, row + 1, scale);
                        window.paint_quad(gpui::fill(Bounds { origin: point(px(x0), px(y0)), size: Size { width: px(x1 - x0), height: px(y1 - y0) } }, module));
                    }
                },
            )
            .absolute()
            .top_0()
            .left_0()
            .size(px(size)),
        );
    }
    let mut root = div().absolute().left_0().top_0().w(px(lcx.w)).h(px(lcx.h)).child(plate_el);
    let label = props.get("label").and_then(Value::as_str).unwrap_or("");
    if !label.is_empty() {
        let gap = if lcx.style.gap > 0.0 { lcx.style.gap } else { eui_core::qr_code::QR_CODE.label_gap as f32 };
        root = root.child(part_text(lcx, "QrCode", "label", label, w).absolute().left(px(x)).top(px(y + size + gap)));
    }
    root.into_any_element()
}

/// The four corner brackets of a viewfinder `edge` px square with corner
/// radius `radius` (style.json `qrScanner`: two strokes cornerLength long,
/// cornerStroke wide, the outer edge following the radius, round caps).
fn corners(edge: f32, radius: f32, color: Hsla) -> gpui::Canvas<()> {
    let spec = &*QR_SCANNER;
    let (len, stroke) = (spec.corner_length, spec.corner_stroke);
    let h = stroke / 2.0;
    let r = (radius - h).clamp(0.0, (len - h).max(0.0));
    canvas(
        |_, _, _| {},
        move |bounds: Bounds<Pixels>, _, window, _| {
            let o = bounds.origin;
            for (fx, fy) in [(false, false), (true, false), (false, true), (true, true)] {
                let at = |x: f32, y: f32| {
                    let px_ = if fx { edge - x } else { x };
                    let py_ = if fy { edge - y } else { y };
                    o + point(px(px_), px(py_))
                };
                let mut s = PathBuilder::stroke(px(stroke));
                s.move_to(at(h, len));
                s.line_to(at(h, h + r));
                if r > 0.0 {
                    s.curve_to(at(h + r, h), at(h, h));
                }
                s.line_to(at(len, h));
                if let Ok(path) = s.build() {
                    window.paint_path(path, color);
                }
                // Round caps: a stroke-wide dot at both open ends.
                for (cx_, cy_) in [(h, len), (len, h)] {
                    let c = at(cx_, cy_);
                    window.paint_quad(gpui::fill(Bounds { origin: c - point(px(h), px(h)), size: Size { width: px(stroke), height: px(stroke) } }, color).corner_radii(px(h)));
                }
            }
        },
    )
}

/// `$ease.standard` without a theme.
pub const STANDARD_EASE: [f64; 4] = [0.2, 0.0, 0.0, 1.0];

/// The scan line's top (px from the viewfinder top) `elapsed_ms` into the
/// sweep: top → bottom → top over `sweepMs` each way (ease standard,
/// alternate); `None` elapsed (reduced motion, disabled) = the centre.
pub fn scan_line_top(edge: f32, elapsed_ms: Option<f64>, ease: [f64; 4]) -> f32 {
    let spec = &*QR_SCANNER;
    let travel = (edge - spec.scan_line_height).max(0.0);
    let Some(t) = elapsed_ms else { return travel / 2.0 };
    let period = spec.sweep_ms.max(1.0);
    let k = (t / period).max(0.0);
    let leg = k.floor() as i64;
    let frac = k - leg as f64;
    let p = if leg % 2 == 0 { frac } else { 1.0 - frac };
    (eui_core::animation::cubic_bezier(ease, p) as f32) * travel
}

/// VAPP-162 `QrScanner` (§7): the viewfinder (a square as wide as the leaf
/// up to maxWidth, centred, the viewfinder recipe's radius, clipping) with
/// the host's `feed` (else the feed colour at feedAlpha and the
/// placeholder), the four corner brackets, the scan line while a feed
/// shows (`line_at` = its elapsed ms, `None` = static at the centre), then
/// the hint under it. `disabled` dims through the root recipe's opacity.
pub fn qr_scanner(lcx: &LeafCx, feed: Option<AnyElement>, line_at: Option<f64>, camera_unavailable: &str) -> AnyElement {
    let spec = &*QR_SCANNER;
    let (x, y, w, _) = lcx.inner();
    let edge = viewfinder_edge(w);
    let left = x + (w - edge) / 2.0;
    let vf_props = lcx.part_props("QrScanner", "viewfinder", &[]);
    let radius = px_prop(&vf_props, "borderRadius").unwrap_or(0.0).min(edge / 2.0).max(0.0);
    let feed_c = color_of(lcx.part_props("QrScanner", "feed", &[]).get("backgroundColor").and_then(Value::as_str)).unwrap_or(lcx.ink);
    let corner_c = color_of(lcx.part_props("QrScanner", "corner", &[]).get("color").and_then(Value::as_str)).unwrap_or(lcx.ink);
    let line_c = color_of(lcx.part_props("QrScanner", "scanLine", &[]).get("backgroundColor").and_then(Value::as_str)).unwrap_or(corner_c);
    let live = feed.is_some();
    let mut vf = div().absolute().left(px(left)).top(px(y)).size(px(edge)).rounded(px(radius)).overflow_hidden();
    match feed {
        Some(el) => vf = vf.child(div().absolute().top_0().left_0().size(px(edge)).child(el)),
        None => {
            vf = vf.bg(feed_c.opacity(spec.feed_alpha));
            let icon_c = color_of(lcx.part_props("QrScanner", "placeholderIcon", &[]).get("color").and_then(Value::as_str)).unwrap_or(lcx.ink);
            let xs = super::parts::spacing(lcx.theme, "xs");
            vf = vf.child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size(px(edge))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(xs))
                    .child(icons::concept(lcx.host, &spec.placeholder_icon, spec.placeholder_icon_size, icon_c))
                    .child(part_text(lcx, "QrScanner", "placeholder", camera_unavailable, edge)),
            );
        }
    }
    vf = vf.child(corners(edge, radius, corner_c).absolute().top_0().left_0().size(px(edge)));
    if live {
        let inset = spec.scan_line_inset;
        let ease = lcx.theme.and_then(|t| t.tokens.ease.get("standard")).filter(|e| e.len() == 4).map_or(STANDARD_EASE, |e| [e[0], e[1], e[2], e[3]]);
        let top = scan_line_top(edge, line_at, ease);
        vf = vf.child(div().absolute().left(px(inset)).top(px(top)).w(px((edge - 2.0 * inset).max(0.0))).h(px(spec.scan_line_height)).rounded(px(spec.scan_line_height / 2.0)).bg(line_c));
    }
    let mut root = div().absolute().left_0().top_0().w(px(lcx.w)).h(px(lcx.h)).child(vf);
    let hint = lcx.str("hint");
    if !hint.is_empty() {
        let gap = if lcx.style.gap > 0.0 { lcx.style.gap } else { spec.hint_gap };
        root = root.child(part_text(lcx, "QrScanner", "hint", hint, w).absolute().left(px(x)).top(px(y + edge + gap)));
    }
    root.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lighter_colour_fills_the_plate() {
        let (p, m) = plate_and_module(gpui::black(), gpui::white());
        assert_eq!((p, m), (gpui::white(), gpui::black()));
        let (p, m) = plate_and_module(gpui::white(), gpui::black());
        assert_eq!((p, m), (gpui::white(), gpui::black()));
    }

    #[test]
    fn module_edges_snap_and_the_outer_size_stays_exact() {
        let (module, inset) = qr_layout(160.0, 21);
        let first = snapped_edge(0.0, inset, module, 0, 2.0);
        let last = snapped_edge(0.0, inset, module, 21, 2.0);
        assert!((first * 2.0).fract().abs() < 1e-4 && (last * 2.0).fract().abs() < 1e-4);
        assert!(((160.0 - last) - first).abs() <= 0.5, "the quiet zone is symmetric to a device pixel");
    }

    #[test]
    fn runs_cover_every_dark_module() {
        let m = qr_matrix("HELLO", QrLevel::H).unwrap();
        let runs: usize = dark_runs(&m).iter().map(|r| r.2).sum();
        let dark: usize = m.rows.iter().map(|r| r.bytes().filter(|b| *b == b'1').count()).sum();
        assert_eq!(runs, dark);
    }

    #[test]
    fn the_scan_line_sweeps_and_rests_at_the_centre() {
        let edge = 280.0;
        let travel = edge - QR_SCANNER.scan_line_height;
        assert_eq!(scan_line_top(edge, None, STANDARD_EASE), travel / 2.0);
        assert!(scan_line_top(edge, Some(0.0), STANDARD_EASE).abs() < 1e-3);
        assert!((scan_line_top(edge, Some(QR_SCANNER.sweep_ms), STANDARD_EASE) - travel).abs() < 1e-3);
        assert!(scan_line_top(edge, Some(2.0 * QR_SCANNER.sweep_ms), STANDARD_EASE).abs() < 1e-3);
        assert_eq!(viewfinder_edge(320.0), QR_SCANNER.max_width);
        assert_eq!(viewfinder_edge(200.0), 200.0);
    }
}
