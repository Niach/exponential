//! VAPP-218 (eApps U1): a package's SCREENS, the navigation manifest. A
//! line-by-line port of `packages/exponential-ui/src/host/screens.ts`;
//! `fixtures/host-router.json` (`validation`, `screens`, `navigate`) locks
//! the same behaviour on every platform. `screens: [{template, title,
//! icon?, params?, nav?, exposed?}]` lists the templates a person can open
//! (keyed by the template); the host draws the navigation; `app.navigate
//! {screen, params?}` opens one, its params landing under `/params/*`.
//! #EXP-1674: `nav: false` = a detail screen, left out of the navigation;
//! `exposed` = the public contract other apps link to.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::js_trim;
use super::package::PackageIssue;
use crate::generated::catalog::ICON_NAMES;

/// The most NAVIGATION screens a package declares (the phone bottom bar's
/// room); `nav: false` screens do not count.
pub const MAX_SCREENS: usize = 5;
/// A screen title's most characters (code points).
pub const MAX_SCREEN_TITLE: usize = 32;
/// An exposed screen's description's most characters (code points).
pub const MAX_EXPOSED_DESCRIPTION: usize = 200;
/// The host function (and action call) that opens a screen; always allowed.
pub const NAVIGATE_FUNCTION: &str = "app.navigate";
/// The template data key a screen's params land under (`/params/*`).
pub const SCREEN_PARAMS_KEY: &str = "params";

const SCREEN_KEYS: &[&str] = &["template", "title", "icon", "params", "nav", "exposed"];
const EXPOSED_KEYS: &[&str] = &["description", "params"];
const EXPOSED_PARAM_KEYS: &[&str] = &["required", "description"];

/// A screen as a host opens it: its template and the merged params.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedScreen {
    pub screen: String,
    pub title: String,
    pub icon: Option<String>,
    pub params: Map<String, Value>,
}

/// A navigate call's target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NavigateTarget {
    pub screen: String,
    pub params: Map<String, Value>,
}

fn issue(path: impl Into<String>, message: impl Into<String>) -> PackageIssue {
    PackageIssue { path: path.into(), message: message.into() }
}

fn escape(k: &str) -> String {
    k.replace('~', "~0").replace('/', "~1")
}

fn sorted_keys(o: &Map<String, Value>) -> Vec<&String> {
    let mut keys: Vec<&String> = o.keys().collect();
    keys.sort();
    keys
}

/// Every problem with a package's `screens` (absent = none).
pub fn validate_package_screens(pkg: &Map<String, Value>) -> Vec<PackageIssue> {
    let Some(list) = pkg.get("screens") else { return Vec::new() };
    let Some(list) = list.as_array().filter(|l| !l.is_empty()) else {
        return vec![issue("/screens", "screens is a non-empty list of {template, title, icon?, params?, nav?, exposed?}")];
    };
    let mut issues = Vec::new();
    let in_nav = list.iter().filter(|s| !s.is_object() || s.get("nav") != Some(&Value::Bool(false))).count();
    if in_nav > MAX_SCREENS {
        issues.push(issue("/screens", format!("at most {MAX_SCREENS} screens in the navigation, got {in_nav}")));
    }
    if in_nav == 0 {
        issues.push(issue("/screens", "at least one screen is in the navigation (not nav: false)"));
    }
    let empty = Map::new();
    let templates = pkg.get("templates").and_then(Value::as_object).unwrap_or(&empty);
    let mut seen: Vec<String> = Vec::new();
    for (i, s) in list.iter().enumerate() {
        let at = format!("/screens/{i}");
        let Some(s) = s.as_object() else {
            issues.push(issue(&at, "a screen is {template, title, icon?, params?, nav?, exposed?}"));
            continue;
        };
        for k in sorted_keys(s) {
            if !SCREEN_KEYS.contains(&k.as_str()) {
                issues.push(issue(format!("{at}/{}", escape(k)), format!("unknown screen key {k}")));
            }
        }
        match s.get("template").and_then(Value::as_str).filter(|t| !t.is_empty()) {
            None => issues.push(issue(format!("{at}/template"), "template is a required string")),
            Some(t) if !templates.contains_key(t) => issues.push(issue(format!("{at}/template"), format!("template {t} names no package template"))),
            Some(t) if seen.iter().any(|x| x == t) => issues.push(issue(format!("{at}/template"), format!("duplicate screen {t}"))),
            Some(t) => seen.push(t.to_string()),
        }
        match s.get("title").and_then(Value::as_str) {
            Some(t) if !js_trim(t).is_empty() => {
                if t.chars().count() > MAX_SCREEN_TITLE {
                    issues.push(issue(format!("{at}/title"), format!("title is at most {MAX_SCREEN_TITLE} characters")));
                }
            }
            _ => issues.push(issue(format!("{at}/title"), "title is a required string")),
        }
        if let Some(icon) = s.get("icon") {
            if !icon.as_str().is_some_and(|n| ICON_NAMES.contains(&n)) {
                let shown = icon.as_str().map(str::to_string).unwrap_or_else(|| icon.to_string());
                issues.push(issue(format!("{at}/icon"), format!("icon {shown} is not an Exponential UI icon")));
            }
        }
        if let Some(params) = s.get("params") {
            match params.as_object() {
                None => issues.push(issue(format!("{at}/params"), "params is an object of strings")),
                Some(params) => {
                    for k in sorted_keys(params) {
                        if !params[k].is_string() {
                            issues.push(issue(format!("{at}/params/{}", escape(k)), format!("params.{k} is a string")));
                        }
                    }
                }
            }
        }
        if s.get("nav").is_some_and(|nav| *nav != Value::Bool(false)) {
            issues.push(issue(format!("{at}/nav"), "nav is false or absent"));
        }
        if let Some(exposed) = s.get("exposed") {
            issues.extend(exposed_issues(exposed, &format!("{at}/exposed")));
        }
    }
    issues.extend(navigate_issues(pkg, &seen));
    issues
}

