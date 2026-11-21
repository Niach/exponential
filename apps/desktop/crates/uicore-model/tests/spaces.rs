//! #EXP-2024: `packages/uicore/fixtures/spaces.json` replayed in Rust (the
//! TypeScript reference, `packages/uicore/src/spaces.ts`, replays the same).

use serde_json::{json, Value};
use uicore_model::spaces::{space_rows, space_view, Display, Extent, SpaceInput, SpaceSession};

const FIXTURE: &str = include_str!("../../../../../packages/uicore/fixtures/spaces.json");

fn fixture() -> Value {
    serde_json::from_str(FIXTURE).expect("spaces.json")
}

#[test]
fn every_rows_case_replays() {
    let f = fixture();
    let cases = f["rows"].as_array().expect("rows cases");
    assert!(cases.len() >= 8, "{} rows cases", cases.len());
    for case in cases {
        let name = case["name"].as_str().unwrap_or("?");
        let spaces: Vec<SpaceInput> = serde_json::from_value(case["spaces"].clone()).unwrap_or_else(|e| panic!("{name}: spaces: {e}"));
        let sessions: Vec<SpaceSession> = serde_json::from_value(case["sessions"].clone()).unwrap_or_else(|e| panic!("{name}: sessions: {e}"));
        assert_eq!(json!(space_rows(&spaces, &sessions)), case["expected"], "{name}");
    }
}

#[test]
fn every_view_case_replays() {
    let f = fixture();
    let cases = f["view"].as_array().expect("view cases");
    assert!(cases.len() >= 10, "{} view cases", cases.len());
    for case in cases {
        let name = case["name"].as_str().unwrap_or("?");
        let space: Extent = serde_json::from_value(case["space"].clone()).unwrap_or_else(|e| panic!("{name}: space: {e}"));
        let display: Display = serde_json::from_value(case["display"].clone()).unwrap_or_else(|e| panic!("{name}: display: {e}"));
        let large = case["large"].as_bool().unwrap_or(false);
        assert_eq!(json!(space_view(space, display, large)), case["expected"], "{name}");
    }
}
