//! VAPP-241 (eApps P1): a package's PUBLIC LINK definitions, the mirror of
//! `packages/exponential-ui/src/host/share.ts`; `fixtures/package-share.json`
//! locks both. The links themselves are the host's.

use serde_json::{Map, Value};

use super::package::PackageIssue;

pub const MAX_SHARES: usize = 5;
pub const MAX_SHARE_EXPIRES_DAYS: i64 = 3650;
const SHARE_KEYS: &[&str] = &["name", "mode", "kind", "fields", "query", "expires"];

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

/// Every problem with a package's `share`, each at its entry.
pub fn validate_package_shares(pkg: &Map<String, Value>) -> Vec<PackageIssue> {
    let Some(share) = pkg.get("share") else { return Vec::new() };
    let Some(list) = share.as_array() else {
        return vec![issue("/share", "share is a list of {name, mode, kind, fields?, query?, expires?}")];
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
            issues.push(issue(at, "a public link is {name, mode, kind, fields?, query?, expires?}"));
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
        if mode == Some("submit") && required.iter().any(|n| n.as_str().and_then(|n| props.and_then(|p| p.get(n))).is_some_and(|f| f.get("format").and_then(Value::as_str) == Some("attachment"))) {
            issues.push(issue(format!("{at}/kind"), "a submit link cannot fill a required file field"));
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
        if let Some(expires) = s.get("expires") {
            let ok = expires.as_i64().filter(|d| expires.as_f64() == Some(*d as f64)).is_some_and(|d| (1..=MAX_SHARE_EXPIRES_DAYS).contains(&d));
            if !ok {
                issues.push(issue(format!("{at}/expires"), format!("expires is a whole number of days, 1 to {MAX_SHARE_EXPIRES_DAYS}")));
            }
        }
    }
    issues
}
