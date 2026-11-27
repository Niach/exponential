//! #EDE-273: `packages/uicore/fixtures/system-settings.json` replayed in Rust
//! (the TypeScript reference, `packages/uicore/src/settings.ts`, replays the same).

use serde_json::{json, Value};
use uicore_model::settings::{settings_grid, settings_modules, settings_navigate, SettingsEvent, SettingsState};

const FIXTURE: &str = include_str!("../../../../../packages/uicore/fixtures/system-settings.json");

fn fixture() -> Value {
    serde_json::from_str(FIXTURE).expect("system-settings.json")
}

fn installed(case: &Value) -> Vec<String> {
    serde_json::from_value(case["installed"].clone()).expect("installed")
}

#[test]
fn every_grid_case_replays() {
    let f = fixture();
    let cases = f["grid"].as_array().expect("grid cases");
    assert!(cases.len() >= 8);
    for case in cases {
        let name = case["name"].as_str().unwrap_or("?");
        assert_eq!(json!(settings_grid(case["query"].as_str().unwrap(), &installed(case))), case["expected"], "{name}");
    }
}

#[test]
fn every_navigate_case_replays() {
    let f = fixture();
    let cases = f["navigate"].as_array().expect("navigate cases");
    assert!(cases.len() >= 20);
    for case in cases {
        let name = case["name"].as_str().unwrap_or("?");
        let state: SettingsState = serde_json::from_value(case["state"].clone()).unwrap_or_else(|e| panic!("{name}: state: {e}"));
        let event: SettingsEvent = serde_json::from_value(case["event"].clone()).unwrap_or_else(|e| panic!("{name}: event: {e}"));
        assert_eq!(json!(settings_navigate(&state, &event, &installed(case))), case["expected"], "{name}");
    }
}

#[test]
fn the_catalogue_reads() {
    assert!(settings_modules().len() >= 40);
}
