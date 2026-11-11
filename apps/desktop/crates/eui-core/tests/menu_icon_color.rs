//! #EXP-1629: a menu item's `iconColor` (a status picker's coloured
//! glyphs): the item's `itemLabel` carries it beside its `icon` for the
//! painter; a literal is a `$color.<name>` token, the host's resolved
//! colour arrives in bound data.

use std::collections::HashMap;

use eui_core::measure::FixedMeasure;
use eui_core::surface::{Surface, SurfaceOptions};
use eui_core::types::NestedNode;
use serde_json::{json, Value};

fn menu(items: Value) -> Value {
    json!({"id": "root", "component": "Box", "children": [
        {"id": "m", "component": "Menu", "props": {"items": items}, "children": [{"id": "b", "component": "Button", "props": {"label": "Status"}}]},
    ]})
}

fn mount(tree: Value) -> (Surface, Vec<eui_core::ReduceIssue>) {
    let tree: NestedNode = serde_json::from_value(tree).expect("tree");
    let mut s = Surface::new("t", SurfaceOptions::default());
    let outcome = s.set_nested(tree);
    s.set_viewport(760.0, 900.0, None);
    (s, outcome.issues)
}

fn open(s: &mut Surface) {
    let mut m = FixedMeasure { sizes: HashMap::new(), wrap: true };
    s.layout(&mut m);
    let b = s.index_of("b").expect("the trigger");
    s.event(b, "press", None);
    s.layout(&mut m);
}

fn label(s: &mut Surface, i: usize) -> serde_json::Map<String, Value> {
    let id = format!("m.itemLabel.{i}");
    s.nodes().into_iter().find(|n| n.id == id).unwrap_or_else(|| panic!("no {id}")).props
}

#[test]
fn the_item_label_carries_the_token_beside_its_icon() {
    let (mut s, issues) = mount(menu(json!([
        {"label": "In Progress", "value": "p", "kind": "checkbox", "checked": true, "icon": "status-in-progress", "iconColor": "$color.warning"},
        {"label": "Plain", "value": "q", "icon": "status-backlog"},
        {"label": "No glyph", "value": "r", "iconColor": "$color.success"},
    ])));
    assert!(issues.is_empty(), "{issues:?}");
    open(&mut s);
    assert_eq!(label(&mut s, 0).get("iconColor"), Some(&json!("$color.warning")));
    assert_eq!(label(&mut s, 0).get("icon"), Some(&json!("status-in-progress")));
    assert_eq!(label(&mut s, 1).get("iconColor"), None);
    assert_eq!(label(&mut s, 2).get("iconColor"), None, "no glyph, nothing to tint");
}

#[test]
fn a_literal_hex_is_refused() {
    let (_, issues) = mount(menu(json!([{"label": "Blocked", "value": "b", "icon": "status-in-progress", "iconColor": "#ef4444"}])));
    assert!(issues.iter().any(|i| i.message.ends_with("expected $color.<name>")), "{issues:?}");
}

#[test]
fn bound_items_carry_the_hosts_resolved_colour() {
    let tree: NestedNode = serde_json::from_value(json!({"id": "root", "component": "Box", "children": [
        {"id": "m", "component": "Menu", "props": {"items": {"@path": "/items"}}, "children": [{"id": "b", "component": "Button", "props": {"label": "Status"}}]},
    ]}))
    .expect("tree");
    let mut s = Surface::new("t", SurfaceOptions::default());
    s.set_data("/items", Some(json!([{"label": "Blocked", "value": "b", "kind": "checkbox", "icon": "status-in-progress", "iconColor": "#ef4444ff"}]))).expect("data");
    // A bound `items` is checked at bind time (the reduce reports the
    // binding itself, as for every bound array prop): only its iconColor
    // matters here.
    let issues = s.set_nested(tree).issues;
    assert!(issues.iter().all(|i| i.path == "/components/m/items"), "{issues:?}");
    s.set_viewport(760.0, 900.0, None);
    open(&mut s);
    assert_eq!(label(&mut s, 0).get("iconColor"), Some(&json!("#ef4444ff")));
}

/// A row with an `identifier` leads with the glyph as its own `itemIcon`
/// (the Issue › rows: status glyph · mono id · title): the tint is on it.
#[test]
fn an_identifier_row_tints_its_own_glyph() {
    let (mut s, issues) = mount(menu(json!([
        {"label": "Fix it", "value": "a", "icon": "status-in-progress", "identifier": "EXP-1", "iconColor": "$color.warning"},
        {"label": "Ship it", "value": "b", "icon": "status-in-progress", "identifier": "EXP-2", "iconColor": "$color.success"},
        {"label": "Plain", "value": "c", "icon": "status-in-progress", "identifier": "EXP-3"},
    ])));
    assert!(issues.is_empty(), "{issues:?}");
    open(&mut s);
    let ink = |s: &Surface, i: usize| {
        let index = s.index_of(&format!("m.itemIcon.{i}")).unwrap_or_else(|| panic!("no glyph {i}"));
        s.visual(index).expect("its visual").color.clone()
    };
    let token = ink(&s, 0);
    let plain = ink(&s, 2);
    assert_ne!(token, plain, "the token tints the glyph");
    assert!(token.as_deref().is_some_and(|c| c.starts_with('#')), "the token resolves: {token:?}");
    assert_ne!(ink(&s, 1), token, "each row its own token");
    assert!(label(&mut s, 0).get("iconColor").is_none(), "the label carries no glyph here");
}

/// The host's resolved colour in bound data reaches an identifier row's
/// glyph too (a team's status colours).
#[test]
fn a_bound_identifier_row_carries_the_hosts_colour() {
    let tree: NestedNode = serde_json::from_value(json!({"id": "root", "component": "Box", "children": [
        {"id": "m", "component": "Menu", "props": {"items": {"@path": "/items"}}, "children": [{"id": "b", "component": "Button", "props": {"label": "Status"}}]},
    ]}))
    .expect("tree");
    let mut s = Surface::new("t", SurfaceOptions::default());
    s.set_data("/items", Some(json!([{"label": "Fix it", "value": "a", "icon": "status-in-progress", "identifier": "EXP-1", "iconColor": "#ef4444ff"}]))).expect("data");
    let _ = s.set_nested(tree);
    s.set_viewport(760.0, 900.0, None);
    open(&mut s);
    let index = s.index_of("m.itemIcon.0").expect("the glyph");
    assert_eq!(s.visual(index).expect("its visual").color.as_deref(), Some("#ef4444ff"));
}