/// #EXP-1674: what is wrong with a screen's `exposed`.
fn exposed_issues(exposed: &Value, at: &str) -> Vec<PackageIssue> {
    let Some(exposed) = exposed.as_object() else {
        return vec![issue(at, "exposed is {description, params?}")];
    };
    let mut issues = Vec::new();
    for k in sorted_keys(exposed) {
        if !EXPOSED_KEYS.contains(&k.as_str()) {
            issues.push(issue(format!("{at}/{}", escape(k)), format!("unknown exposed key {k}")));
        }
    }
    match exposed.get("description").and_then(Value::as_str) {
        Some(d) if !js_trim(d).is_empty() => {
            if d.chars().count() > MAX_EXPOSED_DESCRIPTION {
                issues.push(issue(format!("{at}/description"), format!("description is at most {MAX_EXPOSED_DESCRIPTION} characters")));
            }
        }
        _ => issues.push(issue(format!("{at}/description"), "description is a required string")),
    }
    let Some(params) = exposed.get("params") else { return issues };
    let Some(params) = params.as_object() else {
        issues.push(issue(format!("{at}/params"), "params is an object of {required?, description?}"));
        return issues;
    };
    for k in sorted_keys(params) {
        let here = format!("{at}/params/{}", escape(k));
        if k.is_empty() || !k.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            issues.push(issue(&here, format!("param {k} is letters, digits and _")));
        }
        let Some(param) = params[k].as_object() else {
            issues.push(issue(&here, "a param is {required?, description?}"));
            continue;
        };
        for key in sorted_keys(param) {
            if !EXPOSED_PARAM_KEYS.contains(&key.as_str()) {
                issues.push(issue(format!("{here}/{}", escape(key)), format!("unknown param key {key}")));
            }
        }
        if param.get("required").is_some_and(|r| !r.is_boolean()) {
            issues.push(issue(format!("{here}/required"), "required is a boolean"));
        }
        if param.get("description").is_some_and(|d| !d.is_string()) {
            issues.push(issue(format!("{here}/description"), "description is a string"));
        }
    }
    issues
}

/// A CEL string literal (`'deals'`, `"deals"`) → its text; else None.
fn cel_string_literal(v: &Value) -> Option<String> {
    let s = js_trim(v.as_str()?);
    for q in ['\'', '"'] {
        if s.len() >= 2 && s.starts_with(q) && s.ends_with(q) {
            let inner = &s[1..s.len() - 1];
            if !inner.contains(q) && !inner.contains('\\') {
                return Some(inner.to_string());
            }
        }
    }
    None
}

