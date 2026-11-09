//! #EXP-1629: the Select's `row` trigger (a Group row of parts: the
//! label, a spacer, the value, the chevron), the popup's `align` and
//! `popupWidth` (row: under the VALUE, start, the layout's 262), the
//! options' states (only the chosen one is `selected` and checked), the
//! list opening at the top and the bottom fade while options sit below.

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
fn a_row_trigger_is_label_spacer_value_and_chevron() {
    let mut s = surface(select(json!({"triggerVariant": "row"})));
    let out = s.layout(&mut fixed());
    let t = node(&mut s, "sel.trigger");
    assert_eq!(t.props["label"], json!("Week starts on"));
    assert_eq!(t.props["text"], json!("Sunday"), "the trigger keeps its accessible value");
    let trigger = s.index_of("sel.trigger").unwrap();
    let parts: Vec<String> = s.nodes().into_iter().filter(|n| n.parent == Some(trigger)).map(|n| n.id).collect();
    assert_eq!(parts, ["sel.label", "sel.spacer", "sel.value", "sel.chevron"]);
    assert_eq!(node(&mut s, "sel.label").props["text"], json!("Week starts on"));
    assert_eq!(node(&mut s, "sel.value").props["text"], json!("Sunday"));
    assert_eq!(node(&mut s, "sel.chevron").props["name"], json!("ui-chevron-right"), "Select.chevron");
    let f = frame(&s, &out, "sel.trigger");
    assert!((f.h - 40.82).abs() < 0.01, "the exponential row recipe: the IDE picker row's height, {}", f.h);
    assert_eq!(f.w, 760.0 - 32.0, "the whole row");
    let value = frame(&s, &out, "sel.value");
    let chevron = frame(&s, &out, "sel.chevron");
    assert!(value.x + value.w <= chevron.x + 0.01 && value.x > f.x + f.w / 2.0, "the value trails, before the chevron");

    let mut long = surface(select(json!({"triggerVariant": "row", "options": [{"label": "x".repeat(200), "value": "sunday"}]})));
    let out = long.layout(&mut fixed());
    assert!(frame(&long, &out, "sel.value").w <= 240.01, "selectRowValueMaxWidth");

    let mut field = surface(select(json!({})));
    field.layout(&mut fixed());
    assert!(field.index_of("sel.label").is_some(), "a field keeps its label above");
    let t = node(&mut field, "sel.trigger");
    assert!(t.props.get("label").is_none());
    assert_eq!(t.props["icon"], json!("ui-selector"), "Select.trigger");
}

#[test]
fn a_row_popup_is_262_wide_and_hangs_off_the_value() {
    let mut s = surface(select(json!({"triggerVariant": "row"})));
    let out = open(&mut s);
    let value = frame(&s, &out, "sel.value");
    let root = out.layers[0].frames[0];
    assert_eq!(root.w, 262.0, "selectRowPopupWidth");
    assert!((root.y - (value.y + value.h + 1.0)).abs() < 0.01, "selectRowPopupOffset under the value");
    // Starting with the value, shifted back inside the view (margin 7).
    assert!((root.x - value.x.min(760.0 - 7.0 - 262.0)).abs() < 0.01, "align start, fitted: {} vs {}", root.x, value.x);
    assert!(root.x + root.w <= 760.0 - 7.0 + 0.01);

    let mut s = surface(select(json!({"triggerVariant": "row", "align": "end", "popupWidth": 120})));
    let out = open(&mut s);
    let value = frame(&s, &out, "sel.value");
    let root = out.layers[0].frames[0];
    assert_eq!(root.w, 120.0);
    assert!((root.x + root.w - (value.x + value.w)).abs() < 0.01, "align end with the value");
}

#[test]
fn popups_fit_the_host_bounds() {
    let mut s = surface(select(json!({"triggerVariant": "row"})));
    // The host's window is wider than the surface: the popup keeps its start.
    s.set_popup_bounds(Some((-100.0, -100.0, 2000.0, 2000.0)));
    let out = open(&mut s);
    let value = frame(&s, &out, "sel.value");
    let root = out.layers[0].frames[0];
    assert!((root.x - value.x).abs() < 0.01, "align start, unclamped by the surface");
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
fn every_opening_starts_at_the_top_with_the_fade_below() {
    let options: Vec<Value> = (0..40).map(|i| json!({"label": format!("Zone {i}"), "value": format!("z{i}")})).collect();
    let mut s = surface(json!({"id": "sel", "component": "Select", "props": {"label": "Timezone", "name": "tz", "triggerVariant": "row", "value": "z30", "options": options}}));
    open(&mut s);
    assert_eq!(s.scroll_offset("sel.list").1, 0.0, "the chosen row is not revealed: a fresh pick");
    assert!(s.index_of("sel.fade").is_some(), "more options below: the fade");

    // Scrolled to the end, the fade goes; the offset stays the user's.
    s.scroll_to("sel.list", 0.0, 100_000.0);
    s.layout(&mut fixed());
    let y = s.scroll_offset("sel.list").1;
    assert!(y > 0.0);
    assert!(s.index_of("sel.fade").is_none(), "nothing below: no fade");

    // Reopened: back at the top, the fade back.
    let t = s.index_of("sel.trigger").unwrap();
    s.event(t, "press", None);
    s.layout(&mut fixed());
    s.event(t, "press", None);
    s.layout(&mut fixed());
    assert_eq!(s.scroll_offset("sel.list").1, 0.0);
    assert!(s.index_of("sel.fade").is_some());

    // A short list never fades.
    let mut s = surface(select(json!({"triggerVariant": "row"})));
    open(&mut s);
    assert!(s.index_of("sel.fade").is_none());
}
