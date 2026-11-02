//! VAPP-191: `fixtures/expressions.json` replayed by the Rust core's
//! `expressions` module with the structure of
//! `packages/exponential-ui/src/expressions.test.ts`: the tables equal the
//! fixture's (copied from `catalog/expressions.json`), every eval, cost,
//! check and call-form case equals the TS reference.

mod support;

use eui_core::expressions::{self as x, ExpressionClock};
use serde_json::{json, Map, Value};
use support::fixture;

fn clock_of(c: &Value) -> ExpressionClock {
    let now = eui_core::format::parse_date_value(&c["now"]).unwrap().ms;
    ExpressionClock::fixed(now, c["offsetMinutes"].as_i64().unwrap() as i32)
}

fn obj(v: &Value) -> Map<String, Value> {
    v.as_object().cloned().unwrap_or_default()
}

#[test]
fn the_tables_are_the_fixtures() {
    let f = fixture("expressions.json");
    let tables = x::tables_json();
    for key in ["limits", "functions", "methods", "comprehensions", "reserved", "rowKeys"] {
        assert!(eui_core::json::equal(&tables[key], &f[key]), "{key}: {} != {}", tables[key], f[key]);
    }
    let with_call: Vec<&str> = x::FUNCTIONS.iter().filter(|(_, s)| s.call.is_some()).map(|(n, _)| *n).chain(["expr"]).collect();
    assert_eq!(with_call, x::CALL_NAMES);
    assert_eq!(eui_core::host::RECORDS_ROW_KEYS, x::ROW_KEYS);
    for name in x::CALL_NAMES {
        assert!(eui_core::generated::catalog::BIND_FUNCTION_NAMES.contains(name), "{name} is a bind function");
    }
}

#[test]
fn every_eval_and_cost_case_equals_the_reference() {
    let f = fixture("expressions.json");
    let mut failed = Vec::new();
    let mut n = 0;
    for key in ["evalCases", "costCases"] {
        for c in f[key].as_array().unwrap() {
            n += 1;
            let vars = obj(c.get("vars").unwrap_or(&f["vars"]));
            let got = x::eval_json(c["source"].as_str().unwrap(), &vars, &clock_of(c.get("clock").unwrap_or(&f["clock"])));
            if !eui_core::json::equal(&got, &c["expected"]) {
                failed.push(format!("{key} {}: got {got}, want {}", c["name"], c["expected"]));
            }
        }
    }
    assert!(n > 200);
    assert!(failed.is_empty(), "{} failed:\n{}", failed.len(), failed.join("\n"));
}

#[test]
fn every_check_case_equals_the_reference() {
    let f = fixture("expressions.json");
    let kinds = obj(&f["kinds"]);
    let mut failed = Vec::new();
    for c in f["checkCases"].as_array().unwrap() {
        let (ty, issues) = x::check_expression(c["source"].as_str().unwrap(), &obj(&c["types"]), &kinds);
        let got = json!({"type": ty, "issues": issues});
        if got != c["expected"] {
            failed.push(format!("{}: got {got}, want {}", c["name"], c["expected"]));
        }
    }
    assert!(failed.is_empty(), "{} failed:\n{}", failed.len(), failed.join("\n"));
}

#[test]
fn every_call_case_equals_the_reference() {
    let f = fixture("expressions.json");
    let mut failed = Vec::new();
    for c in f["callCases"].as_array().unwrap() {
        let got = match x::call_expression_function(c["call"].as_str().unwrap(), &obj(&c["args"]), &clock_of(c.get("clock").unwrap_or(&f["clock"]))) {
            Some(v) => json!({ "value": v }),
            None => json!({}),
        };
        if !eui_core::json::equal(&got, &c["expected"]) {
            failed.push(format!("{}: got {got}, want {}", c["name"], c["expected"]));
        }
    }
    assert!(failed.is_empty(), "{} failed:\n{}", failed.len(), failed.join("\n"));
}

#[test]
fn the_bind_pass_runs_the_call_form_on_the_context_clock_and_zone() {
    use eui_core::data::{resolve_value, ResolveContext};
    let data = json!({"n": 2, "h": "7.5"});
    let now = eui_core::format::parse_date_value(&json!("2026-11-02T23:30:00Z")).unwrap().ms;
    let zone: eui_core::format::ZoneOffset = std::sync::Arc::new(|_| 60);
    let call = json!({"call": "expr", "args": {"source": "dateAdd(today(), n, 'day')", "vars": {"n": {"path": "/n"}}}});
    let ctx = ResolveContext::new(&data, "").with_now(now);
    assert_eq!(resolve_value(&call, &ctx.with_zone(&zone)), Some(json!("2026-11-05")));
    assert_eq!(resolve_value(&call, &ctx), Some(json!("2026-11-04")));
    assert_eq!(resolve_value(&json!({"call": "mul", "args": {"a": {"path": "/h"}, "b": 60}}), &ctx), Some(json!(450)));
}
