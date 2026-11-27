//! #EXP-2204: the core's half of LogView/LogLines, JobGraph and the
//! CodeBlock anchors + blame: the leaves' default heights, the resolved
//! state painters read, the two-way writes and the events.

use std::collections::HashMap;

use eui_core::measure::FixedMeasure;
use eui_core::surface::{OutEvent, Surface, SurfaceOptions};
use eui_core::types::NestedNode;
use serde_json::{json, Value};

fn surface(tree: Value, width: f32, data: Value) -> Surface {
    let tree: NestedNode = serde_json::from_value(tree).expect("tree");
    let mut s = Surface::new("t", SurfaceOptions::default());
    s.set_theme(eui_core::themes::builtin_theme("neutral"));
    s.set_nested(tree);
    if !data.is_null() {
        s.set_data("", Some(data)).expect("data");
    }
    s.set_viewport(width, 0.0, None);
    s
}

fn layout(s: &mut Surface) -> eui_core::surface::LayoutOutput {
    let mut m = FixedMeasure { sizes: HashMap::new(), wrap: true };
    s.layout(&mut m)
}

fn frame(s: &mut Surface, id: &str) -> (f32, f32) {
    let out = layout(s);
    let i = s.index_of(id).unwrap_or_else(|| panic!("no {id}"));
    out.frames.iter().find(|f| f.index == i).map(|f| (f.w, f.h)).unwrap_or((0.0, 0.0))
}

fn node(s: &mut Surface, id: &str) -> eui_core::surface::PlacedNode {
    s.nodes().into_iter().find(|n| n.id == id).unwrap_or_else(|| panic!("no {id}: {:?}", s.nodes().iter().map(|n| n.id.clone()).collect::<Vec<_>>()))
}

fn actions(out: &[OutEvent]) -> Vec<(String, Value)> {
    out.iter().filter_map(|e| match e { OutEvent::Action { name, context, .. } => Some((name.clone(), context.clone())), _ => None }).collect()
}

fn writes(out: &[OutEvent]) -> Vec<(String, Value)> {
    out.iter().filter_map(|e| match e { OutEvent::DataChanged { path, value } => Some((path.clone(), value.clone())), _ => None }).collect()
}

const LINES: [&str; 8] = ["##[group]Install", "$ bun install", "Resolved 1204 packages", "##[endgroup]", "##[group]Test", "\u{1b}[31m(fail)\u{1b}[0m store > get", "##[error]1 test failed", "##[endgroup]"];

fn log_view(extra: Value) -> Value {
    let mut props = json!({"lines": LINES, "sections": [{"status": "passed", "duration": "12s"}, {"status": "failed", "duration": "41s"}]});
    for (k, v) in extra.as_object().unwrap() {
        props[k] = v.clone();
    }
    json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column"}, "children": [{"id": "log", "component": "LogView", "props": props, "on": {
        "toggle": {"event": {"name": "toggled"}}, "search": {"event": {"name": "searched"}}, "select": {"event": {"name": "selected"}},
        "follow": {"event": {"name": "followed"}}, "timestamps": {"event": {"name": "stamped"}}
    }}]})
}

fn lines_id(s: &mut Surface) -> String {
    s.nodes().into_iter().find(|n| n.component == "LogLines").expect("LogLines").id
}

#[test]
fn log_lines_is_one_leaf_toolbar_plus_max_lines_tall_with_the_state_resolved() {
    let mut s = surface(log_view(json!({"maxLines": 8})), 800.0, Value::Null);
    let id = lines_id(&mut s);
    let (w, h) = frame(&mut s, &id);
    assert_eq!(w, 800.0);
    assert_eq!(h, 36.0 + 8.0 + 8.0 * 22.75 + 2.0 * (8.0 + 1.0), "toolbar, gap, the body: maxLines lines + its padding and hairline");
    let n = node(&mut s, &id);
    assert_eq!(n.props["open"], json!([1]), "the failed step opens on its own");
    assert_eq!(n.props["follow"], json!(true));
    assert_eq!(n.props["timestamps"], json!(false));
    assert_eq!(n.props["query"], json!(""));
    assert_eq!(n.props["lineCount"], json!(4));
    // The default 24 lines; a node height wins.
    let mut s = surface(log_view(json!({})), 800.0, Value::Null);
    let id = lines_id(&mut s);
    assert_eq!(frame(&mut s, &id).1, 36.0 + 8.0 + 24.0 * 22.75 + 18.0);
}

