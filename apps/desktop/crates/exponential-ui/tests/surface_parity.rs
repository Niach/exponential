//! VAPP-91: web/TS parity of the core surface. The root is a block-level box: a root
//! without a width is as wide as the surface, as in the React renderer.

use std::collections::HashMap;

use exponential_ui::measure::FixedMeasure;
use exponential_ui::surface::{Surface, SurfaceOptions};
use serde_json::json;

#[test]
fn a_root_without_a_width_fills_the_surface() {
    let mut s = Surface::new("s", SurfaceOptions::default());
    s.apply(&json!({"version": "v0.9", "createSurface": {"surfaceId": "s", "catalogId": "https://ui.exponential.at/catalogs/core/v1"}})).unwrap();
    s.apply(&json!({"version": "v0.9", "updateComponents": {"surfaceId": "s", "components": [
        {"id": "root", "component": "Card", "title": "Hi", "children": ["t"]},
        {"id": "t", "component": "Text", "text": "short"}
    ]}})).unwrap();
    s.set_viewport(390.0, 0.0, None);
    let out = s.layout(&mut FixedMeasure { sizes: HashMap::new(), wrap: true });
    assert_eq!(out.frames[0].w, 390.0);
}

#[test]
fn update_components_merges_by_id() {
    let mut s = Surface::new("s", SurfaceOptions::default());
    s.apply(&json!({"version": "v0.9", "createSurface": {"surfaceId": "s", "catalogId": "https://ui.exponential.at/catalogs/core/v1"}})).unwrap();
    s.apply(&json!({"version": "v0.9", "updateComponents": {"surfaceId": "s", "components": [
        {"id": "root", "component": "Box", "children": ["a"]},
        {"id": "a", "component": "Text", "text": "one"}
    ]}})).unwrap();
    s.apply(&json!({"version": "v0.9", "updateComponents": {"surfaceId": "s", "components": [
        {"id": "a", "component": "Text", "text": "two"}
    ]}})).unwrap();
    s.set_viewport(390.0, 0.0, None);
    s.layout(&mut FixedMeasure { sizes: HashMap::new(), wrap: true });
    let nodes = s.nodes();
    let a = nodes.iter().find(|n| n.id == "a").expect("a survives the second update with root kept");
    assert_eq!(a.props.get("text").and_then(|v| v.as_str()), Some("two"));
}

#[test]
fn a_template_row_press_fires_its_handler_with_the_item_scope() {
    use exponential_ui::surface::OutEvent;
    let mut s = Surface::new("s", SurfaceOptions::default());
    s.apply(&json!({"version": "v0.9", "createSurface": {"surfaceId": "s", "catalogId": "https://ui.exponential.at/catalogs/core/v1"}})).unwrap();
    s.apply(&json!({"version": "v0.9", "updateComponents": {"surfaceId": "s", "components": [
        {"id": "root", "component": "List", "children": {"componentId": "row", "path": "/rows"}},
        {"id": "row", "component": "Box", "pressable": true, "children": ["label"],
         "on": {"press": {"functionCall": {"call": "harness.openDevice", "args": {"id": {"path": "id"}}}}}},
        {"id": "label", "component": "Text", "text": {"path": "name"}}
    ]}})).unwrap();
    s.apply(&json!({"version": "v0.9", "updateDataModel": {"surfaceId": "s", "path": "/rows", "value": [{"id": "a", "name": "A"}, {"id": "b", "name": "B"}]}})).unwrap();
    s.set_viewport(390.0, 0.0, None);
    s.layout(&mut FixedMeasure { sizes: HashMap::new(), wrap: true });
    let index = s.nodes().iter().position(|n| n.id == "row.1").expect("row.1 laid out") as u32;
    let events = s.event(index, "press", None);
    assert!(events.iter().any(|e| matches!(e, OutEvent::FunctionCall { name, args, .. } if name == "harness.openDevice" && args["id"] == "b")), "{events:?}");
}
