//! The imported animation: what `import.rs` builds from Lottie JSON and
//! `render.rs` samples. Field names follow the Lottie keys they come from.

use crate::anim::Prop;
use crate::display::Blend;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct Animation {
    pub width: f64,
    pub height: f64,
    pub frame_rate: f64,
    pub in_point: f64,
    pub out_point: f64,
    pub layers: Vec<Layer>,
    /// Precompositions by asset id.
    pub precomps: HashMap<String, Vec<Layer>>,
    pub images: Vec<ImageAsset>,
    pub image_ids: HashMap<String, usize>,
    pub markers: Vec<Marker>,
    pub fonts: Vec<Font>,
    /// Glyph outlines shipped in the file (`chars`).
    pub chars: Vec<Char>,
    /// Fonts the host registered (`Motion::add_font`), for text without `chars`.
    pub font_book: crate::text::FontBook,
}

#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct Marker {
    pub name: String,
    /// Start frame.
    pub time: f64,
    /// Duration in frames.
    pub duration: f64,
}

#[derive(Clone, Debug, Default)]
pub struct ImageAsset {
    pub id: String,
    pub width: f64,
    pub height: f64,
    /// `u` + `p`, or the data URL.
    pub location: String,
}

#[derive(Clone, Debug, Default)]
pub struct Font {
    pub name: String,
    pub family: String,
    pub style: String,
    pub ascent: f64,
}

#[derive(Clone, Debug, Default)]
pub struct Char {
    pub ch: String,
    pub family: String,
    pub style: String,
    pub size: f64,
    pub width: f64,
    /// Glyph shapes (in a 100-unit em, lottie convention).
    pub shapes: Vec<Item>,
}

/// Transform of a layer (`ks`) or a group (`tr`).
#[derive(Clone, Debug, Default)]
pub struct Transform {
    pub anchor: Option<Prop>,
    pub position: Position,
    pub scale: Option<Prop>,
    pub rotation: Option<Prop>,
    pub skew: Option<Prop>,
    pub skew_axis: Option<Prop>,
    pub opacity: Option<Prop>,
    /// Repeater transforms only: start/end opacity.
    pub start_opacity: Option<Prop>,
    pub end_opacity: Option<Prop>,
}

#[derive(Clone, Debug, Default)]
pub enum Position {
    #[default]
    None,
    Joined(Prop),
    Split(Prop, Prop),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatteMode {
    Alpha,
    AlphaInverted,
    Luma,
    LumaInverted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaskMode {
    None,
    Add,
    Subtract,
    Intersect,
    Lighten,
    Darken,
    Difference,
}

#[derive(Clone, Debug)]
pub struct Mask {
    pub mode: MaskMode,
    pub inverted: bool,
    pub path: Prop,
    pub opacity: Option<Prop>,
    pub expansion: Option<Prop>,
    pub feather: Option<Prop>,
}

/// An effect (`ef`): its type and its controls' values by index.
#[derive(Clone, Debug)]
pub struct Effect {
    pub ty: i64,
    pub name: String,
    pub enabled: bool,
    /// Control values in order (`ef[*].v`).
    pub values: Vec<Prop>,
}

#[derive(Clone, Debug)]
pub enum LayerKind {
    Null,
    Shape(Vec<Item>),
    Solid { color: [f64; 4], width: f64, height: f64 },
    Image { asset: usize },
    Precomp { id: String, width: f64, height: f64, time_remap: Option<Prop> },
    Text(Box<crate::text::TextData>),
    /// Camera, audio, data, unknown: evaluated for parenting only.
    Other,
}

#[derive(Clone, Debug)]
pub struct Layer {
    pub kind: LayerKind,
    pub name: String,
    pub index: Option<i64>,
    pub parent: Option<i64>,
    pub in_point: f64,
    pub out_point: f64,
    pub start_time: f64,
    pub stretch: f64,
    pub hidden: bool,
    pub transform: Transform,
    pub auto_orient: bool,
    pub blend: Blend,
    pub masks: Vec<Mask>,
    pub matte_mode: Option<MatteMode>,
    /// `tp`: the `ind` of the matte source (modern), else the layer above.
    pub matte_parent: Option<i64>,
    /// `td`: this layer is a matte source and is not drawn by itself.
    pub is_matte: bool,
    pub effects: Vec<Effect>,
    pub three_d: bool,
}

/// A shape item (`shapes` / `it`).
#[derive(Clone, Debug)]
pub enum Item {
    Group { items: Vec<Item>, transform: Option<Transform> },
    Rect { position: Prop, size: Prop, roundness: Prop, reversed: bool },
    Ellipse { position: Prop, size: Prop, reversed: bool },
    Star(Box<Star>),
    Path { shape: Prop, reversed: bool },
    Fill { color: Prop, opacity: Prop, evenodd: bool },
    Stroke(Box<StrokeItem>),
    GradientFill { gradient: Box<Gradient>, opacity: Prop, evenodd: bool },
    GradientStroke(Box<StrokeItem>),
    Trim { start: Prop, end: Prop, offset: Prop, individually: bool },
    Repeater { copies: Prop, offset: Prop, below: bool, transform: Transform },
    RoundCorners { radius: Prop },
    PuckerBloat { amount: Prop },
    ZigZag { size: Prop, ridges: Prop, smooth: bool },
    OffsetPath { amount: Prop, join: u8, miter: Prop },
    Twist { angle: Prop, center: Prop },
    Merge { mode: u8 },
}

#[derive(Clone, Debug)]
pub struct Star {
    pub polygon: bool,
    pub position: Prop,
    pub outer_radius: Prop,
    pub outer_roundness: Prop,
    pub inner_radius: Prop,
    pub inner_roundness: Prop,
    pub rotation: Prop,
    pub points: Prop,
    pub reversed: bool,
}

#[derive(Clone, Debug)]
pub struct Gradient {
    pub radial: bool,
    pub start: Prop,
    pub end: Prop,
    pub highlight_length: Prop,
    pub highlight_angle: Prop,
    /// Number of colour stops (`g.p`).
    pub count: usize,
    pub stops: Prop,
}

#[derive(Clone, Debug)]
pub struct StrokeItem {
    /// Solid colour, or a gradient for gradient strokes.
    pub color: Option<Prop>,
    pub gradient: Option<Gradient>,
    pub opacity: Prop,
    pub width: Prop,
    pub cap: u8,
    pub join: u8,
    pub miter: f64,
    /// `(kind, value)`: `d` dash, `g` gap, `o` offset.
    pub dashes: Vec<(char, Prop)>,
}
