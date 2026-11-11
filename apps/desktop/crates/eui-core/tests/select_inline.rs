//! #EXP-1629: the Select's `inline` trigger (web's `Picker
//! triggerVariant="inline"`): content-sized, no label part (the label names
//! it), the `Select.dropdown` glyph; its popup is the field's.

use std::collections::HashMap;

use eui_core::measure::FixedMeasure;
use eui_core::surface::{LayoutOutput, PlacedFrame, Surface, SurfaceOptions};
use eui_core::types::NestedNode;
use serde_json::{json, Value};

fn surface(field: Value) -> Surface {
    let tree: NestedNode = serde_json::from_value(json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "alignItems": "flex-end", "width": "100%", "padding": 16}, "children": [field]})).expect("tree");
    let mut s = Surface::new("t", SurfaceOptions::default());
    let outcome = s.set_nested(tree);
    assert!(outcome.issues.is_empty(), "{:?}", outcome.issues);
    s.set_viewport(760.0, 900.0, None);
    s
}

fn fixed() -> FixedMeasure {
    FixedMeasure { sizes: HashMap::new(), wrap: true }
}

fn frame(s: &Surface, out: &LayoutOutput, id: &str) -> PlacedFrame {
    let i = s.index_of(id).unwrap_or_else(|| panic!("no node {id}"));
    out.frames.iter().chain(out.layers.iter().flat_map(|l| l.frames.iter())).find(|f| f.index == i).copied().unwrap_or_else(|| panic!("{id} not drawn"))
}

fn inline() -> Value {
    json!({"id": "sel", "component": "Select", "props": {"label": "Access", "name": "access", "value": "read", "triggerVariant": "inline", "align": "end",
        "options": [{"label": "None", "value": "none"}, {"label": "Read only", "value": "read"}, {"label": "Full", "value": "full"}]}})
}

#[test]
fn an_inline_trigger_hugs_its_content_without_a_label_part_and_names_itself() {
    let mut s = surface(inline());
    let out = s.layout(&mut fixed());
    let t = frame(&s, &out, "sel.trigger");
    let root = frame(&s, &out, "root");
    assert!(t.w < root.w - 32.0, "content-sized, not stretched: {t:?}");
    assert!(s.index_of("sel.label").is_none(), "no label part");
    let n = s.nodes().into_iter().find(|n| n.id == "sel.trigger").unwrap();
    assert_eq!(n.props.get("icon").and_then(Value::as_str), Some("ui-chevron-down"));
    assert_eq!(n.props.get("text").and_then(Value::as_str), Some("Read only"));
    assert_eq!(n.accessibility.as_ref().and_then(|a| a.get("label")).and_then(Value::as_str), Some("Access"));
}

#[test]
fn an_inline_popup_is_the_fields_with_the_chosen_option_checked() {
    let mut s = surface(inline());
    let mut m = fixed();
    s.layout(&mut m);
    let t = s.index_of("sel.trigger").unwrap();
    s.event(t, "press", None);
    let out = s.layout(&mut m);
    let trigger = frame(&s, &out, "sel.trigger");
    let c = frame(&s, &out, "sel.content");
    assert!(c.w >= trigger.w - 0.01, "at least the trigger: {c:?} vs {trigger:?}");
    // `align: end`: the popup's end edge on the trigger's.
    assert!(((c.x + c.w) - (trigger.x + trigger.w)).abs() < 0.01, "{c:?} vs {trigger:?}");
    let chosen = s.nodes().into_iter().find(|n| n.id == "sel.item.1").unwrap();
    assert!(chosen.props.get("check").is_some(), "the chosen row holds the check");
    assert!(chosen.props.get("checkSlot").is_none(), "the field's check slot, not the button menu's");
}
