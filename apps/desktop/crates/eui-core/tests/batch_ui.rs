//! EXP-1259 (the UI batch) in the layout core: the `SearchList` (filter,
//! highlighted row, keys, pick, bottom fade), a searchable Menu submenu,
//! the Tabs close glyph (`closable: hover` borrows its tab's hover) and the
//! + button, Skeleton rows, and the theme's `scale` tokens.

use std::collections::HashMap;

use eui_core::measure::FixedMeasure;
use eui_core::surface::{OutEvent, Surface, SurfaceOptions};
use eui_core::types::NestedNode;
use serde_json::{json, Value};

fn surface(children: Value) -> Surface {
    let tree: NestedNode = serde_json::from_value(json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "padding": 16}, "children": children})).expect("tree");
    let mut s = Surface::new("t", SurfaceOptions::default());
    let outcome = s.set_nested(tree);
    assert!(outcome.issues.is_empty(), "{:?}", outcome.issues);
    s.set_viewport(600.0, 800.0, None);
    s
}

fn fixed() -> FixedMeasure {
    FixedMeasure { sizes: HashMap::new(), wrap: true }
}

fn event(s: &mut Surface, id: &str, name: &str, payload: Option<Value>) -> Vec<OutEvent> {
    let i = s.index_of(id).unwrap_or_else(|| panic!("no node {id}"));
    s.event(i, name, payload)
}

fn action<'a>(events: &'a [OutEvent], name: &str) -> Option<&'a Value> {
    events.iter().find_map(|e| match e {
        OutEvent::Action { name: n, context, .. } if n == name => Some(context),
        _ => None,
    })
}

fn has_state(s: &Surface, id: &str, state: &str) -> bool {
    s.index_of(id).and_then(|i| s.layout_node(i)).and_then(|n| n.part_query.clone()).is_some_and(|q| q.states.iter().any(|x| x == state))
}

fn items(n: usize) -> Value {
    Value::Array((0..n).map(|i| json!({"label": format!("Issue {i}"), "value": format!("v{i}"), "identifier": format!("EXP-{i}")})).collect())
}

#[test]
fn a_search_list_filters_highlights_the_top_row_and_picks_with_the_keys() {
    let mut s = surface(json!([{"id": "sl", "component": "SearchList", "props": {"items": items(12), "value": {"path": "/picked"}}, "on": {"select": {"event": {"name": "pick"}}}}]));
    let mut m = fixed();
    s.layout(&mut m);
    for part in ["sl.head", "sl.search", "sl.body", "sl.list", "sl.item.0", "sl.label.0", "sl.identifier.0", "sl.fade"] {
        assert!(s.index_of(part).is_some(), "{part}");
    }
    assert!(s.index_of("sl.count").is_none(), "no count before a query");
    assert!(has_state(&s, "sl.item.0", "selected"), "the top row is highlighted");
    // Down twice, then up once: row 1.
    for key in ["ArrowDown", "ArrowDown", "ArrowUp"] {
        event(&mut s, "sl.search", "key", Some(json!({"key": key})));
        s.layout(&mut m);
    }
    assert!(has_state(&s, "sl.item.1", "selected") && !has_state(&s, "sl.item.0", "selected"));
    let events = event(&mut s, "sl.search", "key", Some(json!({"key": "Enter"})));
    assert_eq!(action(&events, "pick").unwrap()["value"], json!("v1"));
    assert!(events.iter().any(|e| matches!(e, OutEvent::DataChanged { path, value } if path == "/picked" && value == &json!("v1"))));
    s.layout(&mut m);
    assert!(s.index_of("sl.check.1").is_some(), "the picked row wears the check");
    // Typing filters (label or identifier) and puts the highlight back on top.
    event(&mut s, "sl.search", "change", Some(json!({"value": "exp-1"})));
    s.layout(&mut m);
    assert!(s.index_of("sl.item.0").is_none());
    assert!(s.index_of("sl.item.1").is_some() && s.index_of("sl.item.10").is_some() && s.index_of("sl.item.11").is_some());
    assert!(has_state(&s, "sl.item.1", "selected"));
    let count = s.nodes().into_iter().find(|n| n.id == "sl.count").expect("count");
    assert_eq!(count.props["text"], json!("3"));
    assert!(s.index_of("sl.fade").is_none(), "three rows fit: no fade");
    event(&mut s, "sl.search", "change", Some(json!({"value": "nothing"})));
    s.layout(&mut m);
    assert_eq!(s.nodes().into_iter().find(|n| n.id == "sl.empty").expect("empty").props["text"], json!("No results"));
}

