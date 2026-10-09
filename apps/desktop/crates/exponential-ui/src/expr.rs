//! The macro template's value language (`catalog/macros.json` `$comment`):
//! `{props.x}` paths with `|fallback`, `len(path)`, `range(n)`, `!path`,
//! string interpolation, and the `$map` / `$cond` / `$eq` / `$lt` / `$add` /
//! `$sub` / `$clamp` / `$percent` / `$coalesce` / `$text` / `$not` / `$fill`
//! value objects. Mirrors `src/expr.ts` line by line.
//!
//! Round 1 (docs/round-1-contract.md §1): an input may be an A2UI DYNAMIC
//! value — a data binding `{path}` or a function call `{call, args}` — which
//! has no value at expansion time. The evaluator then EMITS a call over the
//! core functions that the renderer evaluates at bind time (`data::bind_tree`)
//! instead of computing. A whole-string `{props.x}` that is a MEMBER passes a
//! dynamic value through unchanged (two-way binding survives); as an OPERAND
//! of a `$`-object a `|fallback` is kept as `fallback{value, default}`.
//! `$fill` ALWAYS emits `fill` (its template is a `$string.<id>` the
//! surface's string table resolves at bind time).
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

/// An A2UI data binding: an object with exactly one key, `path` (a string).
pub fn is_binding(value: &Value) -> bool {
    value.as_object().is_some_and(|o| o.len() == 1 && o.get("path").is_some_and(Value::is_string))
}

/// An A2UI function call: an object with a string `call`.
pub fn is_call(value: &Value) -> bool {
    value.as_object().is_some_and(|o| o.get("call").is_some_and(Value::is_string))
}

/// A value the expander cannot evaluate: the renderer will.
pub fn is_dynamic(value: &Value) -> bool {
    is_binding(value) || is_call(value)
}

fn is_dynamic_opt(value: &Option<Value>) -> bool {
    value.as_ref().is_some_and(is_dynamic)
}

/// `undefined`, `null`, `false` and `""` are false; 0 is TRUE (a count of
/// zero still shows).
pub fn truthy(value: &Value) -> bool {
    !matches!(value, Value::Null | Value::Bool(false)) && value.as_str() != Some("")
}

/// [`truthy`] over an optional (`None` = undefined = false).
pub fn truthy_opt(value: Option<&Value>) -> bool {
    value.is_some_and(truthy)
}

/// `{call: name, args}` with undefined args dropped (the TS `call`).
pub fn call(name: &str, args: Vec<(&str, Option<Value>)>) -> Value {
    let mut out = Map::new();
    for (k, v) in args {
        if let Some(v) = v {
            out.insert(k.to_string(), v);
        }
    }
    let mut c = Map::new();
    c.insert("call".into(), Value::String(name.to_string()));
    c.insert("args".into(), Value::Object(out));
    Value::Object(c)
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

/// The TS `toNumber`: numbers stay, everything else `Number(v ?? 0) || 0`.
pub fn to_number(value: Option<&Value>) -> f64 {
    match value {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(v) => {
            let n = json::to_number(v);
            if n.is_nan() {
                0.0
            } else {
                n
            }
        }
        None => 0.0,
    }
}

/// A string that spells a finite number (`"5"`, `" 2.5 "`), else `None`.
fn numeric_string(value: &str) -> Option<f64> {
    let text = value.trim();
    if text.is_empty() {
        return None;
    }
    text.parse::<f64>().ok().filter(|n| n.is_finite())
}

/// `eq` (and `$eq`): strict equality, except that a NUMBER and a string
/// spelling that number are equal (`5` vs `"5"`: form and URL state arrive as
/// strings). Nothing else coerces (`0` vs `""`, `1` vs `true` differ).
pub fn values_equal(a: Option<&Value>, b: Option<&Value>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(Value::Number(n)), Some(Value::String(s))) | (Some(Value::String(s)), Some(Value::Number(n))) => numeric_string(s) == n.as_f64(),
        (Some(a), Some(b)) => json::strict_eq(a, b),
        _ => false,
    }
}

