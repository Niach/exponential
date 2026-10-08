//! The surface DATA MODEL and A2UI's dynamic values: a prop may be a `{path}`
//! binding (a JSON Pointer into the model, relative inside a template item)
//! or a `{call, args}` client function; `resolve_value` turns a prop tree
//! into literals for one pass. Mirrors `packages/exponential-ui-react/src/data.ts`.

use serde_json::{Map, Value};

use crate::json;

pub fn is_binding(value: &Value) -> bool {
    value.as_object().is_some_and(|o| o.len() == 1 && o.get("path").is_some_and(Value::is_string))
}

pub fn is_call(value: &Value) -> bool {
    value.as_object().is_some_and(|o| o.get("call").is_some_and(Value::is_string))
}

fn unescape(token: &str) -> String {
    token.replace("~1", "/").replace("~0", "~")
}

/// A pointer's tokens; `` and `/` are the whole model.
pub fn pointer_tokens(pointer: &str) -> Vec<String> {
    if pointer.is_empty() || pointer == "/" {
        return Vec::new();
    }
    pointer.strip_prefix('/').unwrap_or(pointer).split('/').map(unescape).collect()
}

/// `path` made absolute against a template scope (`/items/3`).
pub fn absolute_path(path: &str, scope: &str) -> String {
    if path.starts_with('/') {
        return path.to_string();
    }
    if scope.is_empty() {
        return format!("/{path}");
    }
    if path.is_empty() {
        scope.to_string()
    } else {
        format!("{scope}/{path}")
    }
}

pub fn get_pointer<'a>(data: &'a Value, pointer: &str) -> Option<&'a Value> {
    let mut cur = data;
    for token in pointer_tokens(pointer) {
        cur = match cur {
            Value::Array(items) => items.get(token.parse::<usize>().ok()?)?,
            Value::Object(map) => map.get(&token)?,
            _ => return None,
        };
    }
    Some(cur)
}

/// Set (or remove with `None`) the value at `pointer`, creating objects (or
/// arrays for numeric tokens) on the way.
pub fn set_pointer(data: &mut Value, pointer: &str, value: Option<Value>) {
    let tokens = pointer_tokens(pointer);
    if tokens.is_empty() {
        *data = value.unwrap_or_else(|| Value::Object(Map::new()));
        return;
    }
    fn put(cur: &mut Value, tokens: &[String], value: Option<Value>) {
        let token = &tokens[0];
        let last = tokens.len() == 1;
        let numeric = token.chars().all(|c| c.is_ascii_digit()) && !token.is_empty();
        if !(cur.is_object() || cur.is_array()) {
            *cur = if numeric { Value::Array(Vec::new()) } else { Value::Object(Map::new()) };
        }
        match cur {
            Value::Array(items) => {
                let idx = if token == "-" { items.len() } else { token.parse::<usize>().unwrap_or(items.len()) };
                if last {
                    match value {
                        None => {
                            if idx < items.len() {
                                items.remove(idx);
                            }
                        }
                        Some(v) => {
                            while items.len() <= idx {
                                items.push(Value::Null);
                            }
                            items[idx] = v;
                        }
                    }
                } else {
                    while items.len() <= idx {
                        items.push(Value::Null);
                    }
                    put(&mut items[idx], &tokens[1..], value);
                }
            }
            Value::Object(map) => {
                if last {
                    match value {
                        None => {
                            map.remove(token);
                        }
                        Some(v) => {
                            map.insert(token.clone(), v);
                        }
                    }
                } else {
                    let entry = map.entry(token.clone()).or_insert(Value::Null);
                    put(entry, &tokens[1..], value);
                }
            }
            _ => unreachable!(),
        }
    }
    put(data, &tokens, value);
}

/// What `resolve_value` needs.
pub struct ResolveContext<'a> {
    pub data: &'a Value,
    /// The template item's pointer, for relative paths.
    pub scope: &'a str,
}

fn empty(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::String(s) => s.is_empty(),
        Value::Array(a) => a.is_empty(),
        _ => false,
    }
}

fn num(v: Option<&Value>) -> f64 {
    v.map(json::to_number).unwrap_or(f64::NAN)
}

