//! Style RESOLUTION for layout: a node's style object (token references
//! already resolved by the theme, conditions still nested) → the flat map for
//! this pass (`conditions::resolve_conditions`: media against the SURFACE
//! box, the state keys against the node's states) → logical properties made
//! physical from the surface direction → a taffy `Style` plus the painted
//! [`Visual`].
//!
//! Rules from the VAPP-4 spike that every painter relies on: border-box, flex
//! row default, `min-width: auto`, `overflow: hidden` = clip, the border
//! widths (`borderWidth` and the four sides) are the visual keys with layout
//! effect, absolute nodes paint last among their siblings (no z-index).
//! Round 1 adds the per-side and per-corner keys, gradients, paint-only
//! transforms, transitions (resolved durations + easings), text styling,
//! `overflowX/Y`, `visibility`, `pointerEvents`, `userSelect`, `cursor`.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use taffy::prelude::*;
use taffy::style::{AlignContent, AlignItems, BoxSizing, Direction, Display, FlexDirection, FlexWrap, LengthPercentage, LengthPercentageAuto, Overflow, Position, TextAlign};

pub use crate::conditions::{is_condition_key, resolve_conditions, ConditionContext};
use crate::tracks;

/// One shadow layer, resolved.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ShadowLayer {
    pub x: f32,
    pub y: f32,
    pub blur: f32,
    pub spread: f32,
    pub color: String,
}

/// One gradient stop: a resolved colour at 0..1.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct GradientStop {
    pub color: String,
    pub offset: f32,
}

/// A linear gradient painted OVER `background_color` (CSS angle: 0 = up,
/// 90 = right).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Gradient {
    pub angle: f32,
    pub stops: Vec<GradientStop>,
}

/// One paint-only transform function, in source order (applied around the
/// box's centre, like CSS `transform-origin: center`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum TransformOp {
    Translate { x: f32, y: f32 },
    Scale { factor: f32 },
    Rotate { degrees: f32 },
}

/// The motion a node's colour/opacity/transform/size changes animate with:
/// the resolved `$motion.*` duration (0 under reduced motion) and the
/// `$ease.*` cubic bezier (CSS `ease` when unset).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Transition {
    pub duration_ms: f32,
    pub easing: [f32; 4],
}

/// CSS `ease`, the easing of a `transition` without `transitionEasing`.
pub const DEFAULT_EASING: [f32; 4] = [0.25, 0.1, 0.25, 1.0];

/// Painted properties, RESOLVED (hex colours, px numbers, shadow layers,
/// family names). Painters never read the theme; they paint this.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Visual {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_gradient: Option<Gradient>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// The uniform border width (`borderWidth`); per-side widths are in
    /// `border_widths` when any side differs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_width: Option<f32>,
    /// `[top, right, bottom, left]` when a per-side width is set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border_widths: Option<[f32; 4]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_color: Option<String>,
    /// `solid | dashed | dotted` (solid when unset).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border_style: Option<String>,
    /// The uniform radius in px (a `%` resolves against the laid-out box).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_radius: Option<f32>,
    /// `[topLeft, topRight, bottomRight, bottomLeft]` when a corner is set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corner_radii: Option<[f32; 4]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub box_shadow: Option<Vec<ShadowLayer>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_weight: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_height: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    /// PHYSICAL: `left | right | center | justify` (start/end resolved by
    /// the surface direction).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_align: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub letter_spacing: Option<f32>,
    /// `none | underline | line-through`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_decoration: Option<String>,
    /// `none | uppercase | lowercase | capitalize`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_transform: Option<String>,
    /// `normal | italic`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_style: Option<String>,
    /// The recipe's padding on a MEASURED leaf (a Button's label inset): the
    /// painter draws it, the measurer includes it; taffy never sees it.
    /// The averages of the two sides; `padding` has each side.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub padding_horizontal: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub padding_vertical: Option<f32>,
    /// A leaf's padding `[top, right, bottom, left]` (logical keys resolved).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub padding: Option<[f32; 4]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gap: Option<f32>,
    /// `native: true` on the part's recipe: the painter may use the platform's
    /// own control (geometry still from the tokens).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub native: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub overflow_hidden: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub overflow_scroll: bool,
    /// Per axis: `visible | hidden | clip | scroll | auto` (overflow merged).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overflow_x: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overflow_y: Option<String>,
    /// Paint-only transform functions in source order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<Vec<TransformOp>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition: Option<Transition>,
    /// `visibility: hidden`: the box stays, nothing paints, out of the a11y tree.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub visibility_hidden: bool,
    /// `pointerEvents: none`: presses pass through.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pointer_events_none: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_select: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    /// A Chart's series (or slice) colours, resolved for the mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series_colors: Option<Vec<String>>,
    /// Round 2: a LEAF's resolved direction (`ltr | rtl`): the bidi
    /// paragraph direction its text is shaped with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<String>,
    /// Round 2: `backdropBlur` in px (`$blur.*` resolved): blur what is
    /// behind the box through its translucent background. No platform blur
    /// = paint the background alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backdrop_blur: Option<f32>,
    /// Round 2: the keyframe animation with its resolved timing; a painter
    /// keeps the time the node entered the tree and paints
    /// [`crate::animation::frame_with_timing`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub animation: Option<VisualAnimation>,
    /// Round 2: `position: sticky` (the pinned offsets are
    /// `LayoutOutput::sticky`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub sticky: bool,
}

