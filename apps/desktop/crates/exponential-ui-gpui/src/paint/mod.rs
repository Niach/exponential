//! The native painters. The core hands every node a RESOLVED `Visual`;
//! [`PaintStyle`] is that visual parsed once (colours → `Hsla`, shadows →
//! `BoxShadow`s, per-side borders and per-corner radii expanded, the
//! transform list folded into one affine) and cached per node until the core
//! reports a visual change. Containers paint only their box (background,
//! gradient, border, radii, shadow, opacity, clip); measured leaves paint
//! their content inside the box, inset by the recipe padding + border the
//! measurer counted.

pub mod chart;
pub mod color;
pub mod date;
pub mod icons;
pub mod markdown;
pub mod natives;
pub mod parts;

use exponential_ui::style::{Transition, TransformOp, Visual};
use gpui::{BoxShadow, CursorStyle, Hsla};

use color::{color_of, mix, shadows};

/// `textDecoration`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decoration {
    Underline,
    LineThrough,
}

/// A linear gradient (CSS angle: 0 = up, 90 = right), stops 0..1.
#[derive(Debug, Clone, PartialEq)]
pub struct GradientPaint {
    pub angle: f32,
    pub stops: Vec<(Hsla, f32)>,
}

/// The paint-only transform folded into translate + uniform scale +
/// rotation about the box centre (CSS `transform-origin: center`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Affine {
    pub tx: f32,
    pub ty: f32,
    pub scale: f32,
    /// Degrees, clockwise.
    pub rotate: f32,
}

impl Default for Affine {
    fn default() -> Self {
        Affine { tx: 0.0, ty: 0.0, scale: 1.0, rotate: 0.0 }
    }
}

impl Affine {
    /// The source-order transform list as one affine (`[a b c d e f]`
    /// multiplied left to right like CSS, then decomposed).
    pub fn of(ops: &[TransformOp]) -> Affine {
        // Row-major 2×3: x' = a·x + c·y + e, y' = b·x + d·y + f.
        let mut m = [1.0f32, 0.0, 0.0, 1.0, 0.0, 0.0];
        for op in ops {
            let n = match *op {
                TransformOp::Translate { x, y } => [1.0, 0.0, 0.0, 1.0, x, y],
                TransformOp::Scale { factor } => [factor, 0.0, 0.0, factor, 0.0, 0.0],
                TransformOp::Rotate { degrees } => {
                    let r = degrees.to_radians();
                    [r.cos(), r.sin(), -r.sin(), r.cos(), 0.0, 0.0]
                }
            };
            m = [
                m[0] * n[0] + m[2] * n[1],
                m[1] * n[0] + m[3] * n[1],
                m[0] * n[2] + m[2] * n[3],
                m[1] * n[2] + m[3] * n[3],
                m[0] * n[4] + m[2] * n[5] + m[4],
                m[1] * n[4] + m[3] * n[5] + m[5],
            ];
        }
        let scale = (m[0] * m[0] + m[1] * m[1]).sqrt();
        let rotate = m[1].atan2(m[0]).to_degrees();
        Affine { tx: m[4], ty: m[5], scale, rotate }
    }

    pub fn is_identity(&self) -> bool {
        self.tx == 0.0 && self.ty == 0.0 && (self.scale - 1.0).abs() < 1e-4 && self.rotate.abs() < 1e-3
    }
}

