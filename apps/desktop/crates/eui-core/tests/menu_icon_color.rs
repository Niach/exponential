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
    let (mut s, issues) = mount(json!({"id": "root", "component": "Box", "children": [
        {"id": "m", "component": "Menu", "props": {"items": {"@path": "/items"}}, "children": [{"id": "b", "component": "Button", "props": {"label": "Status"}}]},
    ]}));
    assert!(issues.is_empty(), "{issues:?}");
    s.set_data("/items", Some(json!([{"label": "Blocked", "value": "b", "kind": "checkbox", "icon": "status-in-progress", "iconColor": "#ef4444ff"}]))).expect("data");
    open(&mut s);
    assert_eq!(label(&mut s, 0).get("iconColor"), Some(&json!("#ef4444ff")));
}