fn interpolate(text: &str, ctx: &ResolveContext) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find('}') {
            Some(end) => {
                let expr = after[..end].trim();
                let v = get_pointer(ctx.data, &absolute_path(expr, ctx.scope));
                if let Some(v) = v.filter(|v| !v.is_null()) {
                    out.push_str(&json::to_js_string(v));
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

fn group_thousands(int: &str) -> String {
    let bytes: Vec<char> = int.chars().collect();
    let mut out = String::new();
    for (i, c) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(*c);
    }
    out
}

/// `en` number formatting with min/max fraction digits (`toLocaleString`).
pub fn format_number(n: f64, min_digits: usize, max_digits: usize) -> String {
    if !n.is_finite() {
        return String::new();
    }
    let rounded = format!("{:.*}", max_digits, n.abs());
    let (int, frac) = rounded.split_once('.').unwrap_or((&rounded, ""));
    let mut frac = frac.to_string();
    while frac.len() > min_digits && frac.ends_with('0') {
        frac.pop();
    }
    let mut out = String::new();
    if n < 0.0 && !rounded.trim_matches(|c| c == '0' || c == '.').is_empty() {
        out.push('-');
    }
    out.push_str(&group_thousands(int));
    if !frac.is_empty() {
        out.push('.');
        out.push_str(&frac);
    }
    out
}

fn plural_category(n: f64) -> &'static str {
    if n == 1.0 {
        "one"
    } else {
        "other"
    }
}

fn format_date(value: &str, pattern: Option<&str>) -> String {
    // yyyy-mm-dd[Thh:mm[:ss]] only (no timezone math; the host formats richer dates).
    let date = value.get(0..10).unwrap_or("");
    let mut parts = date.split('-');
    let (Some(y), Some(m), Some(d)) = (parts.next(), parts.next(), parts.next()) else { return String::new() };
    let (Ok(y), Ok(m), Ok(d)) = (y.parse::<i32>(), m.parse::<u32>(), d.parse::<u32>()) else { return String::new() };
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return String::new();
    }
    let time = value.get(11..19).unwrap_or("00:00:00");
    let mut t = time.split(':');
    let hh = t.next().and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
    let mm = t.next().and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
    let ss = t.next().and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
    const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
    const DAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
    // Zeller-style weekday.
    let (zy, zm) = if m < 3 { (y - 1, m + 12) } else { (y, m) };
    let k = zy % 100;
    let j = zy / 100;
    let h = (d as i32 + (13 * (zm as i32 + 1)) / 5 + k + k / 4 + j / 4 + 5 * j) % 7;
    let weekday = ((h + 6) % 7) as usize; // 0 = Sunday
    let Some(pattern) = pattern else {
        return format!("{m}/{d}/{y}");
    };
    let h12 = if hh % 12 == 0 { 12 } else { hh % 12 };
    let mut out = String::new();
    let mut rest = pattern;
    let tokens = ["yyyy", "yy", "MMMM", "MMM", "MM", "M", "dd", "d", "EEEE", "EEE", "HH", "H", "hh", "h", "mm", "ss", "a"];
    'outer: while !rest.is_empty() {
        for tok in tokens {
            if let Some(after) = rest.strip_prefix(tok) {
                let rep = match tok {
                    "yyyy" => y.to_string(),
                    "yy" => format!("{:02}", y % 100),
                    "MMMM" => MONTHS[(m - 1) as usize].to_string(),
                    "MMM" => MONTHS[(m - 1) as usize][..3].to_string(),
                    "MM" => format!("{m:02}"),
                    "M" => m.to_string(),
                    "dd" => format!("{d:02}"),
                    "d" => d.to_string(),
                    "EEEE" => DAYS[weekday].to_string(),
                    "EEE" => DAYS[weekday][..3].to_string(),
                    "HH" => format!("{hh:02}"),
                    "H" => hh.to_string(),
                    "hh" => format!("{h12:02}"),
                    "h" => h12.to_string(),
                    "mm" => format!("{mm:02}"),
                    "ss" => format!("{ss:02}"),
                    "a" => if hh < 12 { "AM".into() } else { "PM".into() },
                    _ => unreachable!(),
                };
                out.push_str(&rep);
                rest = after;
                continue 'outer;
            }
        }
        let ch = rest.chars().next().unwrap();
        out.push(ch);
        rest = &rest[ch.len_utf8()..];
    }
    out
}

