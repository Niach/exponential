//! Round 2 (docs/round-2-contract.md §2, direction): `direction` may sit on
//! ANY node and is inherited; everything direction-dependent reads the
//! NODE's resolved direction: flex row order, logical insets and paddings,
//! mirrored glyphs and `textAlign` start/end (the implicit alignment of
//! text is `start`). Mirrors `src/direction.ts`; `fixtures/text-direction.json`
//! locks it. The surface carries the result into [`crate::style::Visual`]
//! (`direction`, `text_align`) so painters shape text with the node's
//! direction as the bidi paragraph direction.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::conditions::{resolve_conditions, ConditionContext};
use crate::types::UiNode;

/// A text direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextDirection {
    #[default]
    Ltr,
    Rtl,
}

impl TextDirection {
    pub fn parse(s: &str) -> Option<TextDirection> {
        match s {
            "ltr" => Some(TextDirection::Ltr),
            "rtl" => Some(TextDirection::Rtl),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            TextDirection::Ltr => "ltr",
            TextDirection::Rtl => "rtl",
        }
    }

    /// The direction of a locale (`catalog/locale.json` `rtl`).
    pub fn of_locale(locale: &str) -> TextDirection {
        if crate::locale::text_direction(locale) == "rtl" {
            TextDirection::Rtl
        } else {
            TextDirection::Ltr
        }
    }
}

/// A physical text alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PhysicalAlign {
    #[default]
    Left,
    Right,
    Center,
    Justify,
}

impl PhysicalAlign {
    pub fn as_str(self) -> &'static str {
        match self {
            PhysicalAlign::Left => "left",
            PhysicalAlign::Right => "right",
            PhysicalAlign::Center => "center",
            PhysicalAlign::Justify => "justify",
        }
    }
}

fn own_direction(style: Option<&Map<String, Value>>, ctx: Option<&ConditionContext>) -> Option<TextDirection> {
    let style = style?;
    let d = match ctx {
        Some(ctx) => resolve_conditions(style, ctx).get("direction").cloned(),
        None => style.get("direction").cloned(),
    };
    d.as_ref().and_then(Value::as_str).and_then(TextDirection::parse)
}

/// Every node's direction: its own style `direction` (resolved against
/// `ctx` when given), else its parent's; the root's parent is the surface.
/// Slots inherit from their owner. A template's items take the direction of
/// the node that renders them.
pub fn node_directions(root: &UiNode, surface: TextDirection, ctx: Option<&ConditionContext>) -> IndexMap<String, TextDirection> {
    fn walk(n: &UiNode, inherited: TextDirection, ctx: Option<&ConditionContext>, out: &mut IndexMap<String, TextDirection>) {
        let dir = own_direction(n.style.as_ref(), ctx).unwrap_or(inherited);
        out.insert(n.id.clone(), dir);
        for slot in n.slots.iter().flat_map(|s| s.values()) {
            walk(slot, dir, ctx, out);
        }
        for c in &n.children {
            walk(c, dir, ctx, out);
        }
    }
    let mut out = IndexMap::new();
    walk(root, surface, ctx, &mut out);
    out
}

/// `start`/`end` (and an absent or unknown value = `start`) as the physical
/// side for a direction; `left`, `right`, `center` and `justify` stay.
pub fn physical_text_align(align: Option<&str>, direction: TextDirection) -> PhysicalAlign {
    let rtl = direction == TextDirection::Rtl;
    match align {
        Some("left") => PhysicalAlign::Left,
        Some("right") => PhysicalAlign::Right,
        Some("center") => PhysicalAlign::Center,
        Some("justify") => PhysicalAlign::Justify,
        Some("end") => {
            if rtl {
                PhysicalAlign::Left
            } else {
                PhysicalAlign::Right
            }
        }
        _ => {
            if rtl {
                PhysicalAlign::Right
            } else {
                PhysicalAlign::Left
            }
        }
    }
}

/// A node's text alignment: its (flattened) style `textAlign`, else a
/// Text's `align` prop, else `start`, resolved against its direction.
pub fn text_align_of(style: &Map<String, Value>, props: &Map<String, Value>, direction: TextDirection) -> PhysicalAlign {
    let align = style.get("textAlign").or_else(|| props.get("align")).and_then(Value::as_str);
    physical_text_align(align, direction)
}

/// [`text_align_of`] for a reduced node (`nodeTextAlign`).
pub fn node_text_align(node: &UiNode, direction: TextDirection, ctx: Option<&ConditionContext>) -> PhysicalAlign {
    let empty = Map::new();
    let style = match (node.style.as_ref(), ctx) {
        (Some(s), Some(ctx)) => resolve_conditions(s, ctx),
        (Some(s), None) => s.clone(),
        (None, _) => empty,
    };
    text_align_of(&style, &node.props, direction)
}
