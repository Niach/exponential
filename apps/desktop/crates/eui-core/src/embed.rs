//! #EDE-65 lane M: Embed, a nested surface drawn in place
//! (`packages/exponential-ui/fixtures/embed.json`). The Rust twin of
//! `src/embed.ts`: which `surface` value is a card, the error box a bad one
//! shows instead, and what an inner event becomes when it reaches the host
//! as the Embed's `action`. Painters (gpui, SwiftUI, Compose) mount the card
//! as a SANDBOXED nested surface: its own data and ids, never the host's.
//!
//! `surface` is a DATA prop (`catalog::is_data_prop("Embed", "surface")`): a
//! top-level `{"@path"}` binding resolves against the host, nothing inside
//! it ever does (`data::resolve_prop`).

use serde_json::{json, Map, Value};

use crate::catalog::A2UI_VERSION;
use crate::types::NestedNode;

/// The Embed's one event: every inner `event` action reaches the host as it.
pub const EMBED_EVENT: &str = "action";

/// The component id of the error box ([`embed_error_root`]).
pub const EMBED_ERROR_ID: &str = "embed_error";

/// What a bad `surface` paints instead (`fixtures/embed.json` `errorRoot`)
/// as JSON: an error Alert titled `$string.embedInvalid`; the host around
/// it keeps painting. [`embed_error_root`] is the same as a node.
pub fn embed_error_root_value() -> Value {
    json!({"id": EMBED_ERROR_ID, "component": "Alert", "props": {"type": "error", "title": "$string.embedInvalid"}})
}

/// [`embed_error_root_value`] as a [`NestedNode`] (`EMBED_ERROR_ROOT`).
pub fn embed_error_root() -> NestedNode {
    serde_json::from_value(embed_error_root_value()).expect("the error root is a node")
}

/// A nested card: the root and its OWN data model (never the host's).
#[derive(Debug, Clone, PartialEq)]
pub struct EmbedCard {
    /// The sound root as written (`NestedNode` via [`EmbedCard::node`]).
    pub root: Value,
    pub data: Map<String, Value>,
}

impl EmbedCard {
    /// The root as a [`NestedNode`]; `None` when it is sound but does not
    /// deserialize (a malformed `template`, `catalogId`…): the painter shows
    /// the error box then, as for a bad surface.
    pub fn node(&self) -> Option<NestedNode> {
        serde_json::from_value(self.root.clone()).ok()
    }
}

/// What a painter mounts for a `surface` value ([`embed_mount`]).
#[derive(Debug, Clone, PartialEq)]
pub struct EmbedMount {
    pub card: EmbedCard,
    /// `false` = the card is the error box.
    pub valid: bool,
}

/// A structurally sound nested node: string `id` + `component`, `props` /
/// `on` / `style` objects, `children` an array of nodes, `slots` an object
/// of nodes, recursively.
fn sound_node(n: &Value, depth: usize) -> bool {
    let Some(o) = n.as_object() else { return false };
    if depth > 64 {
        return false;
    }
    let non_empty = |k: &str| o.get(k).and_then(Value::as_str).is_some_and(|s| !s.is_empty());
    if !non_empty("id") || !non_empty("component") {
        return false;
    }
    if ["props", "on", "style"].iter().any(|k| o.get(*k).is_some_and(|v| !v.is_object())) {
        return false;
    }
    if let Some(children) = o.get("children") {
        match children.as_array() {
            Some(items) if items.iter().all(|c| sound_node(c, depth + 1)) => {}
            _ => return false,
        }
    }
    if let Some(slots) = o.get("slots") {
        match slots.as_object() {
            Some(map) if map.values().all(|c| sound_node(c, depth + 1)) => {}
            _ => return false,
        }
    }
    true
}

/// The Embed's resolved `surface` prop → its card, or `None` when it is not
/// one (not an object, no sound `root`, a non-object `data`, a `version`
/// other than 1.0): the painter then shows the error box. A missing or null
/// `data` is an empty model. An unknown component inside a sound root is
/// NOT bad: the nested surface paints its Unknown placeholder.
pub fn embed_card(surface: &Value) -> Option<EmbedCard> {
    let o = surface.as_object()?;
    if o.get("version").is_some_and(|v| v.as_str() != Some(A2UI_VERSION)) {
        return None;
    }
    let root = o.get("root")?;
    if !sound_node(root, 1) {
        return None;
    }
    let data = match o.get("data") {
        None | Some(Value::Null) => Map::new(),
        Some(Value::Object(m)) => m.clone(),
        Some(_) => return None,
    };
    Some(EmbedCard { root: root.clone(), data })
}

/// The card a painter mounts for `surface`: the card, else the error box.
pub fn embed_mount(surface: &Value) -> EmbedMount {
    match embed_card(surface) {
        Some(card) => EmbedMount { card, valid: true },
        None => EmbedMount { card: EmbedCard { root: embed_error_root_value(), data: Map::new() }, valid: false },
    }
}

/// An inner event → the Embed's `action` payload (merged over the author's
/// context like any component payload): `{name, context}`, a missing or
/// non-object context = `{}`.
pub fn embed_payload(name: &str, context: Option<&Value>) -> Value {
    let context = match context {
        Some(Value::Object(m)) => Value::Object(m.clone()),
        _ => Value::Object(Map::new()),
    };
    json!({"name": name, "context": context})
}

/// The nested surface's id: the host's surface id and the Embed's (painted)
/// component id, so two cards' inner ids never meet.
pub fn embed_surface_id(host_surface_id: &str, component_id: &str) -> String {
    format!("{host_surface_id}/{component_id}")
}

/// A JSON Pointer token for a top-level data key (`~` → `~0`, `/` → `~1`):
/// a painter writes a card's `data` key by key at `/<token>`.
pub fn data_key_path(key: &str) -> String {
    format!("/{}", key.replace('~', "~0").replace('/', "~1"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_error_root_parses_and_a_bad_surface_mounts_it() {
        assert_eq!(embed_error_root().component, "Alert");
        let m = embed_mount(&json!("x"));
        assert!(!m.valid);
        assert_eq!(m.card.root, embed_error_root_value());
        assert_eq!(embed_surface_id("s", "card#a"), "s/card#a");
        assert_eq!(data_key_path("a/b~c"), "/a~1b~0c");
        assert_eq!(embed_payload("reply", None), json!({"name": "reply", "context": {}}));
    }

    #[test]
    fn depth_is_capped() {
        let mut n = json!({"id": "x", "component": "Box"});
        for _ in 0..70 {
            n = json!({"id": "x", "component": "Box", "children": [n]});
        }
        assert!(embed_card(&json!({"root": n})).is_none());
    }
}
