//! Tolerant hydrate deserializers (masterplan-v3 §5.5) — the `string → native`
//! coercion layer every typed row struct uses.
//!
//! The sync store pins ONE canonical storage form — TEXT — at bind time, but
//! wire-hydrated values can still surface as bare numbers / bools (Electric
//! delivers heterogeneous scalars) and SQLite hydration re-wraps TEXT/NULL as
//! JSON. These deserializers accept every form the four clients have seen in
//! the wild. Notably NOT `serde_with::BoolFromInt` (§5.5): a Postgres/Electric
//! boolean surfaces as `true`/`false`, `"t"`/`"f"`, `"true"`/`"false"`, or
//! `0`/`1` — all must hydrate.

use serde::Deserialize;
use serde_json::Value;

/// String-tolerant bool: accepts `"t"`/`"f"`/`"true"`/`"false"`/`"0"`/`"1"`
/// (any case), bare `0`/`1`, and real JSON bools (§5.5).
pub fn tolerant_bool<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    value_to_bool(&value)
        .ok_or_else(|| serde::de::Error::custom(format!("not a tolerant bool: {value}")))
}

/// `Option` flavor of [`tolerant_bool`] — JSON `null` (or SQL `NULL` rewrapped
/// as JSON null by the store's hydrate read) hydrates to `None`.
pub fn tolerant_opt_bool<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    if value.is_null() {
        return Ok(None);
    }
    value_to_bool(&value)
        .map(Some)
        .ok_or_else(|| serde::de::Error::custom(format!("not a tolerant bool: {value}")))
}

fn value_to_bool(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(b) => Some(*b),
        Value::Number(n) => match n.as_i64() {
            Some(0) => Some(false),
            Some(1) => Some(true),
            _ => None,
        },
        Value::String(s) => match s.to_ascii_lowercase().as_str() {
            "t" | "true" | "1" => Some(true),
            "f" | "false" | "0" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

/// Tolerant i64: accepts bare numbers and their TEXT forms (`"1"` | `1` → 1).
pub fn tolerant_i64<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    value_to_i64(&value)
        .ok_or_else(|| serde::de::Error::custom(format!("not a tolerant i64: {value}")))
}

/// `Option` flavor of [`tolerant_i64`].
pub fn tolerant_opt_i64<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    if value.is_null() {
        return Ok(None);
    }
    value_to_i64(&value)
        .map(Some)
        .ok_or_else(|| serde::de::Error::custom(format!("not a tolerant i64: {value}")))
}

fn value_to_i64(value: &Value) -> Option<i64> {
    match value {
        Value::Number(n) => n
            .as_i64()
            .or_else(|| n.as_f64().filter(|f| f.fract() == 0.0).map(|f| f as i64)),
        Value::String(s) => s.trim().parse::<i64>().ok(),
        _ => None,
    }
}

/// Tolerant f64: accepts bare numbers and their TEXT forms (`"1.5"` → 1.5).
pub fn tolerant_f64<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    value_to_f64(&value)
        .ok_or_else(|| serde::de::Error::custom(format!("not a tolerant f64: {value}")))
}

/// `Option` flavor of [`tolerant_f64`].
pub fn tolerant_opt_f64<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    if value.is_null() {
        return Ok(None);
    }
    value_to_f64(&value)
        .map(Some)
        .ok_or_else(|| serde::de::Error::custom(format!("not a tolerant f64: {value}")))
}

fn value_to_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

/// Tolerant jsonb: the store pins ONE canonical storage form — TEXT — so a
/// `jsonb` column (e.g. `issue_events.payload`) hydrates from SQLite as a
/// STRING holding serialized JSON. Re-parse it back into the structured value;
/// wire-delivered objects/arrays pass through untouched; JSON `null` hydrates
/// to `None`; a string that is not a JSON container stays a string (never
/// invent structure).
pub fn tolerant_opt_json<'de, D>(deserializer: D) -> Result<Option<Value>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(match value {
        Value::Null => None,
        Value::String(s) => {
            let parsed = match s.trim_start().as_bytes().first() {
                Some(b'{') | Some(b'[') => serde_json::from_str::<Value>(&s).ok(),
                _ => None,
            };
            Some(parsed.unwrap_or(Value::String(s)))
        }
        other => Some(other),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tolerant_bool_accepts_all_wire_forms() {
        #[derive(Deserialize)]
        struct T {
            #[serde(deserialize_with = "tolerant_bool")]
            v: bool,
        }
        for (input, expected) in [
            (json!({"v": "t"}), true),
            (json!({"v": "f"}), false),
            (json!({"v": "true"}), true),
            (json!({"v": "false"}), false),
            (json!({"v": "TRUE"}), true),
            (json!({"v": "0"}), false),
            (json!({"v": "1"}), true),
            (json!({"v": 0}), false),
            (json!({"v": 1}), true),
            (json!({"v": true}), true),
            (json!({"v": false}), false),
        ] {
            let t: T = serde_json::from_value(input.clone()).unwrap_or_else(|e| {
                panic!("{input} should deserialize: {e}");
            });
            assert_eq!(t.v, expected, "{input}");
        }
        assert!(serde_json::from_value::<T>(json!({"v": "yes"})).is_err());
        assert!(serde_json::from_value::<T>(json!({"v": 2})).is_err());
    }

    #[test]
    fn tolerant_numbers_accept_text_and_bare_forms() {
        #[derive(Deserialize)]
        struct T {
            #[serde(deserialize_with = "tolerant_i64")]
            n: i64,
            #[serde(deserialize_with = "tolerant_opt_f64")]
            s: Option<f64>,
            #[serde(deserialize_with = "tolerant_opt_bool")]
            b: Option<bool>,
        }
        let t: T = serde_json::from_value(json!({"n": "1", "s": "1.5", "b": "t"})).unwrap();
        assert_eq!((t.n, t.s, t.b), (1, Some(1.5), Some(true)));
        let t: T = serde_json::from_value(json!({"n": 1, "s": 1.5, "b": null})).unwrap();
        assert_eq!((t.n, t.s, t.b), (1, Some(1.5), None));
        let t: T = serde_json::from_value(json!({"n": 1.0, "s": null, "b": true})).unwrap();
        assert_eq!((t.n, t.s, t.b), (1, None, Some(true)));
    }

    #[test]
    fn tolerant_json_reparses_text_stored_containers() {
        #[derive(Deserialize)]
        struct T {
            #[serde(default, deserialize_with = "tolerant_opt_json")]
            p: Option<Value>,
        }
        // The store's TEXT form of a jsonb object re-parses to the object.
        let t: T = serde_json::from_value(json!({"p": "{\"to\":\"done\"}"})).unwrap();
        assert_eq!(t.p, Some(json!({"to": "done"})));
        // Wire-delivered objects pass through.
        let t: T = serde_json::from_value(json!({"p": {"to": "done"}})).unwrap();
        assert_eq!(t.p, Some(json!({"to": "done"})));
        // Arrays re-parse too; plain strings stay strings; null → None.
        let t: T = serde_json::from_value(json!({"p": "[1,2]"})).unwrap();
        assert_eq!(t.p, Some(json!([1, 2])));
        let t: T = serde_json::from_value(json!({"p": "not json"})).unwrap();
        assert_eq!(t.p, Some(Value::String("not json".into())));
        let t: T = serde_json::from_value(json!({"p": null})).unwrap();
        assert_eq!(t.p, None);
        // A malformed container-looking string survives as its text.
        let t: T = serde_json::from_value(json!({"p": "{broken"})).unwrap();
        assert_eq!(t.p, Some(Value::String("{broken".into())));
    }
}
