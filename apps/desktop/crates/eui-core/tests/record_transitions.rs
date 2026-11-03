//! eApps R6 (VAPP-222): `fixtures/record-transitions.json` replayed through
//! the Rust core (the TS reference replays it in `src/host/record-transitions.test.ts`).

mod support;

use eui_core::host::{kind_transitions, transition_refusal, transitions_from, validate_record_transitions, RecordTransitions, TransitionViewer};
use serde_json::{json, Value};
use support::fixture;

fn machine(f: &Value, c: &Value) -> Option<RecordTransitions> {
    match c.get("transitions") {
        Some(t) => kind_transitions(Some(&json!({ "transitions": t }))),
        None => kind_transitions(Some(&f["kind"])),
    }
}

fn viewer(v: &Value) -> TransitionViewer<'_> {
    TransitionViewer { id: v.get("id").and_then(Value::as_str), role: v.get("role").and_then(Value::as_str) }
}

#[test]
fn record_transitions_fixture() {
    let f = fixture("record-transitions.json");
    for c in f["validateCases"].as_array().unwrap() {
        let name = c["kindName"].as_str().unwrap();
        let got = validate_record_transitions(&c["kind"], &format!("/records/{name}"), name);
        assert_eq!(serde_json::to_value(got).unwrap(), c["expected"], "validate: {}", c["name"]);
    }
    for c in f["refusalCases"].as_array().unwrap() {
        let t = machine(&f, c);
        let before = c.get("before").filter(|b| !b.is_null());
        let got = transition_refusal(t.as_ref(), before, &c["after"], &viewer(&c["viewer"]), c.get("row"));
        assert_eq!(serde_json::to_value(got).unwrap(), c["expected"], "refusal: {}", c["name"]);
    }
    for c in f["fromCases"].as_array().unwrap() {
        let t = machine(&f, c);
        let v = c.get("viewer").map(viewer);
        let got = transitions_from(t.as_ref(), c.get("record").filter(|r| !r.is_null()), v.as_ref(), c.get("row"));
        assert_eq!(json!(got), c["expected"], "from: {}", c["name"]);
    }
}
