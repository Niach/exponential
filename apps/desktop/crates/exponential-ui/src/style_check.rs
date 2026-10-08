//! The `Box` style whitelist as a validator: `validateStyle` of
//! `src/style.ts`, keyed off the embedded `catalog/style.json`. The regexes of
//! the TS are small hand-written parsers here (no regex engine).

use std::sync::LazyLock;

use indexmap::IndexMap;
use serde::Deserialize;
use serde_json::Value;

use crate::catalog::{is_known_token, parse_token_ref};
use crate::generated::catalog as g;

/// One `style.json` key spec.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct KeySpec {
    #[serde(default, rename = "type")]
    pub type_: Option<String>,
    #[serde(default, rename = "enum")]
    pub enum_: Option<Vec<Value>>,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default, rename = "rootOnly")]
    pub root_only: Option<bool>,
    #[serde(default, rename = "layoutEffect")]
    pub layout_effect: Option<bool>,
}

#[derive(Deserialize)]
struct StyleFile {
    layout: IndexMap<String, KeySpec>,
    visual: IndexMap<String, KeySpec>,
}

/// Every whitelisted key → its spec (layout keys first, then visual).
pub static STYLE_SPECS: LazyLock<IndexMap<String, KeySpec>> = LazyLock::new(|| {
    let file: StyleFile = serde_json::from_str(g::STYLE_JSON).expect("style.json");
    let mut out = file.layout;
    out.extend(file.visual);
    out
});

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StyleIssue {
    pub path: String,
    pub message: String,
}

const NUMERIC_TOKEN_GROUPS: &[&str] = &["spacing", "radius", "control", "type.size", "type.lineHeight", "opacity", "border"];

fn digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// `\d+(\.\d+)?`
fn decimal(s: &str) -> bool {
    match s.split_once('.') {
        Some((i, f)) => digits(i) && digits(f),
        None => digits(s),
    }
}

/// `^-?\d+(\.\d+)?(px|%)$`
fn is_length(s: &str) -> bool {
    let body = s.strip_prefix('-').unwrap_or(s);
    let num = body.strip_suffix("px").or_else(|| body.strip_suffix('%'));
    num.is_some_and(decimal)
}