/// A node's animation (`animation` + `animationDuration`, resolved).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualAnimation {
    pub name: String,
    pub timing: crate::animation::AnimationTiming,
    /// Reduced motion: paint the rest frame, no band.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reduced: bool,
}

/// Logical properties → physical sides for the surface direction. taffy has
/// only physical rects. A logical key wins over the physical one it maps to
/// (it states the direction-aware intent). `insetBlockStart/End` are
/// `top`/`bottom`; `textAlign: start|end` becomes `left|right`.
pub fn resolve_logical(style: &mut Map<String, Value>, direction: Direction) {
    let (start, end) = match direction {
        Direction::Rtl => ("Right", "Left"),
        _ => ("Left", "Right"),
    };
    for (logical, prefix) in [
        ("marginInlineStart", "margin"),
        ("marginInlineEnd", "margin"),
        ("paddingInlineStart", "padding"),
        ("paddingInlineEnd", "padding"),
        ("insetInlineStart", ""),
        ("insetInlineEnd", ""),
    ] {
        if let Some(v) = style.shift_remove(logical) {
            let side = if logical.ends_with("Start") { start } else { end };
            let physical = if prefix.is_empty() { side.to_lowercase() } else { format!("{prefix}{side}") };
            style.insert(physical, v);
        }
    }
    if let Some(v) = style.shift_remove("insetBlockStart") {
        style.insert("top".into(), v);
    }
    if let Some(v) = style.shift_remove("insetBlockEnd") {
        style.insert("bottom".into(), v);
    }
    let rtl = matches!(direction, Direction::Rtl);
    match style.get("textAlign").and_then(Value::as_str) {
        Some("start") => {
            style.insert("textAlign".into(), Value::String(if rtl { "right" } else { "left" }.into()));
        }
        Some("end") => {
            style.insert("textAlign".into(), Value::String(if rtl { "left" } else { "right" }.into()));
        }
        _ => {}
    }
}

fn num(v: &Value) -> Option<f32> {
    v.as_f64().map(|n| n as f32)
}

fn percent_of(s: &str) -> Option<f32> {
    s.strip_suffix('%').and_then(|n| n.trim().parse::<f32>().ok()).map(|n| n / 100.0)
}

fn px_of(s: &str) -> Option<f32> {
    s.strip_suffix("px").and_then(|n| n.trim().parse::<f32>().ok())
}

fn dimension(v: &Value) -> Option<Dimension> {
    if let Some(n) = num(v) {
        return Some(length(n));
    }
    match v.as_str()? {
        "auto" => Some(Dimension::auto()),
        s => percent_of(s).map(percent).or_else(|| px_of(s).map(length)),
    }
}

fn lpa(v: &Value) -> Option<LengthPercentageAuto> {
    if let Some(n) = num(v) {
        return Some(length(n));
    }
    match v.as_str()? {
        "auto" => Some(LengthPercentageAuto::auto()),
        s => percent_of(s).map(percent).or_else(|| px_of(s).map(length)),
    }
}

