//! eApps R3: `fixtures/record-policy.json` replayed through the Rust core
//! (the TS reference replays it in `src/host/record-policy.test.ts`).

mod support;

use eui_core::host::{readable_rows, record_policy, record_policy_copy, record_refusal, validate_record_policy, RecordOp, RecordViewer, APP_ACCESS_LEVELS, DEFAULT_RECORD_POLICY, RECORD_POLICY_LEVELS, RECORD_POLICY_OPS};
use serde_json::{json, Value};
use support::fixture;

#[test]
fn record_policy_fixture() {
    let f = fixture("record-policy.json");
    assert_eq!(f["ops"], json!(RECORD_POLICY_OPS));
    assert_eq!(f["levels"], json!(RECORD_POLICY_LEVELS));
    assert_eq!(f["access"], json!(APP_ACCESS_LEVELS));
    assert_eq!(f["defaults"], DEFAULT_RECORD_POLICY.to_json());
    assert_eq!(f["copy"], record_policy_copy());
    for c in f["validateCases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        assert_eq!(serde_json::to_value(validate_record_policy(&c["policy"], "")).unwrap(), c["issues"], "validate: {name}");
        assert_eq!(record_policy(Some(&json!({"policy": c["policy"]}))).to_json(), c["effective"], "effective: {name}");
    }
    for c in f["allowCases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let op = RecordOp::parse(c["op"].as_str().unwrap()).unwrap();
        let policy = record_policy(c.get("policy").map(|p| json!({"policy": p})).as_ref());
        let viewer = RecordViewer { id: c["viewer"].as_str(), role: c["role"].as_str(), access: c.get("access").and_then(Value::as_str) };
        let row = json!({"authorId": c.get("author").cloned().unwrap_or(Value::Null), "createdBy": c.get("createdBy").cloned().unwrap_or(Value::Null)});
        let got = record_refusal(&policy, op, &viewer, if op == RecordOp::Create { None } else { Some(&row) });
        assert_eq!(got, c["refusal"].as_str(), "allow: {name}");
    }
    let r = &f["readable"];
    let rows = r["rows"].as_array().unwrap();
    for v in r["views"].as_array().unwrap() {
        let viewer = RecordViewer { id: v["viewer"].as_str(), role: v["role"].as_str(), access: v.get("access").and_then(Value::as_str) };
        let ids: Vec<&Value> = readable_rows(rows, Some(&r["kinds"]), &viewer).into_iter().map(|row| &row["id"]).collect();
        assert_eq!(json!(ids), v["ids"], "readable: {v}");
    }
}
