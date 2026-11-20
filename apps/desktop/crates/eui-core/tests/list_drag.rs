//! EXP-1832: the core owns a reorderable List's drag (`Surface::list_drag`).
//! Every `fixtures/list-reorder.json` dropSlot case is replayed THROUGH a
//! surface: a template List of 30px rows carrying `depth` / `group`, a drag
//! from the dragged row's centre to `at`: the painted drop is the expected
//! one and the release fires `move` with the expected context (nothing when
//! there is no slot). Plus: cancel fires nothing, the paint geometry.

use std::collections::HashMap;

use eui_core::list_reorder::{ListDropPosition, LIST_DROP_LINE, LIST_DROP_RING, LIST_LIFT_OFFSET};
use eui_core::measure::FixedMeasure;
use eui_core::surface::{ListDragInput, OutEvent, Surface, SurfaceOptions};
use eui_core::types::NestedNode;
use serde_json::{json, Value};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/fixtures");

fn surface(items: Value, max_depth: f64) -> Surface {
    let tree: NestedNode = serde_json::from_value(json!({"id": "root", "component": "Box", "children": [
        {"id": "l", "component": "List", "props": {"reorderable": true, "maxDepth": max_depth}, "template": {"component": "row", "path": "/items", "key": "id"}, "on": {"move": {"event": {"name": "move"}}}},
        {"id": "row", "component": "Box", "style": {"height": 30, "flexShrink": 0}, "children": [{"id": "label", "component": "Text", "props": {"text": {"@path": "id"}}}]}
    ]}))
    .unwrap();
    let mut s = Surface::new("t", SurfaceOptions::default());
    s.set_nested(tree);
    s.set_data("", Some(json!({"items": items}))).unwrap();
    s.set_viewport(300.0, 0.0, None);
    s.layout(&mut fixed());
    s.layout(&mut fixed());
    s
}

fn fixed() -> FixedMeasure {
    FixedMeasure { sizes: HashMap::new(), wrap: true }
}

fn moves(events: &[OutEvent]) -> Vec<Value> {
    events
        .iter()
        .filter_map(|e| match e {
            OutEvent::Action { name, payload, .. } if name == "move" => Some(payload.clone().unwrap_or(Value::Null)),
            _ => None,
        })
        .collect()
}

#[test]
fn every_fixture_case_drags_through_the_surface() {
    let f: Value = serde_json::from_str(&std::fs::read_to_string(format!("{FIXTURES}/list-reorder.json")).unwrap()).unwrap();
    for c in f["dropSlot"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let rows = c["rows"].as_array().unwrap();
        let items: Vec<Value> = rows.iter().map(|r| json!({"id": r["key"], "depth": r.get("depth").cloned().unwrap_or(json!(0)), "group": r.get("group").cloned().unwrap_or(json!(false))})).collect();
        let mut s = surface(json!(items), c["maxDepth"].as_f64().unwrap());
        let drag = c["drag"].as_str().unwrap();
        let Some(i) = s.index_of(&format!("label.{drag}")) else {
            assert!(c["expected"].is_null(), "{name}: no such row, no drop");
            continue;
        };
        let at = c["at"].as_f64().unwrap() as f32;
        let row = rows.iter().find(|r| r["key"] == drag).unwrap();
        let start_y = row["top"].as_f64().unwrap() as f32 + 15.0;
        assert!(s.list_drag(i, &ListDragInput::Start { x: 10.0, y: start_y }).is_empty());
        assert!(s.list_dragging());
        assert!(s.list_drag(i, &ListDragInput::Move { x: 40.0, y: at }).is_empty());
        let paint = s.list_drag_paint();
        assert_eq!(paint.len(), 1, "{name}");
        let p = &paint[0];
        assert_eq!(p.key, drag);
        assert_eq!(p.row, s.index_of(&format!("row.{drag}")).unwrap(), "{name}: the dragged row");
        assert_eq!(p.drop.as_ref().map(|d| serde_json::to_value(d).unwrap()).unwrap_or(Value::Null), c["expected"], "{name}: drop");
        let events = s.list_drag(i, &ListDragInput::End { x: 40.0, y: at });
        assert!(!s.list_dragging());
        let fired = moves(&events);
        if c["move"].is_null() {
            assert!(fired.is_empty(), "{name}: nothing fires");
        } else {
            assert_eq!(fired, vec![c["move"].clone()], "{name}: move");
        }
    }
}