/// The 14 client functions of the catalog. `openUrl` returns the url as a
/// string the surface turns into an event for the host.
pub fn call_function(name: &str, args: &Map<String, Value>, ctx: &ResolveContext) -> Option<Value> {
    let a = |k: &str| args.get(k);
    let s = |k: &str| a(k).filter(|v| !v.is_null()).map(json::to_js_string).unwrap_or_default();
    Some(match name {
        "required" => Value::Bool(!a("value").is_none_or(empty)),
        "regex" => Value::Bool(regex_test(&s("pattern"), &s("value"))),
        "length" => {
            let n = s("value").chars().count() as f64;
            Value::Bool(a("min").is_none_or(|m| n >= json::to_number(m)) && a("max").is_none_or(|m| n <= json::to_number(m)))
        }
        "numeric" => {
            let n = num(a("value"));
            Value::Bool(n.is_finite() && a("min").is_none_or(|m| n >= json::to_number(m)) && a("max").is_none_or(|m| n <= json::to_number(m)))
        }
        "email" => {
            let v = s("value");
            let ok = v.split_once('@').is_some_and(|(local, domain)| {
                !local.is_empty() && !local.contains(char::is_whitespace) && domain.contains('.') && !domain.contains('@') && !domain.starts_with('.') && !domain.ends_with('.') && !domain.contains(char::is_whitespace)
            });
            Value::Bool(ok)
        }
        "formatString" => Value::String(interpolate(&s("value"), ctx)),
        "formatNumber" => {
            let n = num(a("value"));
            if !n.is_finite() {
                return Some(Value::String(String::new()));
            }
            let decimals = a("decimals").map(json::to_number).map(|d| d as usize);
            Value::String(format_number(n, decimals.unwrap_or(0), decimals.unwrap_or(2)))
        }
        "formatCurrency" => {
            let n = num(a("value"));
            let currency = a("currency").map(json::to_js_string).unwrap_or_else(|| "USD".into());
            let decimals = a("decimals").map(json::to_number).map(|d| d as usize).unwrap_or(2);
            let body = format_number(n.abs(), decimals, decimals);
            let symbol = match currency.as_str() {
                "USD" => "$",
                "EUR" => "€",
                "GBP" => "£",
                "JPY" => "¥",
                _ => "",
            };
            let sign = if n < 0.0 { "-" } else { "" };
            Value::String(if symbol.is_empty() { format!("{sign}{currency} {body}") } else { format!("{sign}{symbol}{body}") })
        }
        "formatDate" => Value::String(format_date(&s("value"), a("format").filter(|v| !v.is_null()).and_then(Value::as_str))),
        "pluralize" => {
            let n = num(a("value"));
            if n == 0.0 {
                if let Some(zero) = a("zero").filter(|v| !v.is_null()) {
                    return Some(zero.clone());
                }
            }
            let category = plural_category(n);
            a(category).filter(|v| !v.is_null()).or_else(|| a("other").filter(|v| !v.is_null())).cloned().unwrap_or(Value::String(String::new()))
        }
        "openUrl" => return a("url").filter(|v| v.is_string()).cloned(),
        "and" => Value::Bool(a("values").and_then(Value::as_array).is_some_and(|v| v.iter().all(truthy))),
        "or" => Value::Bool(a("values").and_then(Value::as_array).is_some_and(|v| v.iter().any(truthy))),
        "not" => Value::Bool(!a("value").is_some_and(truthy)),
        _ => return None,
    })
}

fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|x| x != 0.0 && !x.is_nan()),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

/// A tiny regex subset for `regex` checks: literals, `.`, `*`, `+`, `?`,
/// `^`/`$` anchors, character classes `[...]` (ranges, negation), `\d \w \s`
/// and alternation-free groups are NOT supported (returns false). Hosts with
/// a real engine override through their painter; the core keeps the surface
/// deterministic without a regex dependency.
fn regex_test(pattern: &str, value: &str) -> bool {
    simple_regex::is_match(pattern, value)
}

mod simple_regex {
    #[derive(Debug, Clone)]
    enum Atom {
        Any,
        Char(char),
        Class(Vec<(char, char)>, bool),
    }
    #[derive(Debug, Clone)]
    struct Piece {
        atom: Atom,
        min: usize,
        max: Option<usize>,
    }

