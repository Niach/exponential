//! Replays `packages/uicore/fixtures/ide-sidebar.json` against the
//! TypeScript reference.

use serde_json::Value;
use uicore_model::ide::{ide_sidebar, IdeSidebarInput};

#[test]
fn every_ide_sidebar_case_replays() {
    let f: Value = serde_json::from_str(include_str!("../../../../../packages/uicore/fixtures/ide-sidebar.json")).expect("ide-sidebar.json");
    let cases = f["cases"].as_array().expect("cases");
    assert!(cases.len() >= 12, "{} cases", cases.len());
    for case in cases {
        let name = case["name"].as_str().unwrap_or("?");
        let mut input = f["base"].clone();
        for (k, v) in case["input"].as_object().expect("input") {
            input[k] = v.clone();
        }
        let input: IdeSidebarInput = serde_json::from_value(input).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(serde_json::to_value(ide_sidebar(&input)).unwrap(), case["expected"], "{name}");
    }
}