/// `clamp`: the value as a number held inside [min, max]; a missing bound
/// (undefined or null) does not clamp; `min` wins when max < min.
pub fn clamp_number(value: Option<&Value>, min: Option<&Value>, max: Option<&Value>) -> f64 {
    let mut n = to_number(value);
    if let Some(max) = max.filter(|m| !m.is_null()) {
        n = n.min(to_number(Some(max)));
    }
    if let Some(min) = min.filter(|m| !m.is_null()) {
        n = n.max(to_number(Some(min)));
    }
    n
}

/// `fill`: the template (already resolved through the string table) with its
/// `{name}` placeholders filled from params (`strings::format_string`).
pub fn fill_template(template: Option<&Value>, params: Option<&Value>) -> String {
    let text = match template {
        None | Some(Value::Null) => String::new(),
        Some(v) => json::to_js_string(v),
    };
    let empty = Map::new();
    let params = params.and_then(Value::as_object).unwrap_or(&empty);
    crate::strings::format_string(&text, params)
}

/// UTF-16 length (JS `.length`).
fn js_len(value: Option<&Value>) -> usize {
    match value {
        Some(Value::Array(items)) => items.len(),
        Some(Value::String(s)) => s.encode_utf16().count(),
        _ => 0,
    }
}

/// `^(len|range)\((.+)\)$`
fn function_form(text: &str) -> Option<(&str, &str)> {
    for name in ["len", "range"] {
        if let Some(rest) = text.strip_prefix(name).and_then(|r| r.strip_prefix('(')).and_then(|r| r.strip_suffix(')')) {
            if !rest.is_empty() && !rest.contains('\n') {
                return Some((name, rest));
            }
        }
    }
    None
}

/// One expression: `len(path)`, `range(n)`, `path|fallback`, `!path`, `path`.
/// The function forms wrap the whole expression and are tried first, so a
/// fallback inside their parentheses stays theirs.
pub fn eval_expr(expr: &str, ctx: &ExprContext) -> Option<Value> {
    let text = expr.trim();
    if let Some((name, inner)) = function_form(text) {
        let inner = eval_expr(inner, ctx);
        if name == "len" {
            if is_dynamic_opt(&inner) {
                return Some(call("len", vec![("value", inner)]));
            }
            return Some(json::number(js_len(inner.as_ref()) as f64));
        }
        // range: the indexes 0..n-1; a bound count cannot be expanded (→ no items).
        if is_dynamic_opt(&inner) {
            return None;
        }
        let n = to_number(inner.as_ref()).floor().max(0.0) as usize;
        return Some(Value::Array((0..n).map(|i| json::number(i as f64)).collect()));
    }
    if let Some(bar) = text.find('|') {
        let value = eval_expr(&text[..bar], ctx);
        let fallback = parse_literal(&text[bar + 1..]);
        return match value {
            None | Some(Value::Null) => Some(fallback),
            Some(v) if is_dynamic(&v) => Some(call("fallback", vec![("value", Some(v)), ("default", Some(fallback))])),
            some => some,
        };
    }
    if let Some(rest) = text.strip_prefix('!') {
        let value = eval_expr(rest, ctx);
        if is_dynamic_opt(&value) {
            return Some(call("not", vec![("value", value)]));
        }
        return Some(Value::Bool(!truthy_opt(value.as_ref())));
    }
    lookup(text, ctx)
}

/// The member rule: a dynamic value passes through untouched; the
/// `fallback` a `|fallback` wrapped it in is unwrapped again.
fn pass_through(value: Option<Value>) -> Option<Value> {
    if let Some(v) = &value {
        if is_call(v) && v.get("call").and_then(Value::as_str) == Some("fallback") {
            if let Some(inner) = v.get("args").and_then(|a| a.get("value")).filter(|x| is_dynamic(x)) {
                return Some(inner.clone());
            }
        }
    }
    value
}

