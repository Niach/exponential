//! The few JavaScript value rules the TS reference leans on, so loosely
//! typed op args behave the same in both ports.

use serde_json::{Map, Number, Value};

/// JS `String(value)`; `None` = `undefined`.
pub(crate) fn string(value: Option<&Value>) -> String {
    match value {
        None => "undefined".into(),
        Some(Value::Null) => "null".into(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(Value::Number(n)) => number_string(n),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| if item.is_null() { String::new() } else { string(Some(item)) })
            .collect::<Vec<_>>()
            .join(","),
        Some(Value::Object(_)) => "[object Object]".into(),
    }
}

fn number_string(n: &Number) -> String {
    if let Some(i) = n.as_i64() {
        return i.to_string();
    }
    if let Some(u) = n.as_u64() {
        return u.to_string();
    }
    let f = n.as_f64().unwrap_or(f64::NAN);
    if f == 0.0 {
        "0".into()
    } else if f.fract() == 0.0 && f.abs() < 1e21 {
        format!("{f:.0}")
    } else {
        f.to_string()
    }
}

/// JS `Number(value)`, `None` = `undefined` (NaN).
pub(crate) fn number(value: Option<&Value>) -> f64 {
    match value {
        None => f64::NAN,
        Some(Value::Null) => 0.0,
        Some(Value::Bool(b)) => f64::from(u8::from(*b)),
        Some(Value::Number(n)) => n.as_f64().unwrap_or(f64::NAN),
        Some(Value::String(s)) => {
            let t = s.trim();
            if t.is_empty() { 0.0 } else { t.parse().unwrap_or(f64::NAN) }
        }
        Some(_) => f64::NAN,
    }
}

/// `value ?? fallback`: `None` for undefined and null.
pub(crate) fn nullish(value: Option<&Value>) -> Option<&Value> {
    value.filter(|v| !v.is_null())
}

/// JS truthiness.
pub(crate) fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

/// `Number.isInteger(value)`.
pub(crate) fn is_int(value: Option<&Value>) -> bool {
    value.and_then(Value::as_f64).is_some_and(|f| f.is_finite() && f.fract() == 0.0)
}

/// `a === b` for a JSON number and an integer.
pub(crate) fn is_number(value: Option<&Value>, n: i64) -> bool {
    value.and_then(Value::as_f64).is_some_and(|f| f == n as f64)
}

/// `a === b` for a value and a string.
pub(crate) fn is_str(value: Option<&Value>, s: &str) -> bool {
    value.and_then(Value::as_str) == Some(s)
}

/// SameValueZero (a `Set`'s equality): objects and arrays only equal themselves.
pub(crate) fn same_value_zero(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(_) | Value::Object(_), _) | (_, Value::Array(_) | Value::Object(_)) => false,
        _ => a == b,
    }
}

/// A JS array index key (`"0"`, `"17"`): those iterate first, ascending.
fn array_index(key: &str) -> Option<u32> {
    if key.is_empty() || (key.len() > 1 && key.starts_with('0')) || !key.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    key.parse::<u32>().ok().filter(|i| *i != u32::MAX)
}

/// `Object.entries(obj)`: array-index keys ascending, then source order.
pub(crate) fn entries(map: &Map<String, Value>) -> Vec<(&String, &Value)> {
    let mut indexed: Vec<(u32, (&String, &Value))> =
        map.iter().filter_map(|(k, v)| array_index(k).map(|i| (i, (k, v)))).collect();
    indexed.sort_by_key(|(i, _)| *i);
    let mut out: Vec<(&String, &Value)> = indexed.into_iter().map(|(_, e)| e).collect();
    out.extend(map.iter().filter(|(k, _)| array_index(k).is_none()));
    out
}
