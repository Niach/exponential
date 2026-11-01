//! The eUI Motion display list: ONE flat, platform-neutral list of drawing
//! operations per sampled frame. Every painter (Canvas2D, SwiftUI, Compose,
//! gpui via the tiny-skia raster) replays exactly this list and nothing else,
//! so "the same look on every renderer" reduces to "replay the same ops".
//!
//! Two encodings carry it across boundaries:
//! - [`DisplayList::encode`]: a packed `f32` stream (WASM `Float32Array`,
//!   uniffi `Vec<f32>`), specified in `README.md` § Packed frames;
//! - serde JSON (the `motion-frames.json` fixtures).
//!
//! Semantics are those of a 2D canvas with an explicit layer stack:
//! - every op carries its own 2×3 transform (`[a, b, c, d, e, f]`, column
//!   major like Canvas2D `setTransform(a, b, c, d, e, f)`); there is no
//!   ambient transform;
//! - `Layer` opens an offscreen group composited on `Pop` with its `alpha`
//!   and `blend`, clipped to its `clip` path; `Clip` intersects the clip of
//!   the current group until its `Pop`;
//! - colours are straight (not premultiplied) sRGB RGBA in `0..=1`.

use serde::{Deserialize, Serialize};

/// A 2×3 affine transform `[a, b, c, d, e, f]` (x' = a·x + c·y + e, y' = b·x + d·y + f).
pub type Transform = [f32; 6];

/// The identity transform.
pub const IDENTITY: Transform = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// A path verb, coordinates in the op's local space. Quadratics are raised
/// to cubics at sampling time so every painter needs only these four.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "v", rename_all = "lowercase")]
pub enum Verb {
    /// Start a subpath.
    M { x: f32, y: f32 },
    /// Line to.
    L { x: f32, y: f32 },
    /// Cubic Bézier to.
    C {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        x: f32,
        y: f32,
    },
    /// Close the subpath.
    Z,
}

/// Fill rule of a fill or clip.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FillRule {
    #[default]
    Nonzero,
    Evenodd,
}

/// A colour stop: offset in `0..=1`, straight RGBA.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stop {
    pub offset: f32,
    pub color: [f32; 4],
}

/// What a fill or stroke paints with. Gradient geometry is in the op's local space.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Brush {
    Solid {
        color: [f32; 4],
    },
    Linear {
        start: [f32; 2],
        end: [f32; 2],
        stops: Vec<Stop>,
    },
    /// A two-point conical gradient: the focal circle (radius 0) at `focal`,
    /// the end circle at `center` with `radius` (Lottie's highlight).
    Radial {
        center: [f32; 2],
        focal: [f32; 2],
        radius: f32,
        stops: Vec<Stop>,
    },
}

/// Stroke cap.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Cap {
    #[default]
    Butt,
    Round,
    Square,
}

/// Stroke join.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Join {
    #[default]
    Miter,
    Round,
    Bevel,
}

/// Stroke style.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StrokeStyle {
    pub width: f32,
    pub cap: Cap,
    pub join: Join,
    pub miter: f32,
    /// Dash lengths (empty = solid).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dashes: Vec<f32>,
    #[serde(default)]
    pub dash_offset: f32,
}

/// How a layer composites onto what is below it: the 16 W3C compositing
/// blend modes, then the operators mattes, masks and merge paths need.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Blend {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
    /// Porter-Duff destination-in: keep what is below where the layer is opaque (alpha matte).
    DstIn,
    /// Porter-Duff destination-out (inverted alpha matte, subtract).
    DstOut,
    /// Destination × the layer's luminance (premultiplied; luma matte).
    LumaIn,
    /// Destination × (1 − the layer's luminance) (inverted luma matte).
    LumaOut,
    /// Porter-Duff xor (merge paths: exclude intersections).
    Xor,
}

impl Blend {
    /// The packed code (stable; `README.md` § Packed frames).
    pub fn code(self) -> u8 {
        self as u8
    }
}

