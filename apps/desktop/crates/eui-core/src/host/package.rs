//! Catalog negotiation and declarative app packages (VAPP-82: a manifest +
//! A2UI templates + bindings, data never code).

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::policy::matches_pattern;
use super::record_store::RECORDS_SCHEME;
use super::records::{is_record_kind, validate_record_schema};
use super::sources::parse_source;
use super::tools::validate_tools;
use crate::catalog::{A2UI_BASIC_CATALOG_ID, A2UI_VERSION, CORE_CATALOG_ID, CORE_LITE_CATALOG_ID};
use crate::types::ExtensionDef;

/// VAPP-161: the package format version this host reads
/// (catalog/host.json `package.formatVersion`; docs/package-format.md). A
/// package without `formatVersion` is version 1; every version-1 package
/// is a valid version-2 package.
pub const PACKAGE_FORMAT_VERSION: u32 = 2;
/// `package.extensions.versionPattern`.
pub const EXTENSION_VERSION_PATTERN: &str = "^(0|[1-9]\\d*)\\.(0|[1-9]\\d*)\\.(0|[1-9]\\d*)$";

/// VAPP-161: an extension as a host registered it ([`available_extensions`]).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AvailableExtension {
    pub id: String,
    /// `None` = satisfies any version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// `None` = not checked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub components: Option<Vec<String>>,
}

/// A host's extension catalogs as [`validate_package_with`] checks them:
/// the version when set (an empty one is unset), the components then the
/// macros.
pub fn available_extensions(defs: &[ExtensionDef]) -> Vec<AvailableExtension> {
    defs.iter()
        .map(|d| AvailableExtension {
            id: d.id.clone(),
            version: d.version.clone().filter(|v| !v.is_empty()),
            components: Some(d.components.keys().chain(d.macros.iter().flat_map(|m| m.keys())).cloned().collect()),
        })
        .collect()
}

/// `^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$` → the three parts (as JS
/// `Number`s).
fn semver(v: &str) -> Option<[f64; 3]> {
    let mut parts = v.split('.');
    let mut out = [0.0; 3];
    for slot in &mut out {
        let p = parts.next()?;
        if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) || (p.len() > 1 && p.starts_with('0')) {
            return None;
        }
        *slot = p.parse().ok()?;
    }
    parts.next().is_none().then_some(out)
}

/// Caret: `have` has `want`'s major and is not lower.
pub fn version_satisfies(have: &str, want: &str) -> bool {
    let (Some([h_major, h_minor, h_patch]), Some([w_major, w_minor, w_patch])) = (semver(have), semver(want)) else { return false };
    h_major == w_major && (h_minor > w_minor || (h_minor == w_minor && h_patch >= w_patch))
}

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
/// `catalog_ids` = the supported ids (default: [`supported_catalog_ids`] of
/// none). A declared extension only has to be in `catalog_ids`
/// ([`validate_package_with`] checks versions and components too).
pub fn validate_package<S: AsRef<str>>(pkg: &Value, catalog_ids: &[S]) -> Vec<PackageIssue> {
    validate_package_with(pkg, catalog_ids, None)
}

/// [`validate_package`] with the host's registered `extensions` (versions
/// and components); `None` = a declared extension only has to be in
/// `catalog_ids`.
pub fn validate_package_with<S: AsRef<str>>(pkg: &Value, catalog_ids: &[S], extensions: Option<&[AvailableExtension]>) -> Vec<PackageIssue> {
    let mut issues = Vec::new();
    let Some(p) = pkg.as_object() else { return vec![issue("", "a package is an object")] };
    if let Some(version) = p.get("formatVersion") {
        match version.as_f64().filter(|v| v.fract() == 0.0 && *v >= 1.0) {
            None => issues.push(issue("/formatVersion", "formatVersion is a positive integer")),
            Some(v) if v > f64::from(PACKAGE_FORMAT_VERSION) => {
                issues.push(issue("/formatVersion", format!("format version {} is newer than this host ({PACKAGE_FORMAT_VERSION})", crate::json::number_to_string(v))))
            }
            Some(_) => {}
        }
    }
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
                let mut names: Vec<&String> = kinds.keys().collect();
                names.sort();
                for kind in names {
                    let def = &kinds[kind];
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
    // VAPP-161: a `records:<kind>` binding names a declared kind.
    for (id, template) in p.get("templates").and_then(Value::as_object).into_iter().flatten() {
        let Some(bindings) = template.as_object().and_then(|t| t.get("bindings")).and_then(Value::as_array) else { continue };
        for (i, b) in bindings.iter().enumerate() {
            let Some(parsed) = b.as_object().and_then(|o| o.get("source")).and_then(Value::as_str).and_then(parse_source) else { continue };
            if parsed.scheme != RECORDS_SCHEME {
                continue;
            }
            if !p.get("records").and_then(Value::as_object).is_some_and(|r| r.contains_key(&parsed.name)) {
                issues.push(issue(format!("/templates/{id}/bindings/{i}/source"), format!("records:{} names no declared kind", parsed.name)));
            }
        }
    }
    if let Some(tools) = p.get("tools") {
        issues.extend(validate_tools(tools));
    }
    if let Some(list) = p.get("extensions") {
        issues.extend(validate_extensions(list, catalog_ids, extensions));
    }
    issues
}

fn validate_extensions<S: AsRef<str>>(list: &Value, catalog_ids: &[S], available: Option<&[AvailableExtension]>) -> Vec<PackageIssue> {
    let Some(list) = list.as_array() else { return vec![issue("/extensions", "extensions is a list of {id, version, components?}")] };
    let mut issues = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    for (i, ext) in list.iter().enumerate() {
        let at = format!("/extensions/{i}");
        let Some(ext) = ext.as_object() else {
            issues.push(issue(at, "an extension is {id, version, components?}"));
            continue;
        };
        let id = ext.get("id").and_then(Value::as_str).filter(|s| !s.is_empty());
        if id.is_none() {
            issues.push(issue(format!("{at}/id"), "id is a required string"));
        }
        let version = ext.get("version").and_then(Value::as_str).filter(|v| semver(v).is_some());
        if version.is_none() {
            issues.push(issue(format!("{at}/version"), "version is MAJOR.MINOR.PATCH"));
        }
        let components: Option<Vec<&str>> = match ext.get("components") {
            None => Some(Vec::new()),
            Some(c) => c.as_array().and_then(|l| l.iter().map(|c| c.as_str().filter(|s| !s.is_empty())).collect()),
        };
        if components.is_none() {
            issues.push(issue(format!("{at}/components"), "components is a list of component names"));
        }
        let Some(id) = id else { continue };
        if !seen.insert(id) {
            issues.push(issue(format!("{at}/id"), format!("duplicate extension {id}")));
            continue;
        }
        let have = available.and_then(|a| a.iter().find(|a| a.id == id));
        if !catalog_ids.iter().any(|c| c.as_ref() == id) || (available.is_some() && have.is_none()) {
            issues.push(issue(format!("{at}/id"), format!("extension {id} is not available on this host")));
            continue;
        }
        let Some(have) = have else { continue };
        if let (Some(version), Some(have_version)) = (version, have.version.as_deref()) {
            if !version_satisfies(have_version, version) {
                issues.push(issue(format!("{at}/version"), format!("extension {id} {have_version} does not satisfy ^{version}")));
            }
        }
        if let Some(have_components) = &have.components {
            for (j, name) in components.iter().flatten().enumerate() {
                if !have_components.iter().any(|c| c == name) {
                    issues.push(issue(format!("{at}/components/{j}"), format!("extension {id} has no component {name}")));
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
