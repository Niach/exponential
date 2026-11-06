//! E6 (#VAPP-266 … #VAPP-269): the runner, the prebuilt shell an exported
//! app runs in. It has no chrome of its own: its navigation, its Permissions
//! entry, a bundle that cannot load and the failed-action toast are genUI
//! surfaces built here from existing components, so every runner (gpui,
//! Compose, SwiftUI over `eengine-ffi`, the web's TS twin) paints the same
//! thing. Locked by `packages/eengine/fixtures/runner.json`.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::broker::{scope_def, ConsentKind, ManifestEntry};
use crate::contract::{strings, RUNNER};
use crate::providers::NetworkPolicy;

/// How wide the window is: `wide` from genUI's `md` breakpoint up, else `compact`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FormFactor {
    Wide,
    Compact,
}

/// Why a bundle cannot run (`runnerFailure`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RunnerFailure {
    /// No package file in the bundle.
    Missing,
    /// `runner.json` has issues.
    Config,
    /// The package file's signature does not verify.
    Signature,
    /// genUI refuses the package.
    Invalid,
    /// The web only: another tab of the origin holds the app.
    Busy,
}

/// The bundle directory and its two files (`bundle`).
pub fn bundle_dir() -> &'static str {
    RUNNER["bundle"]["dir"].as_str().unwrap_or("app")
}

pub fn bundle_package_file() -> &'static str {
    RUNNER["bundle"]["files"]["package"].as_str().unwrap_or("package.app.json")
}

pub fn bundle_config_file() -> &'static str {
    RUNNER["bundle"]["files"]["config"].as_str().unwrap_or("runner.json")
}

/// The Vault key of the `local` identity's device uuid.
pub fn device_key() -> &'static str {
    RUNNER["storage"]["deviceKey"].as_str().unwrap_or("eengine/runner/device")
}

/// A bundle's `runner.json`, validated.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerConfig {
    pub id: String,
    pub name: String,
    pub version: String,
    pub network: NetworkPolicy,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerConfigIssue {
    pub key: String,
    pub code: String,
}

/// `[a-zA-Z][a-zA-Z0-9_]*(\.[a-zA-Z][a-zA-Z0-9_]*)+`: reverse-DNS, valid as an
/// Android applicationId and an iOS bundle id.
fn id_valid(id: &str) -> bool {
    let parts: Vec<&str> = id.split('.').collect();
    parts.len() >= 2
        && parts.iter().all(|p| {
            let mut c = p.chars();
            c.next().is_some_and(|f| f.is_ascii_alphabetic()) && c.all(|x| x.is_ascii_alphanumeric() || x == '_')
        })
}

/// `major.minor.patch`, no leading zeros.
fn version_valid(v: &str) -> bool {
    let parts: Vec<&str> = v.split('.').collect();
    parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) && (p.len() == 1 || !p.starts_with('0')))
}

fn config_value_valid(key: &str, def: &Value, value: &str) -> bool {
    match key {
        "id" => id_valid(value),
        "version" => version_valid(value),
        _ => {
            let max = def.get("maxLength").and_then(Value::as_u64);
            let values = def.get("values").and_then(Value::as_array);
            max.is_none_or(|m| value.chars().count() as u64 <= m && !value.trim().is_empty()) && values.is_none_or(|v| v.iter().any(|x| x.as_str() == Some(value)))
        }
    }
}

/// `runner.json` → the config (absent optional keys = their default) and its
/// issues: the keys in order, then the extra keys sorted (`forbidden` or `unknown`).
pub fn runner_config(input: &Value) -> (Option<RunnerConfig>, Vec<RunnerConfigIssue>) {
    let issue = |key: &str, code: &str| RunnerConfigIssue { key: key.into(), code: code.into() };
    let Some(raw) = input.as_object() else { return (None, vec![issue("", "notAnObject")]) };
    let keys = RUNNER["config"]["keys"].as_object().expect("runner.json config.keys");
    let mut issues = Vec::new();
    let mut out = Map::new();
    for (key, def) in keys {
        match raw.get(key) {
            None => match def.get("default") {
                _ if def.get("required").and_then(Value::as_bool) == Some(true) => issues.push(issue(key, "missing")),
                Some(d) => {
                    out.insert(key.clone(), d.clone());
                }
                None => {}
            },
            Some(Value::String(s)) if config_value_valid(key, def, s) => {
                out.insert(key.clone(), Value::String(s.clone()));
            }
            Some(_) => issues.push(issue(key, "invalid")),
        }
    }
    let forbidden = strings(&RUNNER, "/config/forbidden");
    let mut extra: Vec<&String> = raw.keys().filter(|k| !keys.contains_key(*k)).collect();
    extra.sort();
    for key in extra {
        issues.push(issue(key, if forbidden.contains(key) { "forbidden" } else { "unknown" }));
    }
    if !issues.is_empty() {
        return (None, issues);
    }
    (serde_json::from_value(Value::Object(out)).ok(), issues)
}