/// A filter applied to a layer's content before it composites (effects,
/// mask feather). Applied in order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Filter {
    /// Gaussian blur; standard deviations in composition units (painters
    /// multiply by their fit scale).
    Blur { sigma_x: f32, sigma_y: f32 },
    /// A 4×5 row-major colour matrix on straight RGBA (like SVG `feColorMatrix`).
    ColorMatrix { m: [f32; 20] },
    /// Map each pixel's luminance through `stops`, then mix with the original by `amount`.
    GradientMap { stops: Vec<Stop>, amount: f32 },
    /// A drop shadow under the content: offset in frame units, blur sigma, colour.
    DropShadow { color: [f32; 4], dx: f32, dy: f32, sigma: f32 },
}

/// One drawing operation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum Op {
    /// Open an isolated group, clipped to `clip`, composited on `Pop`.
    Layer {
        blend: Blend,
        alpha: f32,
        transform: Transform,
        clip: Vec<Verb>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        filters: Vec<Filter>,
    },
    /// Intersect the current clip with `path` until the matching `Pop`.
    Clip {
        transform: Transform,
        path: Vec<Verb>,
    },
    /// Close the innermost `Layer` or `Clip`.
    Pop,
    Fill {
        transform: Transform,
        rule: FillRule,
        brush: Brush,
        path: Vec<Verb>,
    },
    Stroke {
        transform: Transform,
        style: StrokeStyle,
        brush: Brush,
        path: Vec<Verb>,
    },
    /// Draw image asset `asset` (an index into [`crate::Motion::images`]) into
    /// the rect `0,0,width,height` of its local space.
    Image {
        transform: Transform,
        asset: u32,
        alpha: f32,
        width: f32,
        height: f32,
    },
}

/// One sampled frame.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DisplayList {
    /// Composition size the ops are laid out in (the painter scales to fit).
    pub width: f32,
    pub height: f32,
    pub ops: Vec<Op>,
}

// --- Packed encoding -----------------------------------------------------

/// Op codes of the packed stream.
pub mod code {
    pub const LAYER: f32 = 1.0;
    pub const CLIP: f32 = 2.0;
    pub const POP: f32 = 3.0;
    pub const FILL: f32 = 4.0;
    pub const STROKE: f32 = 5.0;
    pub const IMAGE: f32 = 6.0;
    pub const VERB_M: f32 = 0.0;
    pub const VERB_L: f32 = 1.0;
    pub const VERB_C: f32 = 2.0;
    pub const VERB_Z: f32 = 3.0;
    pub const BRUSH_SOLID: f32 = 0.0;
    pub const BRUSH_LINEAR: f32 = 1.0;
    pub const BRUSH_RADIAL: f32 = 2.0;
    pub const FILTER_BLUR: f32 = 0.0;
    pub const FILTER_COLOR_MATRIX: f32 = 1.0;
    pub const FILTER_GRADIENT_MAP: f32 = 2.0;
    pub const FILTER_DROP_SHADOW: f32 = 3.0;
    /// Format version, the stream's first word.
    pub const VERSION: f32 = 1.0;
}

impl DisplayList {
    /// Clear for reuse (keeps allocations).
    pub fn clear(&mut self) {
        self.ops.clear();
    }