#[test]
fn cancel_fires_nothing_and_the_paint_follows_the_rules() {
    let items = json!([{"id": "a"}, {"id": "f", "group": true}, {"id": "f1", "depth": 1}, {"id": "b"}]);
    let mut s = surface(items, 2.0);
    let a = s.index_of("label.a").unwrap();
    s.list_drag(a, &ListDragInput::Start { x: 5.0, y: 10.0 });
    // Into f (middle half of 30..60).
    s.list_drag(a, &ListDragInput::Move { x: 5.0, y: 45.0 });
    let p = s.list_drag_paint().remove(0);
    assert_eq!(p.drop.as_ref().unwrap().position, ListDropPosition::Into);
    let f = s.last_frame(s.index_of("row.f").unwrap()).unwrap();
    assert_eq!(p.into, Some(f));
    assert_eq!((p.line, p.ring), (None, None));
    // The lifted copy rides right of the row, following the pointer.
    assert_eq!((p.lift.x, p.lift.y, p.lift.h), (f.x + LIST_LIFT_OFFSET, 45.0 - 10.0, 30.0));
    // Before f1 (depth 1): the line starts at its indent, ring centred on its start.
    s.list_drag(a, &ListDragInput::Move { x: 5.0, y: 61.0 });
    let p = s.list_drag_paint().remove(0);
    assert_eq!(p.drop.as_ref().map(|d| (d.key.as_str(), d.position)), Some(("f1", ListDropPosition::Before)));
    let line = p.line.unwrap();
    assert_eq!((line.x, line.y, line.h), (14.0, 60.0 - LIST_DROP_LINE / 2.0, LIST_DROP_LINE));
    let ring = p.ring.unwrap();
    assert_eq!((ring.x, ring.y, ring.w), (14.0 - LIST_DROP_RING / 2.0, 60.0 - LIST_DROP_RING / 2.0, LIST_DROP_RING));
    assert!(s.list_drag(a, &ListDragInput::Cancel).is_empty());
    assert!(s.list_drag_paint().is_empty());
    // An end after the cancel fires nothing.
    assert!(moves(&s.list_drag(a, &ListDragInput::End { x: 5.0, y: 61.0 })).is_empty());
}

#[test]
fn a_plain_list_never_drags() {
    let tree: NestedNode = serde_json::from_value(json!({"id": "l", "component": "List", "children": [{"id": "x", "component": "Box", "style": {"height": 30}}]})).unwrap();
    let mut s = Surface::new("t", SurfaceOptions::default());
    s.set_nested(tree);
    s.set_viewport(300.0, 0.0, None);
    s.layout(&mut fixed());
    let x = s.index_of("x").unwrap();
    assert_eq!(s.list_reorder_row(x), None);
    s.list_drag(x, &ListDragInput::Start { x: 1.0, y: 1.0 });
    assert!(!s.list_dragging());
}

#[test]
fn a_row_from_outside_drops_between_the_named_rows() {
    // #EXP-2027: an outside row (the desktop's Chats row) over the List:
    // only the named rows take the drop, a List row with its key steps
    // aside, the paint is the drop alone and the release fires the List's
    // own `move` with the outside key.
    let items = json!([{"id": "t", "group": true}, {"id": "f", "depth": 1, "group": true}, {"id": "s", "depth": 1}, {"id": "o", "group": true}, {"id": "o1", "depth": 1}]);
    let mut s = surface(items, 3.0);
    let row = eui_core::surface::Frame { x: 400.0, y: 500.0, w: 200.0, h: 30.0 };
    let keys = Some(vec!["t".to_string(), "f".to_string(), "s".to_string()]);
    assert!(s.list_drag_external("l", "s", row, keys.clone(), &ListDragInput::Start { x: 410.0, y: 510.0 }).is_empty());
    assert!(s.list_dragging());
    // Into the folder f (30..60, middle).
    s.list_drag(0, &ListDragInput::Move { x: 10.0, y: 45.0 });
    let p = s.list_drag_paint().remove(0);
    assert!(p.external);
    assert_eq!(p.drop.as_ref().map(|d| (d.key.as_str(), d.position)), Some(("f", ListDropPosition::Into)));
    assert_eq!(p.into, s.last_frame(s.index_of("row.f").unwrap()));
    // Over the List's own row of the same key (60..90): it stepped aside
    // (the pointer is past the named rows: no slot).
    s.list_drag(0, &ListDragInput::Move { x: 10.0, y: 75.0 });
    assert_eq!(s.list_drag_paint().remove(0).drop, None);
    // Another team's rows (o, o1) never take it.
    s.list_drag(0, &ListDragInput::Move { x: 10.0, y: 135.0 });
    assert_eq!(s.list_drag_paint().remove(0).drop, None);
    // After t's edge, before f.
    s.list_drag(0, &ListDragInput::Move { x: 10.0, y: 31.0 });
    assert_eq!(s.list_drag_paint().remove(0).drop.map(|d| (d.key, d.position)), Some(("f".to_string(), ListDropPosition::Before)));
    // The release fires the List's move with the outside key.
    s.list_drag(0, &ListDragInput::Move { x: 10.0, y: 45.0 });
    let fired = moves(&s.list_drag(0, &ListDragInput::End { x: 10.0, y: 45.0 }));
    assert_eq!(fired, vec![json!({"id": "s", "into": "f"})]);
    assert!(!s.list_dragging());
    // Over the outside row itself: no slot, nothing fires.
    s.list_drag_external("l", "s", row, keys, &ListDragInput::Start { x: 410.0, y: 510.0 });
    s.list_drag(0, &ListDragInput::Move { x: 410.0, y: 515.0 });
    assert!(s.list_drag_paint().remove(0).drop.is_none());
    assert!(moves(&s.list_drag(0, &ListDragInput::End { x: 410.0, y: 515.0 })).is_empty());
    // Not a reorderable List: nothing starts.
    assert!(s.list_drag_external("row.t", "s", row, None, &ListDragInput::Start { x: 0.0, y: 0.0 }).is_empty());
    assert!(!s.list_dragging());
}
