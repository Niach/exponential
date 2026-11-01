//! An app's MCP tools as declarative bindings
//! (`packages/emcp/fixtures/exposes.json`; its `$comment` is the spec):
//! validation, `${input.x}` resolution, `<app>__<tool>` naming and
//! collisions, the upstream call-time check and the `_meta` provenance.
//! Bindings are read as JSON: a malformed one is a validation error, never
//! a parse failure.

use std::collections::HashSet;

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use super::actions;

pub const NAME_MAX: usize = 40;
pub const TITLE_MAX: usize = 80;
pub const DESCRIPTION_MAX: usize = 1024;
pub const MAX_TOOLS: usize = 32;
pub const SEPARATOR: &str = "__";
pub const TOOL_NAME_MAX: usize = 64;
pub const SLUG_HASH_LENGTH: usize = 6;
const RECORD_OPS: &[&str] = &["list", "get", "upsert"];
const READ_RECORD_OPS: &[&str] = &["list", "get"];
const READ_OP_WORDS: &[&str] = &["list", "get", "show", "catalog", "activity", "files"];

/// The `_meta` provenance keys.
pub const META_APP: &str = "at.exponential/app";
pub const META_TOOL: &str = "at.exponential/tool";
pub const META_VIA: &str = "at.exponential/via";
pub const META_UPSTREAM: &str = "at.exponential/upstream";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolAccess {
    Read,
    Write,
}

impl ToolAccess {
    pub fn as_str(self) -> &'static str {
        match self {
            ToolAccess::Read => "read",
            ToolAccess::Write => "write",
        }
    }
}

/// What the app's package declares beside its `exposes`.
#[derive(Clone, Debug, Default)]
pub struct ExposeContext {
    pub kinds: Vec<String>,
    pub upstreams: Vec<String>,
}

/// `{index, code}`; `index` = `None` for the list itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExposeError {
    pub index: Option<usize>,
    pub code: &'static str,
}

/// `records` | `action` | `upstream` (the first key present, in that order).
pub fn run_via(run: &Value) -> &'static str {
    if run.get("records").is_some() {
        "records"
    } else if run.get("action").is_some() {
        "action"
    } else {
        "upstream"
    }
}

/// An action reads when the last word of its op is a read word.
pub fn action_access(name: &str) -> ToolAccess {
    let last = actions::parse_action_name(name).and_then(|(_, op)| op).and_then(|op| op.rsplit('_').next().map(str::to_string));
    match last {
        Some(word) if READ_OP_WORDS.contains(&word.as_str()) => ToolAccess::Read,
        _ => ToolAccess::Write,
    }
}

fn hint(annotations: Option<&Value>, key: &str) -> bool {
    annotations.and_then(|a| a.get(key)).and_then(Value::as_bool) == Some(true)
}

/// Destructive when it says so or a word of its name is a destructive word.
pub fn upstream_destructive(tool: &str, annotations: Option<&Value>) -> bool {
    if hint(annotations, "destructiveHint") {
        return true;
    }
    let destructive = actions::destructive_words();
    tool.to_lowercase().split(|c: char| !c.is_ascii_lowercase()).any(|word| destructive.iter().any(|d| d == word))
}

/// Reads only when it says so (`readOnlyHint: true`).
pub fn upstream_access(annotations: Option<&Value>) -> ToolAccess {
    if hint(annotations, "readOnlyHint") {
        ToolAccess::Read
    } else {
        ToolAccess::Write
    }
}

fn ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// `a.b.c` with every key an identifier.
fn valid_path(path: &str) -> bool {
    path.split('.').all(|key| key.chars().next().is_some_and(ident_start) && key.chars().all(ident_char))
}

/// Every `${input.<path>}` in `text`: (start, end, path), in order.
fn templates(text: &str) -> Vec<(usize, usize, &str)> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(at) = text[from..].find("${input.") {
        let start = from + at;
        let body = start + "${input.".len();
        match text[body..].find('}') {
            Some(len) if valid_path(&text[body..body + len]) => {
                out.push((start, body + len + 1, &text[body..body + len]));
                from = body + len + 1;
            }
            _ => from = start + 1,
        }
    }
    out
}

/// The input paths a value's templates name.
pub fn template_paths(value: &Value) -> Vec<String> {
    let mut out = Vec::new();
    fn walk(value: &Value, out: &mut Vec<String>) {
        match value {
            Value::String(s) => out.extend(templates(s).into_iter().map(|(_, _, path)| path.to_string())),
            Value::Array(items) => items.iter().for_each(|item| walk(item, out)),
            Value::Object(map) => map.values().for_each(|item| walk(item, out)),
            _ => {}
        }
    }
    walk(value, &mut out);
    out
}

