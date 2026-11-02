//! VAPP-191: `fixtures/expressions.json` replayed through the Rust core
//! (the TS reference replays it in `src/cel.test.ts`).

mod support;

use eui_core::cel::{call_expression_function, check_expression, evaluate_expression, parse_timestamp, ExpressionEnv, MAX_DEPTH, MAX_LENGTH, MAX_STEPS};
use eui_core::data::{resolve_value, ResolveContext};
use eui_core::format::{Formatter, ZonedEnglishFormatter};
use eui_core::host::validate_package;
use eui_core::json;
use serde_json::{json, Map, Value};
use support::{first_diff, fixture};

/// A fixture zone: `offset` until the first transition, then each one's.
fn zone(z: &Value) -> impl Fn(f64) -> i32 + Send + Sync + 'static {
    let base = z["offset"].as_i64().unwrap() as i32;
    let steps: Vec<(f64, i32)> = z["transitions"].as_array().unwrap().iter().map(|t| (parse_timestamp(t[0].as_str().unwrap()).unwrap(), t[1].as_i64().unwrap() as i32)).collect();
    move |ms| steps.iter().filter(|(at, _)| ms >= *at).last().map_or(base, |(_, o)| *o)
}

fn check(name: &str, got: &Value, want: &Value, failures: &mut Vec<String>) {
    if !json::equal(got, want) {
        failures.push(format!("{name}: {}", first_diff(&json::canonical(got), &json::canonical(want), "").unwrap_or_else(|| format!("{got} != {want}"))));
    }
}

#[test]
fn expressions_fixture() {
    let f = fixture("expressions.json");
    assert_eq!(f["limits"], json!({"maxLength": MAX_LENGTH, "maxDepth": MAX_DEPTH, "maxSteps": MAX_STEPS}));
    let vars = f["vars"].as_object().unwrap().clone();
    let mut failures = Vec::new();
    let mut n = 0;
    for c in f["eval"].as_array().unwrap() {
        let name = format!("eval: {}", c["name"].as_str().unwrap());
        let now = parse_timestamp(c.get("now").unwrap_or(&f["env"]["now"]).as_str().unwrap());
        let offset = zone(&f["zones"][c.get("zone").unwrap_or(&f["env"]["zone"]).as_str().unwrap()]);
        let env = ExpressionEnv { now, offset: Some(&offset), max_steps: c.get("maxSteps").and_then(Value::as_u64).map(|s| s as usize) };
        let got = match evaluate_expression(c["expr"].as_str().unwrap(), &vars, &env) {
            Ok(v) => json!({ "value": v }),
            Err(e) => json!({ "error": e }),
        };
        let want = match c.get("error") {
            Some(e) => json!({ "error": e }),
            None => json!({ "value": c["value"] }),
        };
        check(&name, &got, &want, &mut failures);
        n += 1;
    }
    let types = f["checkTypes"].as_object().unwrap().clone();
    for c in f["check"].as_array().unwrap() {
        let got = match check_expression(c["expr"].as_str().unwrap(), &types) {
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
    // The A2UI form at bind time, through the surface's resolve path: the
    // clock and the zone the formatter carries.
    let data = f["callData"].clone();
    let data_vars: Map<String, Value> = data.as_object().unwrap().clone();
    let now = parse_timestamp(f["env"]["now"].as_str().unwrap());
    let formatter = ZonedEnglishFormatter::new(std::sync::Arc::new(zone(&f["zones"][f["env"]["zone"].as_str().unwrap()])));
    for c in f["calls"].as_array().unwrap() {
        let name = format!("call: {}", c["name"].as_str().unwrap());
        let mut ctx = ResolveContext::new(&data, "");
        ctx.formatter = Some(&formatter as &dyn Formatter);
        ctx.now = now;
        let got = resolve_value(&c["call"], &ctx);
        let none = c.get("none").and_then(Value::as_bool) == Some(true);
        match (&got, none) {
            (None, true) => {}
            (Some(v), false) => check(&name, v, &c["value"], &mut failures),
            _ => failures.push(format!("{name}: got {got:?}")),
        }
        if let Some(cel) = c.get("cel").and_then(Value::as_str) {
            let offset = |ms: f64| formatter.offset_minutes(ms);
            let env = ExpressionEnv { now, offset: Some(&offset), max_steps: None };
            match (evaluate_expression(cel, &data_vars, &env), none) {
                (Err(_), true) => {}
                (Ok(v), false) => check(&format!("{name} (cel)"), &v, &c["value"], &mut failures),
                (r, _) => failures.push(format!("{name} (cel): {r:?}")),
            }
        }
        n += 1;
    }
    for c in f["packages"].as_array().unwrap() {
        let issues = validate_package(&c["package"], &eui_core::host::supported_catalog_ids::<&str>(&[]));
        check(&format!("package: {}", c["name"].as_str().unwrap()), &serde_json::to_value(&issues).unwrap(), &c["issues"], &mut failures);
        n += 1;
    }
    assert!(failures.is_empty(), "{} of {n} cases failed:\n{}", failures.len(), failures.join("\n"));
    assert!(n > 250);
}

#[test]
fn call_expression_function_outside_the_table() {
    assert_eq!(call_expression_function("percent", &Map::new(), &ExpressionEnv::default()), None);
    assert_eq!(call_expression_function("size", &Map::new(), &ExpressionEnv::default()), None);
    let big: Vec<Value> = (0..5000).map(|i| json!(i)).collect();
    let mut vars = Map::new();
    vars.insert("xs".into(), Value::Array(big));
    assert_eq!(evaluate_expression("xs.map(x, x * 2)", &vars, &ExpressionEnv::default()), Err("step limit exceeded".into()));
}
