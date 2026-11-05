//! The contract replayed (#EDE-8): every case of packages/ede/fixtures is
//! what this crate does, byte for byte with the TS reference.

use ede_ui::apply::{self, ApplyState, DocumentState, Effect, Event};
use ede_ui::doc::Issue;
use serde_json::Value;

const SURFACE: &str = include_str!("../../../../../packages/ede/fixtures/surface.json");
const APPLY: &str = include_str!("../../../../../packages/ede/fixtures/apply.json");

fn issues(v: &Value) -> Vec<Issue> {
    serde_json::from_value(v.clone()).expect("issues")
}

#[test]
fn header_cases() {
    let fixture: Value = serde_json::from_str(SURFACE).unwrap();
    for case in fixture["headerCases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let got = ede_ui::normalize_surface(&case["input"]);
        match (got, case["expected"].get("header"), case["expected"].get("issues")) {
            (Ok(header), Some(expected), _) => assert_eq!(serde_json::to_value(&header).unwrap(), *expected, "{name}"),
            (Err(got), _, Some(expected)) => assert_eq!(got, issues(expected), "{name}"),
            (got, ..) => panic!("{name}: {got:?}"),
        }
    }
}

#[test]
fn document_cases() {
    let fixture: Value = serde_json::from_str(SURFACE).unwrap();
    for case in fixture["documentCases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let got = ede_ui::validate_document("doc", &case["document"]).map(|_| ()).err().unwrap_or_default();
        assert_eq!(got, issues(&case["expected"]["issues"]), "{name}");
    }
}

#[test]
fn rules_match() {
    let fixture: Value = serde_json::from_str(APPLY).unwrap();
    assert_eq!(fixture["rules"]["debounceMs"], apply::DEBOUNCE_MS);
    assert_eq!(fixture["rules"]["frameAckMs"], apply::FRAME_ACK_MS);
    assert_eq!(fixture["rules"]["crashesToRevert"], apply::CRASHES_TO_REVERT);
    assert_eq!(ede_ui::watch::DEBOUNCE.as_millis() as i64, apply::DEBOUNCE_MS);
}

fn hydrate(given: &Value) -> ApplyState {
    let mut state = ApplyState::default();
    for (name, d) in given["documents"].as_object().cloned().unwrap_or_default() {
        let commits: Vec<String> = d.get("commits").map(|c| serde_json::from_value(c.clone()).unwrap()).unwrap_or_else(|| vec![apply::factory_commit(&name)]);
        for c in &commits {
            if let Some(n) = c.strip_prefix('c').and_then(|n| n.parse::<u32>().ok()) {
                state.commit_count = state.commit_count.max(n);
            }
        }
        let newest = commits.last().cloned().unwrap();
        state.documents.insert(
            name.clone(),
            DocumentState {
                shown: d.get("shown").and_then(Value::as_str).map(String::from).unwrap_or(newest.clone()),
                from_factory: d.get("fromFactory").and_then(Value::as_bool).unwrap_or(newest == apply::factory_commit(&name)),
                commits,
                pending: d.get("pending").map(|p| serde_json::from_value(p.clone()).unwrap()),
                crashes: d.get("crashes").and_then(Value::as_u64).unwrap_or(0) as u32,
                rejected: d.get("rejected").map(|r| issues(r)),
                reverted: d.get("reverted").and_then(Value::as_bool).unwrap_or(false),
            },
        );
    }
    state
}

fn event(step: &Value) -> Event {
    let e = &step["event"];
    let at = e["at"].as_i64().unwrap();
    let data = &e["data"];
    match e["kind"].as_str().unwrap() {
        "ui.edit" => Event::Edit { at, name: data["name"].as_str().unwrap().into(), valid: data["valid"].as_bool().unwrap(), issues: data.get("issues").map(issues).unwrap_or_default() },
        "ui.frame" => Event::Frame { at, name: data["name"].as_str().unwrap().into(), commit: data["commit"].as_str().unwrap().into() },
        "shell.crash" => Event::Crash { at },
        "tick" => Event::Tick { at },
        other => panic!("unknown event {other}"),
    }
}

#[test]
fn apply_cases() {
    let fixture: Value = serde_json::from_str(APPLY).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let mut state = hydrate(&case["given"]);
        for (i, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            let (next, effects) = apply::reduce(&state, &event(step));
            if let Some(expected) = step.get("effects") {
                let expected: Vec<Effect> = serde_json::from_value(expected.clone()).unwrap();
                assert_eq!(effects, expected, "{name}, step {i}");
            }
            if let Some(expected) = step.get("state").and_then(Value::as_object) {
                for (doc, fields) in expected {
                    let got = serde_json::to_value(next.documents.get(doc).unwrap_or_else(|| panic!("{name}: no document {doc}"))).unwrap();
                    for (field, value) in fields.as_object().unwrap() {
                        assert_eq!(got.get(field).cloned().unwrap_or(Value::Null), *value, "{name}, step {i}, {doc}.{field}");
                    }
                }
            }
            state = next;
        }
    }
}
