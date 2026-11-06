//! VAPP-280 (round 7): `SettingsRow` in the layout core: the label column
//! (label + description) at the start and the control column at the end on a
//! wide surface; below `stackBelow` (default `sm`, 640 px in the built-in
//! themes) the control goes under the label and stretches to the row's
//! width; `never` keeps the row at every width; RTL mirrors the columns; a
//! short row is at least 44 px tall (the app's settings row: 12 px around a
//! 20 px line). The natives paint these frames.

use std::collections::HashMap;

use eui_core::measure::FixedMeasure;
use eui_core::surface::{LayoutOutput, PlacedFrame, Surface, SurfaceOptions};
use eui_core::types::NestedNode;
use serde_json::{json, Value};

fn surface(tree: Value) -> Surface {
    let tree: NestedNode = serde_json::from_value(tree).expect("tree");
    let mut s = Surface::new("t", SurfaceOptions::default());
    let outcome = s.set_nested(tree);
    assert!(outcome.issues.is_empty(), "{:?}", outcome.issues);
    s
}

fn frame(s: &Surface, out: &LayoutOutput, id: &str) -> PlacedFrame {
    let i = s.index_of(id).unwrap_or_else(|| panic!("no node {id}"));
    out.frames.iter().find(|f| f.index == i).copied().unwrap_or_else(|| panic!("{id} not drawn"))
}

fn tree(direction: &str, stack_below: Option<&str>, control: Value) -> Value {
    let mut props = json!({"label": "Digest", "description": "How often the summary goes out."});
    if let Some(s) = stack_below {
        props["stackBelow"] = json!(s);
    }
    json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "direction": direction}, "children": [
        {"id": "r", "component": "SettingsRow", "props": props, "children": [control]}
    ]})
}

fn select() -> Value {
    json!({"id": "sel", "component": "Select", "props": {"name": "digest", "value": "daily", "options": [{"label": "Daily", "value": "daily"}]}})
}

fn lay(t: Value, width: f32) -> (Surface, LayoutOutput) {
    let mut s = surface(t);
    s.set_viewport(width, 800.0, None);
    let mut m = FixedMeasure { sizes: HashMap::new(), wrap: true };
    let out = s.layout(&mut m);
    (s, out)
}

#[test]
fn wide_the_label_column_leads_and_the_control_ends_the_row() {
    let (s, out) = lay(tree("ltr", None, select()), 900.0);
    let (row, text, control) = (frame(&s, &out, "r"), frame(&s, &out, "r.text"), frame(&s, &out, "r.control"));
    assert_eq!((row.x, row.w), (0.0, 900.0));
    assert_eq!(text.x, 16.0, "padding-x lg");
    assert_eq!(control.x + control.w, 884.0, "the control ends 16 from the edge");
    assert_eq!(control.w, 320.0, "the control column: 40% of the row, at most 320");
    assert!(text.x + text.w + 12.0 <= control.x + 0.01, "gap md between the columns");
    // One line: both columns are centred on the row's axis.
    let mid = |f: PlacedFrame| f.y + f.h / 2.0;
    assert!((mid(text) - mid(control)).abs() < 0.01, "{text:?} {control:?}");
    let label = frame(&s, &out, "r.text.label");
    let description = frame(&s, &out, "r.text.description");
    assert!(description.y >= label.y + label.h, "the description sits under the label");
}

#[test]
fn narrow_the_control_stacks_under_the_label_and_stretches() {
    let (s, out) = lay(tree("ltr", None, select()), 390.0);
    let (text, control) = (frame(&s, &out, "r.text"), frame(&s, &out, "r.control"));
    assert_eq!(text.x, 16.0);
    assert_eq!(control.x, 16.0);
    assert_eq!(control.w, 358.0, "the control column takes the row's inner width");
    assert_eq!(control.y, text.y + text.h + 8.0, "gap sm when stacked");
    assert_eq!(text.w, 358.0);
}

#[test]
fn stack_below_moves_the_breakpoint_and_never_keeps_the_row() {
    // md (768): 700 stacks, 800 does not.
    let (s, out) = lay(tree("ltr", Some("md"), select()), 700.0);
    assert!(frame(&s, &out, "r.control").y > frame(&s, &out, "r.text").y);
    let (s, out) = lay(tree("ltr", Some("md"), select()), 800.0);
    assert_eq!(frame(&s, &out, "r.control").x + frame(&s, &out, "r.control").w, 784.0);
    assert!((frame(&s, &out, "r.control").w - 307.2).abs() < 0.01, "40% of the 768 inner width");
    // never: a phone keeps the switch beside its label.
    let switch = json!({"id": "sw", "component": "Switch", "props": {"label": "", "name": "email", "checked": true}});
    let (s, out) = lay(tree("ltr", Some("never"), switch), 390.0);
    let (text, control) = (frame(&s, &out, "r.text"), frame(&s, &out, "r.control"));
    assert_eq!(control.x + control.w, 374.0);
    assert!(text.x + text.w <= control.x);
}

#[test]
fn a_label_less_switch_keeps_the_web_body_column_and_ends_with_its_track() {
    // `label: ""`: the web's empty `body` (it grows), no empty `label` part,
    // so every renderer places the same nodes and the track ends the row.
    let switch = json!({"id": "sw", "component": "Switch", "props": {"label": "", "name": "email", "checked": true}});
    let (s, out) = lay(tree("ltr", Some("never"), switch), 900.0);
    assert!(s.index_of("sw.label").is_none(), "no empty label part");
    let (control, body, track) = (frame(&s, &out, "r.control"), frame(&s, &out, "sw.body"), frame(&s, &out, "sw.track"));
    assert_eq!(track.x + track.w, control.x + control.w, "the track ends the control column");
    assert!(body.w > 0.0 && body.x + body.w <= track.x);
}

#[test]
fn rtl_mirrors_the_columns() {
    let (s, out) = lay(tree("rtl", None, select()), 900.0);
    let (text, control) = (frame(&s, &out, "r.text"), frame(&s, &out, "r.control"));
    assert_eq!(text.x + text.w, 884.0, "the label column starts at the right");
    assert_eq!(control.x, 16.0, "the control ends at the left");
}

#[test]
fn a_short_row_is_at_least_44_tall() {
    let t = json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%"}, "children": [
        {"id": "r", "component": "SettingsRow", "props": {"label": "Sounds"}, "children": [
            {"id": "sw", "component": "Switch", "props": {"label": "", "name": "sounds"}}
        ]}
    ]});
    let (s, out) = lay(t, 900.0);
    assert!(frame(&s, &out, "r").h >= 44.0, "{:?}", frame(&s, &out, "r"));
    assert!(s.index_of("r.text.description").is_none(), "no description part without one");
}

#[test]
fn the_row_is_a_group_named_by_its_label() {
    let (mut s, _) = lay(tree("ltr", None, select()), 900.0);
    let row = s.nodes().into_iter().find(|n| n.id == "r").unwrap();
    let a11y = serde_json::to_value(&row.accessibility).unwrap();
    assert_eq!(a11y["role"], "group");
    assert_eq!(a11y["label"], "Digest");
    assert_eq!(a11y["description"], "How often the summary goes out.");
}