    fn parse(pattern: &str) -> Option<(bool, bool, Vec<Piece>)> {
        let mut chars: Vec<char> = pattern.chars().collect();
        let anchored_start = chars.first() == Some(&'^');
        if anchored_start {
            chars.remove(0);
        }
        let anchored_end = chars.last() == Some(&'$');
        if anchored_end {
            chars.pop();
        }
        let mut pieces = Vec::new();
        let mut i = 0;
        while i < chars.len() {
            let atom = match chars[i] {
                '.' => Atom::Any,
                '\\' => {
                    i += 1;
                    match chars.get(i)? {
                        'd' => Atom::Class(vec![('0', '9')], false),
                        'w' => Atom::Class(vec![('a', 'z'), ('A', 'Z'), ('0', '9'), ('_', '_')], false),
                        's' => Atom::Class(vec![(' ', ' '), ('\t', '\t'), ('\n', '\n'), ('\r', '\r')], false),
                        c => Atom::Char(*c),
                    }
                }
                '[' => {
                    i += 1;
                    let negated = chars.get(i) == Some(&'^');
                    if negated {
                        i += 1;
                    }
                    let mut ranges = Vec::new();
                    while i < chars.len() && chars[i] != ']' {
                        let mut c = chars[i];
                        if c == '\\' {
                            i += 1;
                            c = *chars.get(i)?;
                        }
                        if chars.get(i + 1) == Some(&'-') && chars.get(i + 2).is_some_and(|e| *e != ']') {
                            ranges.push((c, chars[i + 2]));
                            i += 3;
                        } else {
                            ranges.push((c, c));
                            i += 1;
                        }
                    }
                    if i >= chars.len() {
                        return None;
                    }
                    Atom::Class(ranges, negated)
                }
                '(' | ')' | '|' => return None,
                c => Atom::Char(c),
            };
            i += 1;
            let (min, max) = match chars.get(i) {
                Some('*') => {
                    i += 1;
                    (0, None)
                }
                Some('+') => {
                    i += 1;
                    (1, None)
                }
                Some('?') => {
                    i += 1;
                    (0, Some(1))
                }
                Some('{') => {
                    let close = chars[i..].iter().position(|c| *c == '}')? + i;
                    let body: String = chars[i + 1..close].iter().collect();
                    i = close + 1;
                    match body.split_once(',') {
                        Some((a, b)) => (a.trim().parse().ok()?, if b.trim().is_empty() { None } else { Some(b.trim().parse().ok()?) }),
                        None => {
                            let n: usize = body.trim().parse().ok()?;
                            (n, Some(n))
                        }
                    }
                }
                _ => (1, Some(1)),
            };
            pieces.push(Piece { atom, min, max });
        }
        Some((anchored_start, anchored_end, pieces))
    }

    fn matches_atom(atom: &Atom, c: char) -> bool {
        match atom {
            Atom::Any => c != '\n',
            Atom::Char(x) => *x == c,
            Atom::Class(ranges, negated) => ranges.iter().any(|(a, b)| *a <= c && c <= *b) != *negated,
        }
    }

    fn match_here(pieces: &[Piece], text: &[char], end_anchor: bool) -> bool {
        let Some(piece) = pieces.first() else {
            return !end_anchor || text.is_empty();
        };
        let max = piece.max.unwrap_or(text.len());
        let mut count = 0;
        while count < max && count < text.len() && matches_atom(&piece.atom, text[count]) {
            count += 1;
        }
        if count < piece.min {
            return false;
        }
        let mut n = count;
        loop {
            if match_here(&pieces[1..], &text[n..], end_anchor) {
                return true;
            }
            if n == piece.min {
                return false;
            }
            n -= 1;
        }
    }

    pub fn is_match(pattern: &str, value: &str) -> bool {
        let Some((start, end, pieces)) = parse(pattern) else { return false };
        let text: Vec<char> = value.chars().collect();
        if start {
            return match_here(&pieces, &text, end);
        }
        (0..=text.len()).any(|i| match_here(&pieces, &text[i..], end))
    }
}

/// A prop value with its bindings and calls resolved. Plain objects recurse
/// (a check's `condition`, a menu item's label). `None` = undefined.
pub fn resolve_value(value: &Value, ctx: &ResolveContext) -> Option<Value> {
    if is_binding(value) {
        let path = value["path"].as_str().unwrap_or("");
        return get_pointer(ctx.data, &absolute_path(path, ctx.scope)).cloned();
    }
    if is_call(value) {
        let name = value["call"].as_str().unwrap_or("");
        let mut args = Map::new();
        if let Some(raw) = value.get("args").and_then(Value::as_object) {
            for (k, v) in raw {
                if let Some(r) = resolve_value(v, ctx) {
                    args.insert(k.clone(), r);
                }
            }
        }
        return call_function(name, &args, ctx);
    }
    match value {
        Value::Array(items) => Some(Value::Array(items.iter().map(|i| resolve_value(i, ctx).unwrap_or(Value::Null)).collect())),
        Value::Object(map) => {
            let mut out = Map::new();
            for (k, v) in map {
                if let Some(r) = resolve_value(v, ctx) {
                    out.insert(k.clone(), r);
                }
            }
            Some(Value::Object(out))
        }
        other => Some(other.clone()),
    }
}