    /// The packed `f32` stream: `[VERSION, width, height, op…]`.
    pub fn encode(&self, out: &mut Vec<f32>) {
        out.clear();
        out.extend_from_slice(&[code::VERSION, self.width, self.height]);
        for op in &self.ops {
            match op {
                Op::Layer {
                    blend,
                    alpha,
                    transform,
                    clip,
                    filters,
                } => {
                    out.extend_from_slice(&[code::LAYER, blend.code() as f32, *alpha]);
                    out.extend_from_slice(transform);
                    encode_path(clip, out);
                    out.push(filters.len() as f32);
                    for f in filters {
                        encode_filter(f, out);
                    }
                }
                Op::Clip { transform, path } => {
                    out.push(code::CLIP);
                    out.extend_from_slice(transform);
                    encode_path(path, out);
                }
                Op::Pop => out.push(code::POP),
                Op::Fill {
                    transform,
                    rule,
                    brush,
                    path,
                } => {
                    out.push(code::FILL);
                    out.extend_from_slice(transform);
                    out.push(match rule {
                        FillRule::Nonzero => 0.0,
                        FillRule::Evenodd => 1.0,
                    });
                    encode_brush(brush, out);
                    encode_path(path, out);
                }
                Op::Stroke {
                    transform,
                    style,
                    brush,
                    path,
                } => {
                    out.push(code::STROKE);
                    out.extend_from_slice(transform);
                    out.extend_from_slice(&[
                        style.width,
                        style.cap as u8 as f32,
                        style.join as u8 as f32,
                        style.miter,
                        style.dash_offset,
                        style.dashes.len() as f32,
                    ]);
                    out.extend_from_slice(&style.dashes);
                    encode_brush(brush, out);
                    encode_path(path, out);
                }
                Op::Image {
                    transform,
                    asset,
                    alpha,
                    width,
                    height,
                } => {
                    out.push(code::IMAGE);
                    out.extend_from_slice(transform);
                    out.extend_from_slice(&[*asset as f32, *alpha, *width, *height]);
                }
            }
        }
    }

    /// Number of path vertices in the frame (the limits' vertex count).
    pub fn vertex_count(&self) -> usize {
        self.ops
            .iter()
            .map(|op| match op {
                Op::Layer { clip: p, .. }
                | Op::Clip { path: p, .. }
                | Op::Fill { path: p, .. }
                | Op::Stroke { path: p, .. } => p.len(),
                _ => 0,
            })
            .sum()
    }
}

fn encode_path(path: &[Verb], out: &mut Vec<f32>) {
    out.push(path.len() as f32);
    for verb in path {
        match *verb {
            Verb::M { x, y } => out.extend_from_slice(&[code::VERB_M, x, y]),
            Verb::L { x, y } => out.extend_from_slice(&[code::VERB_L, x, y]),
            Verb::C {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => out.extend_from_slice(&[code::VERB_C, x1, y1, x2, y2, x, y]),
            Verb::Z => out.push(code::VERB_Z),
        }
    }
}

fn encode_brush(brush: &Brush, out: &mut Vec<f32>) {
    let stops = |stops: &[Stop], out: &mut Vec<f32>| {
        out.push(stops.len() as f32);
        for s in stops {
            out.push(s.offset);
            out.extend_from_slice(&s.color);
        }
    };
    match brush {
        Brush::Solid { color } => {
            out.push(code::BRUSH_SOLID);
            out.extend_from_slice(color);
        }
        Brush::Linear { start, end, stops: s } => {
            out.extend_from_slice(&[code::BRUSH_LINEAR, start[0], start[1], end[0], end[1]]);
            stops(s, out);
        }
        Brush::Radial {
            center,
            focal,
            radius,
            stops: s,
        } => {
            out.extend_from_slice(&[
                code::BRUSH_RADIAL,
                center[0],
                center[1],
                focal[0],
                focal[1],
                *radius,
            ]);
            stops(s, out);
        }
    }
}

fn encode_filter(f: &Filter, out: &mut Vec<f32>) {
    match f {
        Filter::Blur { sigma_x, sigma_y } => out.extend_from_slice(&[code::FILTER_BLUR, *sigma_x, *sigma_y]),
        Filter::ColorMatrix { m } => {
            out.push(code::FILTER_COLOR_MATRIX);
            out.extend_from_slice(m);
        }
        Filter::GradientMap { stops, amount } => {
            out.extend_from_slice(&[code::FILTER_GRADIENT_MAP, *amount, stops.len() as f32]);
            for s in stops {
                out.push(s.offset);
                out.extend_from_slice(&s.color);
            }
        }
        Filter::DropShadow { color, dx, dy, sigma } => {
            out.push(code::FILTER_DROP_SHADOW);
            out.extend_from_slice(color);
            out.extend_from_slice(&[*dx, *dy, *sigma]);
        }
    }
}
