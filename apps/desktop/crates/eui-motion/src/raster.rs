//! The `raster` reference painter (tiny-skia): paints a [`DisplayList`]
//! into RGBA pixels. It is the gpui painter's backend, the per-renderer
//! pixel-parity reference and the GIF preview path.

use crate::display::{Blend, Brush, Cap, DisplayList, FillRule, Join, Op, Stop, Verb};
use tiny_skia::{
    BlendMode, GradientStop, LineCap, LineJoin, LinearGradient, Mask, Paint, Path, PathBuilder,
    Pixmap, PixmapPaint, Point, RadialGradient, Shader, SpreadMode, Stroke, StrokeDash, Transform,
};

/// Decoded image assets (premultiplied RGBA pixmaps), by `Op::Image::asset`.
pub type Images<'a> = &'a [Option<Pixmap>];

enum Frame {
    Layer {
        pixmap: Pixmap,
        blend: Blend,
        alpha: f32,
        /// The clip in force for this layer's content and its composite.
        clip: Option<Mask>,
        /// The clip to restore after the layer.
        saved: Option<Mask>,
    },
    Clip {
        saved: Option<Mask>,
    },
}

/// Paint `list` into a `width × height` pixmap, scaled uniformly to fit
/// (contain, centred) on a transparent background.
pub fn paint(list: &DisplayList, width: u32, height: u32, images: Images<'_>) -> Pixmap {
    let mut target = Pixmap::new(width.max(1), height.max(1)).expect("non-zero pixmap");
    let base = fit(list, width as f32, height as f32);
    paint_into(list, &mut target, base, images);
    target
}

/// The contain-fit transform of `list` into `width × height`.
pub fn fit(list: &DisplayList, width: f32, height: f32) -> Transform {
    if list.width <= 0.0 || list.height <= 0.0 {
        return Transform::identity();
    }
    let s = (width / list.width).min(height / list.height);
    let tx = (width - list.width * s) / 2.0;
    let ty = (height - list.height * s) / 2.0;
    Transform::from_row(s, 0.0, 0.0, s, tx, ty)
}

/// Paint `list` into `target` under `base`.
pub fn paint_into(list: &DisplayList, target: &mut Pixmap, base: Transform, images: Images<'_>) {
    let (w, h) = (target.width(), target.height());
    let mut stack: Vec<Frame> = Vec::new();
    let mut clip: Option<Mask> = None;
    // `target` is the bottom of the stack; layers own their pixmaps.
    macro_rules! top {
        () => {
            match stack.iter_mut().rev().find_map(|f| match f {
                Frame::Layer { pixmap, .. } => Some(pixmap),
                _ => None,
            }) {
                Some(p) => p,
                None => &mut *target,
            }
        };
    }
    for op in &list.ops {
        match op {
            Op::Layer {
                blend,
                alpha,
                transform,
                clip: path,
            } => {
                let t = base.pre_concat(tf(transform));
                let layer_clip = intersect(&clip, w, h, path, FillRule::Nonzero, t);
                let saved = std::mem::replace(&mut clip, layer_clip.clone());
                stack.push(Frame::Layer {
                    pixmap: Pixmap::new(w, h).expect("non-zero pixmap"),
                    blend: *blend,
                    alpha: *alpha,
                    clip: layer_clip,
                    saved,
                });
            }
            Op::Clip { transform, path } => {
                let t = base.pre_concat(tf(transform));
                let next = intersect(&clip, w, h, path, FillRule::Nonzero, t);
                let saved = std::mem::replace(&mut clip, next);
                stack.push(Frame::Clip { saved });
            }
            Op::Pop => match stack.pop() {
                Some(Frame::Clip { saved }) => clip = saved,
                Some(Frame::Layer {
                    pixmap,
                    blend,
                    alpha,
                    clip: layer_clip,
                    saved,
                }) => {
                    clip = saved;
                    let paint = PixmapPaint {
                        opacity: alpha.clamp(0.0, 1.0),
                        blend_mode: blend_mode(blend),
                        quality: tiny_skia::FilterQuality::Nearest,
                    };
                    top!().draw_pixmap(
                        0,
                        0,
                        pixmap.as_ref(),
                        &paint,
                        Transform::identity(),
                        layer_clip.as_ref(),
                    );
                }
                None => {}
            },
            Op::Fill {
                transform,
                rule,
                brush,
                path,
            } => {
                let Some(p) = build(path) else { continue };
                let Some(paint) = make_paint(brush) else {
                    continue;
                };
                let t = base.pre_concat(tf(transform));
                top!().fill_path(&p, &paint, fill_rule(*rule), t, clip.as_ref());
            }
            Op::Stroke {
                transform,
                style,
                brush,
                path,
            } => {
                let Some(p) = build(path) else { continue };
                let Some(paint) = make_paint(brush) else {
                    continue;
                };
                let stroke = Stroke {
                    width: style.width,
                    miter_limit: style.miter,
                    line_cap: match style.cap {
                        Cap::Butt => LineCap::Butt,
                        Cap::Round => LineCap::Round,
                        Cap::Square => LineCap::Square,
                    },
                    line_join: match style.join {
                        Join::Miter => LineJoin::Miter,
                        Join::Round => LineJoin::Round,
                        Join::Bevel => LineJoin::Bevel,
                    },
                    dash: if style.dashes.is_empty() {
                        None
                    } else {
                        let mut d = style.dashes.clone();
                        if d.len() % 2 == 1 {
                            d.extend_from_within(..);
                        }
                        StrokeDash::new(d, style.dash_offset)
                    },
                };
                let t = base.pre_concat(tf(transform));
                top!().stroke_path(&p, &paint, &stroke, t, clip.as_ref());
            }
            Op::Image {
                transform,
                asset,
                alpha,
                width,
                height,
            } => {
                let Some(Some(img)) = images.get(*asset as usize) else {
                    continue;
                };
                let sx = if *width > 0.0 {
                    width / img.width() as f32
                } else {
                    1.0
                };
                let sy = if *height > 0.0 {
                    height / img.height() as f32
                } else {
                    1.0
                };
                let t = base
                    .pre_concat(tf(transform))
                    .pre_concat(Transform::from_scale(sx, sy));
                let paint = PixmapPaint {
                    opacity: alpha.clamp(0.0, 1.0),
                    blend_mode: BlendMode::SourceOver,
                    quality: tiny_skia::FilterQuality::Bilinear,
                };
                top!().draw_pixmap(0, 0, img.as_ref(), &paint, t, clip.as_ref());
            }
        }
    }
}

