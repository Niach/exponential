//! VAPP-180: the MCP tools a package EXPOSES (docs/package-format.md
//! "Exposes"): declarative bindings `{name, title, description, input,
//! access, run}` a host's MCP broker lists and runs as the calling person,
//! never code. The format half of the eMCP contract; what a run may reach on
//! a given host is that host's own check. Mirrors
//! `packages/exponential-ui/src/host/exposes.ts` line by line;
//! `fixtures/host-router.json` (validation) replays it.

use serde::Serialize;
use serde_json::{Map, Value};

use super::js_trim;
use super::package::PackageIssue;

/// `package.exposes.limits`.
pub const EXPOSE_NAME_PATTERN: &str = "^[a-z][a-z0-9_]*$";
pub const EXPOSE_NAME_MAX: usize = 40;
pub const EXPOSE_TITLE_MAX: usize = 80;
pub const EXPOSE_DESCRIPTION_MAX: usize = 1024;
pub const EXPOSE_MAX_TOOLS: usize = 32;
pub const EXPOSE_SEPARATOR: &str = "__";
pub const EXPOSE_RUN_KINDS: &[&str] = &["records", "action", "upstream"];
pub const EXPOSE_RECORD_OPS: &[&str] = &["list", "get", "upsert"];
pub const EXPOSE_READ_RECORD_OPS: &[&str] = &["list", "get"];
pub const EXPOSE_READ_OP_WORDS: &[&str] = &["list", "get", "show", "catalog", "activity", "files"];
pub const EXPOSE_DESTRUCTIVE_WORDS: &[&str] = &["delete", "merge", "close", "end", "remove", "trash"];
/// `package.exposes.actionPattern`.
pub const EXPOSE_ACTION_PATTERN: &str = "^[a-z][a-z0-9_]*(\\.[a-z][a-z0-9_]*)+$";

/// One problem with a binding: the eMCP error `code`, the binding's `index`
/// (`None` = the list) and the package issue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExposeIssue {
    pub index: Option<usize>,
    pub code: String,
    pub path: String,
    pub message: String,
}

fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_'
}

fn is_ident(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// The path of a `${input.<path>}` template starting at `at`, and the index
/// after its `}`.
fn template_at(s: &[u8], at: usize) -> Option<(usize, usize)> {
    const OPEN: &[u8] = b"${input.";
    if !s[at..].starts_with(OPEN) {
        return None;
    }
    let start = at + OPEN.len();
    let mut i = start;
    loop {
        if !s.get(i).copied().is_some_and(is_ident_start) {
            return None;
        }
        while s.get(i).copied().is_some_and(is_ident) {
            i += 1;
        }
        if s.get(i) == Some(&b'.') && s.get(i + 1).copied().is_some_and(is_ident_start) {
            i += 1;
            continue;
        }
        break;
    }
    (s.get(i) == Some(&b'}')).then_some((i, i + 1))
}

fn string_templates(s: &str, out: &mut Vec<String>) {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if let Some((end, next)) = template_at(bytes, i) {
            out.push(s[i + 8..end].to_string());
            i = next;
        } else {
            i += 1;
        }
    }
}