/// A `Visual` parsed for gpui.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PaintStyle {
    pub bg: Option<Hsla>,
    pub gradient: Option<GradientPaint>,
    pub border_color: Option<Hsla>,
    /// `[top, right, bottom, left]` (layout-effective: the frames include them).
    pub border: [f32; 4],
    /// `dashed` (and `dotted`, which gpui paints dashed).
    pub dashed: bool,
    /// `[topLeft, topRight, bottomRight, bottomLeft]`.
    pub radii: [f32; 4],
    pub opacity: Option<f32>,
    pub shadows: Vec<BoxShadow>,
    /// The node's OWN text colour (inheritance is resolved separately).
    pub color: Option<Hsla>,
    /// PHYSICAL `left | right | center | justify` (the core resolved start/end).
    pub text_align: Option<String>,
    /// A measured leaf's padding `[top, right, bottom, left]`.
    pub padding: [f32; 4],
    pub gap: f32,
    pub clip_x: bool,
    pub clip_y: bool,
    pub scroll_x: bool,
    pub scroll_y: bool,
    pub transform: Affine,
    pub transition: Option<Transition>,
    /// `visibility: hidden`.
    pub invisible: bool,
    /// `pointerEvents: none`.
    pub pointer_none: bool,
    pub cursor: Option<CursorStyle>,
    pub italic: bool,
    pub decoration: Option<Decoration>,
    pub text_transform: Option<String>,
    pub letter_spacing: f32,
    /// `userSelect: none`.
    pub no_select: bool,
    /// The recipe asked for the platform's own control.
    pub native: bool,
    /// A Chart's resolved series colours.
    pub series: Vec<Hsla>,
    /// Round 2: the node's own resolved direction (`Some(true)` = rtl): a
    /// leaf's text shapes with it as the bidi paragraph direction.
    pub rtl: Option<bool>,
    /// Round 2: `backdropBlur` (px). The pinned gpui samples nothing behind
    /// a box, so it paints the (translucent) background alone (§2 fallback).
    pub backdrop_blur: Option<f32>,
    /// Round 2: a keyframe `animation` with its resolved timing.
    pub animation: Option<exponential_ui::style::VisualAnimation>,
    /// Round 2: `position: sticky` (pinned by the core's sticky offsets).
    pub sticky: bool,
}

fn overflow_clips(v: Option<&str>) -> (bool, bool) {
    match v {
        Some("hidden" | "clip") => (true, false),
        Some("scroll" | "auto") => (true, true),
        _ => (false, false),
    }
}

/// `cursor` → gpui.
pub fn cursor_of(name: &str) -> Option<CursorStyle> {
    Some(match name {
        "pointer" => CursorStyle::PointingHand,
        "text" => CursorStyle::IBeam,
        "not-allowed" => CursorStyle::OperationNotAllowed,
        "grab" => CursorStyle::OpenHand,
        "grabbing" => CursorStyle::ClosedHand,
        "move" => CursorStyle::ClosedHand,
        "col-resize" => CursorStyle::ResizeColumn,
        "row-resize" => CursorStyle::ResizeRow,
        "default" => CursorStyle::Arrow,
        _ => return None,
    })
}

impl PaintStyle {
    pub fn from_visual(v: &Visual) -> PaintStyle {
        let uniform = v.border_width.unwrap_or(0.0).max(0.0);
        let border = v.border_widths.map(|b| b.map(|w| w.max(0.0))).unwrap_or([uniform; 4]);
        let r = v.border_radius.unwrap_or(0.0).max(0.0);
        let radii = v.corner_radii.map(|c| c.map(|x| x.max(0.0))).unwrap_or([r; 4]);
        let padding = v.padding.unwrap_or_else(|| {
            let (h, vv) = (v.padding_horizontal.unwrap_or(0.0), v.padding_vertical.unwrap_or(0.0));
            [vv, h, vv, h]
        });
        let (mut clip_x, mut scroll_x) = overflow_clips(v.overflow_x.as_deref());
        let (mut clip_y, mut scroll_y) = overflow_clips(v.overflow_y.as_deref());
        if v.overflow_hidden {
            clip_x |= v.overflow_x.is_none();
            clip_y |= v.overflow_y.is_none();
        }
        if v.overflow_scroll && v.overflow_x.is_none() && v.overflow_y.is_none() {
            clip_x = true;
            clip_y = true;
            scroll_x = true;
            scroll_y = true;
        }
        let gradient = v.background_gradient.as_ref().and_then(|g| {
            let stops: Vec<(Hsla, f32)> = g.stops.iter().filter_map(|s| color_of(Some(&s.color)).map(|c| (c, s.offset.clamp(0.0, 1.0)))).collect();
            (stops.len() >= 2).then_some(GradientPaint { angle: g.angle, stops })
        });
        PaintStyle {
            bg: color_of(v.background_color.as_deref()).filter(|c| c.a > 0.0),
            gradient,
            border_color: color_of(v.border_color.as_deref()),
            border,
            dashed: matches!(v.border_style.as_deref(), Some("dashed" | "dotted")),
            radii,
            opacity: v.opacity,
            shadows: v.box_shadow.as_deref().map(shadows).unwrap_or_default(),
            color: color_of(v.color.as_deref()),
            text_align: v.text_align.clone(),
            padding,
            gap: v.gap.unwrap_or(0.0),
            clip_x,
            clip_y,
            scroll_x,
            scroll_y,
            transform: v.transform.as_deref().map(Affine::of).unwrap_or_default(),
            transition: v.transition.filter(|t| t.duration_ms > 0.0),
            invisible: v.visibility_hidden,
            pointer_none: v.pointer_events_none,
            cursor: v.cursor.as_deref().and_then(cursor_of),
            italic: v.font_style.as_deref() == Some("italic"),
            decoration: match v.text_decoration.as_deref() {
                Some("underline") => Some(Decoration::Underline),
                Some("line-through") => Some(Decoration::LineThrough),
                _ => None,
            },
            text_transform: v.text_transform.clone().filter(|t| t != "none"),
            letter_spacing: v.letter_spacing.unwrap_or(0.0),
            no_select: v.user_select.as_deref() == Some("none"),
            native: v.native,
            series: v.series_colors.as_deref().map(|c| c.iter().filter_map(|s| color_of(Some(s))).collect()).unwrap_or_default(),
            rtl: v.direction.as_deref().map(|d| d == "rtl"),
            backdrop_blur: v.backdrop_blur.filter(|b| *b > 0.0),
            animation: v.animation.clone(),
            sticky: v.sticky,
        }
    }