/// Every binding path a prop tree reads (absolute), for dependency tracking.
pub fn binding_paths(value: &Value, scope: &str, out: &mut Vec<String>) {
    if is_binding(value) {
        out.push(absolute_path(value["path"].as_str().unwrap_or(""), scope));
        return;
    }
    match value {
        Value::Array(items) => items.iter().for_each(|i| binding_paths(i, scope, out)),
        Value::Object(map) => map.values().for_each(|v| binding_paths(v, scope, out)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn pointers_get_and_set() {
        let mut data = json!({"user": {"name": "Ada"}, "items": [1, 2]});
        assert_eq!(get_pointer(&data, "/user/name"), Some(&json!("Ada")));
        assert_eq!(get_pointer(&data, "/items/1"), Some(&json!(2)));
        set_pointer(&mut data, "/user/age", Some(json!(36)));
        set_pointer(&mut data, "/items/-", Some(json!(3)));
        set_pointer(&mut data, "/items/0", None);
        assert_eq!(data, json!({"user": {"name": "Ada", "age": 36}, "items": [2, 3]}));
        set_pointer(&mut data, "/new/deep/key", Some(json!(true)));
        assert_eq!(get_pointer(&data, "/new/deep/key"), Some(&json!(true)));
        assert_eq!(absolute_path("name", "/items/3"), "/items/3/name");
        assert_eq!(absolute_path("", "/items/3"), "/items/3");
        assert_eq!(absolute_path("/x", "/items/3"), "/x");
        assert_eq!(absolute_path("x", ""), "/x");
    }

    #[test]
    fn functions_resolve() {
        let data = json!({"n": 3, "email": "a@b.co", "price": 1234.5});
        let ctx = ResolveContext { data: &data, scope: "" };
        let call = |name: &str, args: Value| resolve_value(&json!({"call": name, "args": args}), &ctx);
        assert_eq!(call("required", json!({"value": {"path": "/email"}})), Some(json!(true)));
        assert_eq!(call("email", json!({"value": {"path": "/email"}})), Some(json!(true)));
        assert_eq!(call("regex", json!({"value": "ab12", "pattern": "^[a-z]+\\d{2}$"})), Some(json!(true)));
        assert_eq!(call("regex", json!({"value": "ab1", "pattern": "^[a-z]+\\d{2}$"})), Some(json!(false)));
        assert_eq!(call("formatNumber", json!({"value": {"path": "/price"}})), Some(json!("1,234.5")));
        assert_eq!(call("formatNumber", json!({"value": 1234.5, "decimals": 2})), Some(json!("1,234.50")));
        assert_eq!(call("formatCurrency", json!({"value": 1234.5, "currency": "USD"})), Some(json!("$1,234.50")));
        assert_eq!(call("formatString", json!({"value": "n = ${/n}"})), Some(json!("n = 3")));
        assert_eq!(call("pluralize", json!({"value": {"path": "/n"}, "one": "item", "other": "items"})), Some(json!("items")));
        assert_eq!(call("formatDate", json!({"value": "2026-10-07", "format": "d MMM yyyy"})), Some(json!("7 Oct 2026")));
        assert_eq!(call("formatDate", json!({"value": "2026-10-07", "format": "EEE"})), Some(json!("Wed")));
        assert_eq!(call("and", json!({"values": [true, 1]})), Some(json!(true)));
        assert_eq!(call("not", json!({"value": ""})), Some(json!(true)));
        assert_eq!(call("length", json!({"value": "abc", "min": 2, "max": 3})), Some(json!(true)));
        assert_eq!(call("numeric", json!({"value": "12", "min": 10})), Some(json!(true)));
    }

    #[test]
    fn template_scope_resolves_relative_paths() {
        let data = json!({"items": [{"name": "one"}, {"name": "two"}]});
        let ctx = ResolveContext { data: &data, scope: "/items/1" };
        assert_eq!(resolve_value(&json!({"path": "name"}), &ctx), Some(json!("two")));
        let mut paths = Vec::new();
        binding_paths(&json!({"a": {"path": "name"}, "b": [{"path": "/x"}]}), "/items/1", &mut paths);
        assert_eq!(paths, vec!["/items/1/name", "/x"]);
    }
}
