//! #VAPP-285: `fixtures/tree.json` (the TS reference's `src/tree.ts`):
//! the Tree's flat rows and its WAI-ARIA keys.

use eui_core::tree::{tree_key, tree_rows, TreeKeyRow};
use serde_json::Value;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/fixtures");

fn fixture() -> Value {
    serde_json::from_str(&std::fs::read_to_string(format!("{FIXTURES}/tree.json")).expect("fixture")).expect("json")
}

#[test]
fn tree_rows_flatten_like_the_reference() {
    let f = fixture();
    let cases = f["rows"].as_array().unwrap();
    assert!(cases.len() > 5);
    for c in cases {
        let rows = tree_rows(Some(&f["items"]), Some(&c["expanded"]));
        assert_eq!(Value::Array(rows), c["rows"], "{}", c["name"]);
    }
}

#[test]
fn tree_keys_move_and_fold_like_the_reference() {
    let f = fixture();
    let rows: Vec<TreeKeyRow> = f["keyRows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| TreeKeyRow { level: r["level"].as_u64().unwrap() as u32, expanded: r.get("expanded").and_then(Value::as_bool) })
        .collect();
    for c in f["keys"].as_array().unwrap() {
        let got = tree_key(&rows, c["index"].as_u64().unwrap() as usize, c["key"].as_str().unwrap(), c["rtl"].as_bool().unwrap());
        assert_eq!(got.map(|r| r.to_json()).unwrap_or(Value::Null), c["result"], "{c}");
    }
}
