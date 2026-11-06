//! VAPP-194: `fixtures/actions.json` replayed through the Rust core (the TS
//! reference replays it in `src/host/actions.test.ts`): the effect table,
//! the parser's refusals, package validation, runs and the batch.

mod support;

use eui_core::cel::{check_expression, function_effect, parse_timestamp};
use eui_core::host::{self, apply_record_batch, run_action, validate_package, ActionContext};
use eui_core::json;
use serde_json::{json, Map, Value};
use support::{first_diff, fixture, read};

fn check(name: &str, got: &Value, want: &Value, failures: &mut Vec<String>) {
    if !json::equal(got, want) {
        failures.push(format!("{name}: {}", first_diff(&json::canonical(got), &json::canonical(want), "").unwrap_or_else(|| format!("{got} != {want}"))));
    } else if serde_json::to_string(got).unwrap() != serde_json::to_string(want).unwrap() {
        // The plan and the rows keep the TS key order too.
        failures.push(format!("{name}: key order {got} != {want}"));
    }
}

/// A validate case's package: the base with its `with` keys replaced
/// (`addActions` = actions added to the base's).
fn case_package(base: &Value, with: &Value) -> Value {
    let mut pkg = base.as_object().unwrap().clone();
    for (k, v) in with.as_object().unwrap() {
        if k == "addActions" {
            continue;
        }
        pkg.insert(k.clone(), v.clone());
    }
    if let Some(add) = with.get("addActions").and_then(Value::as_object) {
        let mut actions = base["actions"].as_object().cloned().unwrap_or_default();
        for (k, v) in add {
            actions.insert(k.clone(), v.clone());
        }
        pkg.insert("actions".into(), Value::Object(actions));
    }
    Value::Object(pkg)
}

#[test]
fn actions_fixture() {
    let f = fixture("actions.json");
    assert_eq!(f["limits"], json!({"actions": host::ACTION_LIMIT_ACTIONS, "steps": host::ACTION_LIMIT_STEPS, "maxSteps": host::ACTION_LIMIT_MAX_STEPS}));
    let contract = read("catalog/host.json");
    let actions = &contract["package"]["actions"];
    assert_eq!(actions["limits"], f["limits"]);
    assert_eq!(actions["stepKinds"], json!(host::ACTION_STEP_KINDS));
    assert_eq!(actions["namePattern"], json!(host::ACTION_NAME_PATTERN));
    assert_eq!(actions["callPrefix"], json!(host::ACTION_CALL_PREFIX));
    assert_eq!(actions["batchFunction"], json!(host::RECORDS_BATCH_FUNCTION));
    assert_eq!(actions["resultPattern"], json!(host::ACTION_RESULT_PATTERN));

    let mut failures = Vec::new();
    let mut n = 0;
    for c in f["effects"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        check(&format!("effect: {name}"), &json!(function_effect(name).as_str()), &c["effect"], &mut failures);
        n += 1;
    }
    for c in f["check"].as_array().unwrap() {
        let got = match check_expression(c["expr"].as_str().unwrap(), &Map::new()) {
            Ok(t) => json!({ "type": t }),
            Err(e) => json!({ "error": e }),
        };
        let want = match c.get("error") {
            Some(e) => json!({ "error": e }),
            None => json!({ "type": c["type"] }),
        };
        check(&format!("check: {}", c["name"].as_str().unwrap()), &got, &want, &mut failures);
        n += 1;
    }
    let base = &f["package"];
    for c in f["validate"].as_array().unwrap() {
        let issues = validate_package(&case_package(base, &c["with"]), &host::supported_catalog_ids::<&str>(&[]));
        check(&format!("validate: {}", c["name"].as_str().unwrap()), &serde_json::to_value(&issues).unwrap(), &c["issues"], &mut failures);
        n += 1;
    }
    let now = parse_timestamp(f["env"]["now"].as_str().unwrap());
    let offset_minutes = f["env"]["offset"].as_i64().unwrap() as i32;
    let offset = move |_: f64| offset_minutes;
    for c in f["run"].as_array().unwrap() {
        let ctx = ActionContext { data: c.get("data"), me: c.get("me").and_then(Value::as_str), now, offset: Some(&offset), max_steps: c.get("maxSteps").and_then(Value::as_u64).map(|s| s as usize), results: None };
        let got = match run_action(base, c["action"].as_str().unwrap(), c.get("params").unwrap_or(&Value::Null), &ctx) {
            Ok(run) => run.to_value(),
            Err(e) => json!({ "error": e }),
        };
        let want = match c.get("error") {
            Some(e) => json!({ "error": e }),
            None => json!({ "plan": c["plan"] }),
        };
        check(&format!("run: {}", c["name"].as_str().unwrap()), &got, &want, &mut failures);
        n += 1;
    }
    // VAPP-271: a call's answer (`result`) feeds the later steps.
    let r = &f["results"];
    let rpkg = &r["package"];
    for c in r["validate"].as_array().unwrap() {
        let issues = validate_package(&case_package(rpkg, &c["with"]), &host::supported_catalog_ids::<&str>(&[]));
        check(&format!("results validate: {}", c["name"].as_str().unwrap()), &serde_json::to_value(&issues).unwrap(), &c["issues"], &mut failures);
        n += 1;
    }
    for c in r["run"].as_array().unwrap() {
        let ctx = ActionContext { data: c.get("data"), me: None, now, offset: Some(&offset), max_steps: None, results: c.get("results").and_then(Value::as_object) };
        let got = match run_action(rpkg, c["action"].as_str().unwrap(), c.get("params").unwrap_or(&Value::Null), &ctx) {
            Ok(run) => run.to_value(),
            Err(e) => json!({ "error": e }),
        };
        let want = if let Some(e) = c.get("error") {
            json!({ "error": e })
        } else if let Some(p) = c.get("pending") {
            json!({ "pending": p })
        } else {
            json!({ "plan": c["plan"] })
        };
        check(&format!("results run: {}", c["name"].as_str().unwrap()), &got, &want, &mut failures);
        n += 1;
    }
    let b = &f["batch"];
    let new_ids: Vec<String> = b["newIds"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
    for c in b["cases"].as_array().unwrap() {
        let got = match apply_record_batch(b["rows"].as_array().unwrap(), c["writes"].as_array().unwrap(), Some(&b["records"]), b["me"].as_str(), b["now"].as_str().unwrap(), |i| new_ids[i].clone()) {
            Ok((rows, ids)) => json!({ "rows": rows, "ids": ids }),
            Err(e) => json!({ "error": e }),
        };
        let want = match c.get("error") {
            Some(e) => json!({ "error": e }),
            None => json!({ "rows": c["rows"], "ids": c["ids"] }),
        };
        check(&format!("batch: {}", c["name"].as_str().unwrap()), &got, &want, &mut failures);
        n += 1;
    }
    assert!(failures.is_empty(), "{} of {n} cases failed:\n{}", failures.len(), failures.join("\n"));
    assert!(n >= 60, "{n}");
}
