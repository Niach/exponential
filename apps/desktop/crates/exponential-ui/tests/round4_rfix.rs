//! VAPP-103 rfix: the round-4 review's core findings, one regression each.

use std::collections::HashMap;

use exponential_ui::measure::FixedMeasure;
use exponential_ui::surface::{Surface, SurfaceOptions};
use serde_json::json;

fn surface() -> Surface {
    let mut s = Surface::new("t", SurfaceOptions { theme: exponential_ui::themes::builtin_theme("neutral"), ..SurfaceOptions::default() });
    s.set_viewport(400.0, 800.0, None);
    s
}

/// A grid item holding a 160 px child keeps its height; the next item sits
/// under it (the layout cache served a width-only answer's height: 0).
#[test]
fn a_grid_item_keeps_the_height_of_its_content() {
    let mut s = surface();
    s.apply(&json!({"updateComponents": {"surfaceId": "t", "components": [
        {"id": "root", "component": "Box", "style": {"display": "grid"}, "children": ["a", "b"]},
        {"id": "a", "component": "Box", "style": {"width": 180}, "children": ["tall"]},
        {"id": "tall", "component": "Box", "style": {"height": 160}},
        {"id": "b", "component": "Text", "text": "hello world this is text"},
    ]}}))
    .unwrap();
    let out = s.layout(&mut FixedMeasure { sizes: HashMap::new(), wrap: true });
    let frame = |s: &mut Surface, id: &str| {
        let i = s.index_of(id).unwrap();
        out.frames.iter().find(|f| f.index == i).cloned().unwrap()
    };
    let (a, b) = (frame(&mut s, "a"), frame(&mut s, "b"));
    assert_eq!(a.h, 160.0, "{a:?}");
    assert!(b.y >= 160.0, "{b:?}");
}

/// A streamed `updateComponents` reports its own components' issues at
/// once (the tree still reduces lazily).
#[test]
fn update_components_reports_its_components_issues() {
    let mut s = surface();
    let out = s.apply(&json!({"updateComponents": {"surfaceId": "t", "components": [
        {"id": "root", "component": "Stack", "children": ["x", "later"]},
        {"id": "x", "component": "NoSuchThing"},
    ]}}))
    .unwrap();
    let messages: Vec<String> = out.issues.iter().map(|i| format!("{}: {}", i.id, i.message)).collect();
    assert_eq!(messages, vec!["x: unknown component NoSuchThing".to_string()]);
    // The tree's issues (a missing child) come with the reduce.
    assert!(s.issues().iter().any(|i| i.id == "later"), "{:?}", s.issues());
    let ok = s.apply(&json!({"updateComponents": {"surfaceId": "t", "components": [{"id": "later", "component": "Text", "text": "hi"}]}})).unwrap();
    assert!(ok.issues.is_empty(), "{:?}", ok.issues);
}
