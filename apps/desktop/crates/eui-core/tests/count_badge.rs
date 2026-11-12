//! #FEED-232: the CountBadge native in the layout core (gpui paints these
//! nodes; SwiftUI and Compose get them through the FFI): the label lands in
//! the node's `text` (`99+`, `max+`), the box is the recipe's 16 px capsule
//! (min width 16, 4 px sides around the label), and a count of 0 or less
//! is hidden: no box, nothing painted.

use std::collections::HashMap;

use eui_core::measure::{FixedMeasure, FIXED_CHAR_WIDTH};
use eui_core::surface::{Surface, SurfaceOptions};
use eui_core::types::NestedNode;
use serde_json::json;

#[test]
fn labels_boxes_and_hides() {
    let tree: NestedNode = serde_json::from_value(json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "flex-start", "gap": 8}, "children": [
        {"id": "a", "component": "CountBadge", "props": {"count": 3, "tone": "attention"}},
        {"id": "b", "component": "CountBadge", "props": {"count": 120}},
        {"id": "c", "component": "CountBadge", "props": {"count": 12, "max": 9}},
        {"id": "z", "component": "CountBadge", "props": {"count": 0, "tone": "attention"}},
        {"id": "n", "component": "CountBadge", "props": {"count": -3}},
        {"id": "d", "component": "CountBadge", "props": {"count": {"@path": "/unread"}}}
    ]}))
    .unwrap();
    let mut s = Surface::new("t", SurfaceOptions::default());
    let outcome = s.set_nested(tree);
    assert!(outcome.issues.is_empty(), "{:?}", outcome.issues);
    s.set_data("", Some(json!({"unread": 7}))).expect("data");
    s.set_viewport(600.0, 400.0, None);
    let out = s.layout(&mut FixedMeasure { sizes: HashMap::new(), wrap: false });
    let frame = |id: &str| {
        let i = s.index_of(id).unwrap_or_else(|| panic!("no node {id}"));
        out.frames.iter().find(|fr| fr.index == i).copied().map(|fr| (fr.w, fr.h))
    };
    let text = |id: &str| s.layout_node(s.index_of(id).unwrap()).unwrap().props.get("text").and_then(|v| v.as_str()).map(str::to_string);
    assert_eq!(text("a").as_deref(), Some("3"));
    assert_eq!(text("b").as_deref(), Some("99+"));
    assert_eq!(text("c").as_deref(), Some("9+"));
    assert_eq!(text("d").as_deref(), Some("7"));
    assert_eq!(text("z").as_deref(), Some(""));
    // One figure: the label + 4 + 4 is at most the 16 px minimum.
    assert_eq!(frame("a"), Some((16.0f32.max(FIXED_CHAR_WIDTH + 8.0), 16.0)));
    assert_eq!(frame("b"), Some((3.0 * FIXED_CHAR_WIDTH + 8.0, 16.0)));
    for id in ["z", "n"] {
        assert!(s.layout_node(s.index_of(id).unwrap()).unwrap().hidden, "{id} is hidden");
        assert!(frame(id).is_none_or(|(w, h)| w == 0.0 && h == 0.0), "{id} takes no room");
    }
}
