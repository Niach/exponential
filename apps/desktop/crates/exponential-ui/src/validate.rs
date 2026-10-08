//! Props validation against a component definition — the catalog's own mini
//! schema (`PropSchema`), so the reducer, the fixtures and the generator agree
//! without a JSON Schema engine. Mirrors `src/validate.ts`.

use indexmap::IndexMap;
use serde_json::Value;

use crate::catalog::{is_known_token, parse_token_ref, CatalogView};
use crate::style_check::{is_hex_color, validate_style};
use crate::types::{ComponentDef, PropSchema, Props};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PropIssue {
    pub path: String,
    pub message: String,
}

/// An A2UI dynamic value: a data binding (`{path}` alone) or a function call.
pub fn is_dynamic(value: &Value) -> bool {
    let Some(obj) = value.as_object() else { return false };
    if obj.get("path").is_some_and(Value::is_string) && obj.len() == 1 {
        return true;
    }
    obj.get("call").is_some_and(Value::is_string)
}

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
    match schema.type_.as_str() {
        "string" | "markdown" | "url" => {
            if !value.is_string() {
                issue(issues, path, "expected a string");
            }
        }
        "date" => {
            if !value.as_str().is_some_and(|s| s.is_empty() || is_iso_date(s)) {
                issue(issues, path, "expected yyyy-mm-dd");
            }
        }
        "number" => {
            if !value.as_f64().is_some_and(f64::is_finite) {
                issue(issues, path, "expected a number");
            }
        }
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
        "color" => {
            if value.as_str().is_some_and(is_hex_color) {
                return;
            }
            if is_known_token(value) && parse_token_ref(value).is_some_and(|(group, _)| group == "color") {
                return;
            }
            issue(issues, path, "expected #hex or $color.<name>");
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
            let Some(def) = schema.shape.as_ref().and_then(|s| view.defs.get(s)) else {
                issue(issues, path, format!("shape {} is not defined", schema.shape.as_deref().unwrap_or("undefined")));
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
    // `Props` iterates in its own key order (sorted without serde_json's
    // `preserve_order`); the TS iterates the author's order.
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
