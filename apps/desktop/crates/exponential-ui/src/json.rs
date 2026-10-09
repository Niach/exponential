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

/// JavaScript's `String(number)` (ECMAScript `Number::toString`): the
/// shortest round-trip digits, plain from 1e-6 up to 1e21, exponent form
/// (`1e-7`, `1e+21`) outside; `-0` → `0`, `NaN`, `Infinity`.
pub fn number_to_string(v: f64) -> String {
    if v.is_nan() {
        return "NaN".into();
    }
    if v.is_infinite() {
        return if v > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    if v == 0.0 {
        return "0".into();
    }
    let sign = if v < 0.0 { "-" } else { "" };
    let (digits, n) = crate::format::shortest_digits(v.abs());
    let k = digits.len() as i32;
    let body = if k <= n && n <= 21 {
        format!("{digits}{}", "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
    } else if -6 < n && n <= 0 {
        format!("0.{}{digits}", "0".repeat((-n) as usize))
    } else {
        let e = n - 1;
        let exp = if e >= 0 { format!("e+{e}") } else { format!("e{e}") };
        if k == 1 {
            format!("{digits}{exp}")
        } else {
            format!("{}.{}{exp}", &digits[..1], &digits[1..])
        }
    };
    format!("{sign}{body}")
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
        assert_eq!(number_to_string(123456789012345680000.0), "123456789012345680000");
        assert_eq!(number_to_string(1e21), "1e+21");
        assert_eq!(number_to_string(1e-7), "1e-7");
        assert_eq!(number_to_string(1.5e-7), "1.5e-7");
        assert_eq!(number_to_string(1e-6), "0.000001");
        assert_eq!(number_to_string(-0.0), "0");
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
