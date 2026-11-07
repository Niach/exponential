//! VAPP-241 (eApps P1): a package's PUBLIC LINK definitions, the mirror of
//! `packages/exponential-ui/src/host/share.ts`; `fixtures/package-share.json`
//! locks both. The links themselves are the host's.

use serde_json::{json, Map, Value};

use super::expressions::validate_package_expressions;
use super::package::PackageIssue;
use super::query::RECORDS_SCHEME;
use super::sources::parse_source;

pub const MAX_SHARES: usize = 5;
pub const MAX_SHARE_EXPIRES_DAYS: i64 = 3650;
const SHARE_KEYS: &[&str] = &["name", "mode", "kind", "fields", "query", "template", "expires"];

fn issue(path: impl Into<String>, message: impl Into<String>) -> PackageIssue {
    PackageIssue { path: path.into(), message: message.into() }
}

fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase()) && name.chars().count() <= 24 && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn is_file(field: Option<&Value>) -> bool {
    let Some(f) = field else { return false };
    f.get("format").and_then(Value::as_str) == Some("attachment") || f.get("items").and_then(|i| i.get("format")).and_then(Value::as_str) == Some("attachment")
}

fn show(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
        None => "undefined".into(),
    }
}

/// VAPP-246: a template as a guest's host gets it: every read is the link's one
/// snapshot, so no binding or RecordTable names a query. The twin of TS
/// `guestTemplate`.
fn guest_template(template: &Map<String, Value>, kind: &str) -> Map<String, Value> {
    let mut out = template.clone();
    if let Some(bindings) = out.get_mut("bindings").and_then(Value::as_array_mut) {
        for b in bindings.iter_mut().filter_map(Value::as_object_mut) {
            b.insert("source".into(), Value::String(format!("{RECORDS_SCHEME}:{kind}")));
        }
    }
    if let Some(components) = out.get_mut("components").and_then(Value::as_array_mut) {
        for c in components.iter_mut().filter_map(Value::as_object_mut) {
            if c.get("component").and_then(Value::as_str) == Some("RecordTable") {
                c.remove("query");
            }
        }
    }
    out
}

/// Does this source read the link's own snapshot: its kind, and at most the link's query?
fn reads_snapshot(source: &str, kind: &str, query: Option<&Value>) -> bool {
    let Some(parsed) = parse_source(source) else { return false };
    if parsed.scheme != RECORDS_SCHEME || parsed.name != kind {
        return false;
    }
    match parsed.params.len() {
        0 => true,
        1 => parsed.params.get("query").is_some_and(|q| query.and_then(Value::as_str) == Some(q.as_str())),
        _ => false,
    }
}

/// The first `exp:` string anywhere below a value.
fn exp_source(value: &Value) -> Option<&str> {
    match value {
        Value::String(s) => s.to_lowercase().starts_with("exp:").then_some(s.as_str()),
        Value::Array(list) => list.iter().find_map(exp_source),
        Value::Object(map) => map.values().find_map(exp_source),
        _ => None,
    }
}

const GUEST_CALLS: &[&str] = &["set", "openUrl"];

