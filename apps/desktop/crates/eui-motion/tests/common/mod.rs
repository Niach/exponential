//! Shared helpers of the fixture replays (frames.rs, validate.rs).
#![allow(dead_code)]

use serde_json::Value;
use std::path::{Path, PathBuf};

/// `packages/exponential-ui/fixtures/<name>`.
pub fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../packages/exponential-ui/fixtures").join(name)
}

/// Structural equality; numbers within 1e-4 × max(1, |expected|).
pub fn close(actual: &Value, expected: &Value, at: &str) -> Result<(), String> {
    match (actual, expected) {
        (Value::Number(a), Value::Number(e)) => {
            let (a, e) = (a.as_f64().unwrap(), e.as_f64().unwrap());
            if (a - e).abs() <= 1e-4 * e.abs().max(1.0) {
                Ok(())
            } else {
                Err(format!("{at}: {a} != {e}"))
            }
        }
        (Value::Array(a), Value::Array(e)) => {
            if a.len() != e.len() {
                return Err(format!("{at}: {} items != {}", a.len(), e.len()));
            }
            a.iter().zip(e).enumerate().try_for_each(|(i, (a, e))| close(a, e, &format!("{at}/{i}")))
        }
        (Value::Object(a), Value::Object(e)) => {
            let (mut ka, mut ke): (Vec<_>, Vec<_>) = (a.keys().collect(), e.keys().collect());
            ka.sort();
            ke.sort();
            if ka != ke {
                return Err(format!("{at}: keys {ka:?} != {ke:?}"));
            }
            e.iter().try_for_each(|(k, ev)| close(&a[k], ev, &format!("{at}/{k}")))
        }
        _ if actual == expected => Ok(()),
        _ => Err(format!("{at}: {actual} != {expected}")),
    }
}

fn flat(v: &Value) -> bool {
    match v {
        Value::Array(a) => a.iter().all(|x| !x.is_array() && !x.is_object()),
        Value::Object(o) => o.values().all(|x| !x.is_array() && !x.is_object()),
        _ => true,
    }
}

fn write(v: &Value, indent: usize, compact_keys: &[&str], out: &mut String) {
    let pad = "  ".repeat(indent + 1);
    match v {
        Value::Array(a) if !flat(v) && !a.is_empty() => {
            out.push_str("[\n");
            for (i, x) in a.iter().enumerate() {
                out.push_str(&pad);
                write(x, indent + 1, compact_keys, out);
                out.push_str(if i + 1 < a.len() { ",\n" } else { "\n" });
            }
            out.push_str(&"  ".repeat(indent));
            out.push(']');
        }
        Value::Object(o) if !flat(v) && !o.is_empty() => {
            out.push_str("{\n");
            for (i, (k, x)) in o.iter().enumerate() {
                out.push_str(&pad);
                out.push_str(&serde_json::to_string(k).unwrap());
                out.push_str(": ");
                if compact_keys.contains(&k.as_str()) {
                    out.push_str(&serde_json::to_string(x).unwrap());
                } else {
                    write(x, indent + 1, compact_keys, out);
                }
                out.push_str(if i + 1 < o.len() { ",\n" } else { "\n" });
            }
            out.push_str(&"  ".repeat(indent));
            out.push('}');
        }
        _ => out.push_str(&serde_json::to_string(v).unwrap()),
    }
}

/// Pretty JSON with scalar-only arrays/objects on one line and the values
/// of `compact_keys` (e.g. inline Lotties) on one line.
pub fn write_fixture(path: &Path, v: &Value, compact_keys: &[&str]) {
    let mut out = String::new();
    write(v, 0, compact_keys, &mut out);
    out.push('\n');
    std::fs::write(path, out).unwrap();
}
