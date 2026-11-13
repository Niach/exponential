//! #EXP-1813: two rules the repo page's CI settings exposed when web,
//! desktop, iOS and Android drew the same tree.
//!
//! 1. A ChipInput's `input` part carries the placeholder only while it holds
//!    no chips; every painter reads the PART's placeholder (Compose fell back
//!    to the owner's and kept "Check name" beside the chips).
//! 2. A Button is a flex item like any other: a stretch column (a Section's
//!    rows, a vertical Stack) stretches it across, a row or an overlay's
//!    `trigger` slot keeps its own width. React's twin:
//!    packages/exponential-ui-react/browser/exp1813.test.ts.

use eui_core::measure::{DirectMeasureWithBaseline, LeafRequest};
use eui_core::surface::{Surface, SurfaceOptions};
use eui_core::types::NestedNode;
use serde_json::{json, Value};

fn surface(tree: Value, width: f32) -> Surface {
    let tree: NestedNode = serde_json::from_value(tree).expect("tree");
    let mut s = Surface::new("t", SurfaceOptions { theme: eui_core::themes::builtin_theme("neutral"), ..SurfaceOptions::default() });
    let outcome = s.set_nested(tree);
    assert!(outcome.issues.is_empty(), "{:?}", outcome.issues);
    s.set_viewport(width, 0.0, None);
    s
}

fn measure() -> DirectMeasureWithBaseline<impl FnMut(&LeafRequest, Option<f32>) -> (f32, f32, Option<f32>)> {
    DirectMeasureWithBaseline(|leaf: &LeafRequest, _wrap: Option<f32>| (7.0 * leaf.text().chars().count() as f32, leaf.text_style.line_height.max(16.0), None), 11)
}

fn placeholder(values: Value) -> String {
    let mut s = surface(json!({"id": "req", "component": "ChipInput", "props": {"name": "req", "values": values, "placeholder": "Check name"}}), 390.0);
    s.layout(&mut measure());
    let n = s.nodes().into_iter().find(|n| n.id == "req.input").expect("the input part");
    n.props.get("placeholder").and_then(Value::as_str).unwrap_or("").to_string()
}

#[test]
fn a_chip_input_shows_its_placeholder_only_while_empty() {
    assert_eq!(placeholder(json!([])), "Check name");
    assert_eq!(placeholder(json!(["build"])), "", "a chip hides the hint");
}

fn button(id: &str) -> Value {
    json!({"id": id, "component": "Button", "props": {"label": "Add secret", "variant": "outline", "size": "sm"}})
}

#[test]
fn a_button_stretches_in_a_stretch_column_and_keeps_its_width_in_a_row_or_a_trigger_slot() {
    let mut s = surface(
        json!({"id": "ci", "component": "Section", "props": {"title": "CI"}, "children": [
            button("direct"),
            {"id": "col", "component": "Stack", "props": {"direction": "vertical"}, "children": [button("in_column")]},
            {"id": "row", "component": "Stack", "props": {"direction": "horizontal"}, "children": [button("in_row")]},
            {"id": "start", "component": "Stack", "props": {"direction": "vertical", "align": "start"}, "children": [button("in_start")]},
            {"id": "drawer", "component": "Drawer", "props": {"title": "Add secret"}, "slots": {"trigger": button("trigger")}, "children": [{"id": "body", "component": "Text", "props": {"text": "x"}}]}
        ]}),
        390.0,
    );
    s.layout(&mut measure());
    let w = |id: &str| s.last_frame(s.index_of(id).unwrap_or_else(|| panic!("no {id}"))).unwrap_or_else(|| panic!("{id} not placed")).w;
    let full = w("ci");
    assert_eq!(w("direct"), full, "a Section's row stretches a Button");
    assert_eq!(w("in_column"), full, "a vertical Stack (align unset = stretch) stretches it");
    let own = w("in_row");
    assert!(own < full / 2.0, "a row keeps its own width: {own}");
    assert_eq!(w("in_start"), own, "align start keeps its own width");
    assert_eq!(w("trigger"), own, "an overlay's trigger slot keeps its own width");
}