/// An operand of a `$`-object (or its result): a whole string evaluates as
/// its expression, keeping a `|fallback` as `fallback{…}` when dynamic.
fn eval_operand(value: Option<&Value>, ctx: &ExprContext) -> Option<Value> {
    let value = value?;
    if let Value::String(s) = value {
        if let Some(expr) = whole(s) {
            return eval_expr(expr, ctx);
        }
    }
    eval_value(value, ctx)
}

/// A condition decided now (`Bool`) or the dynamic value it depends on.
#[derive(Debug, Clone, PartialEq)]
pub enum Decided {
    Bool(bool),
    Dynamic(Value),
}

/// A condition: a string expression or a value object (`$eq` …). `Bool`
/// when it can be decided now; the dynamic value when it cannot.
pub fn eval_condition_value(cond: Option<&Value>, ctx: &ExprContext) -> Decided {
    let value = match cond {
        Some(Value::String(s)) => eval_expr(s, ctx),
        other => eval_operand(other, ctx),
    };
    match value {
        Some(v) if is_dynamic(&v) => Decided::Dynamic(v),
        other => Decided::Bool(truthy_opt(other.as_ref())),
    }
}

/// A condition decided NOW: a dynamic condition counts as true (the part
/// exists; `visible` carries the bound test, see `macros`).
pub fn eval_condition(cond: &Value, ctx: &ExprContext) -> bool {
    !matches!(eval_condition_value(Some(cond), ctx), Decided::Bool(false))
}

/// `{expr}` occurrences replaced (the TS `/\{([^{}]+)\}/g`); a dynamic piece
/// turns the whole string into `concat{values}`.
fn interpolate(value: &str, ctx: &ExprContext) -> Value {
    let mut parts: Vec<Option<Value>> = Vec::new();
    let mut literal = String::new();
    let mut dynamic = false;
    let mut rest = value;
    while let Some(open) = rest.find('{') {
        literal.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find(['{', '}']) {
            Some(close) if close > 0 && after.as_bytes()[close] == b'}' => {
                if !literal.is_empty() {
                    parts.push(Some(Value::String(std::mem::take(&mut literal))));
                }
                let v = eval_expr(&after[..close], ctx);
                dynamic |= is_dynamic_opt(&v);
                parts.push(v);
                rest = &after[close + 1..];
            }
            _ => {
                literal.push('{');
                rest = after;
            }
        }
    }
    literal.push_str(rest);
    if !literal.is_empty() {
        parts.push(Some(Value::String(literal)));
    }
    if dynamic {
        let values: Vec<Value> = parts.into_iter().flatten().filter(|p| !p.is_null() && p.as_str() != Some("")).collect();
        return call("concat", vec![("values", Some(Value::Array(values)))]);
    }
    let mut out = String::new();
    for p in parts.iter().flatten() {
        if !p.is_null() {
            out.push_str(&json::to_js_string(p));
        }
    }
    Value::String(out)
}

/// The whole-string form `{expr}` (the TS `/^\{([^{}]+)\}$/`).
fn whole(value: &str) -> Option<&str> {
    let inner = value.strip_prefix('{')?.strip_suffix('}')?;
    (!inner.is_empty() && !inner.contains(['{', '}'])).then_some(inner)
}

fn arg(args: &Value, i: usize) -> Option<&Value> {
    args.as_array().and_then(|a| a.get(i))
}

/// The operands of `$eq`/`$lt`/`$add`/`$sub`/`$percent` (the TS maps over
/// the array it finds, so a short array leaves the rest undefined).
fn pair(args: &Value, ctx: &ExprContext) -> (Option<Value>, Option<Value>) {
    (eval_operand(arg(args, 0), ctx), eval_operand(arg(args, 1), ctx))
}


