//! #EXP-1629: the Select's `row` trigger (a Group row: the label leads
//! inside the trigger, the chevron glyph ends it), the popup's `align` and
//! `popupWidth` (row: end, the layout's 260), the options' states (only the
//! chosen one is `selected` and checked) and the first chosen option
//! scrolled into the list's view on opening.

use std::collections::HashMap;

use eui_core::measure::FixedMeasure;
use eui_core::surface::{LayoutOutput, PlacedFrame, Surface, SurfaceOptions};
use eui_core::types::NestedNode;
use serde_json::{json, Value};

fn surface(field: Value) -> Surface {
    let tree: NestedNode = serde_json::from_value(json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "padding": 16}, "children": [field]})).expect("tree");
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

fn node(s: &mut Surface, id: &str) -> eui_core::surface::PlacedNode {
    s.nodes().into_iter().find(|n| n.id == id).unwrap_or_else(|| panic!("no node {id}"))
}

fn select(extra: Value) -> Value {
    let mut props = json!({"label": "Week starts on", "name": "week", "value": "sunday", "options": [{"label": "Monday", "value": "monday"}, {"label": "Sunday", "value": "sunday"}]});
    for (k, v) in extra.as_object().unwrap() {
        props[k] = v.clone();
    }
    json!({"id": "sel", "component": "Select", "props": props})
}

fn open(s: &mut Surface) -> LayoutOutput {
    let mut m = fixed();
    s.layout(&mut m);
    let t = s.index_of("sel.trigger").unwrap();
    s.event(t, "press", None);
    s.layout(&mut m)
}

#[test]
fn a_row_trigger_leads_with_its_label_and_ends_with_the_chevron() {
    let mut s = surface(select(json!({"triggerVariant": "row"})));
    let out = s.layout(&mut fixed());
    assert!(s.index_of("sel.label").is_none(), "no label above the row");
    let t = node(&mut s, "sel.trigger");
    assert_eq!(t.props["label"], json!("Week starts on"));
    assert_eq!(t.props["text"], json!("Sunday"));
    assert_eq!(t.props["icon"], json!("ui-chevron-right"), "Select.chevron");
    let f = frame(&s, &out, "sel.trigger");
    assert!((f.h - 40.82).abs() < 0.01, "the exponential row recipe: the IDE picker row's height, {}", f.h);
    assert_eq!(f.w, 760.0 - 32.0, "the whole row");

    let mut field = surface(select(json!({})));
    field.layout(&mut fixed());
    assert!(field.index_of("sel.label").is_some(), "a field keeps its label above");
    let t = node(&mut field, "sel.trigger");
    assert!(t.props.get("label").is_none());
    assert_eq!(t.props["icon"], json!("ui-selector"), "Select.trigger");
}

#[test]
fn a_row_popup_is_260_wide_and_ends_with_the_row() {
    let mut s = surface(select(json!({"triggerVariant": "row"})));
    let out = open(&mut s);
    let trigger = frame(&s, &out, "sel.trigger");
    let root = out.layers[0].frames[0];
    assert_eq!(root.w, 260.0, "selectRowPopupWidth");
    assert!((root.x + root.w - (trigger.x + trigger.w)).abs() < 0.01, "align end");
    assert!(root.y >= trigger.y + trigger.h, "under the row");

    let mut s = surface(select(json!({"triggerVariant": "row", "align": "start", "popupWidth": 300})));
    let out = open(&mut s);
    let trigger = frame(&s, &out, "sel.trigger");
    let root = out.layers[0].frames[0];
    assert_eq!(root.w, 300.0);
    assert!((root.x - trigger.x).abs() < 0.01, "align start");
}

#[test]
fn a_field_popup_is_at_least_the_trigger_and_starts_with_it() {
    let mut s = surface(select(json!({})));
    let out = open(&mut s);
    let trigger = frame(&s, &out, "sel.trigger");
    let root = out.layers[0].frames[0];
    assert!(root.w >= trigger.w - 0.01);
    assert!((root.x - trigger.x).abs() < 0.01);

    let mut s = surface(select(json!({"align": "end", "popupWidth": 200})));
    let out = open(&mut s);
    let trigger = frame(&s, &out, "sel.trigger");
    let root = out.layers[0].frames[0];
    assert_eq!(root.w, 200.0, "popupWidth wins over the trigger's width");
    assert!((root.x + root.w - (trigger.x + trigger.w)).abs() < 0.01);
}

#[test]
fn only_the_chosen_option_is_selected_and_checked() {
    let mut s = surface(select(json!({"triggerVariant": "row"})));
    open(&mut s);
    let monday = node(&mut s, "sel.item.0");
    let sunday = node(&mut s, "sel.item.1");
    assert!(!monday.states.iter().any(|x| x == "selected") && monday.props.get("check").is_none());
    assert!(sunday.states.iter().any(|x| x == "selected"));
    assert_eq!(sunday.props["check"], json!("ui-check"));
}

#[test]
fn opening_scrolls_the_first_chosen_option_into_view() {
    let options: Vec<Value> = (0..40).map(|i| json!({"label": format!("Zone {i}"), "value": format!("z{i}")})).collect();
    let mut s = surface(json!({"id": "sel", "component": "Select", "props": {"label": "Timezone", "name": "tz", "triggerVariant": "row", "value": "z30", "options": options}}));
    let out = open(&mut s);
    let list = frame(&s, &out, "sel.list");
    let item = frame(&s, &out, "sel.item.30");
    let (_, y) = s.scroll_offset("sel.list");
    assert!(y > 0.0, "scrolled");
    let top = item.y - list.y - y;
    assert!(top >= -0.01 && top + item.h <= list.h + 0.01, "the chosen row is in view: top {top}, list {}", list.h);
    let scroll = out.scrolls.iter().find(|sc| sc.index == s.index_of("sel.list").unwrap()).expect("the list scrolls");
    assert_eq!(scroll.offset_y, y, "this pass already paints at it");

    // Scrolled by the user, the offset stays theirs until the next opening.
    s.scroll_to("sel.list", 0.0, 0.0);
    s.layout(&mut fixed());
    assert_eq!(s.scroll_offset("sel.list").1, 0.0);
    let t = s.index_of("sel.trigger").unwrap();
    s.event(t, "press", None);
    s.layout(&mut fixed());
    s.event(t, "press", None);
    s.layout(&mut fixed());
    assert_eq!(s.scroll_offset("sel.list").1, y, "reopened: back on the chosen row");

    // Nothing chosen: the list starts at the top.
    let mut s = surface(json!({"id": "sel", "component": "Select", "props": {"label": "Timezone", "name": "tz", "triggerVariant": "row", "options": (0..40).map(|i| json!({"label": format!("Zone {i}"), "value": format!("z{i}")})).collect::<Vec<_>>()}}));
    open(&mut s);
    assert_eq!(s.scroll_offset("sel.list").1, 0.0);
}
