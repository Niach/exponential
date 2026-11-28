//! #PERF-15: an in-place `updateComponents` and a data write rebuild only
//! the changed subtrees, and the result is EXACTLY what a fresh surface
//! with the same components and data lays out (nodes, frames, visuals).

use std::collections::HashMap;

use eui_core::measure::FixedMeasure;
use eui_core::surface::{PlacedNode, Surface, SurfaceOptions};
use eui_core::themes::builtin_theme;
use eui_core::FlatComponent;
use serde_json::{json, Value};

fn comps(v: Value) -> Vec<FlatComponent> {
    serde_json::from_value(v).unwrap()
}

fn bench_components() -> Vec<Value> {
    let fixture: Value = serde_json::from_str(include_str!("../../eui-gpui/examples/bench_delta.json")).unwrap();
    fixture["components"].as_array().unwrap().clone()
}

fn surface(components: &[Value], data: &Value) -> Surface {
    let mut s = Surface::new("s", SurfaceOptions::default());
    s.set_theme(builtin_theme("exponential"));
    s.set_components(comps(Value::Array(components.to_vec())));
    if let Value::Object(map) = data {
        for (k, v) in map {
            s.set_data(&format!("/{k}"), Some(v.clone())).unwrap();
        }
    }
    s.set_viewport(1280.0, 1240.0, None);
    s
}

/// Everything a painter reads, keyed by node id.
fn snapshot(s: &mut Surface) -> HashMap<String, (Value, Option<(i32, i32, i32, i32)>, String)> {
    let mut m = FixedMeasure { sizes: Default::default(), wrap: true };
    let out = s.layout(&mut m);
    let nodes: Vec<PlacedNode> = s.nodes();
    let by_index: HashMap<u32, &PlacedNode> = nodes.iter().map(|n| (n.index, n)).collect();
    let frames: HashMap<u32, (i32, i32, i32, i32)> = out.frames.iter().map(|f| (f.index, ((f.x * 100.0) as i32, (f.y * 100.0) as i32, (f.w * 100.0) as i32, (f.h * 100.0) as i32))).collect();
    nodes
        .iter()
        .filter(|n| !n.removed)
        .map(|n| {
            let mut v = serde_json::to_value(n).unwrap();
            let o = v.as_object_mut().unwrap();
            o.remove("index");
            o.insert("parent".into(), json!(n.parent.map(|p| by_index[&p].id.clone())));
            o.insert("children".into(), json!(n.children.iter().map(|c| by_index[c].id.clone()).collect::<Vec<_>>()));
            (n.id.clone(), (v, frames.get(&n.index).copied(), format!("{:?}", s.visual(n.index))))
        })
        .collect()
}

fn assert_same(a: &mut Surface, b: &mut Surface, what: &str) {
    let (x, y) = (snapshot(a), snapshot(b));
    assert_eq!(x.len(), y.len(), "{what}: node count");
    for (id, v) in &y {
        assert_eq!(x.get(id), Some(v), "{what}: node {id}");
    }
}