/// Any template value: strings interpolate, arrays and objects recurse, the
/// `$`-objects compute (or emit a call when an input is dynamic). `None`
/// (undefined) results are dropped from objects and arrays; `null` is kept.
pub fn eval_value(value: &Value, ctx: &ExprContext) -> Option<Value> {
    match value {
        Value::String(s) => {
            if let Some(expr) = whole(s) {
                return pass_through(eval_expr(expr, ctx));
            }
            Some(interpolate(s, ctx))
        }
        Value::Array(items) => Some(Value::Array(items.iter().filter_map(|item| eval_value(item, ctx)).collect())),
        Value::Object(obj) => eval_object(obj, ctx),
        other => Some(other.clone()),
    }
}

fn eval_object(obj: &Map<String, Value>, ctx: &ExprContext) -> Option<Value> {
    if let Some(spec) = obj.get("$map") {
        let from = spec.get("from").and_then(Value::as_str).and_then(|f| eval_expr(f, ctx));
        let empty = Map::new();
        let cases = spec.get("cases").and_then(Value::as_object).unwrap_or(&empty);
        if is_dynamic_opt(&from) {
            let mut out = Map::new();
            for (k, v) in cases {
                if let Some(evaluated) = eval_operand(Some(v), ctx) {
                    out.insert(k.clone(), evaluated);
                }
            }
            let default = eval_operand(spec.get("default"), ctx);
            return Some(call("map", vec![("value", from), ("cases", Some(Value::Object(out))), ("default", default)]));
        }
        let key = match &from {
            None | Some(Value::Null) => String::new(),
            Some(v) => json::to_js_string(v),
        };
        let hit = cases.get(&key).or_else(|| spec.get("default"));
        return eval_operand(hit, ctx);
    }
    if let Some(args) = obj.get("$cond") {
        let decided = eval_condition_value(arg(args, 0), ctx);
        return match decided {
            Decided::Dynamic(test) => {
                Some(call("cond", vec![("if", Some(test)), ("then", eval_operand(arg(args, 1), ctx)), ("else", eval_operand(arg(args, 2), ctx))]))
            }
            Decided::Bool(true) => eval_operand(arg(args, 1), ctx),
            Decided::Bool(false) => eval_operand(arg(args, 2), ctx),
        };
    }
    if let Some(args) = obj.get("$eq") {
        let (a, b) = pair(args, ctx);
        if is_dynamic_opt(&a) || is_dynamic_opt(&b) {
            return Some(call("eq", vec![("a", a), ("b", b)]));
        }
        return Some(Value::Bool(values_equal(a.as_ref(), b.as_ref())));
    }
    if let Some(args) = obj.get("$lt") {
        let (a, b) = pair(args, ctx);
        if is_dynamic_opt(&a) || is_dynamic_opt(&b) {
            return Some(call("lt", vec![("a", a), ("b", b)]));
        }
        return Some(Value::Bool(to_number(a.as_ref()) < to_number(b.as_ref())));
    }
    if let Some(args) = obj.get("$add") {
        let (a, b) = pair(args, ctx);
        if is_dynamic_opt(&a) || is_dynamic_opt(&b) {
            return Some(call("add", vec![("a", a), ("b", b)]));
        }
        return Some(json::number(to_number(a.as_ref()) + to_number(b.as_ref())));
    }
    if let Some(args) = obj.get("$sub") {
        let (a, b) = pair(args, ctx);
        if is_dynamic_opt(&a) || is_dynamic_opt(&b) {
            return Some(call("sub", vec![("a", a), ("b", b)]));
        }
        return Some(json::number(to_number(a.as_ref()) - to_number(b.as_ref())));
    }
    if let Some(args) = obj.get("$clamp") {
        let (v, min, max) = (eval_operand(arg(args, 0), ctx), eval_operand(arg(args, 1), ctx), eval_operand(arg(args, 2), ctx));
        if is_dynamic_opt(&v) || is_dynamic_opt(&min) || is_dynamic_opt(&max) {
            return Some(call("clamp", vec![("value", v), ("min", min), ("max", max)]));
        }
        return Some(json::number(clamp_number(v.as_ref(), min.as_ref(), max.as_ref())));
    }
    if let Some(args) = obj.get("$fill") {
        // Always emitted: the template is (usually) a `$string.<id>` only the
        // surface's string table can resolve, at bind time.
        let mut filled = Map::new();
        if let Some(params) = arg(args, 1).and_then(Value::as_object) {
            for (k, v) in params {
                if let Some(evaluated) = eval_operand(Some(v), ctx) {
                    filled.insert(k.clone(), evaluated);
                }
            }
        }
        return Some(call("fill", vec![("template", eval_operand(arg(args, 0), ctx)), ("params", Some(Value::Object(filled)))]));
    }
    if let Some(args) = obj.get("$percent") {
        let (v, max) = pair(args, ctx);
        if is_dynamic_opt(&v) || is_dynamic_opt(&max) {
            return Some(call("percent", vec![("value", v), ("max", max)]));
        }
        return Some(Value::String(percent(v.as_ref(), max.as_ref())));
    }
    if let Some(spec) = obj.get("$text") {
        let v = match spec {
            Value::String(s) => eval_expr(s, ctx),
            other => eval_operand(Some(other), ctx),
        };
        if is_dynamic_opt(&v) {
            return Some(call("text", vec![("value", v)]));
        }
        return match v {
            None | Some(Value::Null) => None,
            Some(v) => Some(Value::String(json::to_js_string(&v))),
        };
    }
    if let Some(candidates) = obj.get("$coalesce") {
        let mut out: Vec<Value> = Vec::new();
        for candidate in candidates.as_array().into_iter().flatten() {
            let Some(v) = eval_operand(Some(candidate), ctx) else { continue };
            if is_dynamic(&v) {
                out.push(v);
                continue;
            }
            if !truthy(&v) {
                continue;
            }
            if out.is_empty() {
                return Some(v);
            }
            out.push(v);
            break;
        }
        return match out.len() {
            0 => None,
            1 => out.pop(),
            _ => Some(call("coalesce", vec![("values", Some(Value::Array(out)))])),
        };
    }
    if obj.contains_key("$not") {
        return match eval_condition_value(obj.get("$not"), ctx) {
            Decided::Dynamic(v) => Some(call("not", vec![("value", Some(v))])),
            Decided::Bool(b) => Some(Value::Bool(!b)),
        };
    }
    let mut out = Map::new();
    for (k, v) in obj {
        let Some(evaluated) = eval_value(v, ctx) else { continue };
        let key = if k.contains('{') {
            match eval_value(&Value::String(k.clone()), ctx) {
                Some(Value::String(s)) => s,
                _ => k.clone(),
            }
        } else {
            k.clone()
        };
        out.insert(key, evaluated);
    }
    Some(Value::Object(out))
}