fn lookup<'a>(input: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.').try_fold(input, |at, key| at.as_object()?.get(key))
}

fn stringify(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => {
            let mut out = String::new();
            out.push_str(&super::audit::canonical_json(&Value::Number(n.clone())));
            out
        }
        Some(Value::Bool(b)) => b.to_string(),
        Some(other) => other.to_string(),
    }
}

/// Fill a binding's args from the call's input; `None` = absent (dropped
/// from an object, null in an array).
pub fn resolve_args(template: &Value, input: &Value) -> Option<Value> {
    match template {
        Value::String(text) => {
            let found = templates(text);
            if let [(0, end, path)] = found.as_slice() {
                if *end == text.len() {
                    return lookup(input, path).cloned();
                }
            }
            let mut out = String::new();
            let mut at = 0;
            for (start, end, path) in found {
                out.push_str(&text[at..start]);
                out.push_str(&stringify(lookup(input, path)));
                at = end;
            }
            out.push_str(&text[at..]);
            Some(Value::String(out))
        }
        Value::Array(items) => {
            Some(Value::Array(items.iter().map(|item| resolve_args(item, input).unwrap_or(Value::Null)).collect()))
        }
        Value::Object(map) => {
            let mut out = Map::new();
            for (key, item) in map {
                if let Some(value) = resolve_args(item, input) {
                    out.insert(key.clone(), value);
                }
            }
            Some(Value::Object(out))
        }
        other => Some(other.clone()),
    }
}

/// JS string length (UTF-16 units), which the limits count.
fn js_len(text: &str) -> usize {
    text.encode_utf16().count()
}

fn name_ok(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && js_len(name) <= NAME_MAX
        && !name.contains(SEPARATOR)
}

fn text_ok(value: &Value, max: usize) -> bool {
    value.as_str().is_some_and(|text| !text.trim().is_empty() && js_len(text) <= max)
}

/// Every error of an app's `exposes`, in order (empty = valid).
pub fn validate_exposes(exposes: &[Value], ctx: &ExposeContext) -> Vec<ExposeError> {
    let mut errors = Vec::new();
    if exposes.len() > MAX_TOOLS {
        errors.push(ExposeError { index: None, code: "tooMany" });
    }
    let mut seen = HashSet::new();
    for (index, expose) in exposes.iter().enumerate() {
        let mut fail = |code: &'static str| errors.push(ExposeError { index: Some(index), code });
        match expose.get("name").and_then(Value::as_str) {
            Some(name) if name_ok(name) => {
                if !seen.insert(name.to_string()) {
                    fail("duplicate");
                }
            }
            _ => fail("name"),
        }
        if !text_ok(&expose["title"], TITLE_MAX) {
            fail("title");
        }
        if !text_ok(&expose["description"], DESCRIPTION_MAX) {
            fail("description");
        }
        let input = &expose["input"];
        if !input.is_object() || input["type"] != "object" {
            fail("input");
        }
        let access = expose["access"].as_str();
        if access != Some("read") && access != Some("write") {
            fail("access");
        }
        let properties = input.get("properties").and_then(Value::as_object).cloned().unwrap_or_default();
        let run = &expose["run"];
        if !run.is_object() {
            fail("run");
            continue;
        }
        let keys = ["records", "action", "upstream"].iter().filter(|key| run.get(**key).is_some()).count();
        if keys != 1 {
            fail("run");
            continue;
        }
        for path in template_paths(run) {
            if !properties.contains_key(path.split('.').next().unwrap_or_default()) {
                fail("template");
            }
        }
        let level = match run_via(run) {
            "records" => {
                let records = &run["records"];
                let op = records.get("op").and_then(Value::as_str);
                match op {
                    Some(op) if RECORD_OPS.contains(&op) => {}
                    Some("delete") => {
                        fail("destructive");
                        continue;
                    }
                    _ => {
                        fail("run");
                        continue;
                    }
                }
                let kind = records.get("kind").and_then(Value::as_str);
                if !kind.is_some_and(|kind| ctx.kinds.iter().any(|k| k == kind)) {
                    fail("kind");
                }
                if READ_RECORD_OPS.contains(&op.unwrap_or_default()) {
                    ToolAccess::Read
                } else {
                    ToolAccess::Write
                }
            }
            "action" => {
                let name = run["action"].as_str().unwrap_or_default();
                if actions::parse_action_name(name).is_none() {
                    fail("action");
                    continue;
                }
                if actions::call_destructive(name, run.get("args")) {
                    fail("destructive");
                    continue;
                }
                if actions::action_owner_only(name) {
                    fail("ownerOnly");
                    continue;
                }
                action_access(name)
            }
            _ => {
                let upstream = run["upstream"].as_str();
                if !upstream.is_some_and(|name| ctx.upstreams.iter().any(|u| u == name)) {
                    fail("upstream");
                }
                match run.get("tool").and_then(Value::as_str) {
                    Some(tool) if !tool.is_empty() => {
                        if upstream_destructive(tool, None) {
                            fail("destructive");
                        }
                    }
                    _ => fail("run"),
                }
                // An upstream's level is checked again on its live annotations.
                continue;
            }
        };
        if access == Some("read") && level == ToolAccess::Write {
            fail("accessTooLow");
        }
    }
    errors
}

