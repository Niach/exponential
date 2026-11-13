//! Replays every row and case of `packages/ecode/fixtures/*.json` with the
//! semantics of `packages/ecode/src/replay.ts`: each output is recomputed and
//! compared (as JSON values) with the file's.

use ecode_core::*;
use serde_json::{json, Map, Value};

const FORGE: &str = include_str!("../../../../../packages/ecode/fixtures/forge.json");
const CHECKS: &str = include_str!("../../../../../packages/ecode/fixtures/checks.json");
const PIPELINE: &str = include_str!("../../../../../packages/ecode/fixtures/pipeline.json");
const RUNNER: &str = include_str!("../../../../../packages/ecode/fixtures/runner.json");
const REMOTE: &str = include_str!("../../../../../packages/ecode/fixtures/remote.json");

fn parse(src: &str) -> Value {
    serde_json::from_str(src).expect("fixture parses")
}

fn rows<'a>(fixture: &'a Value, key: &str) -> &'a Vec<Value> {
    let rows = fixture[key].as_array().unwrap_or_else(|| panic!("fixture has `{key}`"));
    assert!(!rows.is_empty(), "fixture `{key}` is empty");
    rows
}

fn to_json<T: serde::Serialize>(value: T) -> Value {
    serde_json::to_value(value).unwrap()
}

/// replay.ts `stepOut`: the step as the reference writes it back.
fn step_out<S>(step: &Value, outcome: &Outcome<S>) -> Value {
    let mut out = Map::new();
    out.insert("op".into(), step["op"].clone());
    if let Some(at) = step.get("at") {
        out.insert("at".into(), at.clone());
    }
    if let Some(args) = step.get("args") {
        out.insert("args".into(), args.clone());
    }
    match &outcome.failure {
        Some(failure) => out.insert("failure".into(), to_json(failure)),
        None => out.insert("result".into(), outcome.result.clone().unwrap_or_else(|| json!({}))),
    };
    out.insert("events".into(), Value::Array(outcome.events.clone()));
    Value::Object(out)
}

fn sorted<V: Clone>(map: impl IntoIterator<Item = (String, V)>) -> Vec<(String, V)> {
    let mut entries: Vec<_> = map.into_iter().collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries
}

#[test]
fn forge_capabilities() {
    let fixture = parse(FORGE);
    for row in rows(&fixture, "capabilities") {
        let provider = Provider::parse(row["provider"].as_str().unwrap()).unwrap();
        let capability = Capability::parse(row["capability"].as_str().unwrap()).unwrap();
        assert_eq!(json!(supports(provider, capability)), row["supported"], "forge.json capabilities: {row}");
    }
}

#[test]
fn forge_cases() {
    let fixture = parse(FORGE);
    for case in rows(&fixture, "cases") {
        let name = format!("forge.json: {}", case["name"].as_str().unwrap());
        let given = case.get("given").cloned().unwrap_or_else(|| json!({}));
        let start = hydrate(&given).unwrap_or_else(|e| panic!("{name}: {e}"));
        let mut state = start.clone();
        for (index, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            let op = step["op"].as_str().unwrap();
            let args = step.get("args").cloned().unwrap_or_else(|| json!({}));
            let outcome = forge(&state, op, &args).unwrap_or_else(|e| panic!("{name} step {} ({op}): {e}", index + 1));
            assert_eq!(step_out(step, &outcome), *step, "{name} step {} ({op})", index + 1);
            state = outcome.state;
        }
        let commits: Map<String, Value> = sorted(state.commits.clone())
            .into_iter()
            .filter(|(id, _)| !start.commits.contains_key(id))
            .map(|(id, commit)| (id, to_json(commit)))
            .collect();
        let mut prs = state.prs.clone();
        prs.sort_by_key(|pr| pr.number);
        let expect = json!({
            "branches": to_json(&state.branches),
            "prs": to_json(&prs),
            "commits": commits,
        });
        assert_eq!(expect, case["expect"], "{name}: expect");
    }
}

