//! #EXP-1765: the Select's `pill` trigger (a content-sized capsule, no
//! label part, the `Select.dropdown` glyph; its popup a hairline outside
//! it), `loading` (the Skeleton's rows in place of the options), option
//! `mono` (the mono type; `trailing` its own run) and `searchPlaceholder`.

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

fn pill(extra: Value) -> Value {
    let mut props = json!({"label": "Default branch", "name": "branch", "value": "main", "triggerVariant": "pill",
        "options": [{"label": "main", "value": "main", "mono": true, "trailing": "default"}, {"label": "develop", "value": "develop", "mono": true}, {"label": "plain", "value": "plain", "trailing": "· x"}]});
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
fn a_pill_is_content_sized_without_a_label_part_and_names_itself() {
    let mut s = surface(pill(json!({})));
    let out = s.layout(&mut fixed());
    let t = frame(&s, &out, "sel.trigger");
    let root = frame(&s, &out, "root");
    assert!(t.w < root.w - 32.0, "content-sized, not stretched: {t:?}");
    assert!(s.index_of("sel.label").is_none(), "no label part");
    let n = s.nodes().into_iter().find(|n| n.id == "sel.trigger").unwrap();
    assert_eq!(n.props.get("icon").and_then(Value::as_str), Some("ui-chevron-down"));
    assert_eq!(n.accessibility.as_ref().and_then(|a| a.get("label")).and_then(Value::as_str), Some("Default branch"));
}

#[test]
fn a_pill_popup_sits_a_hairline_outside_the_pill() {
    let mut s = surface(pill(json!({"searchable": true})));
    let out = open(&mut s);
    let t = frame(&s, &out, "sel.trigger");
    let c = frame(&s, &out, "sel.content");
    assert_eq!(c.w, eui_core::layout::SELECT_ROW_POPUP_WIDTH);
    assert!((c.x - (t.x - eui_core::layout::SELECT_PILL_POPUP_INSET)).abs() < 0.01, "{c:?} vs {t:?}");
    assert!((c.y - (t.y + t.h + eui_core::layout::SELECT_PILL_POPUP_OFFSET)).abs() < 0.01, "{c:?} vs {t:?}");
}

#[test]
fn search_placeholder_names_the_filter_field() {
    let mut s = surface(pill(json!({"searchable": true, "searchPlaceholder": "Search branches..."})));
    open(&mut s);
    let n = s.nodes().into_iter().find(|n| n.id == "sel.search").unwrap();
    assert_eq!(n.props.get("placeholder").and_then(Value::as_str), Some("Search branches..."));
}

#[test]
fn a_mono_option_keeps_its_trailing_apart_and_a_plain_one_appends_it() {
    let mut s = surface(pill(json!({})));
    open(&mut s);
    let nodes = s.nodes();
    let item = |i: usize| nodes.iter().find(|n| n.id == format!("sel.item.{i}")).unwrap().clone();
    assert_eq!(item(0).props.get("text").and_then(Value::as_str), Some("main"));
    assert_eq!(item(0).props.get("trailing").and_then(Value::as_str), Some("default"));
    assert_eq!(item(0).props.get("mono"), Some(&json!(true)));
    assert_eq!(item(2).props.get("text").and_then(Value::as_str), Some("plain · x"));
    assert!(item(2).props.get("trailing").is_none());
}

#[test]
fn loading_shows_the_skeleton_rows_in_place_of_the_options() {
    let mut s = surface(pill(json!({"searchable": true, "loading": true, "options": []})));
    let out = open(&mut s);
    assert!(!out.layers.is_empty(), "the popup opens while loading");
    let rows = s.nodes().into_iter().filter(|n| n.part.as_deref() == Some("row") && n.owner.as_deref() == Some("sel.skeleton")).count();
    assert_eq!(rows, eui_core::layout::SELECT_LOADING_ROWS);
    assert!(s.index_of("sel.empty").is_none(), "no empty text while loading");
    assert!(s.index_of("sel.search").is_some(), "the search still shows");
}