/// `runner_config` as the fixture's JSON: `{config, issues}`.
pub fn runner_config_value(input: &Value) -> Value {
    let (config, issues) = runner_config(input);
    json!({"config": config, "issues": issues})
}

/// Does the package declare an `ask` scope (the shell then offers Permissions)?
pub fn runner_has_permissions(pkg: &Value) -> bool {
    ManifestEntry::of_package(pkg).iter().any(|m| scope_def(&m.id).is_some_and(|(_, s)| !s.host_only && s.consent == ConsentKind::Ask))
}

fn event(name: &str, context: Option<Value>) -> Value {
    match context {
        Some(context) => json!({"event": {"name": name, "context": context}}),
        None => json!({"event": {"name": name}}),
    }
}

fn shell(key: &str) -> Value {
    RUNNER["shell"][key].clone()
}

fn copy(key: &str) -> Value {
    RUNNER["copy"][key].clone()
}

/// The shell around one screen's template: its components and its extra data
/// (always `Some`: without navigation or Permissions only the scrolling page box).
fn shell_components(pkg: &Value, screen: &str, components: &[Value], form: FormFactor) -> Option<(Vec<Value>, Option<Value>)> {
    let nav = eui_core::host::nav_screens(pkg);
    let permissions = runner_has_permissions(pkg);
    let tabs = nav.len() >= 2;
    let page = components.iter().map(|c| {
        if c.get("id").and_then(Value::as_str) == Some("root") {
            let mut c = c.clone();
            c["id"] = json!("runner-page");
            c
        } else {
            c.clone()
        }
    });
    if !tabs && !permissions {
        let mut out = vec![json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "height": "100%", "overflowY": "auto"}, "children": ["runner-page"]})];
        out.extend(page);
        return Some((out, None));
    }
    let title_of = |s: &Map<String, Value>| s.get("title").cloned().unwrap_or_else(|| s["template"].clone());
    let icon = shell("permissionsIcon");
    let mut out = Vec::new();
    match form {
        FormFactor::Wide => {
            let mut nav_children = vec![json!("runner-title")];
            if tabs {
                nav_children.push(json!("runner-screens"));
            }
            if permissions {
                nav_children.push(json!("runner-permissions"));
            }
            out.push(json!({"id": "root", "component": "Stack", "direction": "horizontal", "gap": "none", "align": "stretch", "style": {"height": "100%"}, "children": ["runner-sidebar"]}));
            out.push(json!({"id": "runner-sidebar", "component": "Sidebar", "width": shell("sidebarWidth"), "collapseBelow": "never", "children": ["runner-scroll"], "slots": {"sidebar": "runner-nav"}}));
            out.push(json!({"id": "runner-scroll", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "flexGrow": 1, "minHeight": 0, "overflowY": "auto"}, "children": ["runner-page"]}));
            out.push(json!({"id": "runner-nav", "component": "Stack", "gap": "sm", "style": {"padding": "$spacing.sm"}, "children": nav_children}));
            out.push(json!({"id": "runner-title", "component": "Heading", "text": pkg.get("name").and_then(Value::as_str).unwrap_or(""), "level": shell("titleLevel")}));
            if tabs {
                out.push(json!({"id": "runner-screens", "component": "List", "children": (0..nav.len()).map(|i| format!("runner-screen-{i}")).collect::<Vec<_>>()}));
                for (i, s) in nav.iter().enumerate() {
                    let mut row = Map::new();
                    row.insert("id".into(), json!(format!("runner-screen-{i}")));
                    row.insert("component".into(), json!("Row"));
                    row.insert("title".into(), title_of(s));
                    if let Some(icon) = s.get("icon").filter(|i| truthy(i)) {
                        row.insert("icon".into(), icon.clone());
                    }
                    row.insert("selected".into(), json!(s["template"].as_str() == Some(screen)));
                    row.insert("density".into(), json!("compact"));
                    row.insert("pressable".into(), json!(true));
                    row.insert("on".into(), json!({"press": event("runner.navigate", Some(json!({"screen": s["template"]})))}));
                    out.push(Value::Object(row));
                }
            }
            if permissions {
                out.push(json!({"id": "runner-permissions", "component": "Row", "title": copy("permissions"), "icon": icon, "density": "compact", "pressable": true, "on": {"press": event("runner.permissions", None)}}));
            }
            out.extend(page);
            Some((out, None))
        }
        FormFactor::Compact => {
            let mut bar = Vec::new();
            if tabs {
                bar.push(json!("runner-tabs-box"));
            }
            if permissions {
                bar.push(json!("runner-permissions"));
            }
            out.push(json!({"id": "root", "component": "Stack", "gap": "none", "style": {"height": "100%"}, "children": ["runner-main", "runner-separator", "runner-bar"]}));
            out.push(json!({"id": "runner-main", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "flexGrow": 1, "minHeight": 0, "overflowY": "auto"}, "children": ["runner-page"]}));
            out.push(json!({"id": "runner-separator", "component": "Separator"}));
            out.push(json!({"id": "runner-bar", "component": "Stack", "direction": "horizontal", "gap": "sm", "align": "center", "justify": "end", "style": {"padding": "$spacing.sm"}, "children": bar}));
            if tabs {
                out.push(json!({"id": "runner-tabs-box", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "flexGrow": 1, "minWidth": 0}, "children": ["runner-tabs"]}));
                let tab_list: Vec<Value> = nav
                    .iter()
                    .map(|s| {
                        let mut t = Map::new();
                        t.insert("label".into(), title_of(s));
                        t.insert("value".into(), s["template"].clone());
                        if let Some(icon) = s.get("icon").filter(|i| truthy(i)) {
                            t.insert("icon".into(), icon.clone());
                        }
                        Value::Object(t)
                    })
                    .collect();
                out.push(json!({"id": "runner-tabs", "component": "Tabs", "tabs": tab_list, "value": {"path": "/runner/screen"}, "fill": true, "on": {"change": event("runner.navigate", Some(json!({"screen": {"path": "/runner/screen"}})))}}));
            }
            if permissions {
                out.push(json!({"id": "runner-permissions", "component": "Button", "icon": icon, "size": "icon", "variant": "ghost", "on": {"press": event("runner.permissions", None)}}));
            }
            out.extend(page);
            Some((out, tabs.then(|| json!({"runner": {"screen": screen}}))))
        }
    }
}

