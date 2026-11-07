//! The A2UI basic catalog → core catalog map, applied by the reducer. The
//! declarative part is `catalog/basic-map.json` (embedded); the NAMED
//! transforms listed under its `transforms` are implemented here, one function
//! per name. Mirrors `src/basic-map.ts`.

use std::sync::LazyLock;

use indexmap::IndexMap;
use serde_json::{json, Map, Value};

use crate::generated::catalog as g;
use crate::types::{FlatComponent, Props, Template};

/// One `props` entry of a component rule.
#[derive(Debug, Clone, PartialEq)]
pub enum PropRule {
    /// `"basicProp"`: copy the basic prop when defined.
    Copy(String),
    /// `{const}`
    Const(Value),
    /// `{from, map?, default?}`; `default: Some(Null)` is an explicit `null`.
    From { from: Option<String>, map: Option<Map<String, Value>>, default: Option<Value> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComponentRule {
    pub to: String,
    pub props: IndexMap<String, PropRule>,
    /// `children` | `child`
    pub children: Option<String>,
    pub transform: Option<String>,
}

/// `catalog/basic-map.json`, parsed.
#[derive(Debug, Clone, PartialEq)]
pub struct BasicMap {
    pub from: String,
    pub to: String,
    pub placeholder_icon: String,
    pub components: IndexMap<String, ComponentRule>,
    pub transforms: IndexMap<String, String>,
    pub icons: IndexMap<String, Option<String>>,
    pub functions: IndexMap<String, String>,
}

fn str_field(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or_default().to_string()
}

fn parse_rule(rule: &Value) -> ComponentRule {
    let props = rule
        .get("props")
        .and_then(Value::as_object)
        .map(|props| {
            props
                .iter()
                .map(|(core, spec)| {
                    let parsed = match spec {
                        Value::String(s) => PropRule::Copy(s.clone()),
                        Value::Object(o) if o.contains_key("const") => PropRule::Const(o["const"].clone()),
                        Value::Object(o) => PropRule::From {
                            from: o.get("from").and_then(Value::as_str).map(str::to_string),
                            map: o.get("map").and_then(Value::as_object).cloned(),
                            default: o.get("default").cloned(),
                        },
                        _ => PropRule::From { from: None, map: None, default: None },
                    };
                    (core.clone(), parsed)
                })
                .collect()
        })
        .unwrap_or_default();
    ComponentRule {
        to: str_field(rule, "to"),
        props,
        children: rule.get("children").and_then(Value::as_str).map(str::to_string),
        transform: rule.get("transform").and_then(Value::as_str).map(str::to_string),
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawBasicMap {
    from: String,
    to: String,
    placeholder_icon: String,
    components: IndexMap<String, Value>,
    #[serde(default)]
    transforms: IndexMap<String, String>,
    #[serde(default)]
    icons: IndexMap<String, Option<String>>,
    #[serde(default)]
    functions: IndexMap<String, String>,
}

/// `catalog/basic-map.json`, parsed once (top-level tables keep source order).
pub static BASIC_MAP: LazyLock<BasicMap> = LazyLock::new(|| {
    let raw: RawBasicMap = serde_json::from_str(g::BASIC_MAP_JSON).expect("basic-map.json");
    BasicMap {
        from: raw.from,
        to: raw.to,
        placeholder_icon: raw.placeholder_icon,
        components: raw.components.iter().map(|(k, v)| (k.clone(), parse_rule(v))).collect(),
        transforms: raw.transforms,
        icons: raw.icons,
        functions: raw.functions,
    }
});

/// The transforms this module implements (every name `basic-map.json` lists).
pub const BASIC_TRANSFORM_NAMES: &[&str] =
    &["textMarkdown", "imageVariant", "iconName", "tabs", "modal", "buttonChild", "textField", "fieldName", "choicePicker", "sliderSteps"];

/// What a basic component becomes before the reducer nests its children.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MappedComponent {
    pub component: String,
    pub props: Props,
    pub children_ids: Vec<String>,
    pub template: Option<Template>,
    pub slots: IndexMap<String, String>,
    pub on: Option<IndexMap<String, Value>>,
    pub style: Option<Props>,
    /// VAPP-289: what the 1.0 map refuses (an h1 Text, a missing TextField
    /// value) as `(prop, message)`: reducer issues VALIDATION_FAILED.
    pub issues: Vec<(String, String)>,
}

/// A basic Icon name → an icons.json name (the placeholder when unmapped).
pub fn map_basic_icon(name: Option<&Value>) -> String {
    let map = &*BASIC_MAP;
    match name {
        Some(Value::String(n)) => map.icons.get(n).cloned().flatten().unwrap_or_else(|| map.placeholder_icon.clone()),
        _ => map.placeholder_icon.clone(),
    }
}

/// Round 9 §9.1: a basic Icon `name` → the core icon (a value or a `map` call).
pub fn map_basic_icon_value(name: Option<&Value>) -> Value {
    let map = &*BASIC_MAP;
    if let Some(o) = name.and_then(Value::as_object) {
        if let (1, Some(Value::String(path))) = (o.len(), o.get("@path")) {
            let cases: Map<String, Value> =
                map.icons.iter().map(|(k, v)| (k.clone(), Value::String(v.clone().unwrap_or_else(|| map.placeholder_icon.clone())))).collect();
            return json!({"@call": "map", "args": {"value": {"@path": path}, "cases": cases, "default": map.placeholder_icon}});
        }
    }
    Value::String(map_basic_icon(name))
}

/// JS `String(x)` for an id-ish value.
fn id_string(v: &Value) -> String {
    crate::json::to_js_string(v)
}

/// `flat[key]` over the A2UI wire form (named fields and the rest).
struct FlatView {
    obj: Map<String, Value>,
}

impl FlatView {
    fn new(flat: &FlatComponent) -> FlatView {
        match serde_json::to_value(flat) {
            Ok(Value::Object(obj)) => FlatView { obj },
            _ => FlatView { obj: Map::new() },
        }
    }
    fn get(&self, key: &str) -> Option<&Value> {
        self.obj.get(key)
    }
    fn str(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(Value::as_str)
    }
}

fn set(props: &mut Props, key: &str, value: Option<Value>) {
    match value {
        Some(v) => {
            props.insert(key.to_string(), v);
        }
        None => {
            props.remove(key);
        }
    }
}

fn text_markdown(out: &mut MappedComponent, flat: &FlatView) {
    let variant = flat.get("variant");
    out.props.insert("profile".into(), json!("inline"));
    out.props.insert("variant".into(), json!(if variant.and_then(Value::as_str) == Some("caption") { "caption" } else { "body" }));
    if let Some(v) = variant {
        if !matches!(v.as_str(), Some("caption" | "body")) {
            let shown = v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string());
            out.issues.push(("variant".into(), format!("variant: {shown} is not caption|body")));
        }
    }
}

fn is_literal_number(v: &Value) -> bool {
    v.as_f64().is_some_and(f64::is_finite)
}

fn slider_steps(out: &mut MappedComponent, flat: &FlatView) {
    let Some(steps) = flat.get("steps") else { return };
    let max = flat.get("max");
    let zero = json!(0);
    let min = flat.get("min").filter(|m| !m.is_null()).unwrap_or(&zero);
    let literal = max.is_some_and(is_literal_number) && is_literal_number(min) && is_literal_number(steps) && steps.as_f64() != Some(0.0);
    if literal {
        let (hi, lo, n) = (max.and_then(Value::as_f64).unwrap_or(0.0), min.as_f64().unwrap_or(0.0), steps.as_f64().unwrap_or(1.0));
        out.props.insert("step".into(), crate::json::number(((hi - lo) / n * 1e9 + 0.5).floor() / 1e9));
    } else {
        let mut sub = Map::new();
        if let Some(max) = max {
            sub.insert("a".into(), max.clone());
        }
        sub.insert("b".into(), min.clone());
        out.props.insert("step".into(), json!({"@call": "div", "args": {"a": {"@call": "sub", "args": sub}, "b": steps}}));
    }
}

fn image_variant(out: &mut MappedComponent, flat: &FlatView) {
    let variant = match flat.get("variant") {
        None | Some(Value::Null) => "mediumFeature".to_string(),
        Some(v) => id_string(v),
    };
    if variant == "avatar" {
        out.component = "Avatar".into();
        let mut props = Props::new();
        set(&mut props, "src", out.props.get("src").cloned());
        let name = match flat.get("description") {
            None | Some(Value::Null) => json!(""),
            Some(v) => v.clone(),
        };
        props.insert("name".into(), name);
        out.props = props;
        return;
    }
    let style = match variant.as_str() {
        "icon" => json!({"width": 24, "height": 24}),
        "smallFeature" => json!({"width": 120, "height": 120}),
        "header" => json!({"width": "100%", "aspectRatio": 3}),
        _ => json!({"width": "100%", "aspectRatio": 1.7777778}),
    };
    let mut merged = out.style.take().unwrap_or_default();
    if let Value::Object(style) = style {
        merged.extend(style);
    }
    out.style = Some(merged);
}

fn icon_name(out: &mut MappedComponent, flat: &FlatView) {
    out.props.insert("name".into(), map_basic_icon_value(flat.get("name")));
}

fn tabs(out: &mut MappedComponent, flat: &FlatView) {
    let tabs: Vec<Value> = flat.get("tabs").and_then(Value::as_array).cloned().unwrap_or_default();
    let field = |tab: &Value, k: &str| tab.as_object().and_then(|o| o.get(k)).cloned();
    let headers: Vec<Value> = tabs
        .iter()
        .map(|tab| {
            let mut header = Map::new();
            if let Some(title) = field(tab, "title") {
                header.insert("label".into(), title);
            }
            if let Some(child) = field(tab, "child") {
                header.insert("value".into(), child);
            }
            Value::Object(header)
        })
        .collect();
    out.props.insert("tabs".into(), Value::Array(headers));
    out.children_ids = tabs.iter().map(|tab| field(tab, "child").map(|c| id_string(&c)).unwrap_or_else(|| "undefined".into())).collect();
    if let Some(first) = tabs.first() {
        set(&mut out.props, "value", field(first, "child"));
    }
}

fn modal(out: &mut MappedComponent, flat: &FlatView) {
    if let Some(trigger) = flat.str("trigger") {
        out.slots.insert("trigger".into(), trigger.to_string());
    }
    if let Some(content) = flat.str("content") {
        out.children_ids = vec![content.to_string()];
    }
    out.props.insert("open".into(), json!(false));
}

fn button_child(out: &mut MappedComponent, flat: &FlatView, lookup: &dyn Fn(&str) -> Option<FlatComponent>, catalog_of: &dyn Fn(&FlatComponent) -> String) {
    let child_id = flat.str("child").map(str::to_string);
    let child = child_id.as_deref().and_then(lookup);
    // Round 9 §5.5: only a BASIC Text/Icon child is consumed.
    let basic = child.as_ref().is_some_and(|c| crate::functions::is_basic_catalog(&catalog_of(c)));
    match &child {
        Some(c) if basic && c.component == "Text" => set(&mut out.props, "label", c.rest.get("text").cloned()),
        Some(c) if basic && c.component == "Icon" => {
            out.props.insert("icon".into(), map_basic_icon_value(c.rest.get("name")));
        }
        _ => {
            if let Some(id) = &child_id {
                out.children_ids = vec![id.clone()];
            }
        }
    }
    if out.props.contains_key("icon") && !out.props.contains_key("label") {
        out.props.insert("size".into(), json!("icon"));
    }
    if let Some(action) = flat.get("action") {
        let mut on = IndexMap::new();
        on.insert("press".to_string(), action.clone());
        out.on = Some(on);
    }
}

fn flat_id(flat: &FlatView) -> Value {
    flat.get("id").cloned().unwrap_or(Value::Null)
}

fn text_field(out: &mut MappedComponent, flat: &FlatView) {
    out.props.insert("name".into(), flat_id(flat));
    let variant = match flat.get("variant") {
        None | Some(Value::Null) => "shortText".to_string(),
        Some(v) => id_string(v),
    };
    if variant == "longText" {
        out.component = "Textarea".into();
    } else {
        let ty = match variant.as_str() {
            "number" => "number",
            "obscured" => "password",
            _ => "text",
        };
        out.props.insert("type".into(), json!(ty));
    }
    if flat.get("value").is_none() {
        out.props.insert("value".into(), json!(""));
        out.issues.push(("value".into(), "value: required".into()));
    }
}

fn field_name(out: &mut MappedComponent, flat: &FlatView) {
    out.props.insert("name".into(), flat_id(flat));
}

/// `Array.prototype.join` element: null → "".
fn join_part(v: &Value) -> String {
    if v.is_null() {
        String::new()
    } else {
        crate::json::to_js_string(v)
    }
}

fn choice_picker(out: &mut MappedComponent, flat: &FlatView) {
    out.props.insert("name".into(), flat_id(flat));
    let multiple = flat.str("variant") == Some("multipleSelection");
    let joined = match flat.get("value") {
        Some(Value::Array(items)) => Some(if multiple {
            json!(items.iter().map(join_part).collect::<Vec<_>>().join(","))
        } else {
            match items.first() {
                None | Some(Value::Null) => json!(""),
                Some(v) => v.clone(),
            }
        }),
        other => other.cloned(),
    };
    if let Some(joined) = joined {
        out.props.insert("value".into(), joined);
    }
    if flat.get("filterable") == Some(&Value::Bool(true)) {
        out.component = "Select".into();
        out.props.insert("searchable".into(), json!(true));
        out.props.insert("multiple".into(), json!(multiple));
    } else if flat.str("displayStyle") == Some("chips") {
        out.component = "Segmented".into();
        let mut props = Props::new();
        set(&mut props, "items", out.props.get("options").cloned());
        props.insert("type".into(), json!(if multiple { "multiple" } else { "single" }));
        props.insert("variant".into(), json!("outline"));
        set(&mut props, "value", out.props.get("value").cloned());
        out.props = props;
    } else if multiple {
        out.component = "Select".into();
        out.props.insert("multiple".into(), json!(true));
    }
}

fn apply_transform(name: &str, out: &mut MappedComponent, flat: &FlatView, lookup: &dyn Fn(&str) -> Option<FlatComponent>, catalog_of: &dyn Fn(&FlatComponent) -> String) {
    match name {
        "textMarkdown" => text_markdown(out, flat),
        "sliderSteps" => slider_steps(out, flat),
        "imageVariant" => image_variant(out, flat),
        "iconName" => icon_name(out, flat),
        "tabs" => tabs(out, flat),
        "modal" => modal(out, flat),
        "buttonChild" => button_child(out, flat, lookup, catalog_of),
        "textField" => text_field(out, flat),
        "fieldName" => field_name(out, flat),
        "choicePicker" => choice_picker(out, flat),
        other => panic!("basic-map.json names an unimplemented transform: {other}"),
    }
}

/// Map one basic component; `None` when the table has no row (→ Unknown).
///
/// `catalog_of` = a component's resolved catalog (round 9 §5.5).
pub fn map_basic_component(flat: &FlatComponent, lookup: &dyn Fn(&str) -> Option<FlatComponent>, catalog_of: &dyn Fn(&FlatComponent) -> String) -> Option<MappedComponent> {
    let rule = BASIC_MAP.components.get(&flat.component)?;
    let view = FlatView::new(flat);
    let mut props = Props::new();
    for (core_prop, spec) in &rule.props {
        match spec {
            PropRule::Copy(key) => {
                if let Some(v) = view.get(key) {
                    props.insert(core_prop.clone(), v.clone());
                }
            }
            PropRule::Const(v) => {
                props.insert(core_prop.clone(), v.clone());
            }
            PropRule::From { from, map, default } => {
                let mut value = from.as_deref().and_then(|f| view.get(f)).cloned();
                if let (Some(map), Some(Value::String(s))) = (map, &value) {
                    if let Some(mapped) = map.get(s) {
                        value = Some(mapped.clone());
                    }
                }
                if value.is_none() {
                    value = default.clone();
                }
                if let Some(v) = value {
                    props.insert(core_prop.clone(), v);
                }
            }
        }
    }
    let mut out = MappedComponent { component: rule.to.clone(), props, ..Default::default() };
    match rule.children.as_deref() {
        Some("children") => match &flat.children {
            Some(crate::types::FlatChildren::Ids(ids)) => out.children_ids = ids.clone(),
            Some(t @ crate::types::FlatChildren::Template { .. }) => {
                out.template = t.template()
            }
            None => {}
        },
        Some("child") => {
            if let Some(child) = view.str("child") {
                out.children_ids = vec![child.to_string()];
            }
        }
        _ => {}
    }
    if let Some(transform) = &rule.transform {
        apply_transform(transform, &mut out, &view, lookup, catalog_of);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_transform_is_implemented() {
        let listed: Vec<&str> = BASIC_MAP.transforms.keys().map(String::as_str).collect();
        assert_eq!(listed, BASIC_TRANSFORM_NAMES);
        for rule in BASIC_MAP.components.values() {
            if let Some(t) = &rule.transform {
                assert!(BASIC_TRANSFORM_NAMES.contains(&t.as_str()), "{t}");
            }
        }
    }

    #[test]
    fn icons_map_or_fall_back_to_the_placeholder() {
        assert_eq!(map_basic_icon(Some(&json!("add"))), "ui-add");
        assert_eq!(map_basic_icon(Some(&json!("nope"))), BASIC_MAP.placeholder_icon);
        assert_eq!(map_basic_icon(Some(&json!({"@path": "/x"}))), BASIC_MAP.placeholder_icon);
        assert_eq!(BASIC_MAP.placeholder_icon, g::BASIC_PLACEHOLDER_ICON);
    }
}
