//! The `mcp_calls` audit row (`packages/emcp/fixtures/audit.json`):
//! canonical JSON, the args hash, the status rule and retention.

use serde_json::Value;
use sha2::{Digest, Sha256};

pub const HASH_PREFIX: &str = "sha256:";
pub const RETENTION_DAYS: i64 = 90;

/// The row's `status`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallStatus {
    Ok,
    Denied,
    Invalid,
    Failed,
    Upstream,
}

impl CallStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            CallStatus::Ok => "ok",
            CallStatus::Denied => "denied",
            CallStatus::Invalid => "invalid",
            CallStatus::Failed => "failed",
            CallStatus::Upstream => "upstream",
        }
    }
}

/// What happened to one call.
#[derive(Clone, Copy, Debug, Default)]
pub struct CallOutcome {
    pub denied: bool,
    pub invalid: bool,
    pub upstream_error: bool,
    pub error: bool,
}

/// First match wins: denied, invalid, upstream, failed, ok.
pub fn call_status(outcome: CallOutcome) -> CallStatus {
    if outcome.denied {
        CallStatus::Denied
    } else if outcome.invalid {
        CallStatus::Invalid
    } else if outcome.upstream_error {
        CallStatus::Upstream
    } else if outcome.error {
        CallStatus::Failed
    } else {
        CallStatus::Ok
    }
}

fn push_string(out: &mut String, text: &str) {
    out.push('"');
    for c in text.chars() {
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

/// A number as ECMAScript prints it, for the integers and short decimals
/// the contract keeps to (`-0` → `0`).
fn push_number(out: &mut String, n: &serde_json::Number) {
    if let Some(i) = n.as_i64() {
        out.push_str(&i.to_string());
    } else if let Some(u) = n.as_u64() {
        out.push_str(&u.to_string());
    } else {
        let f = n.as_f64().unwrap_or(0.0);
        if !f.is_finite() {
            out.push_str("null");
        } else if f == 0.0 {
            out.push('0');
        } else if f.fract() == 0.0 && f.abs() < 1e21 {
            out.push_str(&format!("{f:.0}"));
        } else {
            // Rust's shortest round-trip print matches ECMAScript's for
            // decimals in ordinary range.
            out.push_str(&f.to_string());
        }
    }
}

fn push_canonical(out: &mut String, value: &Value) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => push_number(out, n),
        Value::String(s) => push_string(out, s),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                push_canonical(out, item);
            }
            out.push(']');
        }
        Value::Object(map) => {
            // Rust's `str` order IS code-point order (UTF-8 preserves it).
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (i, key) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                push_string(out, key);
                out.push(':');
                push_canonical(out, &map[key]);
            }
            out.push('}');
        }
    }
}

/// Canonical JSON: keys sorted by code point, recursively, no whitespace.
pub fn canonical_json(value: &Value) -> String {
    let mut out = String::new();
    push_canonical(&mut out, value);
    out
}

/// `sha256:` + the hex SHA-256 of the canonical JSON of the arguments
/// (absent or null = `{}`).
pub fn args_hash(args: Option<&Value>) -> String {
    let empty = Value::Object(Default::default());
    let args = match args {
        None | Some(Value::Null) => &empty,
        Some(args) => args,
    };
    let digest = Sha256::digest(canonical_json(args).as_bytes());
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    format!("{HASH_PREFIX}{hex}")
}

/// Is a row created at `created_ms` past retention at `now_ms` (Unix ms)?
pub fn audit_expired(created_ms: i64, now_ms: i64) -> bool {
    now_ms - created_ms >= RETENTION_DAYS * 86_400_000
}

/// `YYYY-MM-DDTHH:MM:SS[.fff]Z` → Unix ms; `None` for anything else (the
/// rows' `created_at` as the server prints it).
pub fn parse_utc_ms(text: &str) -> Option<i64> {
    let text = text.strip_suffix('Z')?;
    let (date, time) = text.split_once('T')?;
    let mut date = date.split('-').map(|part| part.parse::<i64>().ok());
    let (y, m, d) = (date.next()??, date.next()??, date.next()??);
    let (time, frac) = time.split_once('.').unwrap_or((time, "0"));
    let mut time = time.split(':').map(|part| part.parse::<i64>().ok());
    let (hh, mm, ss) = (time.next()??, time.next()??, time.next()??);
    let ms: i64 = format!("{frac:0<3}")[..3].parse().ok()?;
    // Days from civil (Howard Hinnant's algorithm).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(((days * 24 + hh) * 60 + mm) * 60_000 + ss * 1000 + ms)
}