#[test]
fn runner_cases() {
    let fixture = parse(RUNNER);
    for case in rows(&fixture, "cases") {
        let name = format!("runner.json: {}", case["name"].as_str().unwrap());
        let mut state = RunnerState::default();
        for (index, step) in case["steps"].as_array().unwrap().iter().enumerate() {
            let op = step["op"].as_str().unwrap();
            let args = step.get("args").cloned().unwrap_or_else(|| json!({}));
            let at = step.get("at").and_then(Value::as_i64).unwrap_or(0);
            let outcome =
                schedule(&state, op, &args, at).unwrap_or_else(|e| panic!("{name} step {} ({op}): {e}", index + 1));
            assert_eq!(step_out(step, &outcome), *step, "{name} step {} ({op})", index + 1);
            state = outcome.state;
        }
        let jobs: Map<String, Value> = state
            .jobs
            .iter()
            .map(|job| {
                let row = json!({ "status": job.status, "conclusion": job.conclusion, "runner": job.runner, "flagged": job.flagged });
                (job.id.clone(), row)
            })
            .collect();
        let expect = json!({ "jobs": jobs, "run": run_conclusion(&state) });
        assert_eq!(expect, case["expect"], "{name}: expect");
    }
}

#[test]
fn checks_rows() {
    let fixture = parse(CHECKS);
    for row in rows(&fixture, "rollup") {
        let checks: Vec<CheckRow> = serde_json::from_value(row["checks"].clone()).unwrap();
        assert_eq!(to_json(rollup(&checks)), row["state"], "checks.json rollup: {}", row["name"]);
    }
    for row in rows(&fixture, "gate") {
        let checks: Vec<CheckRow> = serde_json::from_value(row["checks"].clone()).unwrap();
        let required: Vec<String> = serde_json::from_value(row["required"].clone()).unwrap();
        assert_eq!(to_json(merge_gate(&checks, &required)), row["result"], "checks.json gate: {}", row["name"]);
    }
    for row in rows(&fixture, "checkName") {
        let matrix: Vec<String> = serde_json::from_value(row["matrix"].clone()).unwrap();
        let check = check_name(row["name"].as_str().unwrap(), &matrix);
        assert_eq!(json!(check), row["check"], "checks.json checkName: {}", row["name"]);
    }
}

#[test]
fn pipeline_rows() {
    let fixture = parse(PIPELINE);
    for row in rows(&fixture, "glob") {
        let matched = glob_match(row["pattern"].as_str().unwrap(), row["path"].as_str().unwrap());
        assert_eq!(json!(matched), row["match"], "pipeline.json glob: {row}");
    }
    for row in rows(&fixture, "validate") {
        assert_eq!(to_json(validate_pipeline(&row["file"])), row["errors"], "pipeline.json validate: {}", row["name"]);
    }
    for row in rows(&fixture, "plan") {
        let name = &row["name"];
        let errors = validate_pipeline(&row["file"]);
        assert!(errors.is_empty(), "pipeline.json plan case {name} has an invalid pipeline: {errors:?}");
        let trigger: TriggerInput = serde_json::from_value(row["trigger"].clone()).unwrap();
        assert_eq!(to_json(plan_pipeline(&row["file"], &trigger)), row["plan"], "pipeline.json plan: {name}");
    }
}

#[test]
fn remote_rows() {
    let fixture = parse(REMOTE);
    for row in rows(&fixture, "parse") {
        let url = row["url"].as_str().unwrap();
        assert_eq!(to_json(parse_remote(url)), row["remote"], "remote.json parse: {url}");
        assert_eq!(to_json(credential_helper_key(url)), row["helperKey"], "remote.json helperKey: {url}");
    }
    for row in rows(&fixture, "refresh") {
        let refresh = needs_refresh(row["expiresAt"].as_i64(), row["now"].as_i64().unwrap());
        assert_eq!(json!(refresh), row["refresh"], "remote.json refresh: {row}");
    }
    for row in rows(&fixture, "redact") {
        let secrets: Vec<String> = serde_json::from_value(row["secrets"].clone()).unwrap();
        let out = redact(row["text"].as_str().unwrap(), &secrets);
        assert_eq!(json!(out), row["out"], "remote.json redact: {}", row["text"]);
    }
}

#[test]
fn refusals_leave_the_state_alone() {
    let state = hydrate(&json!({ "commits": { "c1": {} }, "branches": { "main": "c1" } })).unwrap();
    let outcome = forge(&state, "pr.open", &json!({ "head": "nope", "base": "main" })).unwrap();
    assert_eq!(outcome.state, state);
    assert!(outcome.events.is_empty() && outcome.result.is_none());
    assert_eq!(outcome.failure, Some(Failure::new(ForgeReason::Head)));
    assert_eq!(forge(&state, "nope", &json!({})), Err(OpError::UnknownOp("nope".into())));
    assert_eq!(schedule(&RunnerState::default(), "nope", &json!({}), 0), Err(OpError::UnknownOp("nope".into())));
}
