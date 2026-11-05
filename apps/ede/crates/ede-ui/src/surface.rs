//! The `surface` header of a shell document (#EDE-8): how ede-shell places
//! it. Pure and byte-locked by `packages/ede/fixtures/surface.json` (its
//! `$comment` is the spec; `tests/contract.rs` replays the cases); the TS
//! reference is `packages/ede/src/surface.ts`.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::doc::Issue;

pub const LAYERS: &[&str] = &["background", "bottom", "top", "overlay"];
pub const EDGES: &[&str] = &["top", "bottom", "left", "right"];
pub const KEYBOARD: &[&str] = &["none", "on-demand", "exclusive"];
pub const PLACEMENTS: &[&str] = &["bottom-start", "bottom-end", "top-start", "top-end", "left-start", "right-start"];
pub const SURFACE_KEYS: &[&str] = &["layer", "anchor", "size", "exclusiveZone", "margin", "keyboard", "onDemand", "popup", "functions"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Layer {
    Background,
    Bottom,
    Top,
    Overlay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Edge {
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Keyboard {
    None,
    OnDemand,
    Exclusive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Placement {
    BottomStart,
    BottomEnd,
    TopStart,
    TopEnd,
    LeftStart,
    RightStart,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Size {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Margin {
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
    pub left: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Popup {
    pub parent: String,
    pub anchor: String,
    pub placement: Placement,
    pub size: PopupSize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PopupSize {
    pub width: u32,
    pub height: u32,
}

/// A document's place on the screen, every default filled in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum SurfaceHeader {
    /// A wlr-layer-shell surface; `on_demand` = opened only when asked (a
    /// hotkey, a host function), never at start.
    Layer { layer: Layer, anchor: Vec<Edge>, size: Size, #[serde(rename = "exclusiveZone")] exclusive_zone: i32, margin: Margin, keyboard: Keyboard, #[serde(rename = "onDemand")] on_demand: bool, functions: Vec<String> },
    /// A popup of another document's surface (an `xdg_popup` under one of its components).
    Popup { popup: Popup, functions: Vec<String> },
}

impl SurfaceHeader {
    /// The host functions the document may call.
    pub fn functions(&self) -> &[String] {
        match self {
            SurfaceHeader::Layer { functions, .. } | SurfaceHeader::Popup { functions, .. } => functions,
        }
    }
}

fn is_px(v: &Value) -> Option<u32> {
    v.as_u64().filter(|n| *n > 0 && *n <= u32::MAX as u64).map(|n| n as u32)
}

fn is_px0(v: &Value) -> Option<u32> {
    v.as_u64().filter(|n| *n <= u32::MAX as u64).map(|n| n as u32)
}

fn functions_of(input: &Map<String, Value>, issues: &mut Vec<Issue>) -> Vec<String> {
    match input.get("functions") {
        None => Vec::new(),
        Some(Value::Array(list)) if list.iter().all(Value::is_string) => list.iter().filter_map(|v| v.as_str().map(String::from)).collect(),
        Some(_) => {
            issues.push(Issue::new("surface.functions", "expected a list of function names"));
            Vec::new()
        }
    }
}

/// The header with its defaults filled in, or the issues that refuse it.
pub fn normalize_surface(input: &Value) -> Result<SurfaceHeader, Vec<Issue>> {
    let Some(input) = input.as_object() else {
        return Err(vec![Issue::new("surface", "expected an object")]);
    };
    let mut issues = Vec::new();
    for key in input.keys() {
        if !SURFACE_KEYS.contains(&key.as_str()) {
            issues.push(Issue::new(format!("surface.{key}"), format!("unknown key; known: {}", SURFACE_KEYS.join("|"))));
        }
    }
    if let Some(popup) = input.get("popup") {
        for key in ["layer", "anchor", "exclusiveZone", "margin", "keyboard", "onDemand"] {
            if input.contains_key(key) {
                issues.push(Issue::new(format!("surface.{key}"), "a popup surface has no layer, anchor, exclusiveZone, margin, keyboard or onDemand"));
                break;
            }
        }
        let Some(popup) = popup.as_object() else {
            issues.push(Issue::new("surface.popup", "expected an object {parent, anchor, placement?, size}"));
            return Err(issues);
        };
        let parent = popup.get("parent").and_then(Value::as_str).filter(|p| crate::valid_name(p)).map(String::from);
        if parent.is_none() {
            issues.push(Issue::new("surface.popup.parent", "expected a document name (lowercase letters, digits, - and _)"));
        }
        let anchor = popup.get("anchor").and_then(Value::as_str).filter(|a| !a.is_empty()).map(String::from);
        if anchor.is_none() {
            issues.push(Issue::new("surface.popup.anchor", "expected a component id in the parent"));
        }
        let placement = match popup.get("placement") {
            None => Some(Placement::BottomStart),
            Some(v) => serde_json::from_value::<Placement>(v.clone()).ok(),
        };
        if placement.is_none() {
            issues.push(Issue::new("surface.popup.placement", format!("expected one of {}", PLACEMENTS.join("|"))));
        }
        let size = match popup.get("size") {
            None => {
                issues.push(Issue::new("surface.popup.size", "required: a popup names its size"));
                None
            }
            Some(v) => match (v.get("width").and_then(is_px), v.get("height").and_then(is_px)) {
                (Some(width), Some(height)) if v.is_object() => Some(PopupSize { width, height }),
                _ => {
                    issues.push(Issue::new("surface.popup.size", "expected {width, height} in positive whole px"));
                    None
                }
            },
        };
        let functions = functions_of(input, &mut issues);
        return match (issues.is_empty(), parent, anchor, placement, size) {
            (true, Some(parent), Some(anchor), Some(placement), Some(size)) => Ok(SurfaceHeader::Popup { popup: Popup { parent, anchor, placement, size }, functions }),
            _ => Err(issues),
        };
    }

    let layer = match input.get("layer") {
        None => Some(Layer::Top),
        Some(v) => serde_json::from_value::<Layer>(v.clone()).ok(),
    };
    if layer.is_none() {
        issues.push(Issue::new("surface.layer", format!("expected one of {}", LAYERS.join("|"))));
    }

    let mut anchor: Vec<Edge> = Vec::new();
    let mut anchor_ok = true;
    if let Some(v) = input.get("anchor") {
        match v.as_array().map(|list| list.iter().map(|e| serde_json::from_value::<Edge>(e.clone()).ok()).collect::<Option<Vec<Edge>>>()) {
            Some(Some(edges)) => {
                let mut unique = edges.clone();
                unique.sort_by_key(|e| *e as u8);
                unique.dedup();
                if unique.len() != edges.len() {
                    issues.push(Issue::new("surface.anchor", "each edge at most once"));
                    anchor_ok = false;
                } else {
                    anchor = edges;
                }
            }
            _ => {
                issues.push(Issue::new("surface.anchor", format!("expected a list of {}", EDGES.join("|"))));
                anchor_ok = false;
            }
        }
    }

    let mut size = Size::default();
    match input.get("size") {
        Some(v) if !v.is_object() => issues.push(Issue::new("surface.size", "expected {width?, height?}")),
        v => {
            let size_in = v.and_then(Value::as_object).cloned().unwrap_or_default();
            let stretched = [anchor.contains(&Edge::Left) && anchor.contains(&Edge::Right), anchor.contains(&Edge::Top) && anchor.contains(&Edge::Bottom)];
            for (i, axis) in ["width", "height"].iter().enumerate() {
                match size_in.get(*axis) {
                    None => {
                        // A broken anchor list already refuses the header; its size rule would only repeat the noise.
                        if anchor_ok && !stretched[i] {
                            issues.push(Issue::new(format!("surface.size.{axis}"), format!("required: the {} axis is not stretched between two anchors", if i == 0 { "horizontal" } else { "vertical" })));
                        }
                    }
                    Some(v) => match is_px(v) {
                        Some(px) => {
                            if i == 0 {
                                size.width = Some(px)
                            } else {
                                size.height = Some(px)
                            }
                        }
                        None => issues.push(Issue::new(format!("surface.size.{axis}"), "expected a positive whole number of px")),
                    },
                }
            }
        }
    }

    let mut exclusive_zone = 0;
    if let Some(v) = input.get("exclusiveZone") {
        match v.as_i64() {
            Some(n) if n >= -1 && n <= i32::MAX as i64 => {
                if n > 0 && anchor.is_empty() {
                    issues.push(Issue::new("surface.exclusiveZone", "needs at least one anchored edge"));
                } else {
                    exclusive_zone = n as i32;
                }
            }
            _ => issues.push(Issue::new("surface.exclusiveZone", "expected a whole number of px (-1 = never avoided, 0 = none)")),
        }
    }

    let mut margin = Margin::default();
    if let Some(v) = input.get("margin") {
        match v.as_object() {
            None => issues.push(Issue::new("surface.margin", "expected {top?, right?, bottom?, left?}")),
            Some(m) => {
                for side in ["top", "right", "bottom", "left"] {
                    let Some(v) = m.get(side) else { continue };
                    match is_px0(v) {
                        Some(px) => match side {
                            "top" => margin.top = px,
                            "right" => margin.right = px,
                            "bottom" => margin.bottom = px,
                            _ => margin.left = px,
                        },
                        None => issues.push(Issue::new(format!("surface.margin.{side}"), "expected a whole number of px, 0 or more")),
                    }
                }
            }
        }
    }

    let mut keyboard = Keyboard::None;
    if let Some(v) = input.get("keyboard") {
        match serde_json::from_value::<Keyboard>(v.clone()).ok() {
            None => issues.push(Issue::new("surface.keyboard", format!("expected one of {}", KEYBOARD.join("|")))),
            Some(Keyboard::Exclusive) if !matches!(layer, None | Some(Layer::Top) | Some(Layer::Overlay)) => {
                issues.push(Issue::new("surface.keyboard", "exclusive needs the top or overlay layer"))
            }
            Some(k) => keyboard = k,
        }
    }

    let mut on_demand = false;
    if let Some(v) = input.get("onDemand") {
        match v.as_bool() {
            Some(b) => on_demand = b,
            None => issues.push(Issue::new("surface.onDemand", "expected true or false")),
        }
    }

    let functions = functions_of(input, &mut issues);
    match (issues.is_empty(), layer) {
        (true, Some(layer)) => Ok(SurfaceHeader::Layer { layer, anchor, size, exclusive_zone, margin, keyboard, on_demand, functions }),
        _ => Err(issues),
    }
}