#[test]
fn log_lines_is_compact_below_md_with_a_two_row_toolbar_and_the_same_body() {
    let body = 8.0 * 22.75 + 2.0 * (8.0 + 1.0);
    let mut s = surface(log_view(json!({"maxLines": 8})), 1024.0, Value::Null);
    let id = lines_id(&mut s);
    assert_eq!(frame(&mut s, &id).1, 36.0 + 8.0 + body, "wide: one toolbar row");
    assert_eq!(node(&mut s, &id).props["compact"], json!(false));
    let mut s = surface(log_view(json!({"maxLines": 8})), 390.0, Value::Null);
    let id = lines_id(&mut s);
    assert_eq!(frame(&mut s, &id).1, 36.0 + 8.0 + 36.0 + 8.0 + body, "compact: two rows $spacing.sm apart, the body keeps maxLines");
    assert_eq!(node(&mut s, &id).props["compact"], json!(true));
    // A resize across md rebuilds it.
    s.set_viewport(1024.0, 0.0, None);
    assert_eq!(frame(&mut s, &id).1, 36.0 + 8.0 + body);
    assert_eq!(node(&mut s, &id).props["compact"], json!(false));
}

#[test]
fn log_lines_writes_its_state_locally_and_fires() {
    let mut s = surface(log_view(json!({})), 800.0, Value::Null);
    let id = lines_id(&mut s);
    layout(&mut s);
    let i = s.index_of(&id).unwrap();
    let out = s.event(i, "toggle", Some(json!({"section": 0})));
    assert_eq!(actions(&out), vec![("toggled".to_string(), json!({"section": 0, "open": true}))]);
    layout(&mut s);
    assert_eq!(node(&mut s, &id).props["open"], json!([0, 1]));
    let out = s.event(i, "toggle", Some(json!({"section": 1})));
    assert_eq!(actions(&out)[0].1, json!({"section": 1, "open": false}));
    layout(&mut s);
    assert_eq!(node(&mut s, &id).props["open"], json!([0]));
    // reveal opens the step holding the line, firing nothing.
    assert!(actions(&s.event(i, "reveal", Some(json!({"line": 4})))).is_empty());
    layout(&mut s);
    assert_eq!(node(&mut s, &id).props["open"], json!([0, 1]));
    let out = s.event(i, "search", Some(json!({"query": "store"})));
    assert_eq!(actions(&out), vec![("searched".to_string(), json!({"query": "store"}))]);
    let out = s.event(i, "follow", Some(json!({"follow": false})));
    assert_eq!(actions(&out), vec![("followed".to_string(), json!({"follow": false}))]);
    let out = s.event(i, "timestamps", None);
    assert_eq!(actions(&out), vec![("stamped".to_string(), json!({"timestamps": true}))]);
    let out = s.event(i, "select", Some(json!({"line": 3})));
    assert_eq!(actions(&out), vec![("selected".to_string(), json!({"anchor": "L3", "start": 3, "end": 3}))]);
    let out = s.event(i, "select", Some(json!({"line": 1, "shift": true})));
    assert_eq!(actions(&out), vec![("selected".to_string(), json!({"anchor": "L1-L3", "start": 1, "end": 3}))]);
    layout(&mut s);
    let p = node(&mut s, &id).props;
    assert_eq!((p["query"].clone(), p["follow"].clone(), p["timestamps"].clone(), p["selection"].clone()), (json!("store"), json!(false), json!(true), json!("L1-L3")));
}

#[test]
fn log_lines_writes_bound_props_through() {
    let bound = json!({"open": {"@path": "/log/open"}, "query": {"@path": "/log/query"}, "selection": {"@path": "/log/sel"}, "follow": {"@path": "/log/follow"}, "timestamps": {"@path": "/log/ts"}});
    let mut s = surface(log_view(bound), 800.0, json!({"log": {"open": [], "query": "", "sel": "L2", "follow": true, "ts": false}}));
    let id = lines_id(&mut s);
    layout(&mut s);
    let i = s.index_of(&id).unwrap();
    assert_eq!(node(&mut s, &id).props["open"], json!([]), "a bound empty open set stays empty");
    assert_eq!(writes(&s.event(i, "toggle", Some(json!({"section": 1})))), vec![("/log/open".to_string(), json!([1]))]);
    assert_eq!(writes(&s.event(i, "search", Some(json!({"query": "x"})))), vec![("/log/query".to_string(), json!("x"))]);
    assert_eq!(writes(&s.event(i, "select", Some(json!({"line": 4, "shift": true})))), vec![("/log/sel".to_string(), json!("L2-L4"))]);
    assert_eq!(writes(&s.event(i, "follow", Some(json!({"follow": false})))), vec![("/log/follow".to_string(), json!(false))]);
    assert_eq!(writes(&s.event(i, "timestamps", Some(json!({"timestamps": true})))), vec![("/log/ts".to_string(), json!(true))]);
}

fn job_graph() -> Value {
    json!({"id": "root", "component": "Box", "children": [{"id": "jobs", "component": "JobGraph", "props": {"jobs": [
        {"id": "build", "name": "build", "stage": "build", "status": "passed", "duration": "1m 2s"},
        {"id": "test", "name": "test", "stage": "test", "status": "failed", "duration": "41s", "needs": ["build"]},
        {"id": "deploy", "name": "deploy", "stage": "deploy", "status": "skipped", "needs": ["test"]}
    ], "value": {"@path": "/sel"}}, "on": {"select": {"event": {"name": "picked"}}}}]})
}