/// `$percent`'s arithmetic, shared with the core `percent` function: the
/// share of `max` as "N%", clamped to 0..100, two decimals.
pub fn percent(value: Option<&Value>, max: Option<&Value>) -> String {
    let m = to_number(max);
    let pct = if m > 0.0 { to_number(value) / m * 100.0 } else { 0.0 };
    let clamped = pct.clamp(0.0, 100.0);
    let rounded = js_round(clamped * 100.0) / 100.0;
    format!("{}%", json::number_to_string(rounded))
}

/// JS `Math.round` (half rounds UP, also for negatives).
pub fn js_round(v: f64) -> f64 {
    (v + 0.5).floor()
}

/// The core value functions (`core.catalog.json` `functions.core`; `set` is
/// an action, `data::run_action`) once the arguments are resolved: the
/// reference every client function table mirrors (`src/expr.ts
/// CORE_FUNCTIONS`). Outer `None` = not a core function; inner `None` =
/// undefined.
pub fn core_function(name: &str, args: &Map<String, Value>) -> Option<Option<Value>> {
    let a = |k: &str| args.get(k);
    Some(match name {
        "percent" => Some(Value::String(percent(a("value"), a("max")))),
        "add" => Some(json::number(to_number(a("a")) + to_number(a("b")))),
        "sub" => Some(json::number(to_number(a("a")) - to_number(a("b")))),
        "eq" => Some(Value::Bool(values_equal(a("a"), a("b")))),
        "lt" => Some(Value::Bool(to_number(a("a")) < to_number(a("b")))),
        "clamp" => Some(json::number(clamp_number(a("value"), a("min"), a("max")))),
        "cond" => {
            if truthy_opt(a("if")) {
                a("then").cloned()
            } else {
                a("else").cloned()
            }
        }
        "fallback" => match a("value") {
            None | Some(Value::Null) => a("default").cloned(),
            Some(v) => Some(v.clone()),
        },
        "concat" => Some(Value::String(match a("values") {
            Some(Value::Array(values)) => values.iter().map(|v| if v.is_null() { String::new() } else { json::to_js_string(v) }).collect(),
            _ => String::new(),
        })),
        "coalesce" => match a("values") {
            Some(Value::Array(values)) => values.iter().find(|v| truthy(v)).cloned(),
            _ => None,
        },
        "text" => match a("value") {
            None | Some(Value::Null) => None,
            Some(v) => Some(Value::String(json::to_js_string(v))),
        },
        "map" => {
            let key = match a("value") {
                None | Some(Value::Null) => String::new(),
                Some(v) => json::to_js_string(v),
            };
            match a("cases").and_then(Value::as_object).and_then(|t| t.get(&key)) {
                Some(hit) => Some(hit.clone()),
                None => a("default").cloned(),
            }
        }
        "len" => Some(json::number(js_len(a("value")) as f64)),
        "fill" => Some(Value::String(fill_template(a("template"), a("params")))),
        "filter" => Some(Value::Array(filter_items(a("items"), a("query"), a("fields"), a("where")))),
        _ => return None,
    })
}