fn lp(v: &Value) -> Option<LengthPercentage> {
    if let Some(n) = num(v) {
        return Some(length(n));
    }
    let s = v.as_str()?;
    percent_of(s).map(percent).or_else(|| px_of(s).map(length))
}

/// A length in px when it is a plain number or `Npx` (not `%`/`auto`).
pub fn px(v: &Value) -> Option<f32> {
    if let Some(n) = num(v) {
        return Some(n);
    }
    v.as_str().and_then(px_of)
}

/// A `"N%"` length as a fraction (0.5 for `50%`).
pub fn percent_value(v: &Value) -> Option<f32> {
    v.as_str().and_then(percent_of)
}

fn rect_lp(r: &Map<String, Value>, all: &str, horizontal: &str, vertical: &str, sides: [&str; 4]) -> Rect<LengthPercentage> {
    let mut out: Rect<LengthPercentage> = Rect::zero();
    if let Some(v) = r.get(all).and_then(lp) {
        out = Rect { left: v, right: v, top: v, bottom: v };
    }
    if let Some(v) = r.get(horizontal).and_then(lp) {
        out.left = v;
        out.right = v;
    }
    if let Some(v) = r.get(vertical).and_then(lp) {
        out.top = v;
        out.bottom = v;
    }
    let [t, rr, b, l] = sides;
    if let Some(v) = r.get(t).and_then(lp) {
        out.top = v;
    }
    if let Some(v) = r.get(rr).and_then(lp) {
        out.right = v;
    }
    if let Some(v) = r.get(b).and_then(lp) {
        out.bottom = v;
    }
    if let Some(v) = r.get(l).and_then(lp) {
        out.left = v;
    }
    out
}

fn rect_lpa(r: &Map<String, Value>, all: &str, horizontal: &str, vertical: &str, sides: [&str; 4], default: LengthPercentageAuto) -> Rect<LengthPercentageAuto> {
    let mut out = Rect { left: default, right: default, top: default, bottom: default };
    if let Some(v) = r.get(all).and_then(lpa) {
        out = Rect { left: v, right: v, top: v, bottom: v };
    }
    if let Some(v) = r.get(horizontal).and_then(lpa) {
        out.left = v;
        out.right = v;
    }
    if let Some(v) = r.get(vertical).and_then(lpa) {
        out.top = v;
        out.bottom = v;
    }
    let [t, rr, b, l] = sides;
    if let Some(v) = r.get(t).and_then(lpa) {
        out.top = v;
    }
    if let Some(v) = r.get(rr).and_then(lpa) {
        out.right = v;
    }
    if let Some(v) = r.get(b).and_then(lpa) {
        out.bottom = v;
    }
    if let Some(v) = r.get(l).and_then(lpa) {
        out.left = v;
    }
    out
}

/// The px sides `[top, right, bottom, left]` of a padding-like key family
/// (`padding`, `paddingHorizontal`, `paddingVertical`, the four sides; the
/// logical keys are already physical here). `None` when no key is set.
pub fn sides_px(r: &Map<String, Value>, all: &str, horizontal: &str, vertical: &str, sides: [&str; 4]) -> Option<[f32; 4]> {
    let keys = [all, horizontal, vertical, sides[0], sides[1], sides[2], sides[3]];
    if !keys.iter().any(|k| r.contains_key(*k)) {
        return None;
    }
    let mut out = [0.0f32; 4];
    if let Some(v) = r.get(all).and_then(px) {
        out = [v; 4];
    }
    if let Some(v) = r.get(horizontal).and_then(px) {
        out[1] = v;
        out[3] = v;
    }
    if let Some(v) = r.get(vertical).and_then(px) {
        out[0] = v;
        out[2] = v;
    }
    for (i, k) in sides.iter().enumerate() {
        if let Some(v) = r.get(*k).and_then(px) {
            out[i] = v;
        }
    }
    Some(out)
}

/// The border widths `[top, right, bottom, left]` (`borderWidth` then the
/// sides); `None` when no width key is set.
pub fn border_sides(r: &Map<String, Value>) -> Option<[f32; 4]> {
    let sides = ["borderTopWidth", "borderRightWidth", "borderBottomWidth", "borderLeftWidth"];
    if !r.contains_key("borderWidth") && !sides.iter().any(|k| r.contains_key(*k)) {
        return None;
    }
    let all = r.get("borderWidth").and_then(num).unwrap_or(0.0);
    let mut out = [all; 4];
    for (i, k) in sides.iter().enumerate() {
        if let Some(v) = r.get(*k).and_then(num) {
            out[i] = v;
        }
    }
    Some(out)
}