#[test]
fn a_search_list_scrolls_the_highlight_into_view_and_drops_the_fade_at_the_end() {
    let mut s = surface(json!([{"id": "sl", "component": "SearchList", "props": {"items": items(10), "maxRows": 4}}]));
    let mut m = fixed();
    let out = s.layout(&mut m);
    let list = s.index_of("sl.list").unwrap();
    let frame = out.frames.iter().find(|f| f.index == list).copied().unwrap();
    let row = out.frames.iter().find(|f| f.index == s.index_of("sl.item.0").unwrap()).copied().unwrap();
    assert!((frame.h - 4.0 * row.h).abs() < 0.5, "four rows tall: {} vs {}", frame.h, row.h);
    for _ in 0..5 {
        event(&mut s, "sl.search", "key", Some(json!({"key": "ArrowDown"})));
        s.layout(&mut m);
    }
    let offset = s.scroll_offset("sl.list").1;
    assert!((offset - 2.0 * row.h).abs() < 0.5, "row 5 is the window's last: {offset}");
    assert!(s.index_of("sl.fade").is_some(), "rows still below");
    let fade = s.visual(s.index_of("sl.fade").unwrap()).cloned().unwrap();
    let g = fade.background_gradient.expect("the fade is a gradient of the list's ground");
    assert!(g.stops[0].color.ends_with("00") && g.stops[0].color.len() == 9 && fade.background_color.is_none());
    s.scroll("sl.list", 6.0 * row.h);
    s.layout(&mut m);
    assert!(s.index_of("sl.fade").is_none(), "at the end: no fade");
}

#[test]
fn a_host_that_searches_ranks_the_rows_itself() {
    let mut s = surface(json!([{"id": "sl", "component": "SearchList", "props": {"items": items(3)}, "on": {"search": {"event": {"name": "q"}}}}]));
    let mut m = fixed();
    s.layout(&mut m);
    let events = event(&mut s, "sl.search", "change", Some(json!({"value": "zzz"})));
    assert_eq!(action(&events, "q").unwrap()["query"], json!("zzz"));
    s.layout(&mut m);
    assert!(s.index_of("sl.item.2").is_some(), "no local filter under on.search");
}

#[test]
fn a_hover_moves_the_highlight() {
    let mut s = surface(json!([{"id": "sl", "component": "SearchList", "props": {"items": items(4)}}]));
    let mut m = fixed();
    s.layout(&mut m);
    s.set_states("sl.item.2", vec!["hover".into()]);
    s.layout(&mut m);
    assert!(has_state(&s, "sl.item.2", "selected") && !has_state(&s, "sl.item.0", "selected"));
}

#[test]
fn a_searchable_submenu_is_a_search_list_of_its_entries() {
    let mut s = surface(json!([{"id": "m", "component": "Menu", "props": {"label": "Add", "items": [
        {"label": "Implement issue", "kind": "submenu", "searchable": true, "placeholder": "Search issues", "items": [
            {"label": "Ship it", "value": "a"}, {"label": "Fix tabs", "value": "b"}, {"label": "Fix search", "value": "c"}
        ]}
    ]}, "on": {"select": {"event": {"name": "pick"}}}}]));
    let mut m = fixed();
    s.layout(&mut m);
    event(&mut s, "m.trigger", "press", None);
    s.layout(&mut m);
    event(&mut s, "m.item.0", "press", None);
    let out = s.layout(&mut m);
    assert_eq!(out.layers.len(), 2, "the menu and its submenu");
    let search = s.nodes().into_iter().find(|n| n.id == "m.search.0").expect("the submenu's field");
    assert_eq!((search.component.as_str(), search.props["placeholder"].clone()), ("SearchList", json!("Search issues")));
    event(&mut s, "m.search.0", "change", Some(json!({"value": "fix"})));
    s.layout(&mut m);
    assert!(s.index_of("m.item.0.0").is_none() && s.index_of("m.item.0.1").is_some());
    event(&mut s, "m.search.0", "key", Some(json!({"key": "ArrowDown"})));
    s.layout(&mut m);
    let events = event(&mut s, "m.search.0", "key", Some(json!({"key": "Enter"})));
    assert_eq!(action(&events, "pick").unwrap()["value"], json!("c"));
    assert!(s.layout(&mut m).layers.is_empty(), "a pick closes the menu");
    // ArrowLeft (the caret at the start) closes the submenu only.
    event(&mut s, "m.trigger", "press", None);
    s.layout(&mut m);
    event(&mut s, "m.item.0", "press", None);
    s.layout(&mut m);
    event(&mut s, "m.search.0", "key", Some(json!({"key": "ArrowLeft"})));
    assert_eq!(s.layout(&mut m).layers.len(), 1);
}

