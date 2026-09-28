//! StyleX-subset style object → resolved map → taffy `Style` + `Visual`.
//!
//! Conditions are nested objects on the style: `"@media (min-width: Npx)"` is
//! evaluated against the SURFACE width (not the screen), `":pressed"` against
//! the pressed-id set. Resolution order: base → matching media queries in
//! ascending `min-width` → `:pressed`. Everything is evaluated on the client.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use taffy::prelude::*;
use taffy::style::{
    AlignContent, AlignItems, BoxSizing, Direction, Display, FlexDirection, FlexWrap,
    LengthPercentage, LengthPercentageAuto, Overflow, Position, TextAlign,
};

use crate::tracks;
use crate::tree::Kind;

/// What the client knows when it resolves a style object.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StyleContext {
    pub surface_width: f32,
    pub pressed: bool,
}

/// Painted properties, passed through to the native painter untouched
/// (colours stay strings: `#rrggbb[aa]` or `$group.token`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Visual {
    pub background_color: Option<String>,
    pub color: Option<String>,
    pub border_width: Option<f32>,
    pub border_color: Option<String>,
    pub border_radius: Option<f32>,
    pub opacity: Option<f32>,
    pub box_shadow: Option<String>,
    pub font_size: Option<f32>,
    pub font_weight: Option<u16>,
    pub line_height: Option<f32>,
    pub text_align: Option<String>,
    pub overflow_hidden: bool,
}

/// Typography a measured leaf is laid out with (variant defaults + overrides).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TextStyle {
    pub font_size: f32,
    pub font_weight: u16,
    pub line_height: f32,
}

impl TextStyle {
    /// Variant defaults: title 18/600/24 · body 14/400/20 · muted 13/400/18 ·
    /// caption 12/500/16 · label 13/600/18. `Visual` overrides win.
    pub fn for_node(kind: Kind, props: &Map<String, Value>, visual: &Visual) -> TextStyle {
        let variant = props.get("variant").and_then(Value::as_str).unwrap_or("body");
        let (size, weight, lh) = match (kind, variant) {
            (Kind::Text, "title") => (18.0, 600, 24.0),
            (Kind::Text, "muted") => (13.0, 400, 18.0),
            (Kind::Text, "caption") => (12.0, 500, 16.0),
            (Kind::Text, "label") => (13.0, 600, 18.0),
            (Kind::Pill, _) | (Kind::Badge, _) => (12.0, 500, 16.0),
            (Kind::Button, _) => (14.0, 500, 20.0),
            _ => (14.0, 400, 20.0),
        };
        TextStyle {
            font_size: visual.font_size.unwrap_or(size),
            font_weight: visual.font_weight.unwrap_or(weight),
            line_height: visual.line_height.unwrap_or(lh),
        }
    }
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

/// Flatten conditions into one map for the given context.
pub fn resolve(style: &Map<String, Value>, ctx: StyleContext) -> Map<String, Value> {
    let mut out = Map::new();
    for (k, v) in style {
        if !(k.starts_with('@') || k.starts_with(':')) {
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

/// Keys (recursively, inside conditions too) that the v1 whitelist rejects.
pub fn unknown_keys(style: &Map<String, Value>) -> Vec<String> {
    let mut out = Vec::new();
    for (k, v) in style {
        if k.starts_with('@') || k.starts_with(':') {
            if media_min_width(k).is_none() && k != ":pressed" {
                out.push(k.clone());
            }
            if let Some(nested) = v.as_object() {
                out.extend(unknown_keys(nested));
            }
        } else if !crate::WHITELIST.contains(&k.as_str()) {
            out.push(k.clone());
        }
    }
    out
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

fn rect_lp(
    r: &Map<String, Value>,
    all: &str,
    horizontal: &str,
    vertical: &str,
    sides: [&str; 4],
) -> Rect<LengthPercentage> {
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

/// Resolved map → taffy style. `direction` is NOT inherited by taffy, so the
/// caller passes the surface's direction and every node gets it.
pub fn to_taffy(r: &Map<String, Value>, direction: Direction) -> Result<Style, String> {
    let mut s = Style {
        box_sizing: BoxSizing::BorderBox,
        direction,
        ..Style::default()
    };
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
    s.padding = rect_lp(
        r,
        "padding",
        "paddingHorizontal",
        "paddingVertical",
        ["paddingTop", "paddingRight", "paddingBottom", "paddingLeft"],
    );
    if let Some(b) = r.get("borderWidth").and_then(num) {
        s.border = Rect { left: length(b), right: length(b), top: length(b), bottom: length(b) };
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

/// The painted subset of a resolved map.
pub fn visual(r: &Map<String, Value>) -> Visual {
    let str_of = |k: &str| r.get(k).and_then(Value::as_str).map(str::to_string);
    Visual {
        background_color: str_of("backgroundColor"),
        color: str_of("color"),
        border_width: r.get("borderWidth").and_then(num),
        border_color: str_of("borderColor"),
        border_radius: r.get("borderRadius").and_then(num),
        opacity: r.get("opacity").and_then(num),
        box_shadow: str_of("boxShadow"),
        font_size: r.get("fontSize").and_then(num),
        font_weight: r.get("fontWeight").and_then(|v| v.as_u64()).map(|w| w as u16),
        line_height: r.get("lineHeight").and_then(num),
        text_align: str_of("textAlign"),
        overflow_hidden: matches!(r.get("overflow").and_then(Value::as_str), Some("hidden") | Some("clip")),
    }
}

/// The surface direction is read from the ROOT node's resolved style only.
pub fn direction_of(r: &Map<String, Value>) -> Direction {
    match r.get("direction").and_then(Value::as_str) {
        Some("rtl") => Direction::Rtl,
        _ => Direction::Ltr,
    }
}