/// The input paths a run's templates name (`${input.a.b}` → `a.b`), in
/// document order.
pub fn expose_template_paths(value: &Value) -> Vec<String> {
    fn walk(v: &Value, out: &mut Vec<String>) {
        match v {
            Value::String(s) => string_templates(s, out),
            Value::Array(items) => items.iter().for_each(|i| walk(i, out)),
            Value::Object(map) => map.values().for_each(|i| walk(i, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(value, &mut out);
    out
}

/// The words of a function or tool name (the lowercase letter runs).
fn words(name: &str) -> Vec<String> {
    name.to_lowercase().split(|c: char| !c.is_ascii_lowercase()).filter(|w| !w.is_empty()).map(str::to_string).collect()
}

/// An action reads when the last word of its name is a read word; a verb
/// writes.
pub fn expose_action_access(action: &str) -> &'static str {
    let last = words(action.rsplit('.').next().unwrap_or("")).pop();
    if last.is_some_and(|w| EXPOSE_READ_OP_WORDS.contains(&w.as_str())) {
        "read"
    } else {
        "write"
    }
}

/// A tool or action name holding a destructive word is never exposed.
pub fn expose_name_destructive(name: &str) -> bool {
    words(name).iter().any(|w| EXPOSE_DESTRUCTIVE_WORDS.contains(&w.as_str()))
}

fn is_expose_name(name: &str) -> bool {
    let b = name.as_bytes();
    b.first().is_some_and(u8::is_ascii_lowercase) && b.iter().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_')
}

/// [`EXPOSE_ACTION_PATTERN`].
fn is_action_name(name: &str) -> bool {
    let parts: Vec<&str> = name.split('.').collect();
    parts.len() >= 2 && parts.iter().all(|p| is_expose_name(p))
}

fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// Every problem with a package's `upstreams`: the MCP server names its
/// upstream bindings may call.
pub fn validate_upstreams(upstreams: &Value) -> Vec<PackageIssue> {
    let Some(list) = upstreams.as_array().filter(|l| l.iter().all(|u| u.as_str().is_some_and(|s| !s.is_empty()))) else {
        return vec![PackageIssue { path: "/upstreams".into(), message: "upstreams is a list of MCP server names".into() }];
    };
    let mut issues = Vec::new();
    for (i, name) in list.iter().enumerate() {
        if list.iter().position(|u| u == name) != Some(i) {
            issues.push(PackageIssue { path: format!("/upstreams/{i}"), message: format!("duplicate upstream {}", name.as_str().unwrap_or("")) });
        }
    }
    issues
}

/// Every problem with a package's `exposes`, in binding order, with its
/// eMCP code. `records` and `upstreams` = the package's.
pub fn expose_issues(exposes: &Value, records: Option<&Value>, upstreams: Option<&Value>) -> Vec<ExposeIssue> {
    let Some(list) = exposes.as_array() else {
        return vec![ExposeIssue { index: None, code: "run".into(), path: "/exposes".into(), message: "exposes is a list of tools".into() }];
    };
    let mut issues = Vec::new();
    if list.len() > EXPOSE_MAX_TOOLS {
        issues.push(ExposeIssue { index: None, code: "tooMany".into(), path: "/exposes".into(), message: format!("exposes lists at most {EXPOSE_MAX_TOOLS} tools") });
    }
    let kinds: Vec<&String> = records.and_then(Value::as_object).map(|r| r.keys().collect()).unwrap_or_default();
    let servers: Vec<&str> = upstreams.and_then(Value::as_array).map(|l| l.iter().filter_map(Value::as_str).filter(|s| !s.is_empty()).collect()).unwrap_or_default();
    let mut seen: Vec<&str> = Vec::new();
    let empty = Map::new();
    for (index, expose) in list.iter().enumerate() {
        let at = format!("/exposes/{index}");
        let mut fail = |code: &str, path: &str, message: String| issues.push(ExposeIssue { index: Some(index), code: code.into(), path: format!("{at}{path}"), message });
        let Some(expose) = expose.as_object() else {
            fail("run", "", "a tool is {name, title, description, input, access, run}".into());
            continue;
        };
        match expose.get("name").and_then(Value::as_str) {
            Some(name) if is_expose_name(name) && name.len() <= EXPOSE_NAME_MAX && !name.contains(EXPOSE_SEPARATOR) => {
                if seen.contains(&name) {
                    fail("duplicate", "/name", format!("duplicate tool {name}"));
                } else {
                    seen.push(name);
                }
            }
            _ => fail("name", "/name", format!("name is lowercase letters, digits and _ (at most {EXPOSE_NAME_MAX}, no {EXPOSE_SEPARATOR})")),
        }
        if expose.get("title").and_then(Value::as_str).is_none_or(|t| js_trim(t).is_empty() || utf16_len(t) > EXPOSE_TITLE_MAX) {
            fail("title", "/title", format!("title is a non-blank string of at most {EXPOSE_TITLE_MAX} characters"));
        }
        if expose.get("description").and_then(Value::as_str).is_none_or(|t| js_trim(t).is_empty() || utf16_len(t) > EXPOSE_DESCRIPTION_MAX) {
            fail("description", "/description", format!("description is a non-blank string of at most {EXPOSE_DESCRIPTION_MAX} characters"));
        }
        let input = expose.get("input").and_then(Value::as_object);
        let input_ok = input.is_some_and(|i| i.get("type").and_then(Value::as_str) == Some("object") && i.get("properties").is_none_or(Value::is_object));
        if !input_ok {
            fail("input", "/input", "input is a JSON Schema of type object".into());
        }
        let access = expose.get("access").and_then(Value::as_str);
        if !matches!(access, Some("read" | "write")) {
            fail("access", "/access", "access is read or write".into());
        }
        let properties = input.and_then(|i| i.get("properties")).and_then(Value::as_object).unwrap_or(&empty);
        let Some(run) = expose.get("run").and_then(Value::as_object).filter(|r| EXPOSE_RUN_KINDS.iter().filter(|k| r.contains_key(**k)).count() == 1) else {
            fail("run", "/run", "run holds exactly one of records, action, upstream".into());
            continue;
        };
        for path in expose_template_paths(&Value::Object(run.clone())) {
            let first = path.split('.').next().unwrap_or("");
            if !properties.contains_key(first) {
                fail("template", "/run", format!("${{input.{path}}} names no input property"));
            }
        }
        let level: &str;
        if let Some(records) = run.get("records") {
            let op = records.as_object().and_then(|r| r.get("op"));
            let Some(op) = op.and_then(Value::as_str).filter(|o| EXPOSE_RECORD_OPS.contains(o)) else {
                if op.and_then(Value::as_str) == Some("delete") {
                    fail("destructive", "/run/records/op", "a records delete is destructive: never exposed".into());
                } else {
                    fail("run", "/run/records/op", format!("op is {}", EXPOSE_RECORD_OPS.join(", ")));
                }
                continue;
            };
            let kind = records.get("kind").and_then(Value::as_str);
            if !kind.is_some_and(|k| kinds.iter().any(|d| d.as_str() == k)) {
                fail("kind", "/run/records/kind", "kind names no declared record kind".into());
            }
            level = if EXPOSE_READ_RECORD_OPS.contains(&op) { "read" } else { "write" };
        } else if let Some(action) = run.get("action") {
            let Some(action) = action.as_str().filter(|a| is_action_name(a)) else {
                fail("action", "/run/action", "action is a function name like namespace.function".into());
                continue;
            };
            if expose_name_destructive(action) {
                fail("destructive", "/run/action", "a destructive action is never exposed".into());
                continue;
            }
            level = expose_action_access(action);
        } else {
            if !run.get("upstream").and_then(Value::as_str).is_some_and(|u| servers.contains(&u)) {
                fail("upstream", "/run/upstream", "upstream names none of the package's upstreams".into());
            }
            let Some(tool) = run.get("tool").and_then(Value::as_str).filter(|t| !t.is_empty()) else {
                fail("run", "/run/tool", "tool is a non-empty string".into());
                continue;
            };
            if expose_name_destructive(tool) {
                fail("destructive", "/run/tool", "a destructive tool is never exposed".into());
            }
            // An upstream tool's level is its live annotations': the host checks it at call.
            continue;
        }
        if access == Some("read") && level == "write" {
            fail("accessTooLow", "/access", "the run writes: access is write".into());
        }
    }
    issues
}

/// Every problem with a package's `exposes` ([`expose_issues`] as package
/// issues).
pub fn validate_package_exposes(exposes: &Value, records: Option<&Value>, upstreams: Option<&Value>) -> Vec<PackageIssue> {
    expose_issues(exposes, records, upstreams).into_iter().map(|i| PackageIssue { path: i.path, message: i.message }).collect()
}