/// Every literal navigate target names a screen: a press's
/// `functionCall {call: app.navigate, args: {screen}}` (a JSON string) and
/// an action's `{call: app.navigate, args: {screen: <CEL string literal>}}`.
fn navigate_issues(pkg: &Map<String, Value>, screens: &[String]) -> Vec<PackageIssue> {
    let mut issues = Vec::new();
    let check = |screen: Option<String>, at: String, issues: &mut Vec<PackageIssue>| {
        if let Some(screen) = screen {
            if !screens.iter().any(|s| *s == screen) {
                issues.push(issue(at, format!("navigate: {screen} is not a screen")));
            }
        }
    };
    if let Some(templates) = pkg.get("templates").and_then(Value::as_object) {
        for id in sorted_keys(templates) {
            let Some(components) = templates[id].get("components").and_then(Value::as_array) else { continue };
            for (i, c) in components.iter().enumerate() {
                let Some(on) = c.get("on").and_then(Value::as_object) else { continue };
                for event in sorted_keys(on) {
                    let Some(fc) = on[event].get("functionCall").filter(|f| f.is_object()) else { continue };
                    if fc.get("call").and_then(Value::as_str) != Some(NAVIGATE_FUNCTION) {
                        continue;
                    }
                    let at = format!("/templates/{}/components/{i}/on/{}/functionCall/args", escape(id), escape(event));
                    match fc.get("args").and_then(Value::as_object) {
                        None => issues.push(issue(at, "navigate takes {screen, params?}")),
                        Some(args) => check(args.get("screen").and_then(Value::as_str).map(str::to_string), format!("{at}/screen"), &mut issues),
                    }
                }
            }
        }
    }
    if let Some(actions) = pkg.get("actions").and_then(Value::as_object) {
        for name in sorted_keys(actions) {
            let Some(steps) = actions[name].get("steps").and_then(Value::as_array) else { continue };
            for (i, step) in steps.iter().enumerate() {
                if !step.is_object() || step.get("call").and_then(Value::as_str) != Some(NAVIGATE_FUNCTION) {
                    continue;
                }
                let at = format!("/actions/{}/steps/{i}/args", escape(name));
                match step.get("args").and_then(Value::as_object) {
                    None => issues.push(issue(at, "navigate takes {screen, params?}")),
                    Some(args) => check(args.get("screen").and_then(cel_string_literal), format!("{at}/screen"), &mut issues),
                }
            }
        }
    }
    issues
}

/// The screens of a (valid) package, in declared order.
pub fn package_screens(pkg: &Value) -> Vec<&Map<String, Value>> {
    pkg.get("screens").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_object).filter(|s| s.get("template").is_some_and(Value::is_string)).collect()
}

/// #EXP-1674: the screens the host's navigation paints (no `nav: false`).
pub fn nav_screens(pkg: &Value) -> Vec<&Map<String, Value>> {
    package_screens(pkg).into_iter().filter(|s| s.get("nav") != Some(&Value::Bool(false))).collect()
}

/// The screen a host opens for a request: the requested one when the
/// package has it (a `nav: false` one too), else the FIRST in the navigation;
/// its params = the screen's own, then the request's string params key by
/// key. None = the package has no screens.
pub fn resolve_screen(pkg: &Value, screen: Option<&str>, params: Option<&Value>) -> Option<ResolvedScreen> {
    let screens = package_screens(pkg);
    let first = nav_screens(pkg).first().copied();
    let picked = screens.iter().find(|s| s.get("template").and_then(Value::as_str) == screen).copied().or(first)?;
    let mut merged = Map::new();
    for source in [picked.get("params"), params] {
        let Some(o) = source.and_then(Value::as_object) else { continue };
        for k in sorted_keys(o) {
            if o[k].is_string() {
                merged.insert(k.clone(), o[k].clone());
            }
        }
    }
    Some(ResolvedScreen {
        screen: picked["template"].as_str().unwrap_or_default().to_string(),
        title: picked.get("title").and_then(Value::as_str).unwrap_or_default().to_string(),
        icon: picked.get("icon").and_then(Value::as_str).map(str::to_string),
        params: merged,
    })
}

/// A navigate call's args → the target, or why it is refused.
pub fn navigate_target(pkg: &Value, args: &Value) -> Result<NavigateTarget, String> {
    let Some(args) = args.as_object() else { return Err("navigate takes {screen, params?}".into()) };
    let Some(screen) = args.get("screen").and_then(Value::as_str).filter(|s| !s.is_empty()) else { return Err("navigate: screen is a required string".into()) };
    if !package_screens(pkg).iter().any(|s| s.get("template").and_then(Value::as_str) == Some(screen)) {
        return Err(format!("navigate: {screen} is not a screen"));
    }
    let mut params = Map::new();
    match args.get("params") {
        None | Some(Value::Null) => {}
        Some(Value::Object(o)) => {
            for k in sorted_keys(o) {
                if !o[k].is_string() {
                    return Err(format!("navigate: params.{k} is a string"));
                }
                params.insert(k.clone(), o[k].clone());
            }
        }
        Some(_) => return Err("navigate: params is an object of strings".into()),
    }
    Ok(NavigateTarget { screen: screen.to_string(), params })
}

