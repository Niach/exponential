//! VAPP-161: the PACKAGE FILE's signed bytes (docs/package-format.md "The
//! package file"): a context line + the RFC 8785 canonical JSON of the whole
//! file without `signature`. Mirrors `canonicalJson` +
//! `packageFileSignedBytes` of `packages/exponential-ui/src/host/signature.ts`;
//! the Ed25519 itself is the host's own (no crypto here).
//! `fixtures/host-package.json` `canonical` + `signatures[].signedText`
//! replay it.

use serde_json::{Map, Value};

use super::utf16_cmp;
use crate::json::number_to_string;

/// The context the signed bytes start with.
pub const PACKAGE_FILE_SIGNATURE_CONTEXT: &str = "exponential-ui/package-file/v1\n";
pub const PACKAGE_FILE_SIGNATURE_ALG: &str = "ed25519";
pub const AUTHOR_KEY_PREFIX: &str = "ed25519:";
pub const AUTHOR_NAME_MAX: usize = 80;
/// What `verifyPackageFile` answers (`package.signature.statuses`).
pub const PACKAGE_FILE_STATUSES: &[&str] = &["unsigned", "valid", "invalid"];

/// A string as ECMAScript's `JSON.stringify` writes it: `\b \f \n \r \t \" \\`,
/// any other code point below U+0020 as `\u00xx` (lower-case hex), nothing
/// else escaped.
fn write_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn write_value(out: &mut String, value: &Value) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        // One double, as `JSON.stringify` writes it (`1`, `1e+21`, `-0` = `0`).
        Value::Number(n) => out.push_str(&number_to_string(n.as_f64().unwrap_or(0.0))),
        Value::String(s) => write_string(out, s),
        Value::Array(items) => {
            out.push('[');
            for (i, v) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                crate::deep(|| write_value(out, v));
            }
            out.push(']');
        }
        Value::Object(map) => write_object(out, map.iter()),
    }
}

fn write_object<'a>(out: &mut String, entries: impl Iterator<Item = (&'a String, &'a Value)>) {
    let mut entries: Vec<(&String, &Value)> = entries.collect();
    entries.sort_by(|a, b| utf16_cmp(a.0, b.0));
    out.push('{');
    for (i, (k, v)) in entries.into_iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        write_string(out, k);
        out.push(':');
        crate::deep(|| write_value(out, v));
    }
    out.push('}');
}

/// RFC 8785 (JCS): no whitespace, keys sorted by UTF-16 code units, strings
/// and numbers as ECMAScript's `JSON.stringify` writes them. (A
/// `serde_json::Value` holds no non-finite number and no undefined member,
/// so unlike the reference this never fails.)
pub fn canonical_json(value: &Value) -> String {
    let mut out = String::new();
    crate::deep(|| write_value(&mut out, value));
    out
}

/// What the signature covers: the context line + the canonical JSON of the
/// file without `signature`, as UTF-8.
pub fn package_file_signed_bytes(file: &Map<String, Value>) -> Vec<u8> {
    let mut out = String::from(PACKAGE_FILE_SIGNATURE_CONTEXT);
    crate::deep(|| write_object(&mut out, file.iter().filter(|(k, _)| *k != "signature")));
    out.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn numbers_and_strings_follow_json_stringify() {
        let v: Value = serde_json::from_str(r#"[1.0, 1e21, 1E-7, -0.0, 5e-324, 1.5e300, 123456789012345680000, "\u0001\u007f\b"]"#).unwrap();
        assert_eq!(canonical_json(&v), "[1,1e+21,1e-7,0,5e-324,1.5e+300,123456789012345680000,\"\\u0001\u{7f}\\b\"]");
        // U+FFFD sorts after U+1F600 by UTF-16 units (0xFFFD > 0xD83D), not by code point.
        assert_eq!(canonical_json(&json!({"\u{fffd}": 1, "\u{1f600}": 2})), "{\"\u{1f600}\":2,\"\u{fffd}\":1}");
    }
}
