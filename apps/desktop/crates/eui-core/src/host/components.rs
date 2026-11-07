//! VAPP-203: install checks every template the way the renderer will. Each
//! template's components go through the reducer ([`reduce_surface`]:
//! unknown components, missing or wrong props, child ids that name nothing,
//! cycles, slots, styles), so a package that would render broken never
//! installs. A reducer issue (keyed by a component id) becomes a JSON
//! pointer: the component's own entry, or, for an id no component has,
//! every place that names it (`children/<j>`, `children/componentId`,
//! `slots/<name>`). The twin of `src/host/components.ts`
//! (fixtures/host-router.json).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde_json::{Map, Value};

use super::package::PackageIssue;
use crate::catalog::{CatalogView, CORE_CATALOG_ID};
use crate::reducer::{reduce_surface, ReduceOptions};
use crate::types::FlatComponent;

fn issue(path: impl Into<String>, message: impl Into<String>) -> PackageIssue {
    PackageIssue { path: path.into(), message: message.into() }
}

fn escape(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

fn set(v: Option<&Value>) -> Option<&Value> {
    v.filter(|v| !v.is_null())
}

/// The shape the reducer reads: `children` = ids or `{componentId, path,
/// key?}`, `slots` = ids by name, `on` and `style` objects. Null = unset.
fn shape_issues(c: &Map<String, Value>, at: &str, out: &mut Vec<PackageIssue>) {
    if let Some(ch) = set(c.get("children")) {
        let ids = ch.as_array().is_some_and(|list| list.iter().all(Value::is_string));
        let template = ch.as_object().is_some_and(|o| {
            o.get("componentId").is_some_and(Value::is_string) && o.get("path").is_some_and(Value::is_string) && set(o.get("key")).is_none_or(Value::is_string)
        });
        if !ids && !template {
            out.push(issue(format!("{at}/children"), "children is a list of ids or {componentId, path, key?}"));
        }
    }
    if let Some(slots) = set(c.get("slots")) {
        if !slots.as_object().is_some_and(|o| o.values().all(Value::is_string)) {
            out.push(issue(format!("{at}/slots"), "slots is an object of component ids"));
        }
    }
    if set(c.get("on")).is_some_and(|v| !v.is_object()) {
        out.push(issue(format!("{at}/on"), "on is an object of actions"));
    }
    if set(c.get("style")).is_some_and(|v| !v.is_object()) {
        out.push(issue(format!("{at}/style"), "style is an object"));
    }
}

/// Every place a component list names `id` as a child.
fn references(components: &[&Map<String, Value>], id: &str, at: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (i, c) in components.iter().enumerate() {
        let here = format!("{at}/{i}");
        match c.get("children") {
            Some(Value::Array(list)) => {
                for (j, x) in list.iter().enumerate() {
                    if x.as_str() == Some(id) {
                        out.push(format!("{here}/children/{j}"));
                    }
                }
            }
            Some(Value::Object(o)) if o.get("componentId").and_then(Value::as_str) == Some(id) => out.push(format!("{here}/children/componentId")),
            _ => {}
        }
        if let Some(slots) = c.get("slots").and_then(Value::as_object) {
            for (name, x) in slots {
                if x.as_str() == Some(id) {
                    out.push(format!("{here}/slots/{}", escape(name)));
                }
            }
        }
    }
    out
}

/// VAPP-203: every template's components, reduced as the host will render
/// them against `view` (the core plus the extensions the host has
/// definitions for). An unsupported catalog is reported elsewhere. A
/// catalog or declared extension `view` does not know leaves its component
/// names open (an unknown component is not reported); every other check
/// still holds.
pub fn validate_package_components<S: AsRef<str>>(pkg: &Map<String, Value>, catalog_ids: &[S], view: &Arc<CatalogView>) -> Vec<PackageIssue> {
    let mut issues = Vec::new();
    let catalog_id = pkg.get("catalogId").and_then(Value::as_str).unwrap_or("");
    if !catalog_ids.iter().any(|c| c.as_ref() == catalog_id) {
        return issues;
    }
    let Some(templates) = pkg.get("templates").and_then(Value::as_object) else { return issues };
    let declared: Vec<&str> = pkg.get("extensions").and_then(Value::as_array).into_iter().flatten().filter_map(|e| e.get("id").and_then(Value::as_str)).collect();
    let declared_unknown = declared.iter().any(|id| !id.is_empty() && !view.knows_catalog(id));
    let open = !view.knows_catalog(catalog_id) || declared_unknown;
    let reduce_as = if view.knows_catalog(catalog_id) { catalog_id } else { CORE_CATALOG_ID };
    for (id, template) in super::package::sorted_entries(templates) {
        let Some(components) = template.as_object().and_then(|t| t.get("components")).and_then(Value::as_array) else { continue };
        let at = format!("/templates/{}/components", escape(id));
        let mut ids: HashSet<&str> = HashSet::new();
        let mut list: Vec<&Map<String, Value>> = Vec::new();
        let mut clean = !components.is_empty();
        for c in components {
            let entry = c.as_object().filter(|o| o.get("component").is_some_and(Value::is_string));
            match entry.and_then(|o| o.get("id").and_then(Value::as_str).map(|id| (o, id))) {
                Some((o, cid)) if !ids.contains(cid) => {
                    ids.insert(cid);
                    list.push(o);
                }
                _ => clean = false,
            }
        }
        if !clean || !ids.contains("root") {
            continue;
        }
        let mut shape = Vec::new();
        for (i, c) in list.iter().enumerate() {
            shape_issues(c, &format!("{at}/{i}"), &mut shape);
        }
        if !shape.is_empty() {
            issues.extend(shape);
            continue;
        }
        // VAPP-217: the record macros reduce FILLED, as template_messages
        // sends them; an issue on a filled-in row (`<id>__row`) lands on its macro.
        let values: Vec<Value> = list.iter().map(|c| Value::Object((*c).clone())).collect();
        let reduced = if super::record_macros::uses_record_macros(&values) { super::record_macros::fill_record_macros(&values, &[], pkg.get("records"), false).0 } else { values };
        let Ok(flat) = reduced.iter().map(|c| serde_json::from_value::<FlatComponent>(c.clone())).collect::<Result<Vec<_>, _>>() else { continue };
        let mut index: HashMap<String, usize> = list.iter().enumerate().filter_map(|(i, c)| c.get("id").and_then(Value::as_str).map(|id| (id.to_string(), i))).collect();
        for c in &reduced {
            let Some(cid) = c.get("id").and_then(Value::as_str) else { continue };
            if let Some(owner) = cid.strip_suffix("__row").and_then(|o| index.get(o).copied()) {
                index.entry(cid.to_string()).or_insert(owner);
            }
        }
        let mut seen: HashSet<(String, String)> = HashSet::new();
        let mut push = |path: String, message: String, issues: &mut Vec<PackageIssue>| {
            if seen.insert((path.clone(), message.clone())) {
                issues.push(PackageIssue { path, message });
            }
        };
        // Round 9 §5.5: a catalog the host only knows by id is available (its
        // component names stay open), `extensions` report a missing one themselves.
        let mut options = ReduceOptions::new(reduce_as).with_view(view.clone());
        options.available = Some(catalog_ids.iter().map(|c| c.as_ref().to_string()).chain(view.extension_ids.iter().cloned()).collect());
        for found in reduce_surface(&flat, &options).issues {
            if found.code == "UNSUPPORTED_CATALOG" {
                let named = found.message.strip_prefix("catalog ").and_then(|m| m.split_whitespace().next()).unwrap_or(&found.message);
                if view.knows_catalog(named) || declared.contains(&named) {
                    continue;
                }
            }
            if open && found.message.starts_with("unknown component ") {
                continue;
            }
            if let Some(i) = index.get(found.id.as_str()) {
                push(format!("{at}/{i}"), found.message, &mut issues);
                continue;
            }
            let refs = references(&list, &found.id, &at);
            let message = format!("{}: {}", found.id, found.message);
            if refs.is_empty() {
                push(at.clone(), message.clone(), &mut issues);
            }
            for r in refs {
                push(r, message.clone(), &mut issues);
            }
        }
    }
    issues
}