fn align_items(s: &str) -> Option<AlignItems> {
    Some(match s {
        "flex-start" | "start" => AlignItems::FLEX_START,
        "flex-end" | "end" => AlignItems::FLEX_END,
        "center" => AlignItems::CENTER,
        "stretch" => AlignItems::STRETCH,
        "baseline" => AlignItems::BASELINE,
        _ => return None,
    })
}

fn align_content(s: &str) -> Option<AlignContent> {
    Some(match s {
        "flex-start" | "start" => AlignContent::FLEX_START,
        "flex-end" | "end" => AlignContent::FLEX_END,
        "center" => AlignContent::CENTER,
        "stretch" => AlignContent::STRETCH,
        "space-between" => AlignContent::SPACE_BETWEEN,
        "space-around" => AlignContent::SPACE_AROUND,
        "space-evenly" => AlignContent::SPACE_EVENLY,
        _ => return None,
    })
}

fn overflow_of(s: &str) -> Result<Overflow, String> {
    Ok(match s {
        "visible" => Overflow::Visible,
        "hidden" | "clip" => Overflow::Clip,
        "scroll" | "auto" => Overflow::Scroll,
        other => return Err(format!("unsupported overflow {other:?}")),
    })
}

/// The overflow of one axis: the axis key over `overflow`.
pub fn axis_overflow<'a>(r: &'a Map<String, Value>, axis: &str) -> Option<&'a str> {
    r.get(axis).or_else(|| r.get("overflow")).and_then(Value::as_str)
}

/// Is the flat style a scroll container on either axis?
pub fn is_scroll_container(r: &Map<String, Value>) -> (bool, bool) {
    let scrolls = |v: Option<&str>| matches!(v, Some("scroll") | Some("auto"));
    (scrolls(axis_overflow(r, "overflowX")), scrolls(axis_overflow(r, "overflowY")))
}

/// How a node's padding and border reach taffy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoxKind {
    /// A container: padding/border are taffy's (the spike rule).
    Container,
    /// A host-measured leaf: the measurer reports the BORDER box of the whole
    /// control, so recipe padding/border stay out of taffy (`Visual` carries
    /// them for the painter and the measurer).
    Leaf,
}

