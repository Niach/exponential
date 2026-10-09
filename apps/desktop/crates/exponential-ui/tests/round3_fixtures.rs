//! Round 3 (VAPP-102): the core-computed tree guides. Replays
//! `fixtures/tree-guides.json` (depths → guides, the case names of the TS
//! suite) and runs `apply_tree_guides` over the `Row/tree:*` cases of
//! `catalog-macros.json`: the expansion fills the guides, and a tree whose
//! guides were reset to `{depth}` gets them back byte for byte.

use exponential_ui::catalog::CatalogView;
use exponential_ui::json;
use exponential_ui::macros::expand_macros;
use exponential_ui::tree_guides::{apply_tree_guides, is_row_root, tree_guides};
use exponential_ui::UiNode;
use serde_json::{json, Value};

fn fixture(name: &str) -> Value {
    let path = format!("{}/../../../../packages/exponential-ui/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn assert_same(case: &str, got: &impl serde::Serialize, expected: &Value) {
    let got = serde_json::to_value(got).unwrap();
    assert!(json::equal(&got, expected), "{case}:\n got {}\n expected {}", json::canonical(&got), json::canonical(expected));
}

#[test]
fn tree_guides_every_case_matches_the_fixture() {
    let file = fixture("tree-guides.json");
    let cases = file["cases"].as_array().expect("cases");
    assert!(!cases.is_empty());
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let depths: Vec<usize> = case["depths"].as_array().unwrap().iter().map(|d| d.as_u64().unwrap() as usize).collect();
        let got: Vec<Value> = tree_guides(&depths)
            .into_iter()
            .map(|g| json!({"elbowAt": g.elbow_at, "tee": g.tee, "passThrough": g.pass_through}))
            .collect();
        assert_same(name, &got, &case["guides"]);
    }
}

/// Reset every Row's filled guides to the template's bare `{depth}`.
fn strip_guides(node: &mut UiNode) {
    let row = is_row_root(node);
    for child in &mut node.children {
        if row && child.component == "TreeGuides" {
            let depth = child.props.get("depth").cloned().unwrap_or(Value::Null);
            child.props = serde_json::Map::from_iter([("depth".to_string(), depth)]);
        }
        strip_guides(child);
    }
    if let Some(slots) = node.slots.as_mut() {
        slots.values_mut().for_each(strip_guides);
    }
}

#[test]
fn apply_tree_guides_fills_every_row_tree_macro_case() {
    let file = fixture("catalog-macros.json");
    let view = CatalogView::core();
    let mut seen = 0;
    for case in file["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        if !name.starts_with("Row/tree:") {
            continue;
        }
        seen += 1;
        let input: UiNode = serde_json::from_value(case["input"].clone()).unwrap();
        let expanded = expand_macros(&input, &view).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_same(name, &expanded, &case["expected"]);

        let mut stripped: UiNode = serde_json::from_value(case["expected"].clone()).unwrap();
        strip_guides(&mut stripped);
        apply_tree_guides(&mut stripped);
        assert_same(&format!("{name} (re-applied)"), &stripped, &case["expected"]);
    }
    assert!(seen >= 8, "the Row/tree:* cases: {seen}");
}
