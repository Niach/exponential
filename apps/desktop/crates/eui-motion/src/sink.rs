//! The velato `RenderSink` that records the eUI display list.

use crate::display::{
    Blend, Brush, Cap, DisplayList, FillRule, Join, Op, Stop, StrokeStyle, Transform, Verb,
};
use kurbo::{Affine, PathEl, Shape};
use peniko::{Compose, Mix};
use velato::model::{fixed, ImageAsset};

/// Path flattening tolerance for shapes that are not already Bézier paths
/// (ellipses, rounded rects), in local units.
const TOLERANCE: f64 = 0.1;

/// Records a frame. Image assets are interned by id into `images`.
pub(crate) struct Recorder<'a> {
    pub list: &'a mut DisplayList,
    pub images: &'a [String],
}

fn affine(t: Affine) -> Transform {
    let c = t.as_coeffs();
    [
        c[0] as f32,
        c[1] as f32,
        c[2] as f32,
        c[3] as f32,
        c[4] as f32,
        c[5] as f32,
    ]
}

pub(crate) fn path(shape: &impl Shape) -> Vec<Verb> {
    let mut out = Vec::new();
    let mut last = kurbo::Point::ZERO;
    for el in shape.path_elements(TOLERANCE) {
        match el {
            PathEl::MoveTo(p) => {
                out.push(Verb::M {
                    x: p.x as f32,
                    y: p.y as f32,
                });
                last = p;
            }
            PathEl::LineTo(p) => {
                out.push(Verb::L {
                    x: p.x as f32,
                    y: p.y as f32,
                });
                last = p;
            }
            PathEl::QuadTo(q, p) => {
                // Raise to a cubic: c1 = p0 + 2/3 (q - p0), c2 = p + 2/3 (q - p).
                let c1 = last + (q - last) * (2.0 / 3.0);
                let c2 = p + (q - p) * (2.0 / 3.0);
                out.push(Verb::C {
                    x1: c1.x as f32,
                    y1: c1.y as f32,
                    x2: c2.x as f32,
                    y2: c2.y as f32,
                    x: p.x as f32,
                    y: p.y as f32,
                });
                last = p;
            }
            PathEl::CurveTo(a, b, p) => {
                out.push(Verb::C {
                    x1: a.x as f32,
                    y1: a.y as f32,
                    x2: b.x as f32,
                    y2: b.y as f32,
                    x: p.x as f32,
                    y: p.y as f32,
                });
                last = p;
            }
            PathEl::ClosePath => out.push(Verb::Z),
        }
    }
    out
}

fn blend(mode: peniko::BlendMode) -> Blend {
    match mode.compose {
        Compose::SrcIn => return Blend::SrcIn,
        Compose::SrcOut => return Blend::SrcOut,
        Compose::DestIn => return Blend::DstIn,
        Compose::DestOut => return Blend::DstOut,
        _ => {}
    }
    match mode.mix {
        Mix::Multiply => Blend::Multiply,
        Mix::Screen => Blend::Screen,
        Mix::Overlay => Blend::Overlay,
        Mix::Darken => Blend::Darken,
        Mix::Lighten => Blend::Lighten,
        Mix::ColorDodge => Blend::ColorDodge,
        Mix::ColorBurn => Blend::ColorBurn,
        Mix::HardLight => Blend::HardLight,
        Mix::SoftLight => Blend::SoftLight,
        Mix::Difference => Blend::Difference,
        Mix::Exclusion => Blend::Exclusion,
        Mix::Hue => Blend::Hue,
        Mix::Saturation => Blend::Saturation,
        Mix::Color => Blend::Color,
        Mix::Luminosity => Blend::Luminosity,
        _ => Blend::Normal,
    }
}

fn color(c: peniko::Color) -> [f32; 4] {
    c.components
}

fn brush(b: &fixed::Brush) -> Brush {
    match b {
        peniko::Brush::Solid(c) => Brush::Solid { color: color(*c) },
        peniko::Brush::Gradient(g) => {
            let stops = g
                .stops
                .iter()
                .map(|s| Stop {
                    offset: s.offset,
                    color: s.color.to_alpha_color::<peniko::color::Srgb>().components,
                })
                .collect();
            match g.kind {
                peniko::GradientKind::Linear(p) => Brush::Linear {
                    start: [p.start.x as f32, p.start.y as f32],
                    end: [p.end.x as f32, p.end.y as f32],
                    stops,
                },
                peniko::GradientKind::Radial(p) => Brush::Radial {
                    center: [p.end_center.x as f32, p.end_center.y as f32],
                    focal: [p.start_center.x as f32, p.start_center.y as f32],
                    radius: p.end_radius,
                    stops,
                },
                peniko::GradientKind::Sweep(p) => Brush::Solid {
                    // Lottie has no sweep gradients; unreachable from velato.
                    color: stops_first(&stops, p.center),
                },
            }
        }
        peniko::Brush::Image(_) => Brush::Solid {
            color: [0.0, 0.0, 0.0, 0.0],
        },
    }
}

fn stops_first(stops: &[Stop], _: kurbo::Point) -> [f32; 4] {
    stops.first().map(|s| s.color).unwrap_or_default()
}

fn stroke_style(s: &fixed::Stroke) -> StrokeStyle {
    StrokeStyle {
        width: s.width as f32,
        cap: match s.start_cap {
            kurbo::Cap::Butt => Cap::Butt,
            kurbo::Cap::Round => Cap::Round,
            kurbo::Cap::Square => Cap::Square,
        },
        join: match s.join {
            kurbo::Join::Miter => Join::Miter,
            kurbo::Join::Round => Join::Round,
            kurbo::Join::Bevel => Join::Bevel,
        },
        miter: s.miter_limit as f32,
        dashes: s.dash_pattern.iter().map(|d| *d as f32).collect(),
        dash_offset: s.dash_offset as f32,
    }
}

impl velato::RenderSink for Recorder<'_> {
    fn push_layer(
        &mut self,
        mode: impl Into<peniko::BlendMode>,
        alpha: f32,
        transform: Affine,
        shape: &impl Shape,
    ) {
        self.list.ops.push(Op::Layer {
            blend: blend(mode.into()),
            alpha,
            transform: affine(transform),
            clip: path(shape),
        });
    }

    fn push_clip_layer(&mut self, transform: Affine, shape: &impl Shape) {
        self.list.ops.push(Op::Clip {
            transform: affine(transform),
            path: path(shape),
        });
    }

    fn pop_layer(&mut self) {
        self.list.ops.push(Op::Pop);
    }

    fn draw(
        &mut self,
        stroke: Option<&fixed::Stroke>,
        transform: Affine,
        b: &fixed::Brush,
        shape: &impl Shape,
    ) {
        let transform = affine(transform);
        let path = path(shape);
        let brush = brush(b);
        self.list.ops.push(match stroke {
            Some(s) => Op::Stroke {
                transform,
                style: stroke_style(s),
                brush,
                path,
            },
            None => Op::Fill {
                transform,
                // velato fills nonzero; Lottie's `r` fill rule lands with the fork (M1).
                rule: FillRule::Nonzero,
                brush,
                path,
            },
        });
    }

    fn draw_image(&mut self, image: &ImageAsset, transform: Affine, alpha: f64) {
        let Some(asset) = self.images.iter().position(|id| *id == image.id) else {
            return;
        };
        self.list.ops.push(Op::Image {
            transform: affine(transform),
            asset: asset as u32,
            alpha: alpha as f32,
            width: image.width.unwrap_or(0.0) as f32,
            height: image.height.unwrap_or(0.0) as f32,
        });
    }
}
