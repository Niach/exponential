//! genUI round 4: a box's background LAYERS beyond its colour (style.json
//! `backgroundImage`, `backgroundGradient` radial). gpui has no radial
//! brush: the radial gradient is rasterised (tiny-skia, the same circle as
//! CSS `circle farthest-corner`: `eui_core::style::radial_radius`) at the
//! box's device size into a cached picture. The background picture loads
//! like an Image (`media::image_key`, the host's image-source rule) and
//! lands at `eui_core::style::picture_rect`; while it is not painted, the
//! `Image/fallback` fill (else muted) shows in its place.

use std::cell::RefCell;
use std::sync::Arc;

use gpui::{canvas, div, point, prelude::*, px, AnyElement, App, Bounds, Corners, Hsla, Pixels, RenderImage, Rgba, Window};
use tiny_skia::{Color, GradientStop, Paint, Pixmap, Point, RadialGradient, Rect, SpreadMode, Transform};

use super::{GradientPaint, PicturePaint};

/// The radial gradient over a `w`×`h` px pixmap (`None` = an empty box).
pub fn radial_pixmap(g: &GradientPaint, w: u32, h: u32) -> Option<Pixmap> {
    let [cx, cy] = g.centre.unwrap_or([0.5, 0.5]);
    let (fw, fh) = (w as f32, h as f32);
    let radius = eui_core::style::radial_radius(fw, fh, cx, cy);
    let mut pixmap = Pixmap::new(w.max(1), h.max(1))?;
    if radius <= 0.0 || w == 0 || h == 0 {
        return Some(pixmap);
    }
    let stops: Vec<GradientStop> = g
        .stops
        .iter()
        .map(|(c, o)| {
            let c = Rgba::from(*c);
            GradientStop::new(*o, Color::from_rgba(c.r, c.g, c.b, c.a).unwrap_or(Color::TRANSPARENT))
        })
        .collect();
    let centre = Point::from_xy(cx * fw, cy * fh);
    let Some(shader) = RadialGradient::new(centre, centre, radius, stops, SpreadMode::Pad, Transform::identity()) else { return Some(pixmap) };
    let paint = Paint { shader, anti_alias: false, ..Paint::default() };
    pixmap.fill_rect(Rect::from_xywh(0.0, 0.0, fw, fh)?, &paint, Transform::identity(), None);
    Some(pixmap)
}

type RadialKey = (u32, u32, [u32; 2], Vec<([u32; 4], u32)>);

thread_local! {
    /// The last rasterised radial gradients (an atlas image each).
    static RADIALS: RefCell<Vec<(RadialKey, Arc<RenderImage>)>> = const { RefCell::new(Vec::new()) };
}

/// At most this many radial pictures stay cached (the oldest leaves the atlas).
const RADIAL_CACHE: usize = 32;

fn radial_key(g: &GradientPaint, w: u32, h: u32) -> RadialKey {
    let [cx, cy] = g.centre.unwrap_or([0.5, 0.5]);
    let stops = g.stops.iter().map(|(c, o)| ([c.h, c.s, c.l, c.a].map(f32::to_bits), o.to_bits())).collect();
    (w, h, [cx.to_bits(), cy.to_bits()], stops)
}

fn radial_image(g: &GradientPaint, w: u32, h: u32, window: &mut Window, cx: &mut App) -> Option<Arc<RenderImage>> {
    let key = radial_key(g, w, h);
    if let Some(hit) = RADIALS.with(|c| c.borrow().iter().find(|(k, _)| *k == key).map(|(_, i)| i.clone())) {
        return Some(hit);
    }
    let pixmap = radial_pixmap(g, w, h)?;
    let image = Arc::new(RenderImage::new([image::Frame::new(crate::motion::to_bgra(&pixmap))]));
    let evicted = RADIALS.with(|c| {
        let mut c = c.borrow_mut();
        c.push((key, image.clone()));
        (c.len() > RADIAL_CACHE).then(|| c.remove(0).1)
    });
    if let Some(old) = evicted {
        cx.drop_image(old, Some(window));
    }
    Some(image)
}

fn corners(radii: [f32; 4]) -> Corners<Pixels> {
    Corners { top_left: px(radii[0]), top_right: px(radii[1]), bottom_right: px(radii[2]), bottom_left: px(radii[3]) }
}

fn layer(w: f32, h: f32) -> gpui::Div {
    div().absolute().top_0().left_0().w(px(w)).h(px(h))
}

/// A radial gradient filling the `w`×`h` box, clipped to `radii`.
pub fn radial(g: &GradientPaint, w: f32, h: f32, radii: [f32; 4], window: &mut Window, cx: &mut App) -> AnyElement {
    let scale = window.scale_factor();
    let (pw, ph) = ((w * scale).round().max(0.0) as u32, (h * scale).round().max(0.0) as u32);
    let Some(data) = radial_image(g, pw, ph, window, cx) else { return layer(w, h).into_any_element() };
    canvas(|_, _, _| {}, move |bounds: Bounds<Pixels>, _, window, _| {
        let _ = window.paint_image(bounds, bounds, corners(radii), data.clone(), 0, false);
    })
    .absolute()
    .top_0()
    .left_0()
    .w(px(w))
    .h(px(h))
    .into_any_element()
}