fn tf(t: &[f32; 6]) -> Transform {
    Transform::from_row(t[0], t[1], t[2], t[3], t[4], t[5])
}

fn fill_rule(r: FillRule) -> tiny_skia::FillRule {
    match r {
        FillRule::Nonzero => tiny_skia::FillRule::Winding,
        FillRule::Evenodd => tiny_skia::FillRule::EvenOdd,
    }
}

fn intersect(
    current: &Option<Mask>,
    w: u32,
    h: u32,
    path: &[Verb],
    rule: FillRule,
    t: Transform,
) -> Option<Mask> {
    let mut mask = match current {
        Some(m) => m.clone(),
        None => {
            let mut m = Mask::new(w, h)?;
            m.data_mut().fill(255);
            m
        }
    };
    match build(path) {
        Some(p) => mask.intersect_path(&p, fill_rule(rule), true, t),
        None => mask.data_mut().fill(0),
    }
    Some(mask)
}

pub(crate) fn build(path: &[Verb]) -> Option<Path> {
    let mut pb = PathBuilder::with_capacity(path.len(), path.len() * 3);
    for v in path {
        match *v {
            Verb::M { x, y } => pb.move_to(x, y),
            Verb::L { x, y } => pb.line_to(x, y),
            Verb::C {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => pb.cubic_to(x1, y1, x2, y2, x, y),
            Verb::Z => pb.close(),
        }
    }
    pb.finish()
}

fn color(c: [f32; 4]) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba(
        c[0].clamp(0.0, 1.0),
        c[1].clamp(0.0, 1.0),
        c[2].clamp(0.0, 1.0),
        c[3].clamp(0.0, 1.0),
    )
    .unwrap_or(tiny_skia::Color::TRANSPARENT)
}

fn stops(s: &[Stop]) -> Vec<GradientStop> {
    s.iter()
        .map(|s| GradientStop::new(s.offset, color(s.color)))
        .collect()
}

fn make_paint(brush: &Brush) -> Option<Paint<'static>> {
    let shader = match brush {
        Brush::Solid { color: c } => Shader::SolidColor(color(*c)),
        Brush::Linear {
            start,
            end,
            stops: s,
        } => LinearGradient::new(
            Point::from_xy(start[0], start[1]),
            Point::from_xy(end[0], end[1]),
            stops(s),
            SpreadMode::Pad,
            Transform::identity(),
        )?,
        Brush::Radial {
            center,
            focal,
            radius,
            stops: s,
        } => RadialGradient::new(
            Point::from_xy(focal[0], focal[1]),
            Point::from_xy(center[0], center[1]),
            *radius,
            stops(s),
            SpreadMode::Pad,
            Transform::identity(),
        )?,
    };
    Some(Paint {
        shader,
        anti_alias: true,
        ..Paint::default()
    })
}

fn blend_mode(b: Blend) -> BlendMode {
    match b {
        Blend::Normal => BlendMode::SourceOver,
        Blend::Multiply => BlendMode::Multiply,
        Blend::Screen => BlendMode::Screen,
        Blend::Overlay => BlendMode::Overlay,
        Blend::Darken => BlendMode::Darken,
        Blend::Lighten => BlendMode::Lighten,
        Blend::ColorDodge => BlendMode::ColorDodge,
        Blend::ColorBurn => BlendMode::ColorBurn,
        Blend::HardLight => BlendMode::HardLight,
        Blend::SoftLight => BlendMode::SoftLight,
        Blend::Difference => BlendMode::Difference,
        Blend::Exclusion => BlendMode::Exclusion,
        Blend::Hue => BlendMode::Hue,
        Blend::Saturation => BlendMode::Saturation,
        Blend::Color => BlendMode::Color,
        Blend::Luminosity => BlendMode::Luminosity,
        Blend::SrcIn => BlendMode::SourceIn,
        Blend::SrcOut => BlendMode::SourceOut,
        Blend::DstIn => BlendMode::DestinationIn,
        Blend::DstOut => BlendMode::DestinationOut,
    }
}