/// VAPP-246: a view link's `template`, the twin of TS `templateIssues`: a
/// guest's host has one source (the snapshot, narrowed to the link's fields), no
/// function but `set` and `openUrl`, no extension and no `exp:` source, and
/// refuses a package outside that, so install does first.
fn template_issues(pkg: &Map<String, Value>, s: &Map<String, Value>, at: &str, kind: Option<&str>, def: Option<&Value>, props: Option<&Map<String, Value>>, refs: Option<&Map<String, Value>>) -> Vec<PackageIssue> {
    let here = format!("{at}/template");
    if s.get("mode").and_then(Value::as_str) != Some("view") {
        return vec![issue(here, "only a view link has a template")];
    }
    let templates = pkg.get("templates").and_then(Value::as_object);
    let Some((name, template)) = s.get("template").and_then(Value::as_str).and_then(|n| templates.and_then(|t| t.get(n)).map(|t| (n, t))) else {
        return vec![issue(here, format!("{} names no package template", show(s.get("template"))))];
    };
    let (Some(template), Some(def), Some(kind)) = (template.as_object(), def, kind) else { return Vec::new() };
    let mut issues = Vec::new();
    let say = |issues: &mut Vec<PackageIssue>, message: String| issues.push(issue(here.clone(), format!("template {name} {message}")));
    let foreign = |source: &str| format!("reads {source}: a public link reads only its own records:{kind}");
    let query = s.get("query");
    for b in template.get("bindings").and_then(Value::as_array).into_iter().flatten() {
        if let Some(source) = b.get("source").and_then(Value::as_str) {
            if !reads_snapshot(source, kind, query) {
                say(&mut issues, foreign(source));
            }
        }
    }
    let extensions: Vec<&Value> = pkg.get("extensions").and_then(Value::as_array).into_iter().flatten().filter_map(|e| e.get("components").and_then(Value::as_array)).flatten().collect();
    for c in template.get("components").and_then(Value::as_array).into_iter().flatten() {
        let Some(component) = c.as_object() else { continue };
        let what = component.get("component");
        if what.and_then(Value::as_str) == Some("RecordForm") {
            say(&mut issues, "holds a RecordForm: a view link only reads".into());
        }
        if what.and_then(Value::as_str) == Some("RecordTable") {
            let raw = component.get("query").and_then(Value::as_str).map(str::trim).unwrap_or("");
            let table_query = raw.strip_prefix('?').unwrap_or(raw);
            let source = if table_query.is_empty() { format!("records:{}", show(component.get("kind"))) } else { format!("records:{}?{table_query}", show(component.get("kind"))) };
            if !reads_snapshot(&source, kind, query) {
                say(&mut issues, foreign(&source));
            }
            if component.get("editable") != Some(&Value::Bool(false)) {
                say(&mut issues, "has an editable RecordTable: set editable: false".into());
            }
        }
        if what.is_some_and(|w| extensions.contains(&w)) {
            say(&mut issues, format!("uses the extension component {}: a guest's page has no extensions", show(what)));
        }
        for handler in component.get("on").and_then(Value::as_object).into_iter().flat_map(Map::values) {
            if let Some(call) = handler.get("functionCall").and_then(|f| f.get("@call")).and_then(Value::as_str) {
                if !GUEST_CALLS.contains(&call) {
                    say(&mut issues, format!("calls {call}: a guest's page only reads"));
                }
            }
        }
        if let Some(exp) = exp_source(c) {
            say(&mut issues, format!("reads {exp}: a guest has no exp: source"));
        }
    }
    if let Some(exp) = template.get("data").and_then(exp_source) {
        say(&mut issues, format!("reads {exp}: a guest has no exp: source"));
    }
    // What a guest's host will check: the expressions over the narrowed row and no query.
    let empty = Map::new();
    let props = props.unwrap_or(&empty);
    let listed: Vec<&str> = match s.get("fields").and_then(Value::as_array).filter(|l| l.iter().all(Value::is_string)) {
        Some(list) => list.iter().filter_map(Value::as_str).collect(),
        None => props.iter().filter(|(f, field)| !is_file(Some(field)) && !refs.is_some_and(|r| r.contains_key(f.as_str()))).map(|(f, _)| f.as_str()).collect(),
    };
    let properties: Map<String, Value> = listed.into_iter().filter_map(|f| props.get(f).map(|field| (f.to_string(), field.clone()))).collect();
    let mut schema = def.get("schema").and_then(Value::as_object).cloned().unwrap_or_default();
    schema.insert("properties".into(), Value::Object(properties));
    let narrowed = json!({
        "templates": { name: Value::Object(guest_template(template, kind)) },
        "records": { kind: { "schema": Value::Object(schema) } },
    });
    if let Some(narrowed) = narrowed.as_object() {
        for found in validate_package_expressions(narrowed) {
            say(&mut issues, format!("on a guest's page: {}", found.message));
        }
    }
    issues
}

