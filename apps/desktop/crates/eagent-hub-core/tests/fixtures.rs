//! Replays every case of `packages/eagent/fixtures/*.json` with the same
//! semantics as `packages/eagent/src/eagent.test.ts` (and `replay.ts`): the
//! envelope id defaults to `e<n>`, the source is `fixture`; `given` is
//! hydrated with `rules.defaults`; `effects` match exactly and in order,
//! `result` and `failure` by value; `expect.sessions/messages/agents` are
//! partial (the keys a row names; null = the row must not exist).

use eagent_hub_core::*;
use ecore::Envelope;
use serde_json::{json, Map, Value};

const SCENARIOS: &[(&str, &str)] = &[
    ("tree", include_str!("../../../../../packages/eagent/fixtures/tree.json")),
    ("bus", include_str!("../../../../../packages/eagent/fixtures/bus.json")),
    ("wakeups", include_str!("../../../../../packages/eagent/fixtures/wakeups.json")),
    ("watchdog", include_str!("../../../../../packages/eagent/fixtures/watchdog.json")),
    ("reuse", include_str!("../../../../../packages/eagent/fixtures/reuse.json")),
    ("incidents", include_str!("../../../../../packages/eagent/fixtures/incidents.json")),
];

fn fixture(name: &str) -> Value {
    let (_, src) = SCENARIOS.iter().find(|(n, _)| *n == name).unwrap();
    serde_json::from_str(src).unwrap()
}

fn envelope(step: &Value, index: usize) -> HubEvent {
    let event = &step["event"];
    let data = event.get("data").cloned().unwrap_or_else(|| json!({}));
    Envelope {
        v: ecore::ENVELOPE_VERSION as i64,
        id: event.get("id").and_then(Value::as_str).map_or_else(|| format!("e{}", index + 1), str::to_string),
        kind: event["kind"].as_str().unwrap().to_string(),
        at: event["at"].as_i64().unwrap(),
        source: "fixture".into(),
        session_id: data.get("sessionId").and_then(Value::as_str).map(str::to_string),
        data,
    }
}

/// The fields of `actual` that `expected` names (an absent field stays absent: `undefined`).
fn pick(actual: Option<Value>, expected: &Value) -> Value {
    match (actual, expected) {
        (Some(Value::Object(row)), Value::Object(want)) => {
            Value::Object(want.keys().filter_map(|k| Some((k.clone(), row.get(k)?.clone()))).collect::<Map<_, _>>())
        }
        (actual, _) => actual.unwrap_or(Value::Null),
    }
}

/// Replays one case; every mismatch as a line.
fn replay_case(file: &str, case: &Value) -> Vec<String> {
    let name = format!("{file}.json: {}", case["name"].as_str().unwrap());
    let mut errors = vec![];
    let mut state = hydrate(case.get("given").unwrap_or(&json!({}))).unwrap_or_else(|e| panic!("{name}: {e}"));
    for (index, step) in case["steps"].as_array().unwrap().iter().enumerate() {
        let event = envelope(step, index);
        let label = format!("{name} step {} ({})", index + 1, event.kind);
        let envelope_errors = event.errors();
        if !envelope_errors.is_empty() {
            errors.push(format!("{label}: envelope {envelope_errors:?}"));
        }
        let outcome = serde_json::to_value(reduce(&state, &event)).unwrap();
        let failure = outcome.get("failure").cloned();
        match step.get("failure") {
            Some(want) => {
                if failure.as_ref() != Some(want) {
                    errors.push(format!("{label}: failure {failure:?}, want {want}"));
                }
            }
            None => {
                if let Some(failure) = failure {
                    errors.push(format!("{label}: unexpected failure {failure}"));
                }
                for key in ["effects", "result"] {
                    if let Some(want) = step.get(key) {
                        if outcome.get(key) != Some(want) {
                            errors.push(format!(
                                "{label}: {key}\n  got  {}\n  want {}",
                                outcome.get(key).map_or("undefined".into(), Value::to_string),
                                want
                            ));
                        }
                    }
                }
            }
        }
        state = serde_json::from_value(outcome["state"].clone()).unwrap();
    }
    let state = serde_json::to_value(&state).unwrap();
    let expect = case.get("expect").cloned().unwrap_or(json!({}));
    for (key, rows) in [("sessions", &state["sessions"]), ("agents", &state["agents"])] {
        for (id, want) in expect.get(key).and_then(Value::as_object).into_iter().flatten() {
            let got = pick(rows.get(id).cloned(), want);
            if got != *want {
                errors.push(format!("{name}: {key}.{id}\n  got  {got}\n  want {want}"));
            }
        }
    }
    for (id, want) in expect.get("messages").and_then(Value::as_object).into_iter().flatten() {
        let row = state["messages"].as_array().unwrap().iter().find(|m| m["id"] == *id).cloned();
        let got = pick(row, want);
        if got != *want {
            errors.push(format!("{name}: messages.{id}\n  got  {got}\n  want {want}"));
        }
    }
    errors
}

#[test]
fn every_scenario_case_replays() {
    let mut errors = vec![];
    let mut count = 0;
    for (file, _) in SCENARIOS {
        let fixture = fixture(file);
        for case in fixture["cases"].as_array().unwrap() {
            count += 1;
            errors.extend(replay_case(file, case));
        }
    }
    assert!(count >= 70, "only {count} cases");
    assert!(errors.is_empty(), "{} mismatches:\n{}", errors.len(), errors.join("\n"));
}

#[test]
fn reuse_refusal_cases() {
    for case in fixture("reuse")["refusalCases"].as_array().unwrap() {
        let root: ReuseRoot = serde_json::from_value(case["root"].clone()).unwrap();
        let got = reuse_refusal(
            &root,
            case["children"].as_u64().unwrap() as usize,
            case["now"].as_i64().unwrap(),
            case.get("awaitsCaller").and_then(Value::as_bool).unwrap_or(false),
        );
        assert_eq!(got.as_deref(), case["expected"].as_str(), "{}", case["name"]);
    }
}

#[test]
fn watchdog_pure_cases() {
    let f = fixture("watchdog");
    for case in f["silentCases"].as_array().unwrap() {
        let got = copy::silent_minutes(case["alive"].as_i64(), case["now"].as_i64().unwrap());
        assert_eq!(got, case["expected"].as_i64().unwrap(), "{case}");
    }
    for case in f["liveIdCases"].as_array().unwrap() {
        let (ids, dropped) = clean_live_session_ids(case["ids"].as_array().unwrap());
        assert_eq!(json!({ "ids": ids, "dropped": dropped }), case["expected"], "{case}");
    }
}

#[test]
fn json_entrypoints_round_trip() {
    let state = hydrate_json(r#"{"sessions":[{"id":"r1"},{"id":"c1","parentId":"r1","status":"queued","startedAt":null}]}"#).unwrap();
    let event = r#"{"v":1,"id":"e1","kind":"device.claim","at":5,"source":"fixture","sessionId":"c1","data":{"sessionId":"c1","deviceId":"d1"}}"#;
    let outcome: Value = serde_json::from_str(&reduce_json(&state, event).unwrap()).unwrap();
    assert_eq!(outcome["result"], json!({ "sessionId": "c1" }));
    assert_eq!(outcome["state"]["sessions"]["c1"]["status"], "running");
    let unknown = event.replace("device.claim", "device.nope");
    let outcome: Value = serde_json::from_str(&reduce_json(&state, &unknown).unwrap()).unwrap();
    assert_eq!(outcome["failure"]["code"], "invalid");
    assert_eq!(outcome["effects"], json!([]));
}
