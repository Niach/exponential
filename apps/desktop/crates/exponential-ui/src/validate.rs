//! Props validation against a component definition — the catalog's own mini
//! schema (`PropSchema`), so the reducer, the fixtures and the generator agree
//! without a JSON Schema engine. Mirrors `src/validate.ts`.

use indexmap::IndexMap;
use serde_json::Value;

use crate::catalog::{is_known_token, parse_token_ref, CatalogView};
use crate::macros::{is_responsive_value, BREAKPOINTS};
use crate::strings::{is_string_ref, parse_string_ref};
use crate::expr::is_call;
use crate::style_check::validate_style;
use crate::types::UiNode;
use crate::types::{ComponentDef, PropSchema, Props};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PropIssue {
    pub path: String,
    pub message: String,
}

pub use crate::expr::is_dynamic;

/// `^\d{4}-\d{2}-\d{2}$`
fn is_iso_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter().enumerate().all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

/// The Icon prop's vocabulary: `packages/icons/icons.json` at generate time.
fn is_icon_name(value: &Value) -> bool {
    value.as_str().is_some_and(|s| crate::generated::catalog::ICON_NAMES.contains(&s))
}

fn issue(issues: &mut Vec<PropIssue>, path: &str, message: impl Into<String>) {
    issues.push(PropIssue { path: path.to_string(), message: message.into() });
}

fn check_value(schema: &PropSchema, value: &Value, path: &str, view: &CatalogView, issues: &mut Vec<PropIssue>) {
    if schema.bindable == Some(true) && is_dynamic(value) {
        return;
    }
    if schema.responsive == Some(true) && is_responsive_value(value) {
        for (bp, v) in value.as_object().into_iter().flatten() {
            check_scalar(schema, v, &format!("{path}.{bp}"), view, issues);
        }
        return;
    }
    if schema.responsive == Some(true) && value.is_object() && !is_dynamic(value) {
        issue(issues, path, format!("a responsive value needs base and only {} besides", BREAKPOINTS.join("|")));
        return;
    }
    check_scalar(schema, value, path, view, issues);
}

fn check_scalar(schema: &PropSchema, value: &Value, path: &str, view: &CatalogView, issues: &mut Vec<PropIssue>) {
    match schema.type_.as_str() {
        "string" | "markdown" | "url" => match value.as_str() {
            None => issue(issues, path, "expected a string"),
            Some(s) => {
                if parse_string_ref(s).is_some() && !is_string_ref(s) {
                    issue(issues, path, format!("unknown built-in string {s} (catalog/strings.json)"));
                }
            }
        },
        "date" => {
            if !value.as_str().is_some_and(|s| s.is_empty() || is_iso_date(s)) {
                issue(issues, path, "expected yyyy-mm-dd");
            }
        }
        "number" => match value.as_f64().filter(|n| n.is_finite()) {
            None => issue(issues, path, "expected a number"),
            Some(n) => {
                if let Some(min) = schema.minimum.filter(|min| n < *min) {
                    issue(issues, path, format!("expected at least {}", crate::json::number_to_string(min)));
                } else if let Some(max) = schema.maximum.filter(|max| n > *max) {
                    issue(issues, path, format!("expected at most {}", crate::json::number_to_string(max)));
                }
            }
        },
        "boolean" => {
            if !value.is_boolean() {
                issue(issues, path, "expected a boolean");
            }
        }
        "enum" => {
            let values = schema.values.clone().or_else(|| {
                schema.enum_.as_ref().and_then(|e| view.enums.get(e)).map(|v| v.iter().map(|s| Value::String(s.clone())).collect())
            });
            match values {
                None => issue(issues, path, format!("enum {} is not defined", schema.enum_.as_deref().unwrap_or("undefined"))),
                Some(values) => {
                    if !values.iter().any(|v| crate::json::strict_eq(v, value)) {
                        let list = values.iter().map(crate::json::to_js_string).collect::<Vec<_>>().join("|");
                        issue(issues, path, format!("expected one of {list}"));
                    }
                }
            }
        }
        "icon" => {
            if !is_icon_name(value) {
                issue(issues, path, "not an icons.json name");
            }
        }
        // Round 4 (VAPP-103): a node's colour is a TOKEN, never a literal, so
        // the theme's light and dark modes both apply (themes keep literals).
        "color" => {
            if is_known_token(value) && parse_token_ref(value).is_some_and(|(group, _)| group == "color") {
                return;
            }
            issue(issues, path, "expected $color.<name>");
        }
        "style" => {
            for i in validate_style(value, path, Some(false)) {
                issues.push(PropIssue { path: i.path, message: i.message });
            }
        }
        "array" => {
            let Some(items) = value.as_array() else {
                issue(issues, path, "expected an array");
                return;
            };
            if let Some(item_schema) = &schema.items {
                for (i, item) in items.iter().enumerate() {
                    check_value(item_schema, item, &format!("{path}[{i}]"), view, issues);
                }
            }
        }
        "object" => {
            let Some(obj) = value.as_object() else {
                issue(issues, path, "expected an object");
                return;
            };
            let Some(shape) = &schema.shape else { return };
            let Some(def) = view.defs.get(shape) else {
                issue(issues, path, format!("shape {shape} is not defined"));
                return;
            };
            check_props(&def.properties, obj, path, view, issues);
        }
        _ => {}
    }
}