/// Every problem with a package's `share`, each at its entry.
pub fn validate_package_shares(pkg: &Map<String, Value>) -> Vec<PackageIssue> {
    let Some(share) = pkg.get("share") else { return Vec::new() };
    let Some(list) = share.as_array() else {
        return vec![issue("/share", "share is a list of {name, mode, kind, fields?, query?, template?, expires?}")];
    };
    let mut issues = Vec::new();
    if list.len() > MAX_SHARES {
        issues.push(issue("/share", format!("at most {MAX_SHARES} public links, got {}", list.len())));
    }
    let records = pkg.get("records").and_then(Value::as_object);
    let queries = pkg.get("queries").and_then(Value::as_object);
    let mut seen: Vec<&str> = Vec::new();
    for (i, s) in list.iter().enumerate() {
        let at = format!("/share/{i}");
        let Some(s) = s.as_object() else {
            issues.push(issue(at, "a public link is {name, mode, kind, fields?, query?, template?, expires?}"));
            continue;
        };
        let mut keys: Vec<&String> = s.keys().collect();
        keys.sort();
        for k in keys {
            if !SHARE_KEYS.contains(&k.as_str()) {
                issues.push(issue(format!("{at}/{k}"), format!("unknown public link key {k}")));
            }
        }
        match s.get("name").and_then(Value::as_str) {
            Some(name) if valid_name(name) => {
                if seen.contains(&name) {
                    issues.push(issue(format!("{at}/name"), format!("duplicate public link {name}")));
                } else {
                    seen.push(name);
                }
            }
            _ => issues.push(issue(format!("{at}/name"), "a name is a lowercase letter, then lowercase letters, digits and underscores (24 at most)")),
        }
        let mode = s.get("mode").and_then(Value::as_str);
        if !matches!(mode, Some("submit" | "view")) {
            issues.push(issue(format!("{at}/mode"), "mode is submit or view"));
        }
        let kind = s.get("kind").and_then(Value::as_str);
        let def = kind.and_then(|k| records.and_then(|r| r.get(k))).filter(|d| d.is_object());
        if def.is_none() {
            issues.push(issue(format!("{at}/kind"), format!("{} names no declared kind", show(s.get("kind")))));
        }
        let schema = def.and_then(|d| d.get("schema")).and_then(Value::as_object);
        let props = schema.and_then(|x| x.get("properties")).and_then(Value::as_object);
        let required: Vec<&Value> = schema.and_then(|x| x.get("required")).and_then(Value::as_array).map(|l| l.iter().collect()).unwrap_or_default();
        let refs = def.and_then(|d| d.get("refs")).and_then(Value::as_object);
        if let Some(fields) = s.get("fields") {
            match fields.as_array().filter(|l| !l.is_empty() && l.iter().all(Value::is_string)) {
                None => issues.push(issue(format!("{at}/fields"), "fields is a non-empty list of field names")),
                Some(list) => {
                    for (j, f) in list.iter().enumerate() {
                        let name = f.as_str().unwrap_or("");
                        let field = props.and_then(|p| p.get(name));
                        if def.is_some() && field.is_none() {
                            issues.push(issue(format!("{at}/fields/{j}"), format!("{name} is not a field of {}", show(s.get("kind")))));
                        }
                        if is_file(field) {
                            issues.push(issue(format!("{at}/fields/{j}"), format!("a public link cannot carry the file field {name}")));
                        }
                        if refs.is_some_and(|r| r.contains_key(name)) {
                            issues.push(issue(format!("{at}/fields/{j}"), format!("a public link cannot carry the ref field {name}")));
                        }
                    }
                    if mode == Some("submit") {
                        for name in &required {
                            if !list.contains(name) {
                                issues.push(issue(format!("{at}/fields"), format!("a submit link needs the required field {}", show(Some(name)))));
                            }
                        }
                    }
                }
            }
        }
        if mode == Some("submit") && required.iter().any(|n| n.as_str().and_then(|n| props.and_then(|p| p.get(n))).is_some_and(|f| is_file(Some(f)))) {
            issues.push(issue(format!("{at}/kind"), "a submit link cannot fill a required file field"));
        }
        // A guest has no picker for a ref; with `fields` the checks above already speak.
        if mode == Some("submit") && s.get("fields").is_none() {
            for name in required.iter().filter_map(|n| n.as_str()) {
                if refs.is_some_and(|r| r.contains_key(name)) {
                    issues.push(issue(format!("{at}/kind"), format!("a submit link cannot fill the required ref field {name}")));
                }
            }
        }
        if let Some(query) = s.get("query") {
            if mode != Some("view") {
                issues.push(issue(format!("{at}/query"), "only a view link has a query"));
            } else {
                match query.as_str().and_then(|q| queries.and_then(|m| m.get(q)).map(|d| (q, d))) {
                    None => issues.push(issue(format!("{at}/query"), format!("{} names no package query", show(Some(query))))),
                    Some((q, d)) => {
                        let from = d.get("from").and_then(Value::as_str).map(str::trim).unwrap_or("");
                        if !from.is_empty() && Some(from) != kind {
                            issues.push(issue(format!("{at}/query"), format!("query {q} reads records:{from}, the link's kind is {}", show(s.get("kind")))));
                        }
                    }
                }
            }
        }
        if s.get("template").is_some() {
            issues.extend(template_issues(pkg, s, &at, kind, def, props, refs));
        }
        if let Some(expires) = s.get("expires") {
            let ok = expires.as_i64().filter(|d| expires.as_f64() == Some(*d as f64)).is_some_and(|d| (1..=MAX_SHARE_EXPIRES_DAYS).contains(&d));
            if !ok {
                issues.push(issue(format!("{at}/expires"), format!("expires is a whole number of days, 1 to {MAX_SHARE_EXPIRES_DAYS}")));
            }
        }
    }
    issues
}
