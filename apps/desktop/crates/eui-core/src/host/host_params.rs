//! VAPP-206 (spec docs/spec/apps.md §4.1): the HOST-PROVIDED binding params,
//! the twin of `packages/exponential-ui/src/host/host-params.ts` (read its
//! header). `${today}`, `${now}` and `${month}` in the team zone, read at each
//! bind; every host adds them to its `applyTemplate` and `bindDataModel`
//! messages (`hostParams`) and the pure router fills them.

use serde_json::{json, Map, Value};

use super::sources::encode_component;
use crate::cel::{evaluate_expression, ExpressionEnv};

/// The params at `now` (epoch ms; `None` = the wall clock) in the team zone
/// (minutes east of UTC at an instant; `None` = UTC).
pub fn host_params(now: Option<f64>, offset: Option<&(dyn Fn(f64) -> i32 + Send + Sync)>) -> Value {
    let env = ExpressionEnv { now, offset, max_steps: None };
    let vars = Map::new();
    let read = |expr: &str| evaluate_expression(expr, &vars, &env).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
    let today = read("today()");
    let month: String = today.chars().take(7).collect();
    json!({"today": today, "now": read("now()"), "month": month})
}

/// A source's `${today}`, `${now}`, `${month}` from the host's params
/// (percent-encoded like a query value). Without them it stays as written.
pub fn fill_host_params(source: &str, params: Option<&Value>) -> String {
    let mut out = source.to_string();
    let Some(params) = params.and_then(Value::as_object) else { return out };
    for name in ["today", "now", "month"] {
        if let Some(v) = params.get(name).and_then(Value::as_str) {
            out = out.replace(&format!("${{{name}}}"), &encode_component(v));
        }
    }
    out
}

/// The message with `hostParams` in its `applyTemplate` or `bindDataModel`
/// body; any other message as it is.
pub fn with_host_params(message: &Value, params: &Value) -> Value {
    let mut out = message.clone();
    for kind in ["applyTemplate", "bindDataModel"] {
        if let Some(body) = out.get_mut(kind).and_then(Value::as_object_mut) {
            body.insert("hostParams".into(), params.clone());
            return out;
        }
    }
    out
}