fn check_props(schemas: &IndexMap<String, PropSchema>, props: &Props, path: &str, view: &CatalogView, issues: &mut Vec<PropIssue>) {
    for (name, schema) in schemas {
        match props.get(name) {
            None | Some(Value::Null) => {
                if schema.required == Some(true) {
                    issue(issues, &format!("{path}.{name}"), "required");
                }
            }
            Some(value) => check_value(schema, value, &format!("{path}.{name}"), view, issues),
        }
    }
    // `Props` keeps the author's order (`preserve_order`), like the TS.
    for name in props.keys() {
        if !schemas.contains_key(name) {
            issue(issues, &format!("{path}.{name}"), "unknown prop");
        }
    }
}

/// Every issue in a node's props against its component definition; `path`
/// prefixes every issue path (the reducer passes `props`).
pub fn validate_props(def: &ComponentDef, props: &Props, path: &str, view: &CatalogView) -> Vec<PropIssue> {
    let mut issues = Vec::new();
    check_props(&def.props, props, path, view, &mut issues);
    issues
}

/// A function name a surface may call: a catalog function, or a HOST
/// function, which is always namespaced (`app.toast`, `harness.openIssue`).
pub fn is_callable_name(name: &str) -> bool {
    if crate::generated::catalog::FUNCTION_NAMES.contains(&name) {
        return true;
    }
    // `^[A-Za-z_][\w-]*(\.[A-Za-z_][\w-]*)+$` (src/validate.ts).
    let segments: Vec<&str> = name.split('.').collect();
    segments.len() >= 2
        && segments.iter().all(|s| {
            let mut chars = s.chars();
            chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        })
}

fn check_call(call: &Value, path: &str, issues: &mut Vec<PropIssue>) {
    let name = call["call"].as_str().unwrap_or("");
    if !is_callable_name(name) {
        issue(issues, path, format!("unknown function \"{name}\"; known: the catalog functions, or a namespaced host function (app.toast)"));
    }
}

/// Every `{call}` at any depth of an EXPRESSION (`visible`, a call's args,
/// an event context): each resolves at any depth, so every `{call}` in it
/// is a call. `src/validate.ts checkCallsDeep`.
fn check_calls_deep(value: &Value, path: &str, issues: &mut Vec<PropIssue>) {
    match value {
        Value::Array(items) => {
            for (i, v) in items.iter().enumerate() {
                check_calls_deep(v, &format!("{path}[{i}]"), issues);
            }
        }
        Value::Object(obj) => {
            if is_call(value) {
                check_call(value, path, issues);
            }
            for (k, v) in obj {
                check_calls_deep(v, &format!("{path}.{k}"), issues);
            }
        }
        _ => {}
    }
}

/// The calls a PROP makes, walked along its schema exactly as the bind pass
/// resolves it (`data::resolve_prop`): a `{call}` AT a position is a call
/// (its args are expressions); a DATA position (Table `rows`) is literal
/// data, never descended; arrays and shaped objects walk per item /
/// property; a `style` prop resolves at any depth; anything else is a
/// literal. `src/validate.ts checkPropCalls`.
fn check_prop_calls(value: &Value, schema: Option<&PropSchema>, path: &str, view: &CatalogView, issues: &mut Vec<PropIssue>) {
    if is_call(value) {
        check_call(value, path, issues);
        if let Some(args) = value.get("args") {
            check_calls_deep(args, &format!("{path}.args"), issues);
        }
        return;
    }
    let Some(schema) = schema else { return };
    if crate::data::is_data_schema(Some(schema)) {
        return;
    }
    if schema.type_ == "style" {
        return check_calls_deep(value, path, issues);
    }
    if let (Some(items_schema), Value::Array(items)) = (schema.items.as_deref().filter(|_| schema.type_ == "array"), value) {
        for (i, item) in items.iter().enumerate() {
            check_prop_calls(item, Some(items_schema), &format!("{path}[{i}]"), view, issues);
        }
        return;
    }
    let shape = (schema.type_ == "object").then_some(()).and(schema.shape.as_ref()).and_then(|name| view.defs.get(name));
    if let (Some(shape), Value::Object(map)) = (shape, value) {
        for (k, v) in map {
            check_prop_calls(v, shape.properties.get(k), &format!("{path}.{k}"), view, issues);
        }
    }
}

