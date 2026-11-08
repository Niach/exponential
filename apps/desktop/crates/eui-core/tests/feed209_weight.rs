//! FEED-209: a theme's `$type.weight.medium` reaches the text style at 500.

use eui_core::surface::{Surface, SurfaceOptions};
use eui_core::types::NestedNode;
use serde_json::json;

#[test]
fn table_header_text_resolves_medium_weight() {
    let tree: NestedNode = serde_json::from_value(json!({"id": "t", "component": "Table", "props": {"columns": [{"key": "name", "label": "Name"}], "rows": [{"id": "a", "name": "A"}]}})).unwrap();
    let mut s = Surface::new("t", SurfaceOptions::default());
    assert!(s.set_nested(tree).issues.is_empty());
    s.set_viewport(600.0, 400.0, None);
    let mut m = eui_core::measure::FixedMeasure { sizes: Default::default(), wrap: true };
    s.layout(&mut m);
    for n in s.nodes() {
        let i = s.index_of(&n.id).unwrap();
        eprintln!("{} {:?} {:?} w={:?}", n.id, n.component, n.part, s.text_style(i).map(|t| t.font_weight));
    }
    let w = |id: &str| s.text_style(s.index_of(id).unwrap()).unwrap().font_weight;
    assert_eq!(w("t.headerCell.0"), 500);
    assert_eq!(w("t.cell.a.0"), 400);
}
