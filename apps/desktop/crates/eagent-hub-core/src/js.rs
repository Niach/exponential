//! The few JavaScript semantics the copy depends on, byte for byte: string
//! length and `.slice` in UTF-16 code units, `.trim()`'s whitespace set,
//! `JSON.stringify(value, null, 2)`, `Number#toString` and `Date.parse` of
//! an ISO / RFC 3339 stamp.

use serde_json::Value;

/// JavaScript's WhiteSpace + LineTerminator (`String#trim`).
fn is_js_space(c: char) -> bool {
    (c.is_whitespace() && c != '\u{85}') || c == '\u{FEFF}'
}

pub fn trim(s: &str) -> &str {
    s.trim_matches(is_js_space)
}

pub fn trim_end(s: &str) -> &str {
    s.trim_end_matches(is_js_space)
}

/// `s.length`.
pub fn len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// `s.slice(0, end)`. A surrogate pair cut in half leaves its high half,
/// which Rust cannot hold: it becomes U+FFFD.
pub fn slice(s: &str, end: usize) -> String {
    let mut out = String::new();
    let mut units = 0;
    for c in s.chars() {
        let width = c.len_utf16();
        if units + width > end {
            if units < end {
                out.push('\u{FFFD}');
            }
            break;
        }
        out.push(c);
        units += width;
    }
    out
}

/// `Number#toString` for a finite double.
pub fn number(x: f64) -> String {
    if x == 0.0 {
        return "0".into();
    }
    if !x.is_finite() {
        return if x.is_nan() { "NaN".into() } else if x > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    let sign = if x < 0.0 { "-" } else { "" };
    // Rust's `{:e}` is the shortest round-trip digits, like ECMAScript's.
    let sci = format!("{:e}", x.abs());
    let (mantissa, exp) = sci.split_once('e').unwrap_or((&sci, "0"));
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let k = digits.len() as i64;
    let n = exp.parse::<i64>().unwrap_or(0) + 1;
    let body = if k <= n && n <= 21 {
        format!("{digits}{}", "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
    } else if -6 < n && n <= 0 {
        format!("0.{}{digits}", "0".repeat((-n) as usize))
    } else {
        let e = n - 1;
        let e = if e >= 0 { format!("+{e}") } else { e.to_string() };
        if k == 1 {
            format!("{digits}e{e}")
        } else {
            format!("{}.{}e{e}", &digits[..1], &digits[1..])
        }
    };
    format!("{sign}{body}")
}

fn json_number(n: &serde_json::Number) -> String {
    match (n.as_i64(), n.as_u64()) {
        (Some(i), _) if i.unsigned_abs() <= 1 << 53 => i.to_string(),
        (_, Some(u)) if u <= 1 << 53 => u.to_string(),
        _ => number(n.as_f64().unwrap_or(f64::NAN)),
    }
}

fn json_string(s: &str, out: &mut String) {
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

fn stringify_into(value: &Value, indent: &str, out: &mut String) {
    let inner = format!("{indent}  ");
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => out.push_str(&json_number(n)),
        Value::String(s) => json_string(s, out),
        Value::Array(items) if items.is_empty() => out.push_str("[]"),
        Value::Array(items) => {
            out.push_str("[\n");
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(",\n");
                }
                out.push_str(&inner);
                stringify_into(item, &inner, out);
            }
            out.push('\n');
            out.push_str(indent);
            out.push(']');
        }
        Value::Object(map) if map.is_empty() => out.push_str("{}"),
        Value::Object(map) => {
            out.push_str("{\n");
            for (i, (key, item)) in map.iter().enumerate() {
                if i > 0 {
                    out.push_str(",\n");
                }
                out.push_str(&inner);
                json_string(key, out);
                out.push_str(": ");
                stringify_into(item, &inner, out);
            }
            out.push('\n');
            out.push_str(indent);
            out.push('}');
        }
    }
}

/// `JSON.stringify(value, null, 2)` (objects keep their source order: serde_json `preserve_order`).
pub fn stringify_pretty(value: &Value) -> String {
    let mut out = String::new();
    stringify_into(value, "", &mut out);
    out
}

/// `String(value)` for a JSON value.
pub fn to_string(value: &Value) -> String {
    match value {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => json_number(n),
        Value::String(s) => s.clone(),
        Value::Array(items) => items.iter().map(|v| if v.is_null() { String::new() } else { to_string(v) }).collect::<Vec<_>>().join(","),
        Value::Object(_) => "[object Object]".into(),
    }
}

fn digits(s: &[u8], at: &mut usize, n: usize) -> Option<i64> {
    let part = s.get(*at..*at + n)?;
    if !part.iter().all(u8::is_ascii_digit) {
        return None;
    }
    *at += n;
    std::str::from_utf8(part).ok()?.parse().ok()
}

fn expect(s: &[u8], at: &mut usize, c: u8) -> Option<()> {
    (s.get(*at) == Some(&c)).then(|| *at += 1)
}

