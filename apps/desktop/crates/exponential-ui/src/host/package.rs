//! Catalog negotiation and declarative app packages (VAPP-82: a manifest +
//! A2UI templates + bindings, data never code).

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::policy::matches_pattern;
use super::records::{is_record_kind, validate_record_schema};
use super::sources::parse_source;
use crate::catalog::{A2UI_BASIC_CATALOG_ID, A2UI_VERSION, CORE_CATALOG_ID, CORE_LITE_CATALOG_ID};

/// The core, the core lite, the A2UI basic catalog, then every registered
/// extension id in order, de-duplicated.
pub fn supported_catalog_ids<S: AsRef<str>>(extension_ids: &[S]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for id in [CORE_CATALOG_ID, CORE_LITE_CATALOG_ID, A2UI_BASIC_CATALOG_ID].into_iter().chain(extension_ids.iter().map(AsRef::as_ref)) {
        if !out.iter().any(|x| x == id) {
            out.push(id.to_string());
        }
    }
    out
}

/// A2UI's `a2uiClientCapabilities`.
pub fn client_capabilities<S: AsRef<str>>(extension_ids: &[S]) -> Value {
    json!({"v0.9": {"supportedCatalogIds": supported_catalog_ids(extension_ids)}})
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageIssue {
    pub path: String,
    pub message: String,
}

fn issue(path: impl Into<String>, message: impl Into<String>) -> PackageIssue {
    PackageIssue { path: path.into(), message: message.into() }
}

/// `^(\/([^/~]|~[01])*)*$`: empty, or `/`-separated segments whose `~` is
/// always `~0` / `~1`.
fn is_pointer(s: &str) -> bool {
    if s.is_empty() {
        return true;
    }
    if !s.starts_with('/') {
        return false;
    }
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '~' && !matches!(chars.next(), Some('0' | '1')) {
            return false;
        }
    }
    true
}