/// Resolved flat map → taffy style. `direction` is NOT inherited by taffy, so
/// the caller passes the surface's and every node gets it.
pub fn to_taffy(r: &Map<String, Value>, direction: Direction, kind: BoxKind) -> Result<Style, String> {
    let mut s = Style { box_sizing: BoxSizing::BorderBox, direction, ..Style::default() };
    let str_of = |k: &str| r.get(k).and_then(Value::as_str);

    s.display = match str_of("display") {
        None | Some("flex") => Display::Flex,
        Some("grid") => Display::Grid,
        Some("block") => Display::Block,
        Some("none") => Display::None,
        Some(other) => return Err(format!("unsupported display {other:?}")),
    };
    s.flex_direction = match str_of("flexDirection") {
        None | Some("row") => FlexDirection::Row,
        Some("column") => FlexDirection::Column,
        Some("row-reverse") => FlexDirection::RowReverse,
        Some("column-reverse") => FlexDirection::ColumnReverse,
        Some(other) => return Err(format!("unsupported flexDirection {other:?}")),
    };
    s.flex_wrap = match str_of("flexWrap") {
        None | Some("nowrap") => FlexWrap::NoWrap,
        Some("wrap") => FlexWrap::Wrap,
        Some("wrap-reverse") => FlexWrap::WrapReverse,
        Some(other) => return Err(format!("unsupported flexWrap {other:?}")),
    };
    s.position = match str_of("position") {
        None | Some("relative") => Position::Relative,
        Some("absolute") => Position::Absolute,
        // Round 2: sticky = relative in layout; the surface pins it by its
        // insets inside the nearest scroller (`LayoutOutput::sticky`).
        Some("sticky") => Position::Relative,
        Some(other) => return Err(format!("unsupported position {other:?}")),
    };
    let ox = axis_overflow(r, "overflowX").map(overflow_of).transpose()?;
    let oy = axis_overflow(r, "overflowY").map(overflow_of).transpose()?;
    s.overflow = taffy::geometry::Point { x: ox.unwrap_or(Overflow::Visible), y: oy.unwrap_or(Overflow::Visible) };
    if let Some(v) = str_of("justifyContent") {
        s.justify_content = Some(align_content(v).ok_or(format!("justifyContent {v:?}"))?);
    }
    if let Some(v) = str_of("alignContent") {
        s.align_content = Some(align_content(v).ok_or(format!("alignContent {v:?}"))?);
    }
    if let Some(v) = str_of("alignItems") {
        s.align_items = Some(align_items(v).ok_or(format!("alignItems {v:?}"))?);
    }
    if let Some(v) = str_of("alignSelf") {
        s.align_self = Some(align_items(v).ok_or(format!("alignSelf {v:?}"))?);
    }
    if let Some(v) = str_of("justifySelf") {
        s.justify_self = Some(align_items(v).ok_or(format!("justifySelf {v:?}"))?);
    }
    if let Some(v) = str_of("textAlign") {
        s.text_align = match v {
            "left" => TextAlign::LegacyLeft,
            "right" => TextAlign::LegacyRight,
            "center" => TextAlign::LegacyCenter,
            _ => TextAlign::Auto,
        };
    }
    if let Some(v) = r.get("flexGrow").and_then(num) {
        s.flex_grow = v;
    }
    if let Some(v) = r.get("flexShrink").and_then(num) {
        s.flex_shrink = v;
    }
    if let Some(v) = r.get("flexBasis").and_then(dimension) {
        s.flex_basis = v;
    }
    let gap_all = r.get("gap").and_then(lp);
    s.gap = Size {
        width: r.get("columnGap").and_then(lp).or(gap_all).unwrap_or(LengthPercentage::ZERO),
        height: r.get("rowGap").and_then(lp).or(gap_all).unwrap_or(LengthPercentage::ZERO),
    };
    s.size = Size {
        width: r.get("width").and_then(dimension).unwrap_or(Dimension::auto()),
        height: r.get("height").and_then(dimension).unwrap_or(Dimension::auto()),
    };
    s.min_size = Size {
        width: r.get("minWidth").and_then(dimension).unwrap_or(Dimension::auto()),
        height: r.get("minHeight").and_then(dimension).unwrap_or(Dimension::auto()),
    };
    s.max_size = Size {
        width: r.get("maxWidth").and_then(dimension).unwrap_or(Dimension::auto()),
        height: r.get("maxHeight").and_then(dimension).unwrap_or(Dimension::auto()),
    };
    if let Some(v) = r.get("aspectRatio") {
        s.aspect_ratio = num(v).or_else(|| {
            let (a, b) = v.as_str()?.split_once('/')?;
            Some(a.trim().parse::<f32>().ok()? / b.trim().parse::<f32>().ok()?)
        });
    }
    if str_of("position") != Some("sticky") {
        s.inset = rect_lpa(r, "inset", "insetHorizontal", "insetVertical", ["top", "right", "bottom", "left"], LengthPercentageAuto::auto());
    }
    s.margin = rect_lpa(r, "margin", "marginHorizontal", "marginVertical", ["marginTop", "marginRight", "marginBottom", "marginLeft"], LengthPercentageAuto::ZERO);
    if kind == BoxKind::Container {
        s.padding = rect_lp(r, "padding", "paddingHorizontal", "paddingVertical", ["paddingTop", "paddingRight", "paddingBottom", "paddingLeft"]);
        if let Some([t, rr, b, l]) = border_sides(r) {
            s.border = Rect { left: length(l), right: length(rr), top: length(t), bottom: length(b) };
        }
    }
    if let Some(v) = str_of("gridTemplateColumns") {
        s.grid_template_columns = tracks::parse_tracks(v)?;
    }
    if let Some(v) = str_of("gridTemplateRows") {
        s.grid_template_rows = tracks::parse_tracks(v)?;
    }
    if let Some(rows) = r.get("gridTemplateAreas").and_then(Value::as_array) {
        let rows: Vec<String> = rows.iter().filter_map(|v| v.as_str().map(str::to_string)).collect();
        s.grid_template_areas = tracks::parse_areas(&rows)?;
    }
    if let Some(v) = str_of("gridArea") {
        let l = tracks::parse_area_placement(v);
        s.grid_row = l.clone();
        s.grid_column = l;
    }
    if let Some(v) = str_of("gridColumn") {
        s.grid_column = tracks::parse_placement(v)?;
    }
    if let Some(v) = str_of("gridRow") {
        s.grid_row = tracks::parse_placement(v)?;
    }
    Ok(s)
}

