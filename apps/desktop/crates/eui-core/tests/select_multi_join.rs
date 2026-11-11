//! #EXP-1629: a `multiple` Select draws a run of adjacent picked SHOWN
//! rows as ONE block (web's Picker, the IDE's `multi_row_edges`): a picked
//! row under a picked row is `joinTop`, over one `joinBottom`; the
//! `exponential` row recipe then drops that edge's stroke and corners and
//! gives that edge the IDE's `PICKER_ROW_PY + 1` padding. A search that
//! hides a row breaks the run; a single Select never joins.

use std::collections::HashMap;

use eui_core::measure::FixedMeasure;
use eui_core::surface::{LayoutOutput, PlacedFrame, Surface, SurfaceOptions};
use eui_core::theme::Mode;
use eui_core::themes::builtin_theme;
use eui_core::types::NestedNode;
use serde_json::{json, Value};

fn fixed() -> FixedMeasure {
    FixedMeasure { sizes: HashMap::new(), wrap: true }
}

fn opened(extra: Value) -> Surface {
    let options: Vec<Value> = ["a", "b", "c", "d", "e"].iter().map(|v| json!({"label": v.to_uppercase(), "value": v, "icon": "ui-docs", "description": format!("About {v}")})).collect();
    let mut props = json!({"label": "Skills", "name": "skills", "triggerVariant": "row", "multiple": true, "searchable": true, "value": "a,b,c,e", "options": options});
    for (k, v) in extra.as_object().unwrap() {
        props[k] = v.clone();
    }
    let tree: NestedNode = serde_json::from_value(json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%"}, "children": [{"id": "sel", "component": "Select", "props": props}]})).expect("tree");
    let mut s = Surface::new("t", SurfaceOptions { theme: Some(builtin_theme("exponential").expect("theme")), mode: Mode::Dark, ..SurfaceOptions::default() });
    let outcome = s.set_nested(tree);
    assert!(outcome.issues.is_empty(), "{:?}", outcome.issues);
    s.set_viewport(760.0, 900.0, None);
    s.layout(&mut fixed());
    let t = s.index_of("sel.trigger").unwrap();
    s.event(t, "press", None);
    s.layout(&mut fixed());
    s
}

/// Each shown item's `(top border, bottom border, top-left corner, bottom-left corner)`.
fn edges(s: &Surface) -> Vec<(String, [f32; 4])> {
    let mut out = Vec::new();
    for i in 0..5 {
        let id = format!("sel.item.{i}");
        let Some(index) = s.index_of(&id) else { continue };
        let v = s.visual(index).expect("a visual");
        let borders = v.border_widths.unwrap_or([v.border_width.unwrap_or(0.0); 4]);
        let corners = v.corner_radii.unwrap_or([v.border_radius.unwrap_or(0.0); 4]);
        out.push((id, [borders[0], borders[2], corners[0], corners[3]]));
    }
    out
}

fn frame(s: &Surface, out: &LayoutOutput, id: &str) -> PlacedFrame {
    let i = s.index_of(id).unwrap_or_else(|| panic!("no node {id}"));
    out.frames.iter().chain(out.layers.iter().flat_map(|l| l.frames.iter())).find(|f| f.index == i).copied().unwrap_or_else(|| panic!("{id} not drawn"))
}

#[test]
fn a_run_of_picked_rows_is_one_block() {
    let mut s = opened(json!({}));
    let e = edges(&s);
    let r = e[3].1[2];
    assert!(r > 0.0, "an unpicked row keeps its corners");
    assert_eq!(e[0].1, [1.0, 0.0, r, 0.0], "a: the run's top");
    assert_eq!(e[1].1, [0.0, 0.0, 0.0, 0.0], "b: inside the run");
    assert_eq!(e[2].1, [0.0, 1.0, 0.0, r], "c: the run's bottom");
    assert_eq!(e[3].1, [1.0, 1.0, r, r], "d: unpicked");
    assert_eq!(e[4].1, [1.0, 1.0, r, r], "e: picked alone");
    // A free edge is the stroke + 3.5 (the device picker's 4.5 inset), a
    // joined edge the IDE's `PICKER_ROW_PY + 1` = 5 with no stroke: the
    // label sits 4.5 under a free top, 5 under a joined one.
    let out = s.layout(&mut fixed());
    let want = [(9.5, 4.5), (10.0, 5.0), (9.5, 5.0), (9.0, 4.5), (9.0, 4.5)];
    for (i, (extra, inset)) in want.into_iter().enumerate() {
        let item = frame(&s, &out, &format!("sel.item.{i}"));
        let label = frame(&s, &out, &format!("sel.itemLabel.{i}"));
        let description = frame(&s, &out, &format!("sel.itemDescription.{i}"));
        assert!((item.h - (extra + label.h + description.h)).abs() < 0.01, "item {i}: {} tall", item.h);
        assert!((label.y - item.y - inset).abs() < 0.01, "item {i}: label {} down", label.y - item.y);
    }
}

#[test]
fn a_search_joins_over_the_shown_rows() {
    let options: Vec<Value> = [("a", "deploy"), ("b", "lint"), ("c", "debug")].iter().map(|(v, l)| json!({"label": l, "value": v})).collect();
    let mut s = opened(json!({"options": options, "value": "a,c"}));
    let e = edges(&s);
    assert_eq!(e.len(), 3);
    assert_eq!((e[0].1[1], e[2].1[0]), (1.0, 1.0), "deploy and debug are apart while lint shows");
    let search = s.index_of("sel.search").unwrap();
    s.event(search, "change", Some(json!({"value": "de"})));
    s.layout(&mut fixed());
    let e = edges(&s);
    assert_eq!(e.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(), ["sel.item.0", "sel.item.2"]);
    assert_eq!((e[0].1[1], e[1].1[0]), (0.0, 0.0), "lint hidden: the two picks are one block");
}

#[test]
fn a_single_select_never_joins() {
    let s = opened(json!({"multiple": false, "value": "b"}));
    for (id, e) in edges(&s) {
        assert!(e[2] > 0.0 && e[2] == e[3], "{id}: both corners kept");
    }
}