#[test]
fn in_place_updates_lay_out_like_a_fresh_surface() {
    let base = bench_components();
    let mut live = surface(&base, &json!({}));
    snapshot(&mut live);
    let mut current = base.clone();
    let updates = [
        json!({"id": "t17", "component": "Text", "text": "Item 17 now a much longer caption that wraps over lines", "variant": "caption"}),
        json!({"id": "b17", "component": "Button", "label": "Go", "variant": "default", "size": "sm"}),
        json!({"id": "r17", "component": "Row", "title": "Row seventeen", "meta": "x"}),
        json!({"id": "c17", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": 12, "width": 250, "padding": 4, "borderWidth": 2, "borderColor": "$color.primary", "borderRadius": 2}, "children": ["t17", "r17", "b17"]}),
        // Structure changes: these take the full path.
        json!({"id": "c18", "component": "Box", "style": {"display": "flex"}, "children": ["t18", "b18"]}),
        json!({"id": "t19", "component": "Text", "text": "hidden", "visible": false}),
        json!({"id": "heading", "component": "Text", "text": "Retitled", "variant": "title", "style": {"width": "100%"}}),
    ];
    for u in updates {
        let id = u["id"].as_str().unwrap().to_string();
        live.update_components(comps(json!([u.clone()])));
        if let Some(c) = current.iter_mut().find(|c| c["id"] == id.as_str()) {
            *c = u;
        }
        let mut fresh = surface(&current, &json!({}));
        assert_same(&mut live, &mut fresh, &format!("after updating {id}"));
    }
}

#[test]
fn in_place_update_rebuilds_only_its_subtree() {
    let base = bench_components();
    let mut live = surface(&base, &json!({}));
    let mut m = FixedMeasure { sizes: Default::default(), wrap: true };
    let first = live.layout(&mut m);
    live.update_components(comps(json!([{"id": "t17", "component": "Text", "text": "changed", "variant": "caption"}])));
    let out = live.layout(&mut m);
    assert!(out.built_nodes < 5 && first.built_nodes > 200, "built {} (first {})", out.built_nodes, first.built_nodes);
    assert_eq!(out.delta.changed.len(), 1, "one node changed: {:?}", out.delta);
}

#[test]
fn data_writes_rebuild_their_readers_like_a_fresh_surface() {
    let mut base = bench_components();
    for c in base.iter_mut() {
        match c["id"].as_str().unwrap() {
            "t17" => c["text"] = json!({"@path": "/t17"}),
            "b17" => c["visible"] = json!({"@path": "/show"}),
            "r17" => c["title"] = json!({"@path": "/row/title"}),
            _ => {}
        }
    }
    let mut data = json!({"t17": "bound", "show": true, "row": {"title": "A"}});
    let mut live = surface(&base, &data);
    let mut m = FixedMeasure { sizes: Default::default(), wrap: true };
    live.layout(&mut m);
    let writes = [("/t17", json!("bound and now a lot longer, wrapping")), ("/show", json!(false)), ("/row", json!({"title": "B"})), ("/row/title", json!("C")), ("/show", json!(true)), ("/unread", json!(1))];
    for (path, value) in writes {
        live.set_data(path, Some(value.clone())).unwrap();
        let built = live.layout(&mut m).built_nodes;
        assert!(built < 20, "{path}: built {built}");
        eui_core::data::set_pointer(&mut data, path, Some(value)).unwrap();
        let mut fresh = surface(&base, &data);
        assert_same(&mut live, &mut fresh, &format!("after writing {path}"));
    }
}

#[test]
fn a_template_list_row_write_matches_a_fresh_surface() {
    let base = vec![
        json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column"}, "children": ["title", "list"]}),
        json!({"id": "title", "component": "Text", "text": {"@path": "/title"}}),
        json!({"id": "list", "component": "List", "children": {"componentId": "row", "path": "/rows", "key": "id"}}),
        json!({"id": "row", "component": "Row", "title": {"@path": "title"}, "meta": {"@path": "meta"}}),
    ];
    let rows: Vec<Value> = (0..20).map(|i| json!({"id": format!("r{i}"), "title": format!("Row {i}"), "meta": i.to_string()})).collect();
    let mut data = json!({"title": "T", "rows": rows});
    let mut live = surface(&base, &data);
    let mut m = FixedMeasure { sizes: Default::default(), wrap: true };
    live.layout(&mut m);
    for (path, value) in [("/rows/3/title", json!("Row three")), ("/title", json!("New")), ("/rows/20", json!({"id": "r20", "title": "Added", "meta": "20"}))] {
        live.set_data(path, Some(value.clone())).unwrap();
        eui_core::data::set_pointer(&mut data, path, Some(value)).unwrap();
        let mut fresh = surface(&base, &data);
        assert_same(&mut live, &mut fresh, &format!("after writing {path}"));
    }
}