/// The core function names in catalog order (`set` = the action; the
/// format functions live in `format`).
pub const CORE_FUNCTION_NAMES: &[&str] = &["percent", "add", "sub", "eq", "lt", "clamp", "cond", "fallback", "concat", "coalesce", "text", "map", "len", "fill", "set", "filter"];

/// Round 4 (VAPP-103) `filter` (`src/expr.ts filterItems`): the items of an
/// array that match, in order. `where` = field → value, every entry must
/// hold (`eq` equality); an entry whose value is null or "" does NOT
/// constrain. `query` = a case-insensitive substring of ANY of `fields`
/// (the item's own string and number values when `fields` is absent; a
/// string or number item matches itself); a missing or blank query does not
/// constrain. Not an array → [].
pub fn filter_items(items: Option<&Value>, query: Option<&Value>, fields: Option<&Value>, r#where: Option<&Value>) -> Vec<Value> {
    let Some(Value::Array(items)) = items else { return vec![] };
    let needle = match query {
        Some(Value::String(s)) => s.trim().to_lowercase(),
        Some(n @ Value::Number(_)) => json::to_js_string(n),
        _ => String::new(),
    };
    let keys: Option<Vec<&str>> = fields.and_then(Value::as_array).map(|f| f.iter().filter_map(Value::as_str).collect());
    let conditions: Vec<(&String, &Value)> = match r#where {
        Some(Value::Object(m)) => m.iter().filter(|(_, v)| !v.is_null() && v.as_str() != Some("")).collect(),
        _ => vec![],
    };
    let searchable = |v: &Value| match v {
        Value::String(s) => Some(s.to_lowercase()),
        Value::Number(_) => Some(json::to_js_string(v)),
        _ => None,
    };
    items
        .iter()
        .filter(|item| {
            let record = item.as_object();
            if !conditions.iter().all(|(field, want)| values_equal(record.and_then(|r| r.get(field.as_str())), Some(want))) {
                return false;
            }
            if needle.is_empty() {
                return true;
            }
            match record {
                Some(r) => match &keys {
                    Some(keys) => keys.iter().filter_map(|k| r.get(*k)).any(|v| searchable(v).is_some_and(|s| s.contains(&needle))),
                    None => r.values().any(|v| searchable(v).is_some_and(|s| s.contains(&needle))),
                },
                None => searchable(item).is_some_and(|s| s.contains(&needle)),
            }
        })
        .cloned()
        .collect()
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

    fn with_bound(f: impl FnOnce(&ExprContext)) {
        let props = json!({"page": {"path": "/page"}, "total": 5, "open": {"path": "/open"}, "value": {"call": "len", "args": {"value": {"path": "/xs"}}}});
        let props = props.as_object().unwrap().clone();
        let ctx = ExprContext { id: "p", props: &props, vars: IndexMap::new() };
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
            assert_eq!(ev(json!("{range(props.page)}"), ctx), Some(json!([0, 1])));
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
            assert_eq!(ev(json!({"$sub": ["{props.page}", 1]}), ctx), Some(json!(1)));
            assert_eq!(ev(json!({"$lt": ["{props.count}", 1]}), ctx), Some(json!(true)));
            assert_eq!(ev(json!({"$percent": ["{props.page}", 8]}), ctx), Some(json!("25%")));
            assert_eq!(ev(json!({"$percent": [150, 100]}), ctx), Some(json!("100%")));
            assert_eq!(ev(json!({"$percent": [1, 3]}), ctx), Some(json!("33.33%")));
            assert_eq!(ev(json!({"$coalesce": ["{props.title}", "{props.missing}", "Choose"]}), ctx), Some(json!("Choose")));
            assert_eq!(ev(json!({"$text": "props.count"}), ctx), Some(json!("0")));
            assert_eq!(ev(json!({"$text": "props.missing"}), ctx), None);
            assert_eq!(ev(json!({"$not": "props.title"}), ctx), Some(json!(true)));
            assert_eq!(ev(json!({"$clamp": ["{props.page}", 3, 5]}), ctx), Some(json!(3)));
            assert_eq!(ev(json!({"$eq": ["{props.page}", "2"]}), ctx), Some(json!(true)));
            assert_eq!(
                ev(json!({"$fill": ["$string.pageOf", {"page": "{props.page}", "x": "{props.missing}"}]}), ctx),
                Some(json!({"call": "fill", "args": {"template": "$string.pageOf", "params": {"page": 2}}}))
            );
        });
    }

    #[test]
    fn template_values_objects_drop_undefined_members_and_arrays_drop_undefined_items() {
        with_ctx(|ctx| {
            assert_eq!(ev(json!({"a": "{props.missing}", "b": "{props.gap}"}), ctx), Some(json!({"b": "lg"})));
            assert_eq!(ev(json!(["{props.missing}", "{props.gap}"]), ctx), Some(json!(["lg"])));
            assert_eq!(ev(json!({"@media (min-width: $breakpoint.{props.gap})": {"x": 1}}), ctx), Some(json!({"@media (min-width: $breakpoint.lg)": {"x": 1}})));
        });
    }

    #[test]
    fn interpolation_keeps_unmatched_braces() {
        with_ctx(|ctx| {
            assert_eq!(ev(json!("a{}b{{props.gap}}"), ctx), Some(json!("a{}b{lg}")));
            assert_eq!(ev(json!("{props.missing}x{"), ctx), Some(json!("x{")));
        });
    }

    #[test]
    fn bound_inputs_emit_calls_member_vs_operand() {
        with_bound(|ctx| {
            // A member passes the binding through (two-way binding survives),
            // the fallback is dropped.
            assert_eq!(ev(json!("{props.page}"), ctx), Some(json!({"path": "/page"})));
            assert_eq!(ev(json!("{props.page|1}"), ctx), Some(json!({"path": "/page"})));
            // An operand keeps it.
            assert_eq!(
                ev(json!({"$eq": ["{props.page|1}", 1]}), ctx),
                Some(json!({"call": "eq", "args": {"a": {"call": "fallback", "args": {"value": {"path": "/page"}, "default": 1}}, "b": 1}}))
            );
            assert_eq!(ev(json!("{props.page} / {props.total}"), ctx), Some(json!({"call": "concat", "args": {"values": [{"path": "/page"}, " / ", 5]}})));
            assert_eq!(ev(json!("{!props.open}"), ctx), Some(json!({"call": "not", "args": {"value": {"path": "/open"}}})));
            assert_eq!(ev(json!("{range(props.page)}"), ctx), None);
            assert_eq!(ev(json!({"$percent": ["{props.page}", 10]}), ctx), Some(json!({"call": "percent", "args": {"value": {"path": "/page"}, "max": 10}})));
            assert_eq!(ev(json!({"$cond": ["props.open", "a", "b"]}), ctx), Some(json!({"call": "cond", "args": {"if": {"path": "/open"}, "then": "a", "else": "b"}})));
            assert_eq!(
                ev(json!({"$coalesce": ["", "{props.page}", "x", "y"]}), ctx),
                Some(json!({"call": "coalesce", "args": {"values": [{"path": "/page"}, "x"]}}))
            );
            assert_eq!(ev(json!({"$text": "props.page"}), ctx), Some(json!({"call": "text", "args": {"value": {"path": "/page"}}})));
            assert_eq!(
                ev(json!({"$clamp": ["{props.page|1}", 1, "{props.total}"]}), ctx),
                Some(json!({"call": "clamp", "args": {"value": {"call": "fallback", "args": {"value": {"path": "/page"}, "default": 1}}, "min": 1, "max": 5}}))
            );
            assert_eq!(eval_condition_value(Some(&json!("props.open")), ctx), Decided::Dynamic(json!({"path": "/open"})));
            assert!(eval_condition(&json!("props.open"), ctx));
        });
    }

    #[test]
    fn core_functions_evaluate_like_the_reference() {
        let args = |v: Value| v.as_object().unwrap().clone();
        assert_eq!(core_function("percent", &args(json!({"value": 1, "max": 3}))), Some(Some(json!("33.33%"))));
        assert_eq!(core_function("percent", &args(json!({"value": 1, "max": 0}))), Some(Some(json!("0%"))));
        assert_eq!(core_function("add", &args(json!({"a": "x", "b": 2}))), Some(Some(json!(2))));
        assert_eq!(core_function("fallback", &args(json!({"default": 1}))), Some(Some(json!(1))));
        assert_eq!(core_function("concat", &args(json!({"values": ["a", null, 1]}))), Some(Some(json!("a1"))));
        assert_eq!(core_function("coalesce", &args(json!({"values": ["", 0]}))), Some(Some(json!(0))));
        assert_eq!(core_function("text", &args(json!({}))), Some(None));
        assert_eq!(core_function("map", &args(json!({"value": 2, "cases": {"2": "two"}, "default": "?"}))), Some(Some(json!("two"))));
        assert_eq!(core_function("len", &args(json!({"value": "héllo"}))), Some(Some(json!(5))));
        assert_eq!(core_function("eq", &args(json!({"a": 5, "b": " 5 "}))), Some(Some(json!(true))));
        assert_eq!(core_function("eq", &args(json!({"a": 0, "b": ""}))), Some(Some(json!(false))));
        assert_eq!(core_function("eq", &args(json!({"a": 1, "b": true}))), Some(Some(json!(false))));
        assert_eq!(core_function("clamp", &args(json!({"value": 9, "min": 1, "max": 5}))), Some(Some(json!(5))));
        assert_eq!(core_function("clamp", &args(json!({"value": -2, "min": 1, "max": null}))), Some(Some(json!(1))));
        assert_eq!(core_function("clamp", &args(json!({"value": 3, "min": 4, "max": 2}))), Some(Some(json!(4))));
        assert_eq!(core_function("fill", &args(json!({"template": "Page {page} of {total}", "params": {"page": 2}}))), Some(Some(json!("Page 2 of {total}"))));
        assert_eq!(core_function("nope", &args(json!({}))), None);
    }
}