fn shadow_layers(v: &Value) -> Option<Vec<ShadowLayer>> {
    let layers = v.as_array()?;
    Some(
        layers
            .iter()
            .filter_map(|l| {
                let o = l.as_object()?;
                Some(ShadowLayer {
                    x: o.get("x").and_then(num).unwrap_or(0.0),
                    y: o.get("y").and_then(num).unwrap_or(0.0),
                    blur: o.get("blur").and_then(num).unwrap_or(0.0),
                    spread: o.get("spread").and_then(num).unwrap_or(0.0),
                    color: o.get("color").and_then(Value::as_str).unwrap_or("#00000000").to_string(),
                })
            })
            .collect(),
    )
}

fn gradient(v: &Value) -> Option<Gradient> {
    let o = v.as_object()?;
    let stops = o
        .get("stops")?
        .as_array()?
        .iter()
        .filter_map(|s| Some(GradientStop { color: s.get("color")?.as_str()?.to_string(), offset: s.get("offset").and_then(num).unwrap_or(0.0) }))
        .collect::<Vec<_>>();
    (stops.len() >= 2).then(|| Gradient { angle: o.get("angle").and_then(num).unwrap_or(180.0), stops })
}

/// `translate(Xpx, Ypx) scale(N) rotate(Ndeg)` → the ops in order.
pub fn parse_transform(s: &str) -> Option<Vec<TransformOp>> {
    if !crate::style_check::is_transform(s) {
        return None;
    }
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(close) = rest.find(')') {
        let (piece, after) = rest.split_at(close + 1);
        if let Some(args) = piece.strip_prefix("translate(").and_then(|r| r.strip_suffix(')')) {
            let (x, y) = args.split_once(", ")?;
            out.push(TransformOp::Translate { x: x.strip_suffix("px")?.parse().ok()?, y: y.strip_suffix("px")?.parse().ok()? });
        } else if let Some(n) = piece.strip_prefix("scale(").and_then(|r| r.strip_suffix(')')) {
            out.push(TransformOp::Scale { factor: n.parse().ok()? });
        } else if let Some(n) = piece.strip_prefix("rotate(").and_then(|r| r.strip_suffix(')')) {
            out.push(TransformOp::Rotate { degrees: n.strip_suffix("deg")?.parse().ok()? });
        }
        rest = after.strip_prefix(' ').unwrap_or(after);
    }
    Some(out)
}

fn easing(v: &Value) -> Option<[f32; 4]> {
    let a = v.as_array()?;
    if a.len() != 4 {
        return None;
    }
    Some([num(&a[0])?, num(&a[1])?, num(&a[2])?, num(&a[3])?])
}

/// Average of two sides (what a measurer that adds `2 × padding` needs).
fn avg(a: f32, b: f32) -> f32 {
    (a + b) / 2.0
}

