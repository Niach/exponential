//! The macro template's value language (`catalog/macros.json` `$comment`):
//! `{props.x}` paths with `|fallback`, `len(path)`, `!path`, string
//! interpolation, and the `$map` / `$cond` / `$eq` / `$add` / `$percent` /
//! `$text` / `$coalesce` value objects. Mirrors `src/expr.ts` line by line.
//!
//! JavaScript's `undefined` is `None`; JSON `null` is `Some(Value::Null)`.

use indexmap::IndexMap;
use serde_json::{Map, Value};

use crate::json;
use crate::types::Props;

/// What a template expression can see: the node id, its props, and the
/// `$each` loop variables (`item`, `index`, …).
#[derive(Debug, Clone)]
pub struct ExprContext<'a> {
    pub id: &'a str,
    pub props: &'a Props,
    pub vars: IndexMap<String, Value>,
}

/// `undefined`, `null`, `false` and `""` are false; 0 is TRUE (a count of
/// zero still shows).
pub fn truthy(value: &Value) -> bool {
    !matches!(value, Value::Null | Value::Bool(false)) && value.as_str() != Some("")
}

fn truthy_opt(value: Option<&Value>) -> bool {
    value.is_some_and(truthy)
}

fn is_number_literal(text: &str) -> bool {
    let body = text.strip_prefix('-').unwrap_or(text);
    let (int, frac) = match body.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (body, None),
    };
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    digits(int) && frac.is_none_or(digits)
}

fn parse_literal(text: &str) -> Value {
    match text {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        "null" => Value::Null,
        _ if is_number_literal(text) => json::number(text.parse::<f64>().unwrap_or(0.0)),
        _ => Value::String(text.to_string()),
    }
}

/// JS `cur[key]` over a JSON value.
fn index(cur: &Value, key: &str) -> Option<Value> {
    match cur {
        Value::Object(map) => map.get(key).cloned(),
        Value::Array(items) => {
            if key == "length" {
                return Some(json::number(items.len() as f64));
            }
            key.parse::<usize>().ok().filter(|_| !key.starts_with('+')).and_then(|i| items.get(i).cloned())
        }
        Value::String(s) => {
            if key == "length" {
                return Some(json::number(s.encode_utf16().count() as f64));
            }
            let i = key.parse::<usize>().ok()?;
            let unit = *s.encode_utf16().collect::<Vec<_>>().get(i)?;
            Some(Value::String(String::from_utf16_lossy(&[unit])))
        }
        _ => None,
    }
}

fn lookup(path: &str, ctx: &ExprContext) -> Option<Value> {
    if path == "id" {
        return Some(Value::String(ctx.id.to_string()));
    }
    let mut parts = path.split('.');
    let head = parts.next().unwrap_or("");
    let mut cur = if head == "props" {
        Value::Object(ctx.props.clone())
    } else {
        ctx.vars.get(head)?.clone()
    };
    for key in parts {
        if cur.is_null() {
            return None;
        }
        cur = index(&cur, key)?;
    }
    Some(cur)
}

/// One expression: `path`, `!path`, `len(path)`, `path|fallback`.
pub fn eval_expr(expr: &str, ctx: &ExprContext) -> Option<Value> {
    let text = expr.trim();
    if let Some(bar) = text.find('|') {
        let value = eval_expr(&text[..bar], ctx);
        return match value {
            None | Some(Value::Null) => Some(parse_literal(&text[bar + 1..])),
            some => some,
        };
    }
    if let Some(rest) = text.strip_prefix('!') {
        return Some(Value::Bool(!truthy_opt(eval_expr(rest, ctx).as_ref())));
    }
    if let Some(inner) = text.strip_prefix("len(").and_then(|t| t.strip_suffix(')')) {
        if !inner.is_empty() {
            let len = match eval_expr(inner, ctx) {
                Some(Value::Array(items)) => items.len(),
                Some(Value::String(s)) => s.encode_utf16().count(),
                _ => 0,
            };
            return Some(json::number(len as f64));
        }
    }
    lookup(text, ctx)
}

/// A condition: a string expression or a value object (`$eq` …).
pub fn eval_condition(cond: &Value, ctx: &ExprContext) -> bool {
    match cond {
        Value::String(s) => truthy_opt(eval_expr(s, ctx).as_ref()),
        other => truthy_opt(eval_value(other, ctx).as_ref()),
    }
}

