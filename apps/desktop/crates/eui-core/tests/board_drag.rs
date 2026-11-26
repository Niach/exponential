//! genUI round 4 (fixtures/board-drop.json): a Board's cards drag between
//! its columns THROUGH the core (`Surface::list_drag`, which the natives and
//! gpui forward to): the Board macro's column Lists share `board`, the
//! TARGET column's List fires `move` and the macro adds `column`; the paint
//! names the List under the pointer and its lifted copy follows x and y.

use std::collections::HashMap;

use eui_core::list_reorder::{ListDropPosition, LIST_LIFT_OFFSET};
use eui_core::measure::FixedMeasure;
use eui_core::surface::{Frame, ListDragInput, OutEvent, Surface, SurfaceOptions};
use eui_core::types::NestedNode;
use serde_json::{json, Value};

fn fixed() -> FixedMeasure {
    FixedMeasure { sizes: HashMap::new(), wrap: true }
}

fn board() -> Surface {
    let tree: NestedNode = serde_json::from_value(json!({"id": "root", "component": "Box", "children": [
        {"id": "b", "component": "Board", "props": {"columns": {"@path": "/cols"}}, "on": {"move": {"event": {"name": "move"}}}}
    ]}))
    .unwrap();
    let card = |v: &str| json!({"label": format!("Card {v}"), "value": v});
    let cols = json!([
        {"value": "todo", "title": "Todo", "cards": [card("a"), card("b"), card("c")]},
        {"value": "doing", "title": "Doing", "cards": [card("d"), card("e")]},
        {"value": "done", "title": "Done", "cards": []}
    ]);
    let mut s = Surface::new("t", SurfaceOptions::default());
    s.set_nested(tree);
    s.set_data("", Some(json!({"cols": cols}))).unwrap();
    s.set_viewport(1000.0, 700.0, None);
    s.layout(&mut fixed());
    s.layout(&mut fixed());
    s
}

/// The card `key`'s row (the Row root a List row is) and its frame.
fn card(s: &Surface, column: &str, key: &str) -> (u32, Frame) {
    let i = s.index_of(&format!("b.column.cards.card.{column}.{key}")).unwrap_or_else(|| panic!("no card {column}/{key}"));
    (i, s.last_frame(i).unwrap())
}

fn list(s: &Surface, column: &str) -> (u32, Frame) {
    let i = s.index_of(&format!("b.column.cards.{column}")).unwrap_or_else(|| panic!("no list {column}"));
    (i, s.last_frame(i).unwrap())
}

fn moves(events: &[OutEvent]) -> Vec<(Value, Value)> {
    events
        .iter()
        .filter_map(|e| match e {
            OutEvent::Action { name, context, payload, .. } if name == "move" => Some((context.clone(), payload.clone().unwrap_or(Value::Null))),
            _ => None,
        })
        .collect()
}

fn drag(s: &mut Surface, from: (u32, Frame), to: (f32, f32)) -> Vec<OutEvent> {
    let (i, f) = from;
    let start = (f.x + 20.0, f.y + f.h / 2.0);
    assert!(s.list_drag(i, &ListDragInput::Start { x: start.0, y: start.1 }).is_empty());
    assert!(s.list_dragging());
    assert!(s.list_drag(i, &ListDragInput::Move { x: to.0, y: to.1 }).is_empty());
    s.list_drag(i, &ListDragInput::End { x: to.0, y: to.1 })
}

#[test]
fn the_columns_sit_side_by_side_at_least_280_wide() {
    let s = board();
    let (_, todo) = list(&s, "todo");
    let (_, doing) = list(&s, "doing");
    let (_, done) = list(&s, "done");
    assert!(todo.w >= 260.0 && doing.x > todo.x + todo.w && done.x > doing.x + doing.w, "{todo:?} {doing:?} {done:?}");
    assert_eq!(todo.y, doing.y);
}

#[test]
fn a_card_drops_into_another_column_before_a_card() {
    let mut s = board();
    let a = card(&s, "todo", "a");
    let (_, d) = card(&s, "doing", "d");
    let events = drag(&mut s, a, (d.x + 40.0, d.y + d.h * 0.25));
    assert_eq!(moves(&events), vec![(json!({"column": "doing", "id": "a", "before": "d"}), json!({"id": "a", "before": "d"}))]);
}

#[test]
fn a_card_drops_into_an_empty_column() {
    let mut s = board();
    let d = card(&s, "doing", "d");
    let (_, done) = list(&s, "done");
    let events = drag(&mut s, d, (done.x + 40.0, done.y + 10.0));
    assert_eq!(moves(&events), vec![(json!({"column": "done", "id": "d"}), json!({"id": "d"}))]);
}

#[test]
fn a_card_reorders_in_its_own_column() {
    let mut s = board();
    let a = card(&s, "todo", "a");
    let (_, c) = card(&s, "todo", "c");
    let events = drag(&mut s, a, (c.x + 40.0, c.y + c.h * 0.75));
    assert_eq!(moves(&events), vec![(json!({"column": "todo", "id": "a", "after": "c"}), json!({"id": "a", "after": "c"}))]);
}

#[test]
fn over_the_dragged_card_nothing_fires() {
    let mut s = board();
    let b = card(&s, "todo", "b");
    let f = b.1;
    assert!(moves(&drag(&mut s, b, (f.x + 60.0, f.y + 5.0))).is_empty());
}

#[test]
fn the_paint_names_the_list_under_the_pointer_and_follows_both_axes() {
    let mut s = board();
    let (ai, a) = card(&s, "todo", "a");
    let (_, d) = card(&s, "doing", "d");
    let (doing_list, _) = list(&s, "doing");
    s.list_drag(ai, &ListDragInput::Start { x: a.x + 20.0, y: a.y + 10.0 });
    let to = (d.x + 50.0, d.y + d.h * 0.75);
    s.list_drag(ai, &ListDragInput::Move { x: to.0, y: to.1 });
    let paint = s.list_drag_paint();
    assert_eq!(paint.len(), 1);
    let p = &paint[0];
    assert_eq!(p.list_index, doing_list);
    assert_eq!(p.row, ai, "the dragged card dims in its own column");
    assert_eq!(p.drop.as_ref().map(|d| (d.key.as_str(), d.position)), Some(("d", ListDropPosition::After)));
    assert!(p.line.is_some() && p.ring.is_some() && p.into.is_none());
    assert_eq!(p.lift.x, to.0 - 20.0 + LIST_LIFT_OFFSET);
    assert_eq!(p.lift.y, to.1 - 10.0);
    // Over the empty column: the into highlight over its List.
    let (done_list, done) = list(&s, "done");
    s.list_drag(ai, &ListDragInput::Move { x: done.x + 30.0, y: done.y + 5.0 });
    let p = &s.list_drag_paint()[0];
    assert_eq!(p.list_index, done_list);
    assert_eq!(p.into, Some(done));
    assert!(p.drop.is_none() && p.line.is_none());
    assert!(s.list_drag(ai, &ListDragInput::Cancel).is_empty());
    assert!(!s.list_dragging());
}
