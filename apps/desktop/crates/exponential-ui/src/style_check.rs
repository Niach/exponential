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

pub use crate::conditions::{is_condition_key, is_media_key};

/// `-?\d+(\.\d+)?`
fn signed_decimal(s: &str) -> bool {
    decimal(s.strip_prefix('-').unwrap_or(s))
}

/// One transform function: `translate(Xpx, Ypx)`, `scale(N)`, `rotate(Ndeg)`.
fn transform_fn(s: &str) -> bool {
    if let Some(args) = s.strip_prefix("translate(").and_then(|r| r.strip_suffix(')')) {
        return args.split_once(", ").is_some_and(|(x, y)| x.strip_suffix("px").is_some_and(signed_decimal) && y.strip_suffix("px").is_some_and(signed_decimal));
    }
    if let Some(n) = s.strip_prefix("scale(").and_then(|r| r.strip_suffix(')')) {
        return signed_decimal(n);
    }
    if let Some(n) = s.strip_prefix("rotate(").and_then(|r| r.strip_suffix(')')) {
        return n.strip_suffix("deg").is_some_and(signed_decimal);
    }
    false
}

/// `STYLE_TRANSFORM_PATTERN`: one or more transform functions separated by
/// ONE space, no leading or trailing space.
pub fn is_transform(s: &str) -> bool {
    if s.is_empty() || s.starts_with(' ') || s.ends_with(' ') {
        return false;
    }
    // A `translate(Xpx, Ypx)` holds a space itself: split on ") " instead.
    let mut rest = s;
    loop {
        let Some(close) = rest.find(')') else { return false };
        let (piece, after) = rest.split_at(close + 1);
        if !transform_fn(piece) {
            return false;
        }
        if after.is_empty() {
            return true;
        }
        let Some(next) = after.strip_prefix(' ') else { return false };
        if next.is_empty() || next.starts_with(' ') {
            return false;
        }
        rest = next;
    }
}

