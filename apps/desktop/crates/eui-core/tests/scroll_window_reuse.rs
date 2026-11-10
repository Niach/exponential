//! #LOGS-9: a scroll step over a windowed template List is constant work:
//! a step that keeps the window builds nothing, and a step that moves it
//! reuses the instances of the rows that stay (the built tree is the one a
//! fresh build makes).

use std::collections::HashMap;

use eui_core::measure::FixedMeasure;
use eui_core::surface::{Surface, SurfaceOptions};
use eui_core::types::NestedNode;
use serde_json::{json, Value};

fn fixed() -> FixedMeasure {
    FixedMeasure { sizes: HashMap::new(), wrap: true }
}

fn chat(items: usize) -> Surface {
    let tree: NestedNode = serde_json::from_value(json!({
        "id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%"},
        "children": [
            {"id": "list", "component": "List", "style": {"height": 400}, "template": {"component": "row", "path": "/items", "key": "id"}},
            {"id": "row", "component": "Box", "style": {"height": 40, "flexShrink": 0}, "children": [
                {"id": "label", "component": "Text", "props": {"text": {"@path": "text"}}},
                {"id": "badge", "component": "Badge", "props": {"text": {"@path": "id"}}}
            ]}
        ]
    }))
    .expect("tree");
    let mut s = Surface::new("t", SurfaceOptions::default());
    assert!(s.set_nested(tree).issues.is_empty());
    let items: Vec<Value> = (0..items).map(|i| json!({"id": format!("i{i}"), "text": format!("Row {i}")})).collect();
    s.set_data("", Some(json!({ "items": items }))).unwrap();
    s.set_viewport(390.0, 800.0, None);
    s
}

/// Every built node as `(id, component, props)`, in id order.
fn tree(s: &mut Surface) -> Vec<(String, String, String)> {
    let mut out: Vec<(String, String, String)> = s.nodes().into_iter().filter(|n| !n.hidden).map(|n| (n.id, n.component, serde_json::to_string(&n.props).unwrap())).collect();
    out.sort();
    out
}

#[test]
fn a_scroll_step_inside_the_window_builds_nothing() {
    let mut s = chat(2_000);
    let mut m = fixed();
    s.layout(&mut m);
    s.scroll("list", 10.0);
    s.layout(&mut m);
    let window = s.layout(&mut m).lists[0].clone();
    // 20 px on: the same rows and spacers lay out.
    s.scroll("list", 30.0);
    let step = s.layout(&mut m);
    assert!(!step.rebuilt, "a step inside the window rebuilds nothing");
    assert_eq!(step.built_nodes, 0);
    assert_eq!((step.lists[0].start, step.lists[0].end), (window.start, window.end));
    let list = s.index_of("list").unwrap();
    assert_eq!(step.scrolls.iter().find(|x| x.index == list).unwrap().offset_y, 30.0);
    // A row further: the window moves.
    s.scroll("list", 41.0);
    let moved = s.layout(&mut m);
    assert!(moved.rebuilt);
    assert_ne!((moved.lists[0].start, moved.lists[0].end), (window.start, window.end));
}

#[test]
fn a_moved_window_builds_the_tree_a_fresh_surface_builds() {
    let mut s = chat(2_000);
    let mut m = fixed();
    for _ in 0..3 {
        s.layout(&mut m);
    }
    // Down a row at a time, then a long jump, then back: every step reuses
    // the rows that stay.
    for y in (0..30).map(|k| k as f32 * 37.0).chain([20_000.0, 19_960.0, 120.0]) {
        s.scroll("list", y);
        s.layout(&mut m);
        s.layout(&mut m);
        let mut fresh = chat(2_000);
        let mut fm = fixed();
        fresh.layout(&mut fm);
        fresh.scroll("list", y);
        for _ in 0..3 {
            fresh.layout(&mut fm);
        }
        assert_eq!(tree(&mut s), tree(&mut fresh), "at {y}");
    }
}

#[test]
fn new_templates_drop_the_kept_instances() {
    let mut s = chat(200);
    let mut m = fixed();
    for _ in 0..3 {
        s.layout(&mut m);
    }
    // The same ids, another row template.
    let tree2: NestedNode = serde_json::from_value(json!({
        "id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%"},
        "children": [
            {"id": "list", "component": "List", "style": {"height": 400}, "template": {"component": "row", "path": "/items", "key": "id"}},
            {"id": "row", "component": "Box", "style": {"height": 40, "flexShrink": 0}, "children": [
                {"id": "label", "component": "Text", "props": {"text": {"@path": "id"}}}
            ]}
        ]
    }))
    .expect("tree");
    assert!(s.set_nested(tree2).issues.is_empty());
    s.scroll("list", 400.0);
    for _ in 0..3 {
        s.layout(&mut m);
    }
    let labels: Vec<Value> = s.nodes().into_iter().filter(|n| n.id.starts_with("label.") && !n.hidden).map(|n| n.props.get("text").cloned().unwrap_or(Value::Null)).collect();
    assert!(!labels.is_empty());
    assert!(labels.iter().all(|t| t.as_str().is_some_and(|t| t.starts_with('i'))), "{labels:?}");
    assert!(s.nodes().iter().all(|n| !n.id.starts_with("badge.")), "the old template's nodes are gone");
}
