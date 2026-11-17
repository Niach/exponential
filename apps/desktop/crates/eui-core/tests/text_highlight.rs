//! #EDE-95: `fixtures/text-highlight.json` `cases` and `currentCases`
//! replayed against the core's fold + match rule (the TS twin is
//! packages/exponential-ui/src/highlight.test.ts).

use eui_core::text_highlight::{highlight_count, highlight_local_current, highlight_ranges, highlight_spec, HIGHLIGHT_BARRIER};
use serde_json::{json, Value};

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/fixtures/text-highlight.json");

fn fixture() -> Value {
    serde_json::from_str(&std::fs::read_to_string(FIXTURE).expect("fixture")).expect("json")
}

/// A byte offset into `text` as a UTF-16 offset.
fn utf16(text: &str, byte: usize) -> u64 {
    text[..byte].encode_utf16().count() as u64
}

#[test]
fn every_case_replays() {
    let f = fixture();
    let cases = f["cases"].as_array().unwrap();
    assert!(!cases.is_empty());
    for c in cases {
        let (name, text, query) = (c["name"].as_str().unwrap(), c["text"].as_str().unwrap(), c["query"].as_str().unwrap());
        let ranges = highlight_ranges(text, query);
        let got: Vec<Value> = ranges.iter().map(|r| json!([utf16(text, r.start), utf16(text, r.end)])).collect();
        assert_eq!(Value::Array(got), c["ranges"], "{name}: ranges");
        let matches: Vec<Value> = ranges.iter().map(|r| json!(&text[r.clone()])).collect();
        assert_eq!(Value::Array(matches), c["matches"], "{name}: matches");
        assert_eq!(highlight_count(text, query), ranges.len(), "{name}: count");
    }
}

#[test]
fn every_current_case_replays() {
    let f = fixture();
    for c in f["currentCases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let spec = highlight_spec(&c["highlight"]).unwrap_or_else(|| panic!("{name}: a spec"));
        let local = highlight_local_current(&spec).map_or(-1, |l| l as i64);
        assert_eq!(local, c["local"].as_i64().unwrap(), "{name}");
    }
}

#[test]
fn a_barrier_never_matches_and_breaks_a_match() {
    let text = format!("foo{HIGHLIGHT_BARRIER}bar");
    assert_eq!(highlight_count(&text, &HIGHLIGHT_BARRIER.to_string()), 0);
    assert_eq!(highlight_count(&text, "foobar"), 0);
    assert_eq!(highlight_count(&text, &format!("foo{HIGHLIGHT_BARRIER}bar")), 0);
    assert_eq!(highlight_count(&text, "bar"), 1);
}

#[test]
fn a_spec_needs_a_query_that_folds_to_something() {
    assert!(highlight_spec(&json!(null)).is_none());
    assert!(highlight_spec(&json!("foo")).is_none());
    assert!(highlight_spec(&json!({ "query": "" })).is_none());
    assert!(highlight_spec(&json!({ "query": "\u{301}" })).is_none());
    assert!(highlight_spec(&json!({ "query": 3 })).is_none());
    let spec = highlight_spec(&json!({ "query": "a", "current": 2.7, "before": "" })).unwrap();
    assert_eq!((spec.current, spec.before), (Some(2), None));
}