/// `^#([0-9a-fA-F]{6}|[0-9a-fA-F]{8}|[0-9a-fA-F]{3,4})$`
pub fn is_hex_color(s: &str) -> bool {
    s.strip_prefix('#')
        .is_some_and(|h| matches!(h.len(), 3 | 4 | 6 | 8) && h.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// `^\d+(\.\d+)?\/\d+(\.\d+)?$`
fn is_ratio(s: &str) -> bool {
    s.split_once('/').is_some_and(|(w, h)| decimal(w) && decimal(h))
}

/// `^@media \(min-width: \d+(\.\d+)?px\)$` (`STYLE_MEDIA_PATTERN`).
pub fn is_media_key(s: &str) -> bool {
    s.strip_prefix("@media (min-width: ")
        .and_then(|r| r.strip_suffix("px)"))
        .is_some_and(decimal)
}

/// A `@media` or state (`:pressed`) condition key.
pub fn is_condition_key(s: &str) -> bool {
    is_media_key(s) || g::STYLE_STATES.contains(&s)
}

fn token_ok(value: &Value, groups: &[&str]) -> bool {
    parse_token_ref(value).is_some_and(|(group, _)| groups.contains(&group.as_str())) && is_known_token(value)
}

fn finite_number(value: &Value) -> bool {
    value.as_f64().is_some_and(f64::is_finite)
}

fn js_join(values: &[Value]) -> String {
    values.iter().map(crate::json::to_js_string).collect::<Vec<_>>().join("|")
}

fn value_error(key: &str, spec: &KeySpec, value: &Value) -> Option<String> {
    if let Some(values) = &spec.enum_ {
        return if values.iter().any(|v| crate::json::strict_eq(v, value)) {
            None
        } else {
            Some(format!("{key}: expected one of {}", js_join(values)))
        };
    }
    let s = value.as_str();
    match spec.type_.as_deref() {
        Some("number") => {
            if finite_number(value) || token_ok(value, NUMERIC_TOKEN_GROUPS) {
                None
            } else {
                Some(format!("{key}: expected a number or a numeric token"))
            }
        }
        Some("length") => {
            if finite_number(value) || s == Some("auto") || s.is_some_and(is_length) || token_ok(value, NUMERIC_TOKEN_GROUPS) {
                None
            } else {
                Some(format!("{key}: expected px, \"N%\", \"auto\" or a numeric token"))
            }
        }
        Some("color") => {
            if s.is_some_and(is_hex_color) || token_ok(value, &["color"]) {
                None
            } else {
                Some(format!("{key}: expected #hex or $color.<name>"))
            }
        }
        Some("token") => {
            let group = spec.group.as_deref().unwrap_or("");
            if token_ok(value, &[group]) {
                None
            } else {
                Some(format!("{key}: expected ${}.<name>", spec.group.as_deref().unwrap_or("undefined")))
            }
        }
        Some("ratio") => {
            if value.as_f64().is_some_and(|v| v > 0.0) || s.is_some_and(is_ratio) {
                None
            } else {
                Some(format!("{key}: expected a number or \"w/h\""))
            }
        }
        Some("tracks") | Some("string") => {
            if s.is_some() {
                None
            } else {
                Some(format!("{key}: expected a string"))
            }
        }
        Some("areas") => {
            if value.as_array().is_some_and(|rows| rows.iter().all(Value::is_string)) {
                None
            } else {
                Some(format!("{key}: expected an array of row strings"))
            }
        }
        _ => Some(format!("{key}: unknown spec")),
    }
}

/// Every rule the client resolver relies on: whitelisted keys, well-typed
/// values, conditions one level deep, `direction` on the root only
/// (`root == Some(false)` flags it; `None` = unknown, allowed).
pub fn validate_style(style: &Value, path: &str, root: Option<bool>) -> Vec<StyleIssue> {
    let Some(obj) = style.as_object() else {
        return vec![StyleIssue { path: path.to_string(), message: "expected an object".into() }];
    };
    let mut issues = Vec::new();
    walk(obj, path, false, root, &mut issues);
    issues
}

fn walk(obj: &serde_json::Map<String, Value>, at: &str, nested: bool, root: Option<bool>, issues: &mut Vec<StyleIssue>) {
    let specs = &*STYLE_SPECS;
    for (key, value) in obj {
        let here = format!("{at}.{key}");
        if is_condition_key(key) {
            if nested {
                issues.push(StyleIssue { path: here, message: "conditions do not nest".into() });
            } else if let Some(inner) = value.as_object() {
                walk(inner, &here, true, root, issues);
            } else {
                issues.push(StyleIssue { path: here, message: "must be an object".into() });
            }
            continue;
        }
        let Some(spec) = specs.get(key) else {
            issues.push(StyleIssue { path: here, message: "not in the Box style whitelist".into() });
            continue;
        };
        if spec.root_only == Some(true) && root == Some(false) {
            issues.push(StyleIssue { path: here.clone(), message: "allowed on the root only".into() });
        }
        if let Some(error) = value_error(key, spec, value) {
            issues.push(StyleIssue { path: here, message: error });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn v(style: Value) -> Vec<StyleIssue> {
        validate_style(&style, "style", None)
    }

    #[test]
    fn box_style_whitelist_the_spikes_v1_whitelist_plus_the_logical_inline_properties() {
        let spike = [
            "display", "flexDirection", "flexWrap", "justifyContent", "alignItems", "alignContent", "alignSelf", "justifySelf",
            "flexGrow", "flexShrink", "flexBasis", "gap", "rowGap", "columnGap", "direction", "overflow", "width", "height",
            "minWidth", "minHeight", "maxWidth", "maxHeight", "aspectRatio", "position", "top", "right", "bottom", "left", "inset",
            "padding", "paddingTop", "paddingRight", "paddingBottom", "paddingLeft", "paddingHorizontal", "paddingVertical",
            "margin", "marginTop", "marginRight", "marginBottom", "marginLeft", "marginHorizontal", "marginVertical",
            "gridTemplateColumns", "gridTemplateRows", "gridTemplateAreas", "gridArea", "gridColumn", "gridRow",
            "backgroundColor", "color", "borderWidth", "borderColor", "borderRadius", "opacity", "boxShadow", "fontSize",
            "fontWeight", "lineHeight", "textAlign",
        ];
        for key in spike {
            assert!(STYLE_SPECS.contains_key(key), "{key}");
        }
        for key in ["insetInlineStart", "insetInlineEnd", "paddingInlineStart", "paddingInlineEnd", "marginInlineStart", "marginInlineEnd"] {
            assert!(STYLE_SPECS.contains_key(key), "{key}");
        }
        for key in ["zIndex", "transition", "transform", "order", "gridAutoFlow", "justifyItems"] {
            assert!(!STYLE_SPECS.contains_key(key), "{key}");
        }
        assert_eq!(STYLE_SPECS["borderWidth"].layout_effect, Some(true));
        let keys: Vec<&str> = STYLE_SPECS.keys().map(String::as_str).collect();
        assert_eq!(keys, g::STYLE_KEYS);
        assert_eq!(g::STYLE_MEDIA_PATTERN, r"^@media \(min-width: \d+(\.\d+)?px\)$");
    }

    #[test]
    fn box_style_whitelist_values_px_percent_auto_tokens_colours() {
        assert_eq!(
            v(json!({"width": 12, "height": "50%", "minWidth": "auto", "gap": "$spacing.md", "backgroundColor": "#ff0000", "color": "$color.primary", "borderRadius": "$radius.lg", "boxShadow": "$shadow.sm", "opacity": "$opacity.secondary", "aspectRatio": "16/9", "gridTemplateAreas": ["a b"], "fontWeight": 600})),
            vec![]
        );
        assert_eq!(
            v(json!({"width": "12em"})),
            vec![StyleIssue { path: "style.width".into(), message: "width: expected px, \"N%\", \"auto\" or a numeric token".into() }]
        );
        assert!(v(json!({"color": "$spacing.md"}))[0].message.contains("#hex or $color"));
        assert_eq!(v(json!({"gap": "$spacing.huge"})).len(), 1);
        assert!(v(json!({"display": "inline"}))[0].message.contains("expected one of"));
        assert_eq!(v(json!({"zIndex": 2})), vec![StyleIssue { path: "style.zIndex".into(), message: "not in the Box style whitelist".into() }]);
    }

    #[test]
    fn box_style_whitelist_conditions_surface_media_and_pressed_one_level_deep_direction_root_only() {
        assert_eq!(v(json!({"@media (min-width: 600px)": {"gap": 8}, ":pressed": {"opacity": 0.6}})), vec![]);
        assert_eq!(v(json!({"@media (min-width: 600px)": {":pressed": {"opacity": 0.6}}}))[0].message, "conditions do not nest");
        assert_eq!(v(json!({":hover": {"opacity": 1}}))[0].message, "not in the Box style whitelist");
        assert_eq!(validate_style(&json!({"direction": "rtl"}), "style", Some(false))[0].message, "allowed on the root only");
        assert_eq!(validate_style(&json!({"direction": "rtl"}), "style", Some(true)), vec![]);
    }

    #[test]
    fn the_hand_written_patterns() {
        assert!(is_length("-12.5px") && is_length("50%") && !is_length("12") && !is_length("1.px") && !is_length("px"));
        assert!(is_hex_color("#fff") && is_hex_color("#ffff") && is_hex_color("#ff00ff80") && !is_hex_color("#fffff") && !is_hex_color("fff"));
        assert!(is_ratio("16/9") && is_ratio("1.5/1") && !is_ratio("16/") && !is_ratio("-1/2"));
        assert!(is_media_key("@media (min-width: 600px)") && is_media_key("@media (min-width: 37.5px)") && !is_media_key("@media (min-width: 600)"));
        assert!(!is_condition_key(":hover") && is_condition_key(":pressed"));
    }
}
