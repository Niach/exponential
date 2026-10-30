//! Binding sources. A data model path can follow a host source
//! (`exp:issues?board=…`): the host registers one resolver per scheme, the
//! router emits a `bind` op, the resolver's emits land at the path.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::js_trim;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedSource {
    /// The source as written.
    pub uri: String,
    pub scheme: String,
    pub name: String,
    /// Keys sorted; the last value of a repeated key wins.
    pub params: BTreeMap<String, String>,
}

/// JavaScript's `decodeURIComponent(part.replace(/\+/g, " "))`, the RAW part
/// when it throws (a bad escape, invalid UTF-8).
fn decode(part: &str) -> String {
    fn hex(b: u8) -> Option<u8> {
        (b as char).to_digit(16).map(|d| d as u8)
    }
    let spaced = part.replace('+', " ");
    let bytes = spaced.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            match (bytes.get(i + 1).copied().and_then(hex), bytes.get(i + 2).copied().and_then(hex)) {
                (Some(h), Some(l)) => {
                    out.push(h * 16 + l);
                    i += 3;
                }
                _ => return part.to_string(),
            }
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| part.to_string())
}

fn is_line_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// `exp:issues?board=b1&limit=20` → `{scheme: "exp", name: "issues",
/// params: {board: "b1", limit: "20"}}`; `None` when it does not parse
/// (`catalog/host.json` sources.pattern).
pub fn parse_source(uri: &str) -> Option<ParsedSource> {
    let text = js_trim(uri);
    let (scheme, rest) = text.split_once(':')?;
    let mut chars = scheme.chars();
    if !chars.next().is_some_and(|c| c.is_ascii_alphabetic()) || !chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-')) {
        return None;
    }
    let (raw_name, query) = match rest.split_once('?') {
        Some((n, q)) => (n, Some(q)),
        None => (rest, None),
    };
    if raw_name.is_empty() || query.is_some_and(|q| q.chars().any(is_line_terminator)) {
        return None;
    }
    let name = decode(raw_name);
    if name.is_empty() {
        return None;
    }
    let mut params = BTreeMap::new();
    for pair in query.unwrap_or("").split('&') {
        if pair.is_empty() {
            continue;
        }
        let (k, v) = match pair.find('=') {
            Some(eq) => (&pair[..eq], Some(&pair[eq + 1..])),
            None => (pair, None),
        };
        let key = decode(k);
        if key.is_empty() {
            continue;
        }
        params.insert(key, v.map(decode).unwrap_or_default());
    }
    Some(ParsedSource { uri: uri.to_string(), scheme: scheme.to_lowercase(), name, params })
}

/// JavaScript's `encodeURIComponent`: every UTF-8 byte but
/// `A-Z a-z 0-9 - _ . ! ~ * ' ( )` as `%XX` (upper-case hex).
fn encode_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// VAPP-122: `source` with `params` appended as its query, for the `bind`
/// action (`sourceWithParams`): keys sorted, key and value percent-encoded,
/// a null value skipped, an object or array as JSON, after `?` or `&`. A
/// repeated key parses with the last value, so a param overrides the
/// source's own.
pub fn source_with_params(source: &str, params: Option<&serde_json::Value>) -> String {
    let Some(params) = params.and_then(serde_json::Value::as_object) else { return source.to_string() };
    let mut keys: Vec<&String> = params.keys().collect();
    keys.sort();
    let mut pairs = Vec::new();
    for key in keys {
        let v = &params[key];
        if v.is_null() {
            continue;
        }
        let text = match v {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Object(_) | serde_json::Value::Array(_) => v.to_string(),
            other => crate::json::to_js_string(other),
        };
        pairs.push(format!("{}={}", encode_component(key), encode_component(&text)));
    }
    if pairs.is_empty() {
        return source.to_string();
    }
    format!("{source}{}{}", if source.contains('?') { '&' } else { '?' }, pairs.join("&"))
}
