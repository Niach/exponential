//! Style RESOLUTION for layout: a node's style object (token references
//! already resolved by the theme, conditions still nested) → the flat map for
//! this pass (`@media (min-width)` against the SURFACE width, `:pressed`
//! against the node's press state) → logical properties made physical from
//! the surface direction → a taffy `Style` plus the painted [`Visual`].
//!
//! Rules from the VAPP-4 spike that every painter relies on: border-box, flex
//! row default, `min-width: auto`, `overflow: hidden` = clip, `borderWidth` is
//! the one visual key with layout effect, absolute nodes paint last among
//! their siblings (no z-index).

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use taffy::prelude::*;
use taffy::style::{
    AlignContent, AlignItems, BoxSizing, Direction, Display, FlexDirection, FlexWrap,
    LengthPercentage, LengthPercentageAuto, Overflow, Position, TextAlign,
};

use crate::tracks;

/// What the client knows when it flattens a style object.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StyleContext {
    pub surface_width: f32,
    pub pressed: bool,
}

/// One shadow layer, resolved.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ShadowLayer {
    pub x: f32,
    pub y: f32,
    pub blur: f32,
    pub spread: f32,
    pub color: String,
}

/// Painted properties, RESOLVED (hex colours, px numbers, shadow layers,
/// family names). Painters never read the theme; they paint this.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Visual {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_width: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_radius: Option<f32>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_align: Option<String>,
    /// The recipe's padding on a MEASURED leaf (a Button's label inset): the
    /// painter draws it, the measurer includes it; taffy never sees it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub padding_horizontal: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub padding_vertical: Option<f32>,
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
}

fn media_min_width(key: &str) -> Option<f32> {
    let inner = key.strip_prefix("@media")?.trim();
    let inner = inner.strip_prefix('(')?.strip_suffix(')')?;
    let (name, value) = inner.split_once(':')?;
    if name.trim() != "min-width" {
        return None;
    }
    let v = value.trim();
    v.strip_suffix("px").unwrap_or(v).trim().parse::<f32>().ok()
}

/// Is `key` a condition block rather than a property?
pub fn is_condition_key(key: &str) -> bool {
    key.starts_with('@') || key.starts_with(':')
}

/// Flatten conditions into one map: base → matching media queries in
/// ascending `min-width` → `:pressed`.
pub fn resolve_conditions(style: &Map<String, Value>, ctx: StyleContext) -> Map<String, Value> {
    let mut out = Map::new();
    for (k, v) in style {
        if !is_condition_key(k) {
            out.insert(k.clone(), v.clone());
        }
    }
    let mut media: Vec<(f32, &Map<String, Value>)> = style
        .iter()
        .filter_map(|(k, v)| Some((media_min_width(k)?, v.as_object()?)))
        .filter(|(min, _)| ctx.surface_width >= *min)
        .collect();
    media.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    for (_, overrides) in media {
        for (k, v) in overrides {
            out.insert(k.clone(), v.clone());
        }
    }
    if ctx.pressed {
        if let Some(p) = style.get(":pressed").and_then(Value::as_object) {
            for (k, v) in p {
                out.insert(k.clone(), v.clone());
            }
        }
    }
    out
}

/// Logical inline properties → physical sides for the surface direction.
/// taffy 0.12 has only physical rects. A logical key wins over the physical
/// one it maps to (it states the direction-aware intent).
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
        if let Some(v) = style.remove(logical) {
            let side = if logical.ends_with("Start") { start } else { end };
            let physical = if prefix.is_empty() { side.to_lowercase() } else { format!("{prefix}{side}") };
            style.insert(physical, v);
        }
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