/// `{expr}` occurrences replaced (the TS `/\{([^{}]+)\}/g`).
fn interpolate(value: &str, ctx: &ExprContext) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let end = after.find(['{', '}']);
        match end {
            Some(close) if close > 0 && after.as_bytes()[close] == b'}' => {
                let v = eval_expr(&after[..close], ctx);
                match v {
                    None | Some(Value::Null) => {}
                    Some(v) => out.push_str(&json::to_js_string(&v)),
                }
                rest = &after[close + 1..];
            }
            _ => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The whole-string form `{expr}` (the TS `/^\{([^{}]+)\}$/`).
fn whole(value: &str) -> Option<&str> {
    let inner = value.strip_prefix('{')?.strip_suffix('}')?;
    (!inner.is_empty() && !inner.contains(['{', '}'])).then_some(inner)
}

fn arg(args: &Value, i: usize) -> Option<&Value> {
    args.as_array().and_then(|a| a.get(i))
}

fn eval_opt(value: Option<&Value>, ctx: &ExprContext) -> Option<Value> {
    value.and_then(|v| eval_value(v, ctx))
}

fn to_number_opt(value: &Option<Value>) -> f64 {
    value.as_ref().map(json::to_number).unwrap_or(0.0)
}

/// Any template value: strings interpolate, arrays and objects recurse, the
/// `$`-objects compute. `None` (undefined) results are dropped from objects
/// and arrays; `null` is kept.
pub fn eval_value(value: &Value, ctx: &ExprContext) -> Option<Value> {
    match value {
        Value::String(s) => {
            if let Some(expr) = whole(s) {
                return eval_expr(expr, ctx);
            }
            Some(Value::String(interpolate(s, ctx)))
        }
        Value::Array(items) => Some(Value::Array(items.iter().filter_map(|item| eval_value(item, ctx)).collect())),
        Value::Object(obj) => eval_object(obj, ctx),
        other => Some(other.clone()),
    }
}

fn eval_object(obj: &Map<String, Value>, ctx: &ExprContext) -> Option<Value> {
    if let Some(spec) = obj.get("$map") {
        let from = spec.get("from").and_then(Value::as_str).and_then(|f| eval_expr(f, ctx));
        let key = match &from {
            None | Some(Value::Null) => String::new(),
            Some(v) => json::to_js_string(v),
        };
        let hit = spec.get("cases").and_then(|c| c.get(&key)).or_else(|| spec.get("default"));
        return eval_opt(hit, ctx);
    }
    if let Some(args) = obj.get("$cond") {
        let cond = arg(args, 0).cloned().unwrap_or(Value::Null);
        let pick = if arg(args, 0).is_some() && eval_condition(&cond, ctx) { arg(args, 1) } else { arg(args, 2) };
        return eval_opt(pick, ctx);
    }
    if let Some(args) = obj.get("$eq") {
        let a = eval_opt(arg(args, 0), ctx);
        let b = eval_opt(arg(args, 1), ctx);
        let eq = match (&a, &b) {
            (None, None) => true,
            (Some(a), Some(b)) => json::strict_eq(a, b),
            _ => false,
        };
        return Some(Value::Bool(eq));
    }
    if let Some(args) = obj.get("$add") {
        let a = eval_opt(arg(args, 0), ctx);
        let b = eval_opt(arg(args, 1), ctx);
        return Some(json::number(to_number_opt(&a) + to_number_opt(&b)));
    }
    if let Some(args) = obj.get("$percent") {
        let v = eval_opt(arg(args, 0), ctx);
        let max = eval_opt(arg(args, 1), ctx);
        let m = to_number_opt(&max);
        let pct = if m > 0.0 { to_number_opt(&v) / m * 100.0 } else { 0.0 };
        let clamped = pct.clamp(0.0, 100.0);
        let rounded = (clamped * 100.0).round() / 100.0;
        return Some(Value::String(format!("{}%", json::number_to_string(rounded))));
    }
    if let Some(expr) = obj.get("$text") {
        return match expr.as_str().and_then(|e| eval_expr(e, ctx)) {
            None | Some(Value::Null) => None,
            Some(v) => Some(Value::String(json::to_js_string(&v))),
        };
    }
    if let Some(candidates) = obj.get("$coalesce") {
        for candidate in candidates.as_array().into_iter().flatten() {
            if let Some(v) = eval_value(candidate, ctx) {
                if truthy(&v) {
                    return Some(v);
                }
            }
        }
        return None;
    }
    let mut out = Map::new();
    for (k, v) in obj {
        if let Some(evaluated) = eval_value(v, ctx) {
            out.insert(k.clone(), evaluated);
        }
    }
    Some(Value::Object(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn with_ctx(f: impl FnOnce(&ExprContext)) {
        let props = json!({"gap": "lg", "count": 0, "title": "", "items": ["a", "b"], "selected": "b", "page": 2});
        let props = props.as_object().unwrap().clone();
        let mut vars = IndexMap::new();
        vars.insert("item".to_string(), json!({"value": "b", "label": "Bee"}));
        vars.insert("index".to_string(), json!(1));
        let ctx = ExprContext { id: "n1", props: &props, vars };
        f(&ctx);
    }

    fn ev(v: Value, ctx: &ExprContext) -> Option<Value> {
        eval_value(&v, ctx)
    }

    #[test]
    fn template_values_paths_fallbacks_length_negation_and_the_id() {
        with_ctx(|ctx| {
            assert_eq!(ev(json!("{props.gap}"), ctx), Some(json!("lg")));
            assert_eq!(ev(json!("{props.missing|md}"), ctx), Some(json!("md")));
            assert_eq!(ev(json!("{props.missing|3}"), ctx), Some(json!(3)));
            assert_eq!(ev(json!("{props.count}"), ctx), Some(json!(0)));
            assert_eq!(ev(json!("{len(props.items)}"), ctx), Some(json!(2)));
            assert_eq!(ev(json!("{!props.title}"), ctx), Some(json!(true)));
            assert_eq!(ev(json!("{id}"), ctx), Some(json!("n1")));
            assert_eq!(ev(json!("{item.label} #{index}"), ctx), Some(json!("Bee #1")));
            assert_eq!(ev(json!("$spacing.{props.gap|md}"), ctx), Some(json!("$spacing.lg")));
            assert_eq!(ev(json!("{props.missing}"), ctx), None);
        });
    }

    #[test]
    fn template_values_conditions_0_is_true_empty_string_and_missing_are_false() {
        with_ctx(|ctx| {
            assert!(eval_condition(&json!("props.count"), ctx));
            assert!(!eval_condition(&json!("props.title"), ctx));
            assert!(!eval_condition(&json!("props.missing"), ctx));
            assert!(eval_condition(&json!("!props.missing"), ctx));
            assert!(eval_condition(&json!({"$eq": ["{item.value}", "{props.selected}"]}), ctx));
        });
    }

    #[test]
    fn template_values_the_value_objects() {
        with_ctx(|ctx| {
            assert_eq!(ev(json!({"$map": {"from": "props.gap", "cases": {"lg": 16}, "default": 8}}), ctx), Some(json!(16)));
            assert_eq!(ev(json!({"$map": {"from": "props.missing", "cases": {"lg": 16}, "default": "{props.gap}"}}), ctx), Some(json!("lg")));
            assert_eq!(ev(json!({"$cond": ["props.title", "yes", "no"]}), ctx), Some(json!("no")));
            assert_eq!(ev(json!({"$add": ["{props.page}", -1]}), ctx), Some(json!(1)));
            assert_eq!(ev(json!({"$percent": ["{props.page}", 8]}), ctx), Some(json!("25%")));
            assert_eq!(ev(json!({"$percent": [150, 100]}), ctx), Some(json!("100%")));
            assert_eq!(ev(json!({"$percent": [1, 3]}), ctx), Some(json!("33.33%")));
            assert_eq!(ev(json!({"$coalesce": ["{props.title}", "{props.missing}", "Choose"]}), ctx), Some(json!("Choose")));
            assert_eq!(ev(json!({"$text": "props.count"}), ctx), Some(json!("0")));
            assert_eq!(ev(json!({"$text": "props.missing"}), ctx), None);
        });
    }

    #[test]
    fn template_values_objects_drop_undefined_members_and_arrays_drop_undefined_items() {
        with_ctx(|ctx| {
            assert_eq!(ev(json!({"a": "{props.missing}", "b": "{props.gap}"}), ctx), Some(json!({"b": "lg"})));
            assert_eq!(ev(json!(["{props.missing}", "{props.gap}"]), ctx), Some(json!(["lg"])));
        });
    }

    #[test]
    fn interpolation_keeps_unmatched_braces() {
        with_ctx(|ctx| {
            assert_eq!(ev(json!("a{}b{{props.gap}}"), ctx), Some(json!("a{}b{lg}")));
            assert_eq!(ev(json!("{props.missing}x{"), ctx), Some(json!("x{")));
        });
    }
}