/// The painted subset of a resolved flat map. For a [`BoxKind::Leaf`] the
/// recipe's padding/gap ride along for the painter and the measurer.
/// `%` radii are left out here (the surface resolves them against the box).
pub fn visual(r: &Map<String, Value>, kind: BoxKind) -> Visual {
    let str_of = |k: &str| r.get(k).and_then(Value::as_str).map(str::to_string);
    let leaf = kind == BoxKind::Leaf;
    let pads = if leaf { sides_px(r, "padding", "paddingHorizontal", "paddingVertical", ["paddingTop", "paddingRight", "paddingBottom", "paddingLeft"]) } else { None };
    let borders = border_sides(r);
    let uniform_border = borders.filter(|b| b.iter().all(|w| (w - b[0]).abs() < 0.001)).map(|b| b[0]);
    let corners = ["borderTopLeftRadius", "borderTopRightRadius", "borderBottomRightRadius", "borderBottomLeftRadius"];
    let radius = r.get("borderRadius").and_then(px);
    let corner_radii = corners.iter().any(|k| r.get(*k).and_then(px).is_some()).then(|| {
        let base = radius.unwrap_or(0.0);
        let mut out = [base; 4];
        for (i, k) in corners.iter().enumerate() {
            if let Some(v) = r.get(*k).and_then(px) {
                out[i] = v;
            }
        }
        out
    });
    let ox = axis_overflow(r, "overflowX").map(str::to_string);
    let oy = axis_overflow(r, "overflowY").map(str::to_string);
    let any = |v: &Option<String>, set: &[&str]| v.as_deref().is_some_and(|x| set.contains(&x));
    let transition = r.get("transition").and_then(num).map(|ms| Transition { duration_ms: ms, easing: r.get("transitionEasing").and_then(easing).unwrap_or(DEFAULT_EASING) });
    Visual {
        background_color: str_of("backgroundColor"),
        background_gradient: r.get("backgroundGradient").and_then(gradient),
        color: str_of("color"),
        border_width: uniform_border.or_else(|| r.get("borderWidth").and_then(num)),
        border_widths: borders.filter(|_| uniform_border.is_none()),
        border_color: str_of("borderColor"),
        border_style: str_of("borderStyle"),
        border_radius: radius,
        corner_radii,
        opacity: r.get("opacity").and_then(num),
        box_shadow: r.get("boxShadow").and_then(shadow_layers),
        font_size: r.get("fontSize").and_then(px),
        font_weight: r.get("fontWeight").and_then(|v| v.as_u64()).map(|w| w as u16),
        line_height: r.get("lineHeight").and_then(px),
        font_family: str_of("fontFamily"),
        text_align: str_of("textAlign"),
        letter_spacing: r.get("letterSpacing").and_then(px),
        text_decoration: str_of("textDecoration"),
        text_transform: str_of("textTransform"),
        font_style: str_of("fontStyle"),
        padding_horizontal: pads.map(|p| avg(p[1], p[3])),
        padding_vertical: pads.map(|p| avg(p[0], p[2])),
        padding: pads,
        gap: if leaf { r.get("gap").and_then(px) } else { None },
        native: r.get("native").and_then(Value::as_bool).unwrap_or(false),
        overflow_hidden: any(&ox, &["hidden", "clip"]) || any(&oy, &["hidden", "clip"]),
        overflow_scroll: any(&ox, &["scroll", "auto"]) || any(&oy, &["scroll", "auto"]),
        overflow_x: ox,
        overflow_y: oy,
        transform: r.get("transform").and_then(Value::as_str).and_then(parse_transform),
        transition,
        visibility_hidden: r.get("visibility").and_then(Value::as_str) == Some("hidden"),
        pointer_events_none: r.get("pointerEvents").and_then(Value::as_str) == Some("none"),
        user_select: str_of("userSelect"),
        cursor: str_of("cursor"),
        series_colors: None,
        direction: None,
        backdrop_blur: r.get("backdropBlur").and_then(px),
        animation: None,
        sticky: r.get("position").and_then(Value::as_str) == Some("sticky"),
    }
}