/// The call-time check of an upstream binding against the tool's live
/// annotations: `None` = callable, else the error code.
pub fn upstream_call_error(access: ToolAccess, tool: &str, annotations: Option<&Value>) -> Option<&'static str> {
    if upstream_destructive(tool, annotations) {
        Some("destructive")
    } else if access == ToolAccess::Read && upstream_access(annotations) == ToolAccess::Write {
        Some("accessTooLow")
    } else {
        None
    }
}

const BASE32: &[u8] = b"abcdefghijklmnopqrstuvwxyz234567";

/// `n` lowercase base32 characters of the SHA-256 of `text`.
pub fn short_hash(text: &str, n: usize) -> String {
    let digest = Sha256::digest(text.as_bytes());
    let mut out = String::new();
    let (mut bits, mut value) = (0u32, 0u32);
    for byte in digest {
        value = (value << 8) | byte as u32;
        bits += 8;
        while bits >= 5 && out.len() < n {
            out.push(BASE32[((value >> (bits - 5)) & 31) as usize] as char);
            bits -= 5;
        }
        value &= (1 << bits) - 1;
        if out.len() >= n {
            break;
        }
    }
    out
}

/// `<slug>__<name>`; past 64 characters the slug is cut and ends in
/// `-<hash of the full slug>`.
pub fn qualified_tool_name(slug: &str, name: &str) -> String {
    let full = format!("{slug}{SEPARATOR}{name}");
    if js_len(&full) <= TOOL_NAME_MAX {
        return full;
    }
    let hash = short_hash(slug, SLUG_HASH_LENGTH);
    let keep = TOOL_NAME_MAX as isize - SEPARATOR.len() as isize - js_len(name) as isize - hash.len() as isize - 1;
    let cut: String = slug.chars().take(keep.max(0) as usize).collect();
    format!("{}-{hash}{SEPARATOR}{name}", cut.trim_end_matches('-'))
}

/// `<slug>__<name>` → (app, tool), split on the first `__`.
pub fn split_tool_name(qualified: &str) -> Option<(&str, &str)> {
    let at = qualified.find(SEPARATOR)?;
    let tool = &qualified[at + SEPARATOR.len()..];
    (at > 0 && !tool.is_empty() && !tool.contains(SEPARATOR)).then(|| (&qualified[..at], tool))
}

/// One listed tool: (app slug, binding name, qualified name).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListedTool {
    pub app: String,
    pub tool: String,
    pub name: String,
}

/// Several apps' tools on one endpoint, apps sorted by slug; a name an
/// earlier app holds is dropped. `apps` = (slug, binding names).
pub fn list_tool_names(apps: &[(String, Vec<String>)]) -> Vec<ListedTool> {
    let mut sorted: Vec<&(String, Vec<String>)> = apps.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let mut taken = HashSet::new();
    let mut out = Vec::new();
    for (slug, names) in sorted {
        for tool in names {
            let name = qualified_tool_name(slug, tool);
            if taken.insert(name.clone()) {
                out.push(ListedTool { app: slug.clone(), tool: tool.clone(), name });
            }
        }
    }
    out
}

/// The `_meta` every result of an app tool carries.
pub fn provenance_meta(slug: &str, expose: &Value) -> Map<String, Value> {
    let run = &expose["run"];
    let mut meta = Map::new();
    meta.insert(META_APP.into(), slug.into());
    meta.insert(META_TOOL.into(), expose["name"].clone());
    meta.insert(META_VIA.into(), run_via(run).into());
    if let Some(upstream) = run.get("upstream") {
        meta.insert(META_UPSTREAM.into(), upstream.clone());
    }
    meta
}