    /// The content insets of a measured leaf (padding + border) as
    /// `[top, right, bottom, left]`.
    pub fn insets(&self) -> [f32; 4] {
        [self.padding[0] + self.border[0], self.padding[1] + self.border[1], self.padding[2] + self.border[2], self.padding[3] + self.border[3]]
    }

    /// The uniform radius (the largest corner) for parts that paint one.
    pub fn radius(&self) -> f32 {
        self.radii.iter().copied().fold(0.0, f32::max)
    }

    pub fn has_border(&self) -> bool {
        self.border.iter().any(|w| *w > 0.0) && self.border_color.is_some_and(|c| c.a > 0.0)
    }

    pub fn clips(&self) -> bool {
        self.clip_x || self.clip_y
    }

    /// The style `t` (eased, 0..1) of the way from `self` to `to`: colours,
    /// opacity, radii and the transform blend; everything else is `to`'s.
    pub fn lerp(&self, to: &PaintStyle, t: f32) -> PaintStyle {
        let t = t.clamp(0.0, 1.0);
        let c = |a: Option<Hsla>, b: Option<Hsla>| match (a, b) {
            (Some(a), Some(b)) => Some(mix(a, b, t)),
            (None, Some(b)) => Some(mix(Hsla { a: 0.0, ..b }, b, t)),
            (Some(a), None) => (t < 1.0).then(|| mix(a, Hsla { a: 0.0, ..a }, t)),
            (None, None) => None,
        };
        let f = |a: f32, b: f32| a + (b - a) * t;
        let mut out = to.clone();
        out.bg = c(self.bg, to.bg);
        out.border_color = c(self.border_color, to.border_color);
        out.color = match (self.color, to.color) {
            (Some(a), Some(b)) => Some(mix(a, b, t)),
            _ => to.color,
        };
        out.opacity = match (self.opacity, to.opacity) {
            (None, None) => None,
            (a, b) => Some(f(a.unwrap_or(1.0), b.unwrap_or(1.0))),
        };
        out.radii = [f(self.radii[0], to.radii[0]), f(self.radii[1], to.radii[1]), f(self.radii[2], to.radii[2]), f(self.radii[3], to.radii[3])];
        out.transform = Affine {
            tx: f(self.transform.tx, to.transform.tx),
            ty: f(self.transform.ty, to.transform.ty),
            scale: f(self.transform.scale, to.transform.scale),
            rotate: f(self.transform.rotate, to.transform.rotate),
        };
        out
    }
}

