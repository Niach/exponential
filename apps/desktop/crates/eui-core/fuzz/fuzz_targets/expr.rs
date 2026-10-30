//! The expression language: an arbitrary expression string against
//! arbitrary props and vars, plus `eval_value` / `eval_condition` on
//! arbitrary JSON. Never a panic.
#![no_main]

use arbitrary::Arbitrary;
use eui_core::expr::{eval_condition, eval_expr, eval_value, ExprContext};
use libfuzzer_sys::fuzz_target;

#[derive(Arbitrary, Debug)]
struct Input {
    expr: String,
    props: String,
    value: String,
}

fuzz_target!(|input: Input| {
    let props = serde_json::from_str::<serde_json::Value>(&input.props).ok().and_then(|v| v.as_object().cloned()).unwrap_or_default();
    let mut vars = indexmap_like(&props);
    vars.insert("item".to_string(), serde_json::Value::String(input.expr.clone()));
    let ctx = ExprContext { id: "fuzz", props: &props, vars };
    let _ = eval_expr(&input.expr, &ctx);
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&input.value) {
        let _ = eval_value(&value, &ctx);
        let _ = eval_condition(&value, &ctx);
    }
});

fn indexmap_like(props: &serde_json::Map<String, serde_json::Value>) -> indexmap::IndexMap<String, serde_json::Value> {
    props.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
}