/// `navigate_target` as the fixture's JSON: `{screen, params}` | `{error}`.
pub fn navigate_target_value(pkg: &Value, args: &Value) -> Value {
    match navigate_target(pkg, args) {
        Ok(t) => json!({"screen": t.screen, "params": t.params}),
        Err(error) => json!({"error": error}),
    }
}

/// #EXP-1674: what a link FROM ANOTHER APP may open in this package, or why it
/// is refused. The target (`screen`, else the first in the navigation) must be
/// `exposed`; its params = the screen's own, then the request's string params
/// the contract declares (anything else dropped); a `required` one must be
/// present and non-empty.
pub fn cross_app_target(pkg: &Value, screen: Option<&str>, params: Option<&Value>) -> Result<NavigateTarget, String> {
    let picked = match screen.filter(|s| !s.is_empty()) {
        Some(screen) => package_screens(pkg).into_iter().find(|s| s.get("template").and_then(Value::as_str) == Some(screen)),
        None => nav_screens(pkg).first().copied(),
    };
    let Some(picked) = picked else { return Err("no such screen".into()) };
    let template = picked.get("template").and_then(Value::as_str).unwrap_or_default();
    let Some(exposed) = picked.get("exposed").and_then(Value::as_object) else { return Err(format!("{template} is not exposed")) };
    let empty = Map::new();
    let declared = exposed.get("params").and_then(Value::as_object).unwrap_or(&empty);
    let mut merged = Map::new();
    if let Some(own) = picked.get("params").and_then(Value::as_object) {
        for k in sorted_keys(own) {
            if own[k].is_string() {
                merged.insert(k.clone(), own[k].clone());
            }
        }
    }
    if let Some(asked) = params.and_then(Value::as_object) {
        for k in sorted_keys(asked) {
            if declared.contains_key(k) && asked[k].is_string() {
                merged.insert(k.clone(), asked[k].clone());
            }
        }
    }
    for k in sorted_keys(declared) {
        let required = declared[k].get("required").and_then(Value::as_bool) == Some(true);
        if required && merged.get(k).and_then(Value::as_str).is_none_or(str::is_empty) {
            return Err(format!("{template} needs {k}"));
        }
    }
    Ok(NavigateTarget { screen: template.to_string(), params: merged })
}

/// `cross_app_target` as the fixture's JSON: `{screen, params}` | `{error}`.
pub fn cross_app_target_value(pkg: &Value, screen: Option<&str>, params: Option<&Value>) -> Value {
    match cross_app_target(pkg, screen, params) {
        Ok(t) => json!({"screen": t.screen, "params": t.params}),
        Err(error) => json!({"error": error}),
    }
}

/// The data a screen's template opens with: `{params}`.
pub fn screen_data(screen: &ResolvedScreen) -> Value {
    json!({ SCREEN_PARAMS_KEY: screen.params })
}

/// A binding source's `${params.<key>}` filled from a screen's params
/// (percent-encoded like a query value; a missing key = empty).
pub fn fill_screen_params(source: &str, params: Option<&Value>) -> String {
    let Some(params) = params.and_then(Value::as_object) else { return source.to_string() };
    if !source.contains("${params.") {
        return source.to_string();
    }
    let mut out = String::with_capacity(source.len());
    let mut rest = source;
    while let Some(at) = rest.find("${params.") {
        out.push_str(&rest[..at]);
        let after = &rest[at + "${params.".len()..];
        let key_len = after.bytes().take_while(|b| b.is_ascii_alphanumeric() || *b == b'_').count();
        if key_len > 0 && after[key_len..].starts_with('}') {
            if let Some(v) = params.get(&after[..key_len]).and_then(Value::as_str) {
                out.push_str(&super::sources::encode_component(v));
            }
            rest = &after[key_len + 1..];
        } else {
            out.push_str("${params.");
            rest = after;
        }
    }
    out.push_str(rest);
    out
}