/// The background picture of a `w`×`h` box, clipped to `radii`: the
/// decoded picture at its `picture_rect`, else `placeholder` (loading,
/// failed, refused, no src); then the scrim over it.
pub fn picture(p: &PicturePaint, w: f32, h: f32, radii: [f32; 4], placeholder: Option<Hsla>, host: &dyn crate::host::HostPlugin, window: &mut Window, cx: &mut App) -> AnyElement {
    let refused = !matches!(host.image_source(&p.src, ""), None | Some(crate::host::ImageSourceVerdict::Load));
    let decoded = if refused {
        None
    } else {
        crate::media::image_key(host, &p.src).and_then(|key| super::natives::render_image(&crate::media::key_source(key), window, cx)).and_then(Result::ok)
    };
    let fill = |d: gpui::Div, c: Option<Hsla>| match c {
        Some(c) => d.bg(c),
        None => d,
    };
    let mut out = layer(w, h).overflow_hidden().rounded_tl(px(radii[0])).rounded_tr(px(radii[1])).rounded_br(px(radii[2])).rounded_bl(px(radii[3]));
    match decoded {
        Some(data) => {
            let (contain, focal) = (p.contain, p.focal);
            out = out.child(
                canvas(|_, _, _| {}, move |bounds: Bounds<Pixels>, _, window, _| {
                    let size = data.size(0);
                    let (bw, bh) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
                    let (x, y, dw, dh) = eui_core::style::picture_rect(if contain { "contain" } else { "cover" }, (bw, bh), (size.width.0 as f32, size.height.0 as f32), focal);
                    let image_bounds = Bounds { origin: point(bounds.origin.x + px(x), bounds.origin.y + px(y)), size: gpui::size(px(dw), px(dh)) };
                    // gpui rounds the VISIBLE rect: only a corner it shares
                    // with the box takes the box's radius (a contained
                    // picture's inner edge stays square, as on the web).
                    let (vx0, vy0, vx1, vy1) = (x.max(0.0), y.max(0.0), (x + dw).min(bw), (y + dh).min(bh));
                    let at = |a: f32, b: f32| (a - b).abs() < 0.5;
                    let r = [
                        if at(vx0, 0.0) && at(vy0, 0.0) { radii[0] } else { 0.0 },
                        if at(vx1, bw) && at(vy0, 0.0) { radii[1] } else { 0.0 },
                        if at(vx1, bw) && at(vy1, bh) { radii[2] } else { 0.0 },
                        if at(vx0, 0.0) && at(vy1, bh) { radii[3] } else { 0.0 },
                    ];
                    let _ = window.paint_image(bounds, image_bounds, corners(r), data.clone(), 0, false);
                })
                .absolute()
                .top_0()
                .left_0()
                .w(px(w))
                .h(px(h)),
            );
        }
        None => out = fill(out, placeholder),
    }
    if let Some(scrim) = p.scrim {
        // gpui clips children to the rect only: the scrim rounds itself.
        out = out.child(layer(w, h).bg(scrim).rounded_tl(px(radii[0])).rounded_tr(px(radii[1])).rounded_br(px(radii[2])).rounded_bl(px(radii[3])));
    }
    out.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// fixtures/box-backgrounds.json `radial`: the rasterised circle paints
    /// the stop colour at t = |P − C| / R (black → white: the red channel
    /// IS t).
    #[test]
    fn the_rasterised_radial_paints_the_shared_circle() {
        let file: serde_json::Value = serde_json::from_str(include_str!("../../../../../../packages/exponential-ui/fixtures/box-backgrounds.json")).unwrap();
        let black = Hsla { h: 0.0, s: 0.0, l: 0.0, a: 1.0 };
        let white = Hsla { h: 0.0, s: 0.0, l: 1.0, a: 1.0 };
        for c in file["radial"].as_array().unwrap() {
            let (w, h) = (c["size"][0].as_u64().unwrap() as u32, c["size"][1].as_u64().unwrap() as u32);
            if w == 0 || h == 0 {
                continue;
            }
            let g = GradientPaint { angle: 180.0, stops: vec![(black, 0.0), (white, 1.0)], centre: Some([c["centre"][0].as_f64().unwrap() as f32, c["centre"][1].as_f64().unwrap() as f32]) };
            let pixmap = radial_pixmap(&g, w, h).unwrap();
            for s in c["samples"].as_array().unwrap() {
                // The pixel whose centre is nearest the sample (inside the box).
                let (x, y) = ((s[0].as_f64().unwrap() as u32).min(w - 1), (s[1].as_f64().unwrap() as u32).min(h - 1));
                let t = eui_core::style::radial_t(w as f32, h as f32, g.centre.unwrap()[0], g.centre.unwrap()[1], x as f32 + 0.5, y as f32 + 0.5);
                let red = pixmap.pixel(x, y).unwrap().red() as f32 / 255.0;
                assert!((red - t).abs() < 0.03, "{}: ({x}, {y}) red {red} / t {t}", c["name"]);
            }
        }
    }
}
