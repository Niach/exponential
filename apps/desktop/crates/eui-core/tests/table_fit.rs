//! #FEED-224: `fixtures/table-fit.json` replayed against the core's column
//! fit (the TS twin is packages/exponential-ui/src/table-fit.test.ts).

use eui_core::table_fit::{fit_table_columns, FitColumn, TABLE_WORD_CAP};
use serde_json::Value;

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/fixtures/table-fit.json");

#[test]
fn every_case_replays() {
    let f: Value = serde_json::from_str(&std::fs::read_to_string(FIXTURE).expect("fixture")).expect("json");
    assert_eq!(f["wordCap"].as_f64(), Some(TABLE_WORD_CAP));
    let cases = f["cases"].as_array().unwrap();
    assert!(!cases.is_empty());
    for c in cases {
        let columns: Vec<FitColumn> = c["columns"].as_array().unwrap().iter().map(|col| FitColumn { min: col["min"].as_f64().unwrap(), max: col["max"].as_f64().unwrap() }).collect();
        let want: Vec<f64> = c["widths"].as_array().unwrap().iter().map(|w| w.as_f64().unwrap()).collect();
        assert_eq!(fit_table_columns(&columns, c["available"].as_f64().unwrap(), TABLE_WORD_CAP), want, "{}", c["name"]);
    }
}