/// JS truthiness of a screen's `icon` (the TS twin spreads it when truthy).
fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::String(s) => !s.is_empty(),
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        _ => true,
    }
}

/// The package the runner's PAINTER installs: every screen's template
/// wrapped in the shell for `form`; a package without screens gets one (its
/// first template). The Engine keeps the original package (signature, grants).
pub fn runner_package(pkg: &Value, form: FormFactor) -> Value {
    let Some(obj) = pkg.as_object() else { return pkg.clone() };
    let templates = obj.get("templates").and_then(Value::as_object).cloned().unwrap_or_default();
    let mut out = obj.clone();
    if eui_core::host::package_screens(pkg).is_empty() {
        let Some(first) = templates.keys().next() else { return pkg.clone() };
        let title = obj.get("name").and_then(Value::as_str).unwrap_or(first);
        out.insert("screens".into(), json!([{"template": first, "title": title}]));
    }
    let out_value = Value::Object(out.clone());
    let mut wrapped = templates.clone();
    for s in eui_core::host::package_screens(&out_value) {
        let id = s["template"].as_str().unwrap_or_default();
        let Some(t) = templates.get(id).and_then(Value::as_object) else { continue };
        let Some(components) = t.get("components").and_then(Value::as_array) else { continue };
        let Some((components, data)) = shell_components(&out_value, id, components, form) else { continue };
        let mut t = t.clone();
        t.insert("components".into(), Value::Array(components));
        if let Some(Value::Object(extra)) = data {
            let mut base = t.get("data").and_then(Value::as_object).cloned().unwrap_or_default();
            base.extend(extra);
            t.insert("data".into(), Value::Object(base));
        }
        wrapped.insert(id.to_string(), Value::Object(t));
    }
    out.insert("templates".into(), Value::Object(wrapped));
    Value::Object(out)
}

/// What the runner paints in place of the app when its bundle cannot load.
pub fn runner_failure(kind: RunnerFailure) -> Vec<Value> {
    let key = serde_json::to_value(kind).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
    vec![json!({"id": "root", "component": "EmptyState", "icon": shell("failureIcon"), "title": RUNNER["copy"]["failed"]["title"], "description": RUNNER["copy"]["failed"][key.as_str()]})]
}

/// The failed-action toast (surface `toast`); a broker reason code reads as `copy.refusals`.
pub fn runner_toast(message: &str) -> Vec<Value> {
    let title = RUNNER["copy"]["refusals"].get(message).and_then(Value::as_str).unwrap_or(message);
    vec![json!({"id": "root", "component": "Toast", "title": title, "type": "error"})]
}
