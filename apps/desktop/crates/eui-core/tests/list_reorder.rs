//! EXP-1832: `fixtures/list-reorder.json` replayed against the core's twin
//! (`list_reorder.rs`; the TS one is packages/exponential-ui/src/list-reorder.test.ts):
//! every case, the whole expected drop and `move` context.

use eui_core::list_reorder::{board_drop_slot, board_move_context, list_drop_slot, list_move_context, BoardColumn, ListReorderRow};
use serde_json::{json, Value};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/fixtures");

#[test]
fn every_drop_slot_case_replays() {
    let f: Value = serde_json::from_str(&std::fs::read_to_string(format!("{FIXTURES}/list-reorder.json")).unwrap()).unwrap();
    let cases = f["dropSlot"].as_array().unwrap();
    assert!(!cases.is_empty());
    for c in cases {
        let rows: Vec<ListReorderRow> = serde_json::from_value(c["rows"].clone()).unwrap();
        let drag = c["drag"].as_str().unwrap();
        let got = list_drop_slot(&rows, drag, c["at"].as_f64().unwrap() as f32, c["maxDepth"].as_f64().unwrap());
        let got_value = got.as_ref().map(|d| serde_json::to_value(d).unwrap()).unwrap_or(Value::Null);
        assert_eq!(got_value, c["expected"], "{}", c["name"]);
        let moved = got.as_ref().map(|d| list_move_context(drag, d)).unwrap_or(Value::Null);
        assert_eq!(moved, c["move"], "{}", c["name"]);
    }
    assert_eq!(list_drop_slot(&[], "a", 0.0, 2.0), None);
    let _ = json!(null);
}

#[test]
fn every_board_drop_slot_case_replays() {
    let f: Value = serde_json::from_str(&std::fs::read_to_string(format!("{FIXTURES}/board-drop.json")).unwrap()).unwrap();
    let columns: Vec<BoardColumn> = serde_json::from_value(f["columns"].clone()).unwrap();
    let cases = f["boardDropSlot"].as_array().unwrap();
    assert!(!cases.is_empty());
    for c in cases {
        let drag = c["drag"].as_str().unwrap();
        let got = board_drop_slot(&columns, drag, c["x"].as_f64().unwrap() as f32, c["y"].as_f64().unwrap() as f32);
        let got_value = got.as_ref().map(|d| serde_json::to_value(d).unwrap()).unwrap_or(Value::Null);
        assert_eq!(got_value, c["expected"], "{}", c["name"]);
        let moved = got.as_ref().map(|d| board_move_context(drag, d)).unwrap_or(Value::Null);
        assert_eq!(moved, c["move"], "{}", c["name"]);
    }
    assert_eq!(board_drop_slot(&[], "a", 0.0, 0.0), None);
}