/// A resolved style's own `direction` (inherited down the tree by the
/// surface; the root's, else the locale's, is the surface direction).
pub fn direction_of(r: &Map<String, Value>) -> Option<Direction> {
    match r.get("direction").and_then(Value::as_str) {
        Some("rtl") => Some(Direction::Rtl),
        Some("ltr") => Some(Direction::Ltr),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ctx(width: f32, states: &[&str]) -> ConditionContext {
        ConditionContext { width, states: states.iter().map(|s| s.to_string()).collect(), breakpoints: crate::conditions::default_breakpoints(), ..Default::default() }
    }

    #[test]
    fn media_and_states_flatten_in_source_order() {
        let style = json!({"gap": 4, "@media (min-width: 600px)": {"gap": 8}, "@media (min-width: 900px)": {"gap": 12}, ":pressed": {"opacity": 0.6}});
        let m = style.as_object().unwrap();
        assert_eq!(resolve_conditions(m, &ctx(390.0, &[])).get("gap"), Some(&json!(4)));
        let wide = resolve_conditions(m, &ctx(900.0, &["pressed"]));
        assert_eq!(wide.get("gap"), Some(&json!(12)));
        assert_eq!(wide.get("opacity"), Some(&json!(0.6)));
    }

    #[test]
    fn logical_properties_follow_the_direction() {
        let mut m = json!({"marginInlineStart": 8, "insetInlineEnd": 4, "paddingInlineEnd": 2, "insetBlockEnd": 3, "textAlign": "start"}).as_object().unwrap().clone();
        resolve_logical(&mut m, Direction::Rtl);
        assert_eq!(m.get("marginRight"), Some(&json!(8)));
        assert_eq!(m.get("left"), Some(&json!(4)));
        assert_eq!(m.get("paddingLeft"), Some(&json!(2)));
        assert_eq!(m.get("bottom"), Some(&json!(3)));
        assert_eq!(m.get("textAlign"), Some(&json!("right")));
        assert!(m.get("marginInlineStart").is_none());
    }

    #[test]
    fn leaf_padding_stays_out_of_taffy_with_every_side() {
        let m = json!({"paddingHorizontal": 16, "paddingTop": 2, "paddingLeft": 10, "height": 36, "borderWidth": 1}).as_object().unwrap().clone();
        let leaf = to_taffy(&m, Direction::Ltr, BoxKind::Leaf).unwrap();
        assert_eq!(leaf.padding, Rect::zero());
        assert_eq!(leaf.border, Rect::zero());
        let container = to_taffy(&m, Direction::Ltr, BoxKind::Container).unwrap();
        assert_eq!(container.padding.left, length(10.0));
        assert_eq!(container.padding.right, length(16.0));
        let v = visual(&m, BoxKind::Leaf);
        assert_eq!(v.padding, Some([2.0, 16.0, 0.0, 10.0]));
        assert_eq!(v.padding_horizontal, Some(13.0));
        assert_eq!(v.padding_vertical, Some(1.0));
        assert_eq!(v.border_width, Some(1.0));
    }

    #[test]
    fn per_side_borders_reach_taffy_and_the_visual() {
        let m = json!({"borderWidth": 1, "borderBottomWidth": 3, "borderStyle": "dashed", "borderRadius": 8, "borderTopLeftRadius": 2}).as_object().unwrap().clone();
        let s = to_taffy(&m, Direction::Ltr, BoxKind::Container).unwrap();
        assert_eq!(s.border.bottom, length(3.0));
        assert_eq!(s.border.top, length(1.0));
        let v = visual(&m, BoxKind::Container);
        assert_eq!(v.border_widths, Some([1.0, 1.0, 3.0, 1.0]));
        assert_eq!(v.border_width, Some(1.0));
        assert_eq!(v.border_style.as_deref(), Some("dashed"));
        assert_eq!(v.corner_radii, Some([2.0, 8.0, 8.0, 8.0]));
    }

    #[test]
    fn overflow_per_axis_and_paint_only_keys() {
        let m = json!({"overflowX": "scroll", "overflow": "hidden", "transform": "translate(4px, -2px) rotate(90deg)", "transition": 150, "transitionEasing": [0.2, 0, 0, 1], "visibility": "hidden", "pointerEvents": "none", "backgroundGradient": {"angle": 90, "stops": [{"color": "#ff0000", "offset": 0}, {"color": "#0000ff", "offset": 1}]}}).as_object().unwrap().clone();
        let s = to_taffy(&m, Direction::Ltr, BoxKind::Container).unwrap();
        assert_eq!(s.overflow.x, Overflow::Scroll);
        assert_eq!(s.overflow.y, Overflow::Clip);
        assert_eq!(is_scroll_container(&m), (true, false));
        let v = visual(&m, BoxKind::Container);
        assert_eq!(v.transform, Some(vec![TransformOp::Translate { x: 4.0, y: -2.0 }, TransformOp::Rotate { degrees: 90.0 }]));
        assert_eq!(v.transition, Some(Transition { duration_ms: 150.0, easing: [0.2, 0.0, 0.0, 1.0] }));
        assert!(v.visibility_hidden && v.pointer_events_none);
        assert_eq!(v.background_gradient.as_ref().unwrap().stops.len(), 2);
    }
}
