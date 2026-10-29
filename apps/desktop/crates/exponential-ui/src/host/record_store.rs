//! VAPP-161: the format's NEUTRAL records (docs/package-format.md
//! "Records"). A surface reads a kind through the binding
//! `records:<kind>?query` and writes through the host functions
//! `records.upsert` / `records.delete`; the HOST provides the store. Only
//! the pure query parse is mirrored here (`parseRecordsSource` of
//! `packages/exponential-ui/src/host/record-store.ts`; the store and the
//! value derivation stay on each host). `fixtures/host-package.json`
//! `recordsQuery` replays it.

use serde::{Deserialize, Serialize};

use super::sources::ParsedSource;
use super::{js_trim, utf16_cmp};

/// The binding scheme: `records:<kind>[?query]`.
pub const RECORDS_SCHEME: &str = "records";
/// The write functions (no consent card: the app's own rows).
pub const RECORDS_FUNCTIONS: &[&str] = &["records.upsert", "records.delete"];
/// The neutral query's defaults (catalog/host.json `package.records.query`).
pub const RECORDS_SORT_DEFAULT: &str = "createdAt";
pub const RECORDS_ORDER_DEFAULT: &str = "desc";
pub const RECORDS_LIMIT_DEFAULT: u32 = 100;
pub const RECORDS_LIMIT_MAX: u32 = 500;
/// Query keys a host may give a meaning (Exponential does); never a field filter.
pub const RECORDS_RESERVED_PARAMS: &[&str] = &["app", "by", "kind", "limit", "mine", "month", "order", "sort", "sum", "user"];

const OPS: &[&str] = &["gt", "gte", "lt", "lte"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordsFilter {
    pub field: String,
    /// `eq` | `gt` | `gte` | `lt` | `lte`.
    pub op: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordsQuery {
    pub kind: String,
    pub mine: bool,
    pub sort: String,
    /// `asc` | `desc`.
    pub order: String,
    pub limit: u32,
    /// By field, then operator.
    pub filters: Vec<RecordsFilter>,
}

/// JavaScript's `Number(string)`: trimmed, empty = 0, `0x` / `0o` / `0b`
/// integers, `[+-]Infinity`, a decimal literal; anything else NaN.
fn js_string_number(s: &str) -> f64 {
    let t = js_trim(s);
    if t.is_empty() {
        return 0.0;
    }
    for (prefix, radix) in [("0x", 16), ("0X", 16), ("0o", 8), ("0O", 8), ("0b", 2), ("0B", 2)] {
        if let Some(digits) = t.strip_prefix(prefix) {
            if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
                return f64::NAN;
            }
            return digits.chars().fold(0.0, |n, c| n * f64::from(radix) + f64::from(c.to_digit(radix).unwrap_or(0)));
        }
    }
    let (sign, body) = match t.as_bytes()[0] {
        b'-' => (-1.0, &t[1..]),
        b'+' => (1.0, &t[1..]),
        _ => (1.0, t),
    };
    if body == "Infinity" {
        return sign * f64::INFINITY;
    }
    let b = body.as_bytes();
    let mut i = 0;
    let digits = |i: &mut usize| {
        let start = *i;
        while b.get(*i).is_some_and(u8::is_ascii_digit) {
            *i += 1;
        }
        *i - start
    };
    let int = digits(&mut i);
    let frac = if b.get(i) == Some(&b'.') {
        i += 1;
        digits(&mut i)
    } else {
        0
    };
    if int == 0 && frac == 0 {
        return f64::NAN;
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(b.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        if digits(&mut i) == 0 {
            return f64::NAN;
        }
    }
    if i != b.len() {
        return f64::NAN;
    }
    body.parse::<f64>().map(|n| sign * n).unwrap_or(f64::NAN)
}

/// `records:<kind>?…` (as [`super::parse_source`] gives it) → the query;
/// `None` = not a records source.
pub fn parse_records_source(source: &ParsedSource) -> Option<RecordsQuery> {
    if source.scheme != RECORDS_SCHEME {
        return None;
    }
    let p = &source.params;
    let limit_raw = p.get("limit").map_or(f64::NAN, |l| js_string_number(l));
    let mut keys: Vec<&String> = p.keys().collect();
    keys.sort_by(|a, b| utf16_cmp(a, b));
    let mut filters = Vec::new();
    for key in keys {
        if RECORDS_RESERVED_PARAMS.contains(&key.as_str()) {
            continue;
        }
        let value = p[key].clone();
        match key.rfind('.') {
            Some(dot) if dot > 0 && OPS.contains(&&key[dot + 1..]) => filters.push(RecordsFilter { field: key[..dot].to_string(), op: key[dot + 1..].to_string(), value }),
            _ => filters.push(RecordsFilter { field: key.clone(), op: "eq".into(), value }),
        }
    }
    filters.sort_by(|a, b| utf16_cmp(&a.field, &b.field).then_with(|| utf16_cmp(&a.op, &b.op)));
    let sort = p.get("sort").map(|s| js_trim(s)).filter(|s| !s.is_empty()).unwrap_or(RECORDS_SORT_DEFAULT);
    let order = match p.get("order").map(String::as_str) {
        Some("asc") => "asc",
        Some("desc") => "desc",
        _ => RECORDS_ORDER_DEFAULT,
    };
    let limit = if limit_raw.is_finite() && limit_raw.fract() == 0.0 && limit_raw > 0.0 { limit_raw.min(f64::from(RECORDS_LIMIT_MAX)) as u32 } else { RECORDS_LIMIT_DEFAULT };
    Some(RecordsQuery { kind: source.name.clone(), mine: matches!(p.get("mine").map(String::as_str), Some("1" | "true")), sort: sort.to_string(), order: order.to_string(), limit, filters })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_parses_like_js() {
        assert_eq!(js_string_number(" 12 "), 12.0);
        assert_eq!(js_string_number("0x10"), 16.0);
        assert_eq!(js_string_number("1e2"), 100.0);
        assert_eq!(js_string_number(""), 0.0);
        assert!(js_string_number("12px").is_nan());
        assert!(js_string_number("inf").is_nan());
        assert!(js_string_number("-0x1").is_nan());
        assert_eq!(js_string_number("-Infinity"), f64::NEG_INFINITY);
    }
}