#[test]
fn tabs_close_on_hover_and_add() {
    let mut s = surface(json!([{"id": "t", "component": "Tabs", "props": {"tabs": [{"label": "One", "value": "1"}, {"label": "Two", "value": "2"}], "closable": "hover", "addable": true},
        "on": {"close": {"event": {"name": "close"}}, "new": {"event": {"name": "new"}}}, "children": [{"id": "p1", "component": "Text", "props": {"text": "1"}}, {"id": "p2", "component": "Text", "props": {"text": "2"}}]}]));
    let mut m = fixed();
    s.layout(&mut m);
    let close = s.nodes().into_iter().find(|n| n.id == "t.close.1").expect("a close glyph per tab");
    assert_eq!(close.props["label"], json!("Close Two"));
    assert!(s.index_of("t.new").is_some());
    let opacity = |s: &Surface| s.index_of("t.close.1").and_then(|i| s.visual(i)).and_then(|v| v.opacity);
    assert_eq!(opacity(&s), Some(0.0), "hidden until hovered");
    s.set_states("t.tab.1", vec!["hover".into()]);
    s.layout(&mut m);
    assert_eq!(opacity(&s), Some(1.0), "its tab's hover shows it");
    assert_eq!(action(&event(&mut s, "t.close.1", "press", None), "close").unwrap()["value"], json!("2"));
    assert!(action(&event(&mut s, "t.new", "press", None), "new").is_some());
}

#[test]
fn skeleton_rows_are_rows_of_a_dot_and_two_bars() {
    let mut s = surface(json!([{"id": "sk", "component": "Skeleton", "props": {"rows": 3, "rowHeight": 44}}, {"id": "nodot", "component": "Skeleton", "props": {"rows": 1, "dot": false}}]));
    let out = s.layout(&mut fixed());
    for id in ["sk.row.0", "sk.dot.0", "sk.bar.2.0", "sk.bar.2.1", "sk.row.2"] {
        assert!(s.index_of(id).is_some(), "{id}");
    }
    let row = out.frames.iter().find(|f| f.index == s.index_of("sk.row.1").unwrap()).unwrap();
    assert_eq!(row.h, 44.0);
    assert!(s.index_of("nodot.dot.0").is_none());
    let bar = s.nodes().into_iter().find(|n| n.id == "sk.bar.0.1").unwrap();
    assert_eq!(bar.component, "Skeleton", "the pieces pulse like a Skeleton");
}

#[test]
fn the_built_in_themes_carry_the_motion_scale() {
    for id in eui_core::themes::BUILTIN_THEME_IDS {
        let t = eui_core::themes::builtin_theme(id).unwrap();
        assert!(t.tokens.scale.contains_key("press") && t.tokens.scale.contains_key("popoverFrom"), "{id}");
    }
    let e = eui_core::themes::builtin_theme("exponential").unwrap();
    assert_eq!((e.tokens.scale["press"], e.tokens.scale["popoverFrom"]), (0.97, 0.96), "design-tokens motion.scale");
}

#[test]
fn a_pointer_over_a_submenu_row_opens_it() {
    let mut s = surface(json!([{"id": "m", "component": "Menu", "props": {"label": "Add", "items": [
        {"label": "Rename", "value": "rename"},
        {"label": "Implement issue", "kind": "submenu", "searchable": true, "items": [{"label": "One", "value": "1"}]}
    ]}}]));
    let mut m = fixed();
    s.layout(&mut m);
    event(&mut s, "m.trigger", "press", None);
    assert_eq!(s.layout(&mut m).layers.len(), 1);
    s.set_states("m.item.1", vec!["hover".into()]);
    assert_eq!(s.layout(&mut m).layers.len(), 2, "hover opens the submenu");
    s.set_states("m.item.1", vec![]);
    s.set_states("m.item.0", vec!["hover".into()]);
    assert_eq!(s.layout(&mut m).layers.len(), 1, "a sibling row closes it");
}

/// The D10 error toast: title, description and a Retry action that fires
/// `action` (and dismisses it), in the toast band.
#[test]
fn an_error_toast_with_an_action_fires_it() {
    let mut s = surface(json!([{"id": "t", "component": "Toast", "props": {"title": "Couldn't save the board", "description": "The server did not answer.", "type": "error", "actionLabel": "Retry", "duration": 0},
        "on": {"action": {"event": {"name": "retry"}}}}]));
    let mut m = fixed();
    let out = s.layout(&mut m);
    assert_eq!(out.toasts.len(), 1);
    let root = s.nodes().into_iter().find(|n| n.id == "t.root").expect("the toast");
    assert_eq!(root.live.as_deref(), Some("assertive"), "an error interrupts");
    let action = s.nodes().into_iter().find(|n| n.id == "t.action").expect("the action");
    assert_eq!(action.props["label"], json!("Retry"));
    let events = event(&mut s, "t.action", "press", None);
    assert!(action_named(&events, "retry"));
    assert!(s.layout(&mut m).toasts.is_empty(), "the action dismisses it");
}

fn action_named(events: &[OutEvent], name: &str) -> bool {
    action(events, name).is_some()
}