/// Every problem with a package, each with a JSON pointer into it.
/// `catalog_ids` = the supported ids (default: [`supported_catalog_ids`] of none).
pub fn validate_package<S: AsRef<str>>(pkg: &Value, catalog_ids: &[S]) -> Vec<PackageIssue> {
    let mut issues = Vec::new();
    let Some(p) = pkg.as_object() else { return vec![issue("", "a package is an object")] };
    for key in ["id", "name", "version", "catalogId"] {
        if p.get(key).and_then(Value::as_str).is_none_or(str::is_empty) {
            issues.push(issue(format!("/{key}"), format!("{key} is a required string")));
        }
    }
    if let Some(catalog_id) = p.get("catalogId").and_then(Value::as_str).filter(|s| !s.is_empty()) {
        if !catalog_ids.iter().any(|c| c.as_ref() == catalog_id) {
            issues.push(issue("/catalogId", format!("unsupported catalog {catalog_id}")));
        }
    }
    match p.get("templates").and_then(Value::as_object).filter(|t| !t.is_empty()) {
        None => issues.push(issue("/templates", "templates is a non-empty object")),
        Some(templates) => {
            for (id, template) in templates {
                let at = format!("/templates/{id}");
                let Some(template) = template.as_object() else {
                    issues.push(issue(at, "a template is an object"));
                    continue;
                };
                match template.get("components").and_then(Value::as_array).filter(|c| !c.is_empty()) {
                    None => issues.push(issue(format!("{at}/components"), "components is a non-empty array")),
                    Some(components) => {
                        let mut ids: HashSet<&str> = HashSet::new();
                        for (i, c) in components.iter().enumerate() {
                            let id = c.as_object().filter(|o| o.get("component").is_some_and(Value::is_string)).and_then(|o| o.get("id")).and_then(Value::as_str);
                            match id {
                                None => issues.push(issue(format!("{at}/components/{i}"), "a component needs an id and a component")),
                                Some(id) if ids.contains(id) => issues.push(issue(format!("{at}/components/{i}/id"), format!("duplicate id {id}"))),
                                Some(id) => {
                                    ids.insert(id);
                                }
                            }
                        }
                        if !ids.contains("root") {
                            issues.push(issue(format!("{at}/components"), "no component has the id root"));
                        }
                    }
                }
                if let Some(bindings) = template.get("bindings") {
                    match bindings.as_array() {
                        None => issues.push(issue(format!("{at}/bindings"), "bindings is an array")),
                        Some(list) => {
                            for (i, b) in list.iter().enumerate() {
                                let bp = format!("{at}/bindings/{i}");
                                let b = b.as_object();
                                if b.and_then(|o| o.get("path")).and_then(Value::as_str).is_none_or(|p| !is_pointer(p)) {
                                    issues.push(issue(format!("{bp}/path"), "path is a JSON pointer"));
                                }
                                if b.and_then(|o| o.get("source")).and_then(Value::as_str).is_none_or(|s| parse_source(s).is_none()) {
                                    issues.push(issue(format!("{bp}/source"), "source is a scheme:name URI"));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    if let Some(functions) = p.get("functions") {
        let ok = functions.as_array().is_some_and(|list| list.iter().all(|f| f.as_str().is_some_and(|s| !s.is_empty())));
        if !ok {
            issues.push(issue("/functions", "functions is a list of names or prefix* patterns"));
        }
    }
    // VAPP-122: `records` = the app's kinds, each with a schema.
    if let Some(records) = p.get("records") {
        match records.as_object() {
            None => issues.push(issue("/records", "records is an object of kinds")),
            Some(kinds) => {
                for (kind, def) in kinds {
                    let at = format!("/records/{kind}");
                    if !is_record_kind(kind) {
                        issues.push(issue(&at, "a kind is lowercase letters, digits and underscores"));
                    }
                    match def.as_object() {
                        None => issues.push(issue(&at, "a kind is {schema, title?}")),
                        Some(def) => issues.extend(validate_record_schema(def.get("schema").unwrap_or(&Value::Null), &format!("{at}/schema"))),
                    }
                }
            }
        }
    }
    // VAPP-107: `sources` = the binding sources the package's surfaces may
    // follow; a template binding outside it is an issue.
    if let Some(sources) = p.get("sources") {
        match sources.as_array().filter(|list| list.iter().all(|f| f.as_str().is_some_and(|s| !s.is_empty()))) {
            None => issues.push(issue("/sources", "sources is a list of scheme:name sources or prefix* patterns")),
            Some(list) => {
                let allowed: Vec<String> = list.iter().filter_map(Value::as_str).map(str::to_string).collect();
                for (id, template) in p.get("templates").and_then(Value::as_object).into_iter().flatten() {
                    for (i, b) in template.get("bindings").and_then(Value::as_array).into_iter().flatten().enumerate() {
                        if let Some(source) = b.get("source").and_then(Value::as_str).filter(|s| parse_source(s).is_some()) {
                            if !source_allowed(Some(&allowed), source) {
                                issues.push(issue(format!("/templates/{id}/bindings/{i}/source"), format!("source {source} is not in the package's sources")));
                            }
                        }
                    }
                }
            }
        }
    }
    issues
}

/// VAPP-107: may a package whose `sources` list is `allowed` follow this
/// source URI? `None` = yes; else the URI's `scheme:name` (scheme
/// lower-cased, the query dropped) must match a listed source exactly or a
/// `prefix*` pattern.
pub fn source_allowed(allowed: Option<&[String]>, source: &str) -> bool {
    let Some(allowed) = allowed else { return true };
    let Some(parsed) = parse_source(source) else { return false };
    let name = format!("{}:{}", parsed.scheme, parsed.name);
    allowed.iter().any(|p| matches_pattern(p, &name) || matches_pattern(p, source))
}

/// A package's `sources` allowlist (`None` = unset: any source).
pub fn package_sources(pkg: &Value) -> Option<Vec<String>> {
    pkg.get("sources").and_then(Value::as_array).map(|l| l.iter().filter_map(Value::as_str).map(str::to_string).collect())
}

/// A template as the messages that create it: createSurface, the components,
/// the merged initial data, one bindDataModel per binding. `None` when the
/// package has no such template. `data` = the caller's data (`Some(Null)` is
/// a value, unlike `None`).
pub fn template_messages(pkg: &Value, template_id: &str, surface_id: &str, data: Option<&Value>) -> Option<Vec<Value>> {
    let template = pkg.get("templates").and_then(|t| t.get(template_id)).filter(|t| super::js_truthy(t))?;
    let version = A2UI_VERSION;
    let mut create = Map::new();
    create.insert("surfaceId".into(), Value::String(surface_id.into()));
    if let Some(catalog_id) = pkg.get("catalogId") {
        create.insert("catalogId".into(), catalog_id.clone());
    }
    let mut components = Map::new();
    components.insert("surfaceId".into(), Value::String(surface_id.into()));
    if let Some(list) = template.get("components") {
        components.insert("components".into(), list.clone());
    }
    let mut out = vec![json!({"version": version, "createSurface": create}), json!({"version": version, "updateComponents": components})];
    if let Some(merged) = merge_data(template.get("data"), data) {
        out.push(json!({"version": version, "updateDataModel": {"surfaceId": surface_id, "path": "/", "value": merged}}));
    }
    for b in template.get("bindings").and_then(Value::as_array).into_iter().flatten() {
        let mut bind = Map::new();
        bind.insert("surfaceId".into(), Value::String(surface_id.into()));
        for key in ["path", "source"] {
            if let Some(v) = b.get(key) {
                bind.insert(key.into(), v.clone());
            }
        }
        out.push(json!({"version": version, "bindDataModel": bind}));
    }
    Some(out)
}

/// The template's data under the caller's (objects merge one level deep in
/// key order; anything else: the caller's wins). `None` = undefined.
pub fn merge_data(base: Option<&Value>, over: Option<&Value>) -> Option<Value> {
    match (base, over) {
        (base, None) => base.cloned(),
        (Some(Value::Object(b)), Some(Value::Object(o))) => {
            let mut merged = b.clone();
            for (k, v) in o {
                merged.insert(k.clone(), v.clone());
            }
            Some(Value::Object(merged))
        }
        (_, Some(over)) => Some(over.clone()),
    }
}
