//! #EXP-1629: a Select option's `iconColor` (a status picker's tinted
//! glyphs): the option row's `itemIcon` and, with `triggerIcon`, the row's
//! `valueIcon` take it as their own colour, over any recipe; a literal is a
//! `$color.<name>` token, the host's resolved colour arrives bound.

use std::collections::HashMap;

use eui_core::measure::FixedMeasure;
use eui_core::surface::{Surface, SurfaceOptions};
use eui_core::types::NestedNode;
use serde_json::{json, Value};

fn tree(options: Value) -> NestedNode {
    serde_json::from_value(json!({"id": "root", "component": "Box", "children": [
        {"id": "sel", "component": "Select", "props": {"label": "When a PR opens", "name": "pr", "triggerVariant": "row", "triggerIcon": true, "value": "review", "options": options}},
    ]}))
    .expect("tree")
}

fn open(s: &mut Surface) {
    let mut m = FixedMeasure { sizes: HashMap::new(), wrap: true };
    s.set_viewport(760.0, 900.0, None);
    s.layout(&mut m);
    let t = s.index_of("sel.trigger").expect("the trigger");
    s.event(t, "press", None);
    s.layout(&mut m);
}

fn color(s: &Surface, id: &str) -> Option<String> {
    let i = s.index_of(id).unwrap_or_else(|| panic!("no {id}"));
    s.visual(i).and_then(|v| v.color.clone())
}

#[test]
fn the_option_glyphs_and_the_value_glyph_take_their_tint() {
    let mut s = Surface::new("t", SurfaceOptions::default());
    let issues = s.set_nested(tree(json!([
        {"label": "In Progress", "value": "progress", "icon": "status-in-progress", "iconColor": "$color.warning"},
        {"label": "In Review", "value": "review", "icon": "status-in-review", "iconColor": "$color.success"},
        {"label": "Do nothing", "value": "none", "icon": "ui-minus"},
    ])))
    .issues;
    assert!(issues.is_empty(), "{issues:?}");
    open(&mut s);
    let theme = s.effective_theme().expect("a theme").clone();
    let dark = |name: &str| theme.modes.get(s.mode()).color.get(name).cloned();
    assert_eq!(color(&s, "sel.itemIcon.0"), dark("warning"));
    assert_eq!(color(&s, "sel.itemIcon.1"), dark("success"));
    assert_ne!(color(&s, "sel.itemIcon.2"), dark("warning"), "no iconColor: the row's ink");
    assert_eq!(color(&s, "sel.valueIcon"), dark("success"), "the chosen option's tint");
}

#[test]
fn a_literal_hex_is_refused() {
    let mut s = Surface::new("t", SurfaceOptions::default());
    let issues = s.set_nested(tree(json!([{"label": "Blocked", "value": "review", "icon": "status-in-progress", "iconColor": "#ef4444"}]))).issues;
    assert!(issues.iter().any(|i| i.message.ends_with("expected $color.<name>")), "{issues:?}");
}

#[test]
fn a_bound_host_colour_paints_as_is() {
    let mut s = Surface::new("t", SurfaceOptions::default());
    s.set_data("/blocked", Some(json!("#ef4444ff"))).expect("data");
    let issues = s.set_nested(tree(json!([{"label": "Blocked", "value": "review", "icon": "status-in-progress", "iconColor": {"@path": "/blocked"}}]))).issues;
    assert!(issues.is_empty(), "{issues:?}");
    open(&mut s);
    assert_eq!(color(&s, "sel.itemIcon.0").as_deref(), Some("#ef4444ff"));
    assert_eq!(color(&s, "sel.valueIcon").as_deref(), Some("#ef4444ff"));
}
