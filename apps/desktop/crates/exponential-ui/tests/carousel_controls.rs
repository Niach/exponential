//! Round 2 §7: a Carousel with 2+ pages places a `controls` row under the
//! pages: `previous`, the dots (`indicator`), `next`. A button press pages.

use std::collections::HashMap;

use exponential_ui::measure::FixedMeasure;
use exponential_ui::surface::{Surface, SurfaceOptions};
use serde_json::{json, Value};

fn carousel(extra: Value) -> Surface {
    let mut s = Surface::new("s", SurfaceOptions::default());
    s.apply(&json!({"version": "v0.9", "createSurface": {"surfaceId": "s", "catalogId": "https://ui.exponential.at/catalogs/core/v1"}})).unwrap();
    let mut root = json!({"id": "root", "component": "Carousel", "children": ["a", "b", "c"]});
    root.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
    s.apply(&json!({"version": "v0.9", "updateComponents": {"surfaceId": "s", "components": [
        root,
        {"id": "a", "component": "Text", "text": "1"},
        {"id": "b", "component": "Text", "text": "2"},
        {"id": "c", "component": "Text", "text": "3"}
    ]}})).unwrap();
    s.set_viewport(390.0, 0.0, None);
    s.layout(&mut FixedMeasure { sizes: HashMap::new(), wrap: true });
    s
}

fn find(s: &mut Surface, id: &str) -> (u32, Value) {
    let nodes = s.nodes();
    let i = nodes.iter().position(|n| n.id == id).unwrap_or_else(|| panic!("{id} placed"));
    (i as u32, Value::Object(nodes[i].props.clone()))
}

#[test]
fn the_controls_row_holds_previous_the_dots_and_next() {
    let mut s = carousel(json!({}));
    let nodes = s.nodes();
    let controls = nodes.iter().position(|n| n.id == "root.controls").expect("root.controls") as u32;
    let kids: Vec<&str> = nodes.iter().filter(|n| n.parent == Some(controls)).map(|n| n.id.as_str()).collect();
    assert_eq!(kids, ["root.previous", "root.indicator", "root.next"]);
    let (_, prev) = find(&mut s, "root.previous");
    assert_eq!(prev["disabled"], json!(true), "page 0 without loop");
    assert_eq!(prev["label"], json!("Previous"));
}

#[test]
fn next_and_previous_page_and_loop_wraps() {
    let mut s = carousel(json!({}));
    let (next, _) = find(&mut s, "root.next");
    s.event(next, "press", None);
    s.layout(&mut FixedMeasure { sizes: HashMap::new(), wrap: true });
    let (_, ind) = find(&mut s, "root.indicator");
    assert_eq!(ind["page"], json!(1));
    let (_, prev) = find(&mut s, "root.previous");
    assert_eq!((prev["target"].clone(), prev["disabled"].clone()), (json!(0), json!(false)));

    let mut s = carousel(json!({"loop": true}));
    let (_, prev) = find(&mut s, "root.previous");
    assert_eq!((prev["target"].clone(), prev["disabled"].clone()), (json!(2), json!(false)));
}

#[test]
fn one_page_has_no_controls() {
    let mut s = Surface::new("s", SurfaceOptions::default());
    s.apply(&json!({"version": "v0.9", "createSurface": {"surfaceId": "s", "catalogId": "https://ui.exponential.at/catalogs/core/v1"}})).unwrap();
    s.apply(&json!({"version": "v0.9", "updateComponents": {"surfaceId": "s", "components": [
        {"id": "root", "component": "Carousel", "children": ["a"]},
        {"id": "a", "component": "Text", "text": "1"}
    ]}})).unwrap();
    s.set_viewport(390.0, 0.0, None);
    s.layout(&mut FixedMeasure { sizes: HashMap::new(), wrap: true });
    assert!(!s.nodes().iter().any(|n| n.id == "root.controls"));
}
