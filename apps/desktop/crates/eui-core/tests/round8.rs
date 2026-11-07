//! Round 8 (packages/exponential-ui/docs/round-8-contract.md): the core's
//! half of the batch: the `@container` style condition (Stepper
//! `collapse: auto`), the Canvas box, the inert / pointer Canvas events.

use std::collections::HashMap;

use eui_core::conditions::{parse_media_condition, resolve_conditions, ConditionContext};
use eui_core::measure::FixedMeasure;
use eui_core::surface::{OutEvent, Surface, SurfaceOptions};
use eui_core::types::NestedNode;
use serde_json::{json, Value};

fn surface(tree: Value, width: f32) -> Surface {
    let tree: NestedNode = serde_json::from_value(tree).expect("tree");
    let mut s = Surface::new("t", SurfaceOptions::default());
    s.set_nested(tree);
    s.set_viewport(width, 0.0, None);
    s
}

fn frame(s: &mut Surface, id: &str) -> (f32, f32) {
    let mut m = FixedMeasure { sizes: HashMap::new(), wrap: true };
    let out = s.layout(&mut m);
    let i = s.index_of(id).unwrap_or_else(|| panic!("no {id}"));
    out.frames.iter().find(|f| f.index == i).map(|f| (f.w, f.h)).unwrap_or((0.0, 0.0))
}

fn stepper_in(width: f32) -> Value {
    json!({"id": "root", "component": "Box", "children": [{"id": "col", "component": "Box", "style": {"width": width}, "children": [
        {"id": "wizard", "component": "Stepper", "props": {"steps": [{"label": "Account"}, {"label": "Team"}, {"label": "Done"}], "current": 1, "collapse": "auto"}}
    ]}]})
}

#[test]
fn container_conditions_parse_and_match_the_parent_width() {
    let c = parse_media_condition("@container (max-width: $breakpoint.sm)").expect("parsed");
    assert!(c.container);
    assert!(parse_media_condition("@container (min-height: 10px)").is_none());
    let style = json!({"display": "flex", "@container (max-width: $breakpoint.sm)": {"display": "none"}});
    let ctx = |w: Option<f32>| ConditionContext { width: 1200.0, breakpoints: eui_core::conditions::default_breakpoints(), container_width: w, ..Default::default() };
    let flat = |w| resolve_conditions(style.as_object().unwrap(), &ctx(w))["display"].clone();
    assert_eq!(flat(None), json!("flex"), "not laid out yet: never matches");
    assert_eq!(flat(Some(639.0)), json!("none"));
    assert_eq!(flat(Some(640.0)), json!("flex"));
}

#[test]
fn an_auto_stepper_collapses_in_a_narrow_column_of_a_wide_surface() {
    let mut s = surface(stepper_in(400.0), 1200.0);
    let full = frame(&mut s, "wizard.full");
    let compact = frame(&mut s, "wizard.compact");
    assert_eq!(full.1, 0.0, "the full stepper is hidden: {full:?}");
    assert!(compact.1 > 0.0, "the one line shows: {compact:?}");
    let line = s.nodes().into_iter().find(|n| n.id == "wizard.compact.line.1").expect("line");
    assert_eq!(line.props["text"], json!("Step 2 of 3 · Team"));
}

#[test]
fn an_auto_stepper_stays_full_in_a_wide_column_of_a_narrow_surface_s_width() {
    let mut s = surface(stepper_in(800.0), 1200.0);
    assert!(frame(&mut s, "wizard.full").1 > 0.0);
    assert_eq!(frame(&mut s, "wizard.compact").1, 0.0);
}

#[test]
fn a_canvas_boxes_like_a_motion_and_is_inert_without_pointer() {
    let shapes = json!([{"type": "rect", "position": [50, 25], "size": [100, 50]}, {"type": "fill", "color": "$color.primary"}]);
    let tree = |pointer: bool| json!({"id": "root", "component": "Box", "style": {"width": 120}, "children": [
        {"id": "cv", "component": "Canvas", "props": {"size": [200, 100], "shapes": shapes, "pointer": pointer}, "on": {"press": {"event": {"name": "hit"}}}}
    ]});
    let mut s = surface(tree(true), 400.0);
    // 200 px wide, shrunk to the 120 px parent, aspect 2:1 kept.
    assert_eq!(frame(&mut s, "cv"), (120.0, 60.0));
    let i = s.index_of("cv").unwrap();
    let fired: Vec<Value> = s.event(i, "press", Some(json!({"x": 12.5, "y": 40.0}))).into_iter().filter_map(|e| match e { OutEvent::Action { name, context, .. } if name == "hit" => Some(context), _ => None }).collect();
    assert_eq!(fired.len(), 1);
    assert_eq!(fired[0]["x"], json!(12.5));
    let mut s = surface(tree(false), 400.0);
    frame(&mut s, "cv");
    let i = s.index_of("cv").unwrap();
    assert!(s.event(i, "press", Some(json!({"x": 1, "y": 1}))).is_empty(), "pointer false: inert");
}