/// The calls an ACTION makes: its `functionCall` (name + args) and its event
/// context (an expression). Nothing else in it is evaluated.
fn check_action_calls(action: &Value, path: &str, issues: &mut Vec<PropIssue>) {
    if let Some(call) = action.get("functionCall").filter(|c| is_call(c)) {
        let at = format!("{path}.functionCall");
        check_call(call, &at, issues);
        if let Some(args) = call.get("args") {
            check_calls_deep(args, &format!("{at}.args"), issues);
        }
    }
    if let Some(context) = action.get("event").and_then(|e| e.get("context")) {
        check_calls_deep(context, &format!("{path}.event.context"), issues);
    }
}

/// [`validate_node_in`] against the core catalog alone.
pub fn validate_node(node: &UiNode) -> Vec<PropIssue> {
    validate_node_in(node, &CatalogView::core())
}

/// What `validate_props` leaves out, round 4 (VAPP-103; `src/validate.ts
/// validateNode`): the node's `style` (whitelisted keys, known tokens,
/// `$color.*` colours), every function a prop (along its schema in `view`:
/// never inside DATA), `visible` or an `on` action calls, and a Table's slot
/// columns. In this order.
pub fn validate_node_in(node: &UiNode, view: &CatalogView) -> Vec<PropIssue> {
    let mut issues = Vec::new();
    if let Some(style) = &node.style {
        for i in validate_style(&Value::Object(style.clone()), "style", None) {
            issues.push(PropIssue { path: i.path, message: i.message });
        }
    }
    let def = view.component(&node.component);
    for (k, v) in &node.props {
        check_prop_calls(v, def.and_then(|d| d.props.get(k)), &format!("props.{k}"), view, &mut issues);
    }
    if let Some(visible) = &node.visible {
        check_calls_deep(visible, "visible", &mut issues);
    }
    for (event, action) in node.on.iter().flatten() {
        check_action_calls(action, &format!("on.{event}"), &mut issues);
    }
    if node.component == "Table" {
        if let Some(Value::Array(columns)) = node.props.get("columns") {
            let slots: Vec<&str> = node.slots.iter().flat_map(|s| s.keys()).map(String::as_str).collect();
            for (i, column) in columns.iter().enumerate() {
                if column.get("type").and_then(Value::as_str) != Some("slot") {
                    continue;
                }
                let path = format!("props.columns[{i}].slot");
                match column.get("slot").and_then(Value::as_str) {
                    None => issue(&mut issues, &path, "a slot column names one of the Table's slots"),
                    Some(slot) if !slots.contains(&slot) => {
                        let known = if slots.is_empty() { "none".to_string() } else { slots.join("|") };
                        issue(&mut issues, &path, format!("no slot \"{slot}\" on this Table; slots: {known}"));
                    }
                    Some(_) => {}
                }
            }
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn dynamic_values_and_dates() {
        assert!(is_dynamic(&json!({"path": "/x"})));
        assert!(!is_dynamic(&json!({"path": "/x", "y": 1})));
        assert!(is_dynamic(&json!({"call": "regex", "args": {}})));
        assert!(is_iso_date("2026-10-07") && !is_iso_date("2026-1-07") && !is_iso_date("2026/10/07"));
    }

    #[test]
    fn issues_name_their_paths() {
        let view = CatalogView::core();
        let def = view.component("Text").unwrap().clone();
        let props = json!({"text": 3, "bogus": true}).as_object().unwrap().clone();
        let issues = validate_props(&def, &props, "props", &view);
        assert!(issues.contains(&PropIssue { path: "props.text".into(), message: "expected a string".into() }));
        assert!(issues.contains(&PropIssue { path: "props.bogus".into(), message: "unknown prop".into() }));
    }
}
