//! VAPP-289 (round 9 §6.1): the `formatString` template. `${<pointer>}` reads
//! the data, `${fn(name: value, …)}` calls a function: both become ordinary
//! dynamic values the caller's own evaluator resolves (bindings against the
//! scope, calls against the catalog the enclosing formatString resolved to:
//! `${…}` has no syntax for a catalog). `src/format-template.ts` is the twin;
//! `fixtures/a2ui10-functions.json` `evaluations` locks both.

use serde_json::{json, Map, Value};

use crate::functions::INDEX_FUNCTION;
use crate::ident::{is_identifier, is_xid_continue};
use crate::limits::MAX_DEPTH;

/// A template part that does not parse (§6.8).
pub const MALFORMED_TEMPLATE: &str = "formatString: malformed expression";

#[derive(Debug, Clone, PartialEq)]
pub enum TemplatePart {
    Text(String),
    Path(String),
    Call(Value),
    Malformed,
}

fn skip_space(text: &str, mut at: usize) -> usize {
    while text.as_bytes().get(at).is_some_and(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r')) {
        at += 1;
    }
    at
}

fn name_end(text: &str, from: usize) -> usize {
    text[from..].char_indices().find(|(_, c)| !is_xid_continue(*c)).map_or(text.len(), |(i, _)| from + i)
}

/// Is a `${…}` body a call: `name(` after optional spaces? The name and the
/// index of its `(`.
fn call_name(body: &str) -> Option<(&str, usize)> {
    let start = skip_space(body, 0);
    let from = if body[start..].starts_with('@') { start + 1 } else { start };
    let end = name_end(body, from);
    let name = &body[start..end];
    let open = skip_space(body, end);
    (body[open..].starts_with('(') && (name == INDEX_FUNCTION || is_identifier(name))).then_some((name, open))
}

/// The index of the `}` closing a body that starts at `from` (nested braces
/// and quoted strings skipped).
fn closing(text: &str, from: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 1;
    let mut i = from;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            quote @ (b'\'' | b'"') => {
                i += 1;
                while i < bytes.len() && bytes[i] != quote {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// The parts of a formatString template. `\${` is a literal `${`; an unclosed
/// `${` is literal text. `catalog` = what the enclosing call resolved to
/// (every call but `@index` names it).
pub fn parse_template(text: &str, catalog: &str) -> Vec<TemplatePart> {
    let mut parts = Vec::new();
    let mut literal = String::new();
    let mut rest = text;
    while let Some(at) = rest.find("${") {
        // `\${`: the backslash right before it.
        if rest[..at].ends_with('\\') {
            literal.push_str(&rest[..at - 1]);
            literal.push_str("${");
            rest = &rest[at + 2..];
            continue;
        }
        let call = call_name(&rest[at + 2..]).is_some();
        let Some(end) = (if call { closing(rest, at + 2) } else { rest[at + 2..].find('}').map(|i| at + 2 + i) }) else { break };
        literal.push_str(&rest[..at]);
        if !literal.is_empty() {
            parts.push(TemplatePart::Text(std::mem::take(&mut literal)));
        }
        let body = &rest[at + 2..end];
        parts.push(if call { whole(body, catalog, 1).map_or(TemplatePart::Malformed, TemplatePart::Call) } else { TemplatePart::Path(body.trim().to_string()) });
        rest = &rest[end + 1..];
    }
    literal.push_str(rest);
    if !literal.is_empty() {
        parts.push(TemplatePart::Text(literal));
    }
    parts
}

fn whole(body: &str, catalog: &str, depth: usize) -> Option<Value> {
    let (call, at) = call(body, 0, catalog, depth)?;
    (skip_space(body, at) == body.len()).then_some(call)
}

/// A call at `at`: its value and the index behind its `)`.
fn call(text: &str, at: usize, catalog: &str, depth: usize) -> Option<(Value, usize)> {
    if depth > MAX_DEPTH {
        return None;
    }
    let (name, open) = call_name(&text[at..])?;
    let mut args = Map::new();
    let mut at = skip_space(text, at + open + 1);
    while !text[at..].starts_with(')') {
        let end = name_end(text, at);
        let arg = &text[at..end];
        if !is_identifier(arg) {
            return None;
        }
        at = skip_space(text, end);
        if !text[at..].starts_with(':') {
            return None;
        }
        let (value, end) = crate::deep(|| value(text, at + 1, catalog, depth))?;
        args.insert(arg.to_string(), value);
        at = skip_space(text, end);
        if text[at..].starts_with(',') {
            at = skip_space(text, at + 1);
        } else if !text[at..].starts_with(')') {
            return None;
        }
    }
    let call = if name == INDEX_FUNCTION { json!({"@call": name, "args": args}) } else { json!({"@call": name, "catalogId": catalog, "args": args}) };
    Some((call, at + 1))
}

/// A quoted string, a number, true / false / null, `${…}`, a call, a pointer.
fn value(text: &str, at: usize, catalog: &str, depth: usize) -> Option<(Value, usize)> {
    let at = skip_space(text, at);
    let rest = &text[at..];
    if rest.starts_with("${") {
        let end = closing(text, at + 2)?;
        let body = &text[at + 2..end];
        if call_name(body).is_none() {
            let path = body.trim();
            return (!path.is_empty()).then(|| (json!({"@path": path}), end + 1));
        }
        return Some((whole(body, catalog, depth + 1)?, end + 1));
    }
    if rest.starts_with(['\'', '"']) {
        return string(text, at);
    }
    if let Some(len) = number_len(rest) {
        return Some((crate::json::number(rest[..len].parse().ok()?), at + len));
    }
    if call_name(rest).is_some() {
        return call(text, at, catalog, depth + 1);
    }
    let mut end = at;
    loop {
        let next = name_end(text, end);
        if next > end {
            end = next;
        } else if text[end..].starts_with(['/', '.', '-', '~']) {
            end += 1;
        } else {
            break;
        }
    }
    let token = &text[at..end];
    let value = match token {
        "" => return None,
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        "null" => Value::Null,
        path => json!({"@path": path}),
    };
    Some((value, end))
}

/// `-?[0-9]+(\.[0-9]+)?([eE][+-]?[0-9]+)?`
fn number_len(text: &str) -> Option<usize> {
    let b = text.as_bytes();
    let digits = |from: usize| b[from.min(b.len())..].iter().take_while(|c| c.is_ascii_digit()).count();
    let start = usize::from(b.first() == Some(&b'-'));
    let mut end = start + digits(start);
    if end == start {
        return None;
    }
    if b.get(end) == Some(&b'.') && digits(end + 1) > 0 {
        end += 1 + digits(end + 1);
    }
    if matches!(b.get(end), Some(b'e' | b'E')) {
        let sign = usize::from(matches!(b.get(end + 1), Some(b'+' | b'-')));
        let n = digits(end + 1 + sign);
        if n > 0 {
            end += 1 + sign + n;
        }
    }
    Some(end)
}

fn string(text: &str, at: usize) -> Option<(Value, usize)> {
    let mut chars = text[at..].char_indices();
    let (_, quote) = chars.next()?;
    let mut out = String::new();
    while let Some((i, c)) = chars.next() {
        if c == quote {
            return Some((Value::String(out), at + i + 1));
        }
        if c != '\\' {
            out.push(c);
            continue;
        }
        out.push(match chars.next()?.1 {
            'n' => '\n',
            't' => '\t',
            'r' => '\r',
            other => other,
        });
    }
    None
}
