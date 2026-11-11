//! #EXP-1629: a `button` Select trigger leads with the picked option's own
//! `icon` + `iconColor` (web's PR trigger): the picked option is the one
//! source, there is no prop. An option with no glyph, no pick or a `multiple`
//! Select leave the trigger bare.

use eui_core::surface::{Surface, SurfaceOptions};
use eui_core::types::NestedNode;
use serde_json::{json, Value};

fn trigger(extra: Value) -> serde_json::Map<String, Value> {
    let mut props = json!({"label": "When a PR opens", "name": "pr", "triggerVariant": "button", "options": [
        {"label": "In Review", "value": "review", "icon": "status-in-review", "iconColor": "$color.success"},
        {"label": "Backlog", "value": "backlog", "icon": "status-backlog"},
        {"label": "Do nothing", "value": "none"},
    ]});
    for (k, v) in extra.as_object().unwrap() {
        props[k] = v.clone();
    }
    let tree: NestedNode = serde_json::from_value(json!({"id": "root", "component": "Box", "children": [{"id": "sel", "component": "Select", "props": props}]})).expect("tree");
    let mut s = Surface::new("t", SurfaceOptions::default());
    let issues = s.set_nested(tree).issues;
    assert!(issues.is_empty(), "{issues:?}");
    s.nodes().into_iter().find(|n| n.id == "sel.trigger").expect("the trigger").props
}

#[test]
fn the_picked_option_gives_the_trigger_its_glyph_and_tint() {
    let props = trigger(json!({"value": "review"}));
    assert_eq!(props.get("valueIcon"), Some(&json!("status-in-review")));
    assert_eq!(props.get("iconColor"), Some(&json!("$color.success")));
    assert_eq!(props.get("text"), Some(&json!("In Review")));
}

#[test]
fn a_glyph_without_a_tint_carries_no_colour() {
    let props = trigger(json!({"value": "backlog"}));
    assert_eq!(props.get("valueIcon"), Some(&json!("status-backlog")));
    assert_eq!(props.get("iconColor"), None);
}

#[test]
fn no_glyph_no_pick_or_multiple_leave_the_trigger_bare() {
    for extra in [json!({"value": "none"}), json!({}), json!({"value": "review,backlog", "multiple": true})] {
        let props = trigger(extra.clone());
        assert_eq!(props.get("valueIcon"), None, "{extra}");
    }
}

#[test]
fn another_variant_never_gets_it() {
    for variant in ["field", "pill"] {
        let props = trigger(json!({"value": "review", "triggerVariant": variant}));
        assert_eq!(props.get("valueIcon"), None, "{variant}");
    }
}

/// A Select's `align` places its popup: it must not align the trigger's text.
#[test]
fn the_popup_align_leaves_the_trigger_text_alone() {
    let tree: NestedNode = serde_json::from_value(json!({"id": "root", "component": "Box", "children": [{"id": "sel", "component": "Select", "props": {"label": "Scale", "name": "s", "triggerVariant": "button", "align": "end", "value": "a", "options": [{"label": "A", "value": "a"}]}}]})).expect("tree");
    let mut s = Surface::new("t", SurfaceOptions::default());
    assert!(s.set_nested(tree).issues.is_empty());
    s.set_viewport(760.0, 400.0, None);
    s.layout(&mut eui_core::measure::FixedMeasure { sizes: std::collections::HashMap::new(), wrap: true });
    let t = s.index_of("sel.trigger").expect("the trigger");
    assert_eq!(s.visual(t).and_then(|v| v.text_align.as_deref()), Some("left"));
}