/// Days since 1970-01-01 of a proleptic Gregorian date (month 1-12; out-of-range days roll over like `MakeDay`).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `Date.parse` of an ISO date (`YYYY-MM-DD`, UTC) or an RFC 3339 date-time
/// with its offset (`Z` or `±HH:MM`, optional fraction); anything else = None
/// (NaN). A date-time without an offset is local time in JavaScript, so it
/// is refused here rather than guessed.
pub fn date_parse(stamp: &str) -> Option<i64> {
    let s = stamp.as_bytes();
    let mut at = 0;
    let year = match s.first()? {
        b'+' | b'-' => {
            let negative = s[0] == b'-';
            at = 1;
            let y = digits(s, &mut at, 6)?;
            if negative && y == 0 {
                return None;
            }
            if negative { -y } else { y }
        }
        _ => digits(s, &mut at, 4)?,
    };
    expect(s, &mut at, b'-')?;
    let month = digits(s, &mut at, 2)?;
    expect(s, &mut at, b'-')?;
    let day = digits(s, &mut at, 2)?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let date = days_from_civil(year, month, day) * 86_400_000;
    if at == s.len() {
        return Some(date);
    }
    if !matches!(s[at], b'T' | b't') {
        return None;
    }
    at += 1;
    let hour = digits(s, &mut at, 2)?;
    expect(s, &mut at, b':')?;
    let minute = digits(s, &mut at, 2)?;
    let mut second = 0;
    let mut millis = 0;
    if s.get(at) == Some(&b':') {
        at += 1;
        second = digits(s, &mut at, 2)?;
        if s.get(at) == Some(&b'.') {
            at += 1;
            let start = at;
            while s.get(at).is_some_and(u8::is_ascii_digit) {
                at += 1;
            }
            if at == start {
                return None;
            }
            let frac = std::str::from_utf8(&s[start..at]).ok()?;
            let ms = format!("{:0<3}", &frac[..frac.len().min(3)]);
            millis = ms.parse::<i64>().ok()?;
        }
    }
    let valid_time = (hour < 24 && minute < 60 && second < 60) || (hour == 24 && minute == 0 && second == 0 && millis == 0);
    if !valid_time {
        return None;
    }
    let offset = match s.get(at)? {
        b'Z' | b'z' => {
            at += 1;
            0
        }
        sign @ (b'+' | b'-') => {
            let negative = *sign == b'-';
            at += 1;
            let h = digits(s, &mut at, 2)?;
            expect(s, &mut at, b':')?;
            let m = digits(s, &mut at, 2)?;
            if h > 23 || m > 59 {
                return None;
            }
            let minutes = h * 60 + m;
            if negative { -minutes } else { minutes }
        }
        _ => return None,
    };
    if at != s.len() {
        return None;
    }
    Some(date + ((hour * 60 + minute - offset) * 60 + second) * 1000 + millis)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_print_like_javascript() {
        for (x, want) in [
            (1.0, "1"),
            (-0.0, "0"),
            (0.1, "0.1"),
            (1.5e-7, "1.5e-7"),
            (0.000001, "0.000001"),
            (1e21, "1e+21"),
            (123456789012345680000.0, "123456789012345680000"),
            (2.5e25, "2.5e+25"),
            (-12.75, "-12.75"),
        ] {
            assert_eq!(number(x), want, "{x}");
        }
    }

    #[test]
    fn stringify_matches_json_stringify() {
        let v: Value = serde_json::from_str(r#"{"b":[1,2.0,{}],"a":{"x":[],"y":"q\"\u0001é"},"n":null}"#).unwrap();
        assert_eq!(
            stringify_pretty(&v),
            "{\n  \"b\": [\n    1,\n    2,\n    {}\n  ],\n  \"a\": {\n    \"x\": [],\n    \"y\": \"q\\\"\\u0001é\"\n  },\n  \"n\": null\n}"
        );
    }

    #[test]
    fn utf16_slices_and_trims() {
        assert_eq!(len("a😀"), 3);
        assert_eq!(slice("a😀b", 3), "a😀");
        assert_eq!(slice("a😀b", 2), "a\u{FFFD}");
        assert_eq!(trim("\u{FEFF} x \u{2028}"), "x");
        assert_eq!(trim("\u{85}x"), "\u{85}x");
    }

    #[test]
    fn dates_parse_like_date_parse() {
        assert_eq!(date_parse("1970-01-01T00:05:00.000Z"), Some(300_000));
        assert_eq!(date_parse("2026-11-01T00:00:00Z"), Some(1_793_491_200_000));
        assert_eq!(date_parse("2026-11-01T01:00:00+01:00"), Some(1_793_491_200_000));
        assert_eq!(date_parse("1970-01-02"), Some(86_400_000));
        assert_eq!(date_parse("1970-01-01T00:00:00.5Z"), Some(500));
        assert_eq!(date_parse("soon"), None);
        assert_eq!(date_parse("2026-11-01T00:00:00"), None);
    }
}
