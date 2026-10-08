//! The native painters. The core hands every node a RESOLVED `Visual`;
//! [`PaintStyle`] is that visual parsed once (colours → `Hsla`, shadows →
//! `BoxShadow`s) and cached per node until the core reports a visual change.
//! Containers paint only their box (background, border, radius, shadow,
//! opacity, clip); measured leaves paint their content inside the box,
//! inset by the recipe padding the measurer counted.

pub mod chart;
pub mod color;
pub mod date;
pub mod icons;
pub mod markdown;
pub mod natives;
pub mod parts;

use exponential_ui::style::Visual;
use gpui::{BoxShadow, Hsla};

use color::{color_of, shadows};

/// A `Visual` parsed for gpui.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PaintStyle {
    pub bg: Option<Hsla>,
    pub border_color: Option<Hsla>,
    pub border_width: f32,
    pub radius: f32,
    pub opacity: Option<f32>,
    pub shadows: Vec<BoxShadow>,
    /// The node's OWN text colour (inheritance is resolved separately).
    pub color: Option<Hsla>,
    pub text_align: Option<String>,
    pub pad_h: f32,
    pub pad_v: f32,
    pub gap: f32,
    pub overflow_hidden: bool,
    pub overflow_scroll: bool,
}

impl PaintStyle {
    pub fn from_visual(v: &Visual) -> PaintStyle {
        PaintStyle {
            bg: color_of(v.background_color.as_deref()).filter(|c| c.a > 0.0),
            border_color: color_of(v.border_color.as_deref()),
            border_width: v.border_width.unwrap_or(0.0).max(0.0),
            radius: v.border_radius.unwrap_or(0.0).max(0.0),
            opacity: v.opacity,
            shadows: v.box_shadow.as_deref().map(shadows).unwrap_or_default(),
            color: color_of(v.color.as_deref()),
            text_align: v.text_align.clone(),
            pad_h: v.padding_horizontal.unwrap_or(0.0),
            pad_v: v.padding_vertical.unwrap_or(0.0),
            gap: v.gap.unwrap_or(0.0),
            overflow_hidden: v.overflow_hidden,
            overflow_scroll: v.overflow_scroll,
        }
    }

    /// The content inset of a measured leaf (padding + border), per axis.
    pub fn inset(&self) -> (f32, f32) {
        (self.pad_h + self.border_width, self.pad_v + self.border_width)
    }
}
