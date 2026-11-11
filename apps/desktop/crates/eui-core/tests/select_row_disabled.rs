//! #EXP-1629: a disabled `row` Select (the IDE's Effort row locked by
//! ultracode) dims its `value` and `chevron` to 0.35 in the exponential
//! theme (the row's 0.7 value tone halved, web's disabled select); the
//! label and description stay as they are.

use std::collections::HashMap;

use eui_core::measure::FixedMeasure;
use eui_core::surface::{Surface, SurfaceOptions};
use eui_core::theme::Mode;
use eui_core::types::NestedNode;
use serde_json::json;

fn row(disabled: bool) -> Surface {
    let tree: NestedNode = serde_json::from_value(json!({"id": "root", "component": "Box", "children": [
        {"id": "sel", "component": "Select", "props": {"label": "Effort", "name": "effort", "triggerVariant": "row", "description": "ultracode sets effort", "disabled": disabled, "value": "max",
            "options": [{"label": "High", "value": "high"}, {"label": "Max", "value": "max"}]}},
    ]}))
    .expect("tree");
    let options = SurfaceOptions { theme: eui_core::themes::builtin_theme("exponential"), mode: Mode::Dark, ..SurfaceOptions::default() };
    let mut s = Surface::new("t", options);
    let issues = s.set_nested(tree).issues;
    assert!(issues.is_empty(), "{issues:?}");
    s.set_viewport(760.0, 400.0, None);
    s.layout(&mut FixedMeasure { sizes: HashMap::new(), wrap: true });
    s
}

fn opacity(s: &Surface, id: &str) -> f32 {
    let i = s.index_of(id).unwrap_or_else(|| panic!("no {id}"));
    s.visual(i).and_then(|v| v.opacity).unwrap_or(1.0)
}

#[test]
fn a_disabled_row_dims_its_value_and_chevron_only() {
    let live = row(false);
    let locked = row(true);
    assert!((opacity(&live, "sel.value") - 0.7).abs() < 1e-4, "live value tone: {}", opacity(&live, "sel.value"));
    assert!((opacity(&locked, "sel.value") - 0.35).abs() < 1e-4, "{}", opacity(&locked, "sel.value"));
    assert!((opacity(&locked, "sel.chevron") - 0.35).abs() < 1e-4, "{}", opacity(&locked, "sel.chevron"));
    assert_eq!(opacity(&locked, "sel.label"), opacity(&live, "sel.label"), "the label stays");
    assert_eq!(opacity(&locked, "sel.description"), opacity(&live, "sel.description"), "the hint stays");
}