/// CSS `cubic-bezier(x1, y1, x2, y2)` at progress `x` (0..1).
pub fn cubic_bezier(e: [f32; 4], x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    let [x1, y1, x2, y2] = e;
    let bez = |t: f32, p1: f32, p2: f32| {
        let u = 1.0 - t;
        3.0 * u * u * t * p1 + 3.0 * u * t * t * p2 + t * t * t
    };
    let d = |t: f32, p1: f32, p2: f32| {
        let u = 1.0 - t;
        3.0 * u * u * p1 + 6.0 * u * t * (p2 - p1) + 3.0 * t * t * (1.0 - p2)
    };
    // Newton on x(t) = x, bisection when the slope vanishes.
    let mut t = x;
    for _ in 0..8 {
        let err = bez(t, x1, x2) - x;
        if err.abs() < 1e-5 {
            return bez(t, y1, y2);
        }
        let slope = d(t, x1, x2);
        if slope.abs() < 1e-6 {
            break;
        }
        t = (t - err / slope).clamp(0.0, 1.0);
    }
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    t = x;
    for _ in 0..30 {
        let v = bez(t, x1, x2);
        if (v - x).abs() < 1e-5 {
            break;
        }
        if v < x {
            lo = t;
        } else {
            hi = t;
        }
        t = (lo + hi) / 2.0;
    }
    bez(t, y1, y2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use exponential_ui::style::{Gradient, GradientStop};

    #[test]
    fn per_side_borders_corners_and_padding_expand() {
        let v = Visual { border_width: Some(1.0), border_widths: Some([0.0, 0.0, 2.0, 0.0]), border_color: Some("#ff0000".into()), border_style: Some("dotted".into()), border_radius: Some(4.0), corner_radii: Some([8.0, 0.0, 0.0, 8.0]), padding: Some([1.0, 2.0, 3.0, 4.0]), ..Default::default() };
        let s = PaintStyle::from_visual(&v);
        assert_eq!(s.border, [0.0, 0.0, 2.0, 0.0]);
        assert!(s.dashed);
        assert_eq!(s.radii, [8.0, 0.0, 0.0, 8.0]);
        assert_eq!(s.insets(), [1.0, 2.0, 5.0, 4.0]);
        let u = PaintStyle::from_visual(&Visual { border_width: Some(1.0), border_radius: Some(6.0), padding_horizontal: Some(12.0), padding_vertical: Some(4.0), ..Default::default() });
        assert_eq!(u.border, [1.0; 4]);
        assert_eq!(u.radii, [6.0; 4]);
        assert_eq!(u.insets(), [5.0, 13.0, 5.0, 13.0]);
    }

    #[test]
    fn overflow_axes_gradients_and_text_keys_parse() {
        let v = Visual {
            overflow_y: Some("auto".into()),
            overflow_x: Some("hidden".into()),
            background_gradient: Some(Gradient { angle: 90.0, stops: vec![GradientStop { color: "#000000".into(), offset: 0.0 }, GradientStop { color: "#ffffff".into(), offset: 1.0 }] }),
            text_decoration: Some("line-through".into()),
            font_style: Some("italic".into()),
            text_transform: Some("uppercase".into()),
            cursor: Some("pointer".into()),
            ..Default::default()
        };
        let s = PaintStyle::from_visual(&v);
        assert!(s.clip_x && s.clip_y && s.scroll_y && !s.scroll_x);
        assert_eq!(s.gradient.as_ref().map(|g| g.stops.len()), Some(2));
        assert_eq!(s.decoration, Some(Decoration::LineThrough));
        assert!(s.italic);
        assert_eq!(s.text_transform.as_deref(), Some("uppercase"));
        assert_eq!(s.cursor, Some(CursorStyle::PointingHand));
    }

    #[test]
    fn transforms_fold_in_source_order() {
        let a = Affine::of(&[TransformOp::Translate { x: 10.0, y: 0.0 }, TransformOp::Scale { factor: 2.0 }]);
        assert_eq!((a.tx, a.ty, a.scale), (10.0, 0.0, 2.0));
        let b = Affine::of(&[TransformOp::Scale { factor: 2.0 }, TransformOp::Translate { x: 10.0, y: 0.0 }]);
        assert_eq!((b.tx, b.scale), (20.0, 2.0), "a scale first doubles the later translate");
        let r = Affine::of(&[TransformOp::Rotate { degrees: 90.0 }]);
        assert!((r.rotate - 90.0).abs() < 1e-3);
        assert!(Affine::default().is_identity());
    }

    #[test]
    fn easing_and_style_blending() {
        assert!((cubic_bezier([0.25, 0.1, 0.25, 1.0], 0.0)).abs() < 1e-4);
        assert!((cubic_bezier([0.25, 0.1, 0.25, 1.0], 1.0) - 1.0).abs() < 1e-4);
        let linear = cubic_bezier([0.0, 0.0, 1.0, 1.0], 0.3);
        assert!((linear - 0.3).abs() < 1e-3, "{linear}");
        let mid = cubic_bezier([0.2, 0.0, 0.0, 1.0], 0.5);
        assert!(mid > 0.5, "decelerating curves run ahead: {mid}");
        let from = PaintStyle { opacity: Some(0.0), ..Default::default() };
        let to = PaintStyle { opacity: Some(1.0), ..Default::default() };
        assert_eq!(from.lerp(&to, 0.5).opacity, Some(0.5));
        assert_eq!(from.lerp(&to, 1.0), to);
    }
}