fn rect_lpa(
    r: &Map<String, Value>,
    all: &str,
    horizontal: &str,
    vertical: &str,
    sides: [&str; 4],
    default: LengthPercentageAuto,
) -> Rect<LengthPercentageAuto> {
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
        Some(other) => return Err(format!("unsupported position {other:?}")),
    };
    if let Some(o) = str_of("overflow") {
        let ov = match o {
            "visible" => Overflow::Visible,
            "hidden" | "clip" => Overflow::Clip,
            "scroll" | "auto" => Overflow::Scroll,
            other => return Err(format!("unsupported overflow {other:?}")),
        };
        s.overflow = taffy::geometry::Point { x: ov, y: ov };
    }
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
    s.inset = rect_lpa(r, "inset", "insetHorizontal", "insetVertical", ["top", "right", "bottom", "left"], LengthPercentageAuto::auto());
    s.margin = rect_lpa(
        r,
        "margin",
        "marginHorizontal",
        "marginVertical",
        ["marginTop", "marginRight", "marginBottom", "marginLeft"],
        LengthPercentageAuto::ZERO,
    );
    if kind == BoxKind::Container {
        s.padding = rect_lp(r, "padding", "paddingHorizontal", "paddingVertical", ["paddingTop", "paddingRight", "paddingBottom", "paddingLeft"]);
        if let Some(b) = r.get("borderWidth").and_then(num) {
            s.border = Rect { left: length(b), right: length(b), top: length(b), bottom: length(b) };
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

/// The painted subset of a resolved flat map. For a [`BoxKind::Leaf`] the
/// recipe's padding/gap ride along for the painter and the measurer.
pub fn visual(r: &Map<String, Value>, kind: BoxKind) -> Visual {
    let str_of = |k: &str| r.get(k).and_then(Value::as_str).map(str::to_string);
    let overflow = r.get("overflow").and_then(Value::as_str);
    let leaf = kind == BoxKind::Leaf;
    let pad_h = r.get("paddingHorizontal").or_else(|| r.get("padding")).and_then(px);
    let pad_v = r.get("paddingVertical").or_else(|| r.get("padding")).and_then(px);
    Visual {
        background_color: str_of("backgroundColor"),
        color: str_of("color"),
        border_width: r.get("borderWidth").and_then(num),
        border_color: str_of("borderColor"),
        border_radius: r.get("borderRadius").and_then(px),
        opacity: r.get("opacity").and_then(num),
        box_shadow: r.get("boxShadow").and_then(shadow_layers),
        font_size: r.get("fontSize").and_then(px),
        font_weight: r.get("fontWeight").and_then(|v| v.as_u64()).map(|w| w as u16),
        line_height: r.get("lineHeight").and_then(px),
        font_family: str_of("fontFamily"),
        text_align: str_of("textAlign"),
        padding_horizontal: if leaf { pad_h } else { None },
        padding_vertical: if leaf { pad_v } else { None },
        gap: if leaf { r.get("gap").and_then(px) } else { None },
        native: r.get("native").and_then(Value::as_bool).unwrap_or(false),
        overflow_hidden: matches!(overflow, Some("hidden") | Some("clip")),
        overflow_scroll: matches!(overflow, Some("scroll") | Some("auto")),
    }
}

/// The surface direction is read from the ROOT node's resolved style only.
pub fn direction_of(r: &Map<String, Value>) -> Direction {
    match r.get("direction").and_then(Value::as_str) {
        Some("rtl") => Direction::Rtl,
        _ => Direction::Ltr,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn media_and_pressed_flatten_in_order() {
        let style = json!({"gap": 4, "@media (min-width: 600px)": {"gap": 8}, "@media (min-width: 900px)": {"gap": 12}, ":pressed": {"opacity": 0.6}});
        let m = style.as_object().unwrap();
        let narrow = resolve_conditions(m, StyleContext { surface_width: 390.0, pressed: false });
        assert_eq!(narrow.get("gap"), Some(&json!(4)));
        let wide = resolve_conditions(m, StyleContext { surface_width: 900.0, pressed: true });
        assert_eq!(wide.get("gap"), Some(&json!(12)));
        assert_eq!(wide.get("opacity"), Some(&json!(0.6)));
    }

    #[test]
    fn logical_properties_follow_the_direction() {
        let mut m = json!({"marginInlineStart": 8, "insetInlineEnd": 4, "paddingInlineEnd": 2}).as_object().unwrap().clone();
        resolve_logical(&mut m, Direction::Rtl);
        assert_eq!(m.get("marginRight"), Some(&json!(8)));
        assert_eq!(m.get("left"), Some(&json!(4)));
        assert_eq!(m.get("paddingLeft"), Some(&json!(2)));
        assert!(m.get("marginInlineStart").is_none());
    }

    #[test]
    fn leaf_padding_stays_out_of_taffy() {
        let m = json!({"paddingHorizontal": 16, "height": 36, "borderWidth": 1}).as_object().unwrap().clone();
        let leaf = to_taffy(&m, Direction::Ltr, BoxKind::Leaf).unwrap();
        assert_eq!(leaf.padding, Rect::zero());
        assert_eq!(leaf.border, Rect::zero());
        let container = to_taffy(&m, Direction::Ltr, BoxKind::Container).unwrap();
        assert_eq!(container.padding.left, length(16.0));
        let v = visual(&m, BoxKind::Leaf);
        assert_eq!(v.padding_horizontal, Some(16.0));
        assert_eq!(v.border_width, Some(1.0));
    }
}
