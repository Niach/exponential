//! JSON helpers shared by the reducer, the expander and the theme resolver:
//! the JavaScript number model (one double; integral values print without a
//! fraction) and a canonical form for byte-for-byte fixture comparisons.

use serde_json::{Map, Number, Value};

/// A JS-style number: integral doubles become JSON integers (`1`, not `1.0`),
/// everything else stays a float. `NaN`/`±∞` become `null` like `JSON.stringify`.
pub fn number(v: f64) -> Value {
    if !v.is_finite() {
        return Value::Null;
    }
    if v.fract() == 0.0 && v.abs() < 9_007_199_254_740_992.0 {
        if v < 0.0 {
            return Value::from(v as i64);
        }
        return Value::from(v as u64);
    }
    Number::from_f64(v).map(Value::Number).unwrap_or(Value::Null)
}

/// Every number as `f64`.
pub fn as_f64(v: &Value) -> Option<f64> {
    v.as_f64()
}

/// The canonical form two JSON documents are compared in: numbers through
/// [`number`] (so `2.0 == 2`), `-0 == 0`, objects and arrays recursively.
pub fn canonical(v: &Value) -> Value {
    match v {
        Value::Number(n) => number(n.as_f64().unwrap_or(0.0)),
        Value::Array(items) => Value::Array(items.iter().map(canonical).collect()),
        Value::Object(map) => Value::Object(map.iter().map(|(k, v)| (k.clone(), canonical(v))).collect()),
        other => other.clone(),
    }
}

/// `true` when both documents are the same in canonical form.
pub fn equal(a: &Value, b: &Value) -> bool {
    canonical(a) == canonical(b)
}

/// JavaScript's `String(number)`: integral values without a fraction, the
/// shortest round-trip form otherwise.
pub fn number_to_string(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e21 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

/// JavaScript's `String(value)` for the scalar values a template may
/// interpolate: strings verbatim, numbers through [`number_to_string`],
/// booleans `true`/`false`, `null`, arrays comma-joined, objects
/// `[object Object]`.
pub fn to_js_string(v: &Value) -> String {
    match v {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => number_to_string(n.as_f64().unwrap_or(0.0)),
        Value::String(s) => s.clone(),
        Value::Array(items) => items.iter().map(to_js_string).collect::<Vec<_>>().join(","),
        Value::Object(_) => "[object Object]".into(),
    }
}

/// JavaScript's `Number(value) || 0`.
pub fn to_number(v: &Value) -> f64 {
    match v {
        Value::Null => 0.0,
        Value::Bool(b) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::String(s) => {
            let t = s.trim();
            if t.is_empty() {
                0.0
            } else {
                t.parse::<f64>().unwrap_or(0.0)
            }
        }
        Value::Array(items) => {
            if items.len() == 1 {
                to_number(&items[0])
            } else {
                0.0
            }
        }
        Value::Object(_) => 0.0,
    }
}

/// JavaScript `===` over JSON values (numbers by value, no coercion).
pub fn strict_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Object(_), Value::Object(_)) | (Value::Array(_), Value::Array(_)) => false,
        _ => a == b,
    }
}

/// The empty object.
pub fn obj() -> Map<String, Value> {
    Map::new()
}

/// `value` as an object, if it is one.
pub fn as_obj(v: &Value) -> Option<&Map<String, Value>> {
    v.as_object()
}

/// The object's key `k` as a `&str`.
pub fn str_of<'a>(m: &'a Map<String, Value>, k: &str) -> Option<&'a str> {
    m.get(k).and_then(Value::as_str)
}

/// A deep clone through `Value` (the TS `JSON.parse(JSON.stringify(x))`).
pub fn clone_value<T: serde::Serialize + serde::de::DeserializeOwned>(v: &T) -> T {
    serde_json::from_value(serde_json::to_value(v).expect("serialize")).expect("deserialize")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn numbers_print_like_javascript() {
        assert_eq!(number(2.0), json!(2));
        assert_eq!(number(-3.0), json!(-3));
        assert_eq!(number(66.67), json!(66.67));
        assert_eq!(number_to_string(100.0), "100");
        assert_eq!(number_to_string(66.67), "66.67");
        assert_eq!(number_to_string(1.7777778), "1.7777778");
        assert!(equal(&json!({"a": 2.0}), &json!({"a": 2})));
        assert!(!equal(&json!({"a": 2.5}), &json!({"a": 2})));
    }

    #[test]
    fn js_coercions() {
        assert_eq!(to_number(&json!("12")), 12.0);
        assert_eq!(to_number(&json!("x")), 0.0);
        assert_eq!(to_number(&json!(null)), 0.0);
        assert!(strict_eq(&json!(1), &json!(1.0)));
        assert!(!strict_eq(&json!("1"), &json!(1)));
    }
}