#[test]
fn job_graph_is_the_layout_s_height_and_a_list_below_md() {
    let mut s = surface(job_graph(), 1024.0, json!({"sel": "test"}));
    assert_eq!(frame(&mut s, "jobs").1, 24.0 + 8.0 + 40.0, "graph: stages + one row");
    assert_eq!(node(&mut s, "jobs").props["compact"], json!(false));
    let mut s = surface(job_graph(), 390.0, json!({"sel": "test"}));
    assert_eq!(frame(&mut s, "jobs").1, 3.0 * 40.0, "list: three rows");
    assert_eq!(node(&mut s, "jobs").props["compact"], json!(true));
    // A resize across md rebuilds it.
    s.set_viewport(1024.0, 0.0, None);
    assert_eq!(frame(&mut s, "jobs").1, 72.0);
    let i = s.index_of("jobs").unwrap();
    let out = s.event(i, "select", Some(json!({"id": "deploy"})));
    assert_eq!(writes(&out), vec![("/sel".to_string(), json!("deploy"))]);
    assert_eq!(actions(&out), vec![("picked".to_string(), json!({"id": "deploy"}))]);
}

fn code_block(extra: Value) -> Value {
    let mut props = json!({"code": "a\nb\nc\nd\ne", "language": "ts"});
    for (k, v) in extra.as_object().unwrap() {
        props[k] = v.clone();
    }
    json!({"id": "root", "component": "Box", "children": [{"id": "code", "component": "CodeBlock", "props": props, "on": {"select": {"event": {"name": "lines"}}}}]})
}

fn themed(tree: Value, data: Value) -> Surface {
    let tree: NestedNode = serde_json::from_value(tree).expect("tree");
    let mut s = Surface::new("t", SurfaceOptions::default());
    s.set_theme(eui_core::themes::builtin_theme("neutral"));
    s.set_nested(tree);
    if !data.is_null() {
        s.set_data("", Some(data)).expect("data");
    }
    s.set_viewport(800.0, 0.0, None);
    s
}

#[test]
fn code_block_anchors_turn_the_numbers_on_and_select_lines() {
    let mut s = themed(code_block(json!({"anchors": true, "selection": "L2-L3"})), Value::Null);
    layout(&mut s);
    let num = node(&mut s, "code.lineNumber.0");
    assert!(num.pressable);
    assert_eq!(num.props["line"], json!(1));
    let selected: Vec<bool> = (0..5).map(|i| node(&mut s, &format!("code.line.{i}")).props.get("selected") == Some(&json!(true))).collect();
    assert_eq!(selected, vec![false, true, true, false, false]);
    let i = s.index_of("code.lineNumber.4").unwrap();
    let out = s.event(i, "press", Some(json!({"shift": true})));
    assert_eq!(actions(&out), vec![("lines".to_string(), json!({"anchor": "L2-L5", "start": 2, "end": 5}))]);
    layout(&mut s);
    assert_eq!(node(&mut s, "code.line.4").props.get("selected"), Some(&json!(true)));
    // Without anchors a number is inert.
    let mut s = themed(code_block(json!({"lineNumbers": true})), Value::Null);
    layout(&mut s);
    assert!(!node(&mut s, "code.lineNumber.0").pressable);
}

#[test]
fn code_block_blame_adds_the_author_column_before_the_numbers() {
    let blame = json!([{"start": 1, "author": "Ada Lovelace", "age": "3 weeks ago"}, {"start": 3, "author": "Grace Hopper", "age": "2 days ago"}]);
    let mut s = themed(code_block(json!({"gutter": "blame", "blame": blame, "lineNumbers": true})), Value::Null);
    layout(&mut s);
    let ids: Vec<String> = s.nodes().into_iter().filter(|n| n.id.starts_with("code.")).map(|n| n.id).collect();
    assert!(ids.contains(&"code.blame.0".to_string()), "{ids:?}");
    assert_eq!(node(&mut s, "code.blameAuthor.0").props["text"], json!("Ada Lovelace"));
    assert_eq!(node(&mut s, "code.blameAge.2").props["text"], json!("2 days ago"));
    assert!(!ids.contains(&"code.blameAuthor.1".to_string()));
    assert_eq!(frame(&mut s, "code.blame.1").0, 200.0);
    assert_eq!(node(&mut s, "code.line.2").props.get("blameBorder"), Some(&json!(true)));
    assert_eq!(node(&mut s, "code.line.0").props.get("blameBorder"), None);
    let i = s.index_of("code.line.2").unwrap();
    let v = s.visual(i).expect("visual").clone();
    assert_eq!(v.border_widths.map(|w| w[0] > 0.0), Some(true), "{v:?}");
    assert!(v.border_color.is_some(), "{v:?}");
}
