//! #EXP-1778: `fixtures/text-truncate.json` replayed against the core's pure
//! middle cut (the TS twin is packages/exponential-ui/src/text-truncate.test.ts).

use eui_core::text_truncate::middle_truncate_width;
use serde_json::Value;

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/fixtures/text-truncate.json");

#[test]
fn every_case_replays() {
    let f: Value = serde_json::from_str(&std::fs::read_to_string(FIXTURE).expect("fixture")).expect("json");
    let affix = f["affix"].as_str().unwrap();
    for c in f["cases"].as_array().unwrap() {
        let widths = &c["widths"];
        let default = widths["default"].as_f64().unwrap();
        let measure = |ch: char| widths.get("chars").and_then(|m| m.get(ch.to_string())).and_then(Value::as_f64).unwrap_or(default);
        assert_eq!(middle_truncate_width(c["text"].as_str().unwrap(), c["width"].as_f64().unwrap(), measure, affix), c["shown"].as_str().unwrap(), "{}", c["name"]);
    }
}