fn gradient_error(key: &str, value: &Value) -> Option<String> {
    let bad = || Some(format!("{key}: expected {{angle, stops: [{{color, offset 0..1}}, …]}} with two or more stops"));
    let Some(g) = value.as_object() else { return bad() };
    if !g.get("angle").is_some_and(finite_number) {
        return bad();
    }
    let Some(stops) = g.get("stops").and_then(Value::as_array).filter(|s| s.len() >= 2) else { return bad() };
    for stop in stops {
        let Some(stop) = stop.as_object() else { return bad() };
        // Round 4: a node colour is a `$color.*` token, never a literal.
        let color_ok = stop.get("color").is_some_and(|c| token_ok(c, &["color"]));
        if !color_ok {
            return bad();
        }
        if !stop.get("offset").and_then(Value::as_f64).is_some_and(|o| (0.0..=1.0).contains(&o)) {
            return bad();
        }
    }
    if g.keys().any(|k| k != "angle" && k != "stops") {
        return bad();
    }
    None
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
        // Round 4 (VAPP-103): a node colour is a `$color.*` TOKEN, never a
        // literal, so the theme's light and dark modes both apply (a theme's
        // recipes keep literal hex: theme.rs checks those).
        Some("color") => {
            if token_ok(value, &["color"]) {
                None
            } else {
                Some(format!("{key}: expected $color.<name>"))
            }
        }
        Some("gradient") => gradient_error(key, value),
        Some("transform") => {
            if s.is_some_and(is_transform) {
                None
            } else {
                Some(format!("{key}: expected translate(Xpx, Ypx), scale(N) and/or rotate(Ndeg)"))
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
            if let Some(name) = key.find("$breakpoint.").map(|i| &key[i + "$breakpoint.".len()..]) {
                let name: String = name.chars().take_while(char::is_ascii_alphanumeric).collect();
                let known = crate::macros::BREAKPOINTS.contains(&name);
                if !known {
                    issues.push(StyleIssue { path: here.clone(), message: format!("unknown breakpoint $breakpoint.{name}; known: {}", crate::macros::BREAKPOINTS.join("|")) });
                }
            }
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
            let message = if key.starts_with("@media") || key.starts_with(':') { "not a supported condition" } else { "not in the Box style whitelist" };
            issues.push(StyleIssue { path: here, message: message.into() });
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
        for key in ["zIndex", "order", "gridAutoFlow", "justifyItems", "borderTopColor"] {
            assert!(!STYLE_SPECS.contains_key(key), "{key}");
        }
        for key in ["transition", "transitionEasing", "transform", "backgroundGradient", "borderTopWidth", "borderTopLeftRadius", "overflowX", "insetBlockStart", "visibility", "pointerEvents", "userSelect", "cursor", "letterSpacing", "textDecoration", "textTransform", "fontStyle", "borderStyle"] {
            assert!(STYLE_SPECS.contains_key(key), "{key}");
        }
        assert_eq!(STYLE_SPECS["borderWidth"].layout_effect, Some(true));
        let keys: Vec<&str> = STYLE_SPECS.keys().map(String::as_str).collect();
        assert_eq!(keys, g::STYLE_KEYS);
        assert!(g::STYLE_MEDIA_PATTERN.starts_with(r"^@media \((min-width|max-width|min-height|max-height): "));
        assert_eq!(g::STYLE_LAYOUT_EFFECT_KEYS, ["borderWidth", "borderTopWidth", "borderRightWidth", "borderBottomWidth", "borderLeftWidth"]);
    }

    #[test]
    fn box_style_whitelist_values_px_percent_auto_tokens_colours() {
        assert_eq!(
            v(json!({"width": 12, "height": "50%", "minWidth": "auto", "gap": "$spacing.md", "backgroundColor": "$color.card", "color": "$color.primary", "borderRadius": "$radius.lg", "boxShadow": "$shadow.sm", "opacity": "$opacity.secondary", "aspectRatio": "16/9", "gridTemplateAreas": ["a b"], "fontWeight": 600})),
            vec![]
        );
        assert_eq!(
            v(json!({"width": "12em"})),
            vec![StyleIssue { path: "style.width".into(), message: "width: expected px, \"N%\", \"auto\" or a numeric token".into() }]
        );
        assert_eq!(v(json!({"color": "$spacing.md"}))[0].message, "color: expected $color.<name>");
        assert_eq!(v(json!({"gap": "$spacing.huge"})).len(), 1);
        assert!(v(json!({"display": "inline"}))[0].message.contains("expected one of"));
        assert_eq!(v(json!({"zIndex": 2})), vec![StyleIssue { path: "style.zIndex".into(), message: "not in the Box style whitelist".into() }]);
    }

    #[test]
    fn box_style_whitelist_conditions_surface_media_and_pressed_one_level_deep_direction_on_any_node() {
        assert_eq!(v(json!({"@media (min-width: 600px)": {"gap": 8}, ":pressed": {"opacity": 0.6}})), vec![]);
        assert_eq!(v(json!({"@media (min-width: 600px)": {":pressed": {"opacity": 0.6}}}))[0].message, "conditions do not nest");
        assert_eq!(v(json!({":hover": {"opacity": 1}, ":focus-visible": {"borderColor": "$color.ring"}})), vec![]);
        assert_eq!(v(json!({":focus": {"opacity": 1}}))[0].message, "not a supported condition");
        assert_eq!(v(json!({"@media (min-width: 600)": {"gap": 1}}))[0].message, "not a supported condition");
        assert_eq!(v(json!({"@media (min-width: $breakpoint.huge)": {"gap": 1}}))[0].message, "unknown breakpoint $breakpoint.huge; known: sm|md|lg|xl");
        assert_eq!(v(json!({"@media (max-width: $breakpoint.md)": {"display": "none"}, "@media (orientation: portrait)": {"gap": 2}, "@media (hover: hover)": {"cursor": "pointer"}})), vec![]);
        // Round 2: `direction` is valid on any node (style.json drops `rootOnly`).
        assert_eq!(validate_style(&json!({"direction": "rtl"}), "style", Some(false)), vec![]);
        assert_eq!(validate_style(&json!({"direction": "rtl"}), "style", Some(true)), vec![]);
    }

    #[test]
    fn the_hand_written_patterns() {
        assert!(is_length("-12.5px") && is_length("50%") && !is_length("12") && !is_length("1.px") && !is_length("px"));
        assert!(is_hex_color("#fff") && is_hex_color("#ffff") && is_hex_color("#ff00ff80") && !is_hex_color("#fffff") && !is_hex_color("fff"));
        assert!(is_ratio("16/9") && is_ratio("1.5/1") && !is_ratio("16/") && !is_ratio("-1/2"));
        assert!(is_media_key("@media (min-width: 600px)") && is_media_key("@media (min-width: 37.5px)") && !is_media_key("@media (min-width: 600)"));
        assert!(is_condition_key(":hover") && is_condition_key(":pressed") && !is_condition_key(":focus"));
        assert!(is_transform("translate(4px, -2px) scale(1.5) rotate(90deg)") && is_transform("scale(1)"));
        assert!(!is_transform("scale(1) ") && !is_transform("scale(1)  rotate(1deg)") && !is_transform("translate(4, 2)") && !is_transform("skew(1deg)") && !is_transform(""));
    }

    #[test]
    fn round_1_value_types() {
        assert_eq!(v(json!({"transition": "$motion.fast", "transitionEasing": "$ease.standard", "transform": "rotate(90deg)", "backgroundGradient": {"angle": 90, "stops": [{"color": "$color.card", "offset": 0}, {"color": "$color.primary", "offset": 1}]}, "borderTopWidth": 2, "borderStyle": "dashed", "borderTopLeftRadius": 8, "letterSpacing": 1, "visibility": "hidden", "fontWeight": 300})), vec![]);
        assert!(v(json!({"transition": 200}))[0].message.contains("$motion"));
        assert!(v(json!({"backgroundGradient": {"angle": 90, "stops": [{"color": "#fff", "offset": 0}]}}))[0].message.contains("two or more stops"));
        // Round 4: literal colours are refused in a node's style.
        assert_eq!(v(json!({"backgroundColor": "#ff0000"}))[0].message, "backgroundColor: expected $color.<name>");
        assert_eq!(v(json!({"backgroundGradient": {"angle": 90, "stops": [{"color": "$color.primary", "offset": 0}, {"color": "#00000000", "offset": 1}]}})).len(), 1);
        assert!(v(json!({"transform": "translate(1px,2px)"}))[0].message.contains("translate"));
        assert!(v(json!({"width": {"path": "/w"}}))[0].message.contains("expected px"), "authors cannot write dynamic style values");
    }

    #[test]
    fn round_2_style_keys_sticky_backdrop_blur_token_only_animation_and_its_duration_token() {
        assert_eq!(v(json!({"position": "sticky", "top": 0, "backdropBlur": "$blur.md", "animation": "pulse", "animationDuration": "$motion.slow"})), vec![]);
        assert_eq!(v(json!({"backdropBlur": 8})).len(), 1);
        assert_eq!(v(json!({"animation": "wobble"})).len(), 1);
        assert_eq!(v(json!({"animationDuration": 300})).len(), 1);
    }
}
