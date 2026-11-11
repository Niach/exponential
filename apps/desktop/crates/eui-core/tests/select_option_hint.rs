//! #EXP-1629: a Select option's `hint` is the `itemHint` part: a small
//! muted run at the row's end, before the chosen one's check. `trailing`
//! stays the label's own run.

use std::collections::HashMap;

use eui_core::measure::FixedMeasure;
use eui_core::surface::{Surface, SurfaceOptions};
use eui_core::types::NestedNode;
use serde_json::json;

fn open() -> Surface {
    let tree: NestedNode = serde_json::from_value(json!({"id": "root", "component": "Box", "children": [
        {"id": "sel", "component": "Select", "props": {"label": "Estimate scale", "name": "scale", "triggerVariant": "button", "value": "tshirt", "options": [
            {"label": "T-shirt", "value": "tshirt", "hint": "XS, S, M, L, XL"},
            {"label": "Fibonacci", "value": "fibonacci", "hint": "1, 2, 3, 5, 8 points"},
            {"label": "Not in use", "value": "none"},
        ]}},
    ]}))
    .expect("tree");
    let mut s = Surface::new("t", SurfaceOptions::default());
    let issues = s.set_nested(tree).issues;
    assert!(issues.is_empty(), "{issues:?}");
    s.set_viewport(760.0, 900.0, None);
    let mut m = FixedMeasure { sizes: HashMap::new(), wrap: true };
    s.layout(&mut m);
    let t = s.index_of("sel.trigger").expect("the trigger");
    s.event(t, "press", None);
    s.layout(&mut m);
    s
}

fn kids(s: &mut Surface, id: &str) -> Vec<String> {
    let i = s.index_of(id).unwrap_or_else(|| panic!("no {id}"));
    s.nodes().into_iter().filter(|n| n.parent == Some(i)).map(|n| n.id).collect()
}

#[test]
fn the_hint_sits_at_the_row_end_before_the_check() {
    let mut s = open();
    assert_eq!(kids(&mut s, "sel.item.0"), ["sel.itemText.0", "sel.itemHint.0", "sel.check.0"]);
    assert_eq!(kids(&mut s, "sel.item.1"), ["sel.itemText.1", "sel.itemHint.1"]);
    let hint = s.nodes().into_iter().find(|n| n.id == "sel.itemHint.0").expect("the hint");
    assert_eq!(hint.props.get("text"), Some(&json!("XS, S, M, L, XL")));
}

#[test]
fn an_option_with_no_hint_stays_one_leaf() {
    let mut s = open();
    assert!(kids(&mut s, "sel.item.2").is_empty());
}
