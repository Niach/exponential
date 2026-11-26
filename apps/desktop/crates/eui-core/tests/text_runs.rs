//! Round 4 group B: `fixtures/text-runs.json` replayed against the core's
//! `text_runs` (the TS twin is packages/exponential-ui/src/text-runs.test.ts),
//! and the layout tree's hand-off: `runs` in the normal form, `text` = the
//! joined text, `dropCap` = its lines.

use std::collections::HashMap;

use eui_core::measure::{FixedMeasure, FIXED_CHAR_WIDTH, FIXED_LINE_HEIGHT};
use eui_core::surface::{Surface, SurfaceOptions};
use eui_core::text_runs::{drop_cap_lines, drop_cap_size, drop_cap_split, normal_runs, runs_text, text_runs, TextRun, DROP_CAP_GAP_TOKEN, DROP_CAP_LINES, DROP_CAP_MAX, DROP_CAP_MIN, DROP_CAP_RATIO};
use eui_core::types::NestedNode;
use serde_json::{json, Value};

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/fixtures/text-runs.json");

fn fixture() -> Value {
    serde_json::from_str(&std::fs::read_to_string(FIXTURE).expect("fixture")).expect("json")
}

#[test]
fn the_numbers() {
    let f = fixture();
    let d = &f["dropCap"];
    assert_eq!(d["min"], json!(DROP_CAP_MIN));
    assert_eq!(d["max"], json!(DROP_CAP_MAX));
    assert_eq!(d["lines"], json!(DROP_CAP_LINES));
    assert_eq!(d["ratio"].as_f64().unwrap() as f32, DROP_CAP_RATIO);
    assert_eq!(d["gap"], json!(format!("$spacing.{DROP_CAP_GAP_TOKEN}")));
}

#[test]
fn normal_cases() {
    for c in fixture()["normalCases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let runs = text_runs(Some(&c["runs"]));
        assert_eq!(runs.as_deref().map(normal_runs).unwrap_or(Value::Null), c["out"], "{name}");
        assert_eq!(runs.as_deref().map(runs_text).map(Value::from).unwrap_or(Value::Null), c["text"], "{name}: text");
    }
}

#[test]
fn lines_cases() {
    for c in fixture()["linesCases"].as_array().unwrap() {
        assert_eq!(json!(drop_cap_lines(c["props"].as_object().unwrap())), c["lines"], "{}", c["name"]);
    }
}

#[test]
fn split_cases() {
    for c in fixture()["splitCases"].as_array().unwrap() {
        let runs = text_runs(Some(&c["runs"])).unwrap();
        let got = drop_cap_split(&runs).map(|(cap, rest)| json!({"cap": cap.to_json(), "rest": normal_runs(&rest)})).unwrap_or(Value::Null);
        assert_eq!(got, c["split"], "{}", c["name"]);
    }
}

#[test]
fn size_cases() {
    for c in fixture()["sizeCases"].as_array().unwrap() {
        let size = drop_cap_size(c["fontSize"].as_f64().unwrap() as f32, c["lineHeight"].as_f64().unwrap() as f32, c["lines"].as_u64().unwrap() as u32);
        assert_eq!(size as f64, c["size"].as_f64().unwrap(), "{c}");
    }
}

#[test]
fn the_layout_tree_hands_the_natives_the_normal_form() {
    let tree: NestedNode = serde_json::from_value(json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "alignItems": "flex-start"}, "children": [
        {"id": "r", "component": "Text", "props": {"text": "ignored", "runs": ["Ship ", {"text": "today", "weight": "bold", "tone": "pink"}]}},
        {"id": "b", "component": "Text", "props": {"runs": {"@path": "/runs"}}},
        {"id": "d", "component": "Text", "props": {"text": "Once", "dropCap": 2.6}},
        {"id": "off", "component": "Text", "props": {"text": "Once", "dropCap": 3, "lines": 2}}
    ]}))
    .unwrap();
    let mut s = Surface::new("t", SurfaceOptions::default());
    // The validator reports the string run and the unknown tone; the
    // normal form still takes them (bound data gets no validation).
    let outcome = s.set_nested(tree);
    assert_eq!(outcome.issues.len(), 2, "{:?}", outcome.issues);
    s.set_data("", Some(json!({"runs": [{"text": "a", "mark": "code"}, "b"]}))).expect("data");
    s.set_viewport(600.0, 400.0, None);
    let out = s.layout(&mut FixedMeasure { sizes: HashMap::new(), wrap: false });
    let props = |id: &str| s.layout_node(s.index_of(id).unwrap()).unwrap().props.clone();
    let frame = |id: &str| {
        let i = s.index_of(id).unwrap();
        out.frames.iter().find(|fr| fr.index == i).map(|fr| (fr.w, fr.h)).unwrap()
    };
    assert_eq!(props("r")["text"], json!("Ship today"));
    assert_eq!(props("r")["runs"], json!([{"text": "Ship "}, {"text": "today", "weight": "bold"}]));
    assert_eq!(props("b")["text"], json!("ab"));
    assert_eq!(TextRun::from_json(&props("b")["runs"][0]).unwrap().mark.as_deref(), Some("code"));
    assert_eq!(frame("r"), (10.0 * FIXED_CHAR_WIDTH, FIXED_LINE_HEIGHT));
    assert_eq!(props("d")["dropCap"], json!(3));
    assert_eq!(frame("d"), (4.0 * FIXED_CHAR_WIDTH, 3.0 * FIXED_LINE_HEIGHT));
    assert!(props("off").get("dropCap").is_none());
    assert_eq!(frame("off").1, FIXED_LINE_HEIGHT);
}
