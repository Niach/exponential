//! #EXP-2161: a Select option's `section`: the options without one first,
//! unheaded, then each section's run under its `sectionHeading` part (web's
//! picker group heading: `text-xs font-medium text-muted-foreground`,
//! `px-(--menu-item-padding-x) py-1.5`), never pressable; a heading after a
//! row sits `layout::SELECT_SECTION_GAP` under it (web's group padding).
//! fixtures/interactions-round1.json `selectSections` pins the order.

use std::collections::HashMap;

use eui_core::measure::FixedMeasure;
use eui_core::surface::{LayoutOutput, PlacedFrame, Surface, SurfaceOptions};
use eui_core::theme::Mode;
use eui_core::themes::builtin_theme;
use eui_core::types::NestedNode;
use serde_json::json;

fn fixed() -> FixedMeasure {
    FixedMeasure { sizes: HashMap::new(), wrap: true }
}

fn open(theme: &str) -> (Surface, LayoutOutput) {
    let tree: NestedNode = serde_json::from_value(json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%"}, "children": [
        {"id": "sel", "component": "Select", "props": {"label": "Device", "name": "device", "triggerVariant": "row", "popupMinWidth": 170, "value": "mint", "options": [
            {"label": "build", "value": "acme:build", "icon": "server", "description": "Offline", "disabled": true, "section": "Acme"},
            {"label": "mint", "value": "mint", "icon": "monitor", "hint": "this one"},
            {"label": "unraid", "value": "acme:unraid", "icon": "server", "section": "Acme"},
            {"label": "build", "value": "side:build", "icon": "server", "section": "Side project"},
        ]}},
    ]}))
    .expect("tree");
    let mut s = Surface::new("t", SurfaceOptions { theme: Some(builtin_theme(theme).expect("theme")), mode: Mode::Dark, ..SurfaceOptions::default() });
    let issues = s.set_nested(tree).issues;
    assert!(issues.is_empty(), "{issues:?}");
    s.set_viewport(760.0, 900.0, None);
    s.layout(&mut fixed());
    let t = s.index_of("sel.trigger").expect("the trigger");
    s.event(t, "press", None);
    let out = s.layout(&mut fixed());
    (s, out)
}

fn frame(s: &Surface, out: &LayoutOutput, id: &str) -> PlacedFrame {
    let i = s.index_of(id).unwrap_or_else(|| panic!("no node {id}"));
    out.frames.iter().chain(out.layers.iter().flat_map(|l| l.frames.iter())).find(|f| f.index == i).copied().unwrap_or_else(|| panic!("{id} not drawn"))
}

#[test]
fn the_list_reads_plain_rows_then_each_section_under_its_heading() {
    let (mut s, _) = open("exponential");
    let list = s.index_of("sel.list").expect("the list");
    let ids: Vec<String> = s.nodes().into_iter().filter(|n| n.parent == Some(list)).map(|n| n.id).collect();
    assert_eq!(ids, ["sel.item.1", "sel.sectionHeading.0", "sel.item.0", "sel.item.2", "sel.sectionHeading.1", "sel.item.3"]);
    let heading = s.nodes().into_iter().find(|n| n.id == "sel.sectionHeading.1").expect("a heading");
    assert_eq!(heading.props.get("text"), Some(&json!("Side project")));
    assert!(!heading.pressable, "a heading never presses");
}

#[test]
fn a_heading_is_web_pickers_group_heading() {
    for theme in ["exponential", "neutral"] {
        let (s, out) = open(theme);
        let heading = s.index_of("sel.sectionHeading.0").expect("a heading");
        let v = s.visual(heading).expect("its visual");
        assert_eq!((v.font_size, v.font_weight, v.line_height), (Some(12.0), Some(500), Some(16.0)), "{theme}: text-xs font-medium");
        // The muted tone: the hint's (`text-muted-foreground`).
        let hint = s.visual(s.index_of("sel.itemHint.1").expect("the hint")).expect("its visual");
        assert_eq!(v.color, hint.color, "{theme}: muted");
        // py-1.5 around the line, the label inset like a row's (px 8): the
        // measurer adds it (the fixed measure's Text is its 20 line alone).
        assert_eq!(v.padding, Some([6.0, 8.0, 6.0, 8.0]), "{theme}: px-2 py-1.5");
        let f = frame(&s, &out, "sel.sectionHeading.0");
        let row = frame(&s, &out, "sel.item.1");
        assert_eq!(f.y - (row.y + row.h), 8.0, "{theme}: the group gap under a row");
        assert_eq!(f.x, row.x, "{theme}: as wide as the rows");
    }
}
