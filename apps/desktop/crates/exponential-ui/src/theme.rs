//! The theme loader and resolver (VAPP-92), a line-by-line port of
//! `src/theme.ts` + `src/theme-types.ts`. A theme is data, never code: one
//! JSON file of token VALUES per mode plus component RECIPES.
//! [`validate_theme`] turns a theme file into readable issues (never a
//! panic), [`load_theme`] flattens its `extends` chain into a
//! [`ResolvedTheme`] and [`resolve_recipe`] answers a painter's question for
//! one mode with concrete values.
//!
//! Precedence a painter applies ([`resolve_node_style`]): the native's own
//! recipe < the node's style < the macro part's recipe.
//!
//! Source objects are `serde_json::Value`s in SOURCE order (`preserve_order`),
//! so issues come out in the TS order.
//!
//! Round 1 (docs/round-1-contract.md §5): `breakpoint` (px), `ease` (cubic
//! bezier control points) and `density` (multipliers) token groups, a
//! high-contrast `contrast` overlay per mode, recipe keys for per-side
//! borders, text styling and motion, [`resolve_mode`] / [`apply_density`] /
//! [`apply_contrast`] for the surface settings, and [`resolve_condition_key`]
//! (a `$breakpoint` inside a media key resolves to px).

use std::fmt;
use std::sync::{Arc, LazyLock};

use indexmap::IndexMap;
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::{Map, Value};

use crate::catalog::{parse_token_ref, CatalogView, TOKEN_GROUPS};
use crate::json;
use crate::recipes::{RecipeIndex, RECIPE_KEYS, RECIPE_STATES};
use crate::types::{PartSpec, Props, UiNode};

pub use crate::generated::themes::THEME_SCHEMA_ID;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Light,
    Dark,
}

impl Mode {
    pub const ALL: [Mode; 2] = [Mode::Light, Mode::Dark];
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Light => "light",
            Mode::Dark => "dark",
        }
    }
    pub fn parse(s: &str) -> Option<Mode> {
        match s {
            "light" => Some(Mode::Light),
            "dark" => Some(Mode::Dark),
            _ => None,
        }
    }
}

/// The mode names, in resolution order.
pub const MODES: [&str; 2] = ["light", "dark"];

/// One shadow layer. Numbers serialize in the JS number model (`1`, not `1.0`).
#[derive(Clone, PartialEq, Debug, Deserialize)]
pub struct Shadow {
    pub x: f64,
    pub y: f64,
    pub blur: f64,
    pub spread: f64,
    pub color: String,
}

impl Serialize for Shadow {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut st = s.serialize_struct("Shadow", 5)?;
        st.serialize_field("x", &json::number(self.x))?;
        st.serialize_field("y", &json::number(self.y))?;
        st.serialize_field("blur", &json::number(self.blur))?;
        st.serialize_field("spread", &json::number(self.spread))?;
        st.serialize_field("color", &self.color)?;
        st.end()
    }
}

fn ser_nums<S: Serializer>(m: &IndexMap<String, f64>, s: S) -> Result<S::Ok, S::Error> {
    s.collect_map(m.iter().map(|(k, v)| (k, json::number(*v))))
}

fn ser_curves<S: Serializer>(m: &IndexMap<String, Vec<f64>>, s: S) -> Result<S::Ok, S::Error> {
    s.collect_map(m.iter().map(|(k, v)| (k, v.iter().map(|n| json::number(*n)).collect::<Vec<_>>())))
}

fn ser_opt_nums<S: Serializer>(v: &Option<Vec<f64>>, s: S) -> Result<S::Ok, S::Error> {
    match v {
        Some(list) => s.collect_seq(list.iter().map(|n| json::number(*n))),
        None => s.serialize_none(),
    }
}

/// The per-mode values: colours and shadows.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct ThemeMode {
    pub color: IndexMap<String, String>,
    pub shadow: IndexMap<String, Vec<Shadow>>,
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct Modes {
    pub light: ThemeMode,
    pub dark: ThemeMode,
}

impl Modes {
    pub fn get(&self, mode: Mode) -> &ThemeMode {
        match mode {
            Mode::Light => &self.light,
            Mode::Dark => &self.dark,
        }
    }
    pub fn get_mut(&mut self, mode: Mode) -> &mut ThemeMode {
        match mode {
            Mode::Light => &mut self.light,
            Mode::Dark => &mut self.dark,
        }
    }
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct TypeTokens {
    #[serde(serialize_with = "ser_nums")]
    pub size: IndexMap<String, f64>,
    #[serde(rename = "lineHeight", serialize_with = "ser_nums")]
    pub line_height: IndexMap<String, f64>,
    #[serde(serialize_with = "ser_nums")]
    pub weight: IndexMap<String, f64>,
    /// Family NAMES (`Inter`); `fonts` describes how the host registers them.
    pub family: IndexMap<String, String>,
}

impl TypeTokens {
    fn numbers(&self, sub: &str) -> Option<&IndexMap<String, f64>> {
        match sub {
            "size" => Some(&self.size),
            "lineHeight" => Some(&self.line_height),
            "weight" => Some(&self.weight),
            _ => None,
        }
    }
    fn numbers_mut(&mut self, sub: &str) -> Option<&mut IndexMap<String, f64>> {
        match sub {
            "size" => Some(&mut self.size),
            "lineHeight" => Some(&mut self.line_height),
            "weight" => Some(&mut self.weight),
            _ => None,
        }
    }
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct ThemeTokens {
    #[serde(serialize_with = "ser_nums")]
    pub spacing: IndexMap<String, f64>,
    #[serde(serialize_with = "ser_nums")]
    pub radius: IndexMap<String, f64>,
    pub r#type: TypeTokens,
    #[serde(serialize_with = "ser_nums")]
    pub control: IndexMap<String, f64>,
    #[serde(serialize_with = "ser_nums")]
    pub opacity: IndexMap<String, f64>,
    #[serde(serialize_with = "ser_nums")]
    pub border: IndexMap<String, f64>,
    /// Milliseconds.
    #[serde(serialize_with = "ser_nums")]
    pub motion: IndexMap<String, f64>,
    /// Surface widths in px the style conditions may name (`$breakpoint.md`).
    #[serde(default, serialize_with = "ser_nums")]
    pub breakpoint: IndexMap<String, f64>,
    /// `cubic-bezier(x1, y1, x2, y2)` control points.
    #[serde(default, serialize_with = "ser_curves")]
    pub ease: IndexMap<String, Vec<f64>>,
    /// Multipliers over `control` and `spacing` for the non-default densities.
    #[serde(default, serialize_with = "ser_nums")]
    pub density: IndexMap<String, f64>,
    /// Round 2: backdrop blur radii in px (`backdropBlur: $blur.md`).
    #[serde(default, serialize_with = "ser_nums")]
    pub blur: IndexMap<String, f64>,
}

/// The numeric groups directly under `tokens`, in overlay order.
const NUMBER_GROUPS: [&str; 9] = ["spacing", "radius", "control", "opacity", "border", "motion", "breakpoint", "density", "blur"];
/// Token groups whose values are cubic-bezier tuples.
const TUPLE_GROUPS: [&str; 1] = ["ease"];
const TYPE_SUBS: [&str; 4] = ["size", "lineHeight", "weight", "family"];

impl ThemeTokens {
    /// A numeric group by its flattened name (`spacing`, `type.size`, …).
    pub fn numbers(&self, group: &str) -> Option<&IndexMap<String, f64>> {
        match group {
            "spacing" => Some(&self.spacing),
            "radius" => Some(&self.radius),
            "control" => Some(&self.control),
            "opacity" => Some(&self.opacity),
            "border" => Some(&self.border),
            "motion" => Some(&self.motion),
            "breakpoint" => Some(&self.breakpoint),
            "density" => Some(&self.density),
            "blur" => Some(&self.blur),
            g => g.strip_prefix("type.").and_then(|sub| self.r#type.numbers(sub)),
        }
    }
    fn numbers_mut(&mut self, group: &str) -> Option<&mut IndexMap<String, f64>> {
        match group {
            "spacing" => Some(&mut self.spacing),
            "radius" => Some(&mut self.radius),
            "control" => Some(&mut self.control),
            "opacity" => Some(&mut self.opacity),
            "border" => Some(&mut self.border),
            "motion" => Some(&mut self.motion),
            "breakpoint" => Some(&mut self.breakpoint),
            "density" => Some(&mut self.density),
            "blur" => Some(&mut self.blur),
            g => g.strip_prefix("type.").and_then(|sub| self.r#type.numbers_mut(sub)),
        }
    }
}

/// How a host registers a family the theme names.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct FontSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none", serialize_with = "ser_opt_nums")]
    pub weights: Option<Vec<f64>>,
    /// `system` | `host`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// One rule: applies when every `when` entry matches; rules merge in order.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct RecipeRule {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<Map<String, Value>>,
    #[serde(default)]
    pub style: Map<String, Value>,
}

/// Component → part → rules.
pub type ThemeRecipes = IndexMap<String, IndexMap<String, Vec<RecipeRule>>>;

/// What a painter receives: complete, flat, `extends` gone.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct ResolvedTheme {
    pub id: String,
    pub name: String,
    /// The chain this theme was built from, root first.
    pub chain: Vec<String>,
    pub modes: Modes,
    /// The high-contrast overlays per mode (partial tables, may be empty).
    #[serde(default)]
    pub contrast: Modes,
    pub tokens: ThemeTokens,
    pub fonts: IndexMap<String, FontSpec>,
    pub recipes: ThemeRecipes,
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ThemeIssue {
    pub path: String,
    pub message: String,
}

fn issue(path: impl Into<String>, message: impl Into<String>) -> ThemeIssue {
    ThemeIssue { path: path.into(), message: message.into() }
}

/// Every issue of a theme that failed to load.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ThemeError {
    pub id: String,
    pub issues: Vec<ThemeIssue>,
}

impl fmt::Display for ThemeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Theme \"{}\" is invalid:", self.id)?;
        for i in &self.issues {
            write!(f, "\n  {}: {}", i.path, i.message)?;
        }
        Ok(())
    }
}

impl std::error::Error for ThemeError {}

/// What an `extends` may name: a theme source or an already resolved theme.
#[derive(Clone, Debug)]
pub enum ThemeRef {
    Source(Value),
    Resolved(Arc<ResolvedTheme>),
}

impl ThemeRef {
    /// The theme's id (`None` for a source without a string id).
    pub fn id(&self) -> Option<&str> {
        match self {
            ThemeRef::Source(v) => v.get("id").and_then(Value::as_str),
            ThemeRef::Resolved(t) => Some(&t.id),
        }
    }
}

static CORE_VIEW: LazyLock<Arc<CatalogView>> = LazyLock::new(CatalogView::core);

/// The registry an `extends` resolves against and the catalog view recipes
/// are validated against.
#[derive(Clone, Copy)]
pub struct ThemeOptions<'a> {
    pub themes: &'a [ThemeRef],
    pub view: &'a CatalogView,
}

impl<'a> ThemeOptions<'a> {
    /// The given registry over the core catalog.
    pub fn core(themes: &'a [ThemeRef]) -> ThemeOptions<'a> {
        ThemeOptions { themes, view: &CORE_VIEW }
    }
}

impl Default for ThemeOptions<'static> {
    fn default() -> Self {
        ThemeOptions { themes: &[], view: &CORE_VIEW }
    }
}

/// The question a painter asks.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct RecipeQuery {
    pub component: String,
    pub part: String,
    #[serde(default)]
    pub props: Props,
    #[serde(default)]
    pub states: Vec<String>,
}

impl RecipeQuery {
    pub fn new(component: impl Into<String>, part: impl Into<String>, props: Props, states: Vec<String>) -> RecipeQuery {
        RecipeQuery { component: component.into(), part: part.into(), props, states }
    }
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Token groups whose values are per-mode (under `modes`), not under `tokens`.
const MODE_GROUPS: [&str; 2] = ["color", "shadow"];
const STRING_GROUPS: [&str; 1] = ["type.family"];
/// Which token groups each recipe key accepts.
const KEY_GROUPS: &[(&str, &[&str])] = &[
    ("backgroundColor", &["color"]),
    ("color", &["color"]),
    ("borderColor", &["color"]),
    ("borderWidth", &["border", "control"]),
    ("borderTopWidth", &["border", "control"]),
    ("borderRightWidth", &["border", "control"]),
    ("borderBottomWidth", &["border", "control"]),
    ("borderLeftWidth", &["border", "control"]),
    ("borderRadius", &["radius", "spacing"]),
    ("borderStyle", &[]),
    ("padding", &["spacing"]),
    ("paddingHorizontal", &["spacing"]),
    ("paddingVertical", &["spacing"]),
    ("gap", &["spacing"]),
    ("width", &["control", "spacing", "border"]),
    ("height", &["control", "spacing", "border"]),
    ("minWidth", &["control", "spacing"]),
    ("minHeight", &["control", "spacing"]),
    ("fontSize", &["type.size"]),
    ("fontWeight", &["type.weight"]),
    ("lineHeight", &["type.lineHeight"]),
    ("fontFamily", &["type.family"]),
    ("letterSpacing", &[]),
    ("textDecoration", &[]),
    ("textTransform", &[]),
    ("fontStyle", &[]),
    ("boxShadow", &["shadow"]),
    ("opacity", &["opacity"]),
    ("transition", &["motion"]),
    ("transitionEasing", &["ease"]),
    ("transform", &[]),
    ("backdropBlur", &["blur"]),
    ("animation", &[]),
];
const COLOR_KEYS: [&str; 3] = ["backgroundColor", "color", "borderColor"];
const TOKEN_ONLY_KEYS: [&str; 5] = ["fontFamily", "boxShadow", "transition", "transitionEasing", "backdropBlur"];
const ENUM_KEYS: [&str; 5] = ["borderStyle", "textDecoration", "textTransform", "fontStyle", "animation"];
const THEME_KEYS: [&str; 10] = ["$schema", "$comment", "id", "name", "extends", "modes", "contrast", "tokens", "fonts", "recipes"];

/// The enum a style key takes (`borderStyle` → solid|dashed|dotted).
fn style_key_enum(key: &str) -> Vec<Value> {
    crate::style_check::STYLE_SPECS.get(key).and_then(|s| s.enum_.clone()).unwrap_or_default()
}

fn key_groups(key: &str) -> &'static [&'static str] {
    KEY_GROUPS.iter().find(|(k, _)| *k == key).map(|(_, g)| *g).unwrap_or(&[])
}

fn known<S: AsRef<str>>(names: &[S]) -> String {
    names.iter().map(AsRef::as_ref).collect::<Vec<_>>().join("|")
}

fn token_names(group: &str) -> Option<&'static Vec<String>> {
    TOKEN_GROUPS.get(group)
}

/// The token groups in SOURCE order. `catalog::TOKEN_GROUPS` parses the
/// nested `type` object through a sorted `Value`, so its `type.*` entries
/// come out alphabetically; the generated name list keeps tokens.json order.
fn token_groups_in_order() -> impl Iterator<Item = (&'static str, &'static Vec<String>)> {
    crate::generated::catalog::TOKEN_GROUPS.iter().filter_map(|g| TOKEN_GROUPS.get(*g).map(|names| (*g, names)))
}

fn names_include(names: &[String], name: &str) -> bool {
    names.iter().any(|n| n == name)
}

/// `#rrggbb` or `#rrggbbaa`, lowercase (`color.ts` `isThemeHex`).
pub fn is_theme_hex(value: &Value) -> bool {
    let Some(s) = value.as_str() else { return false };
    let Some(hex) = s.strip_prefix('#') else { return false };
    (hex.len() == 6 || hex.len() == 8) && hex.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn is_kebab_id(s: &str) -> bool {
    let mut bytes = s.bytes();
    matches!(bytes.next(), Some(b) if b.is_ascii_lowercase()) && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn is_finite_number(v: &Value) -> Option<f64> {
    v.as_f64().filter(|n| n.is_finite())
}

// ---------------------------------------------------------------------------
// Validation (one file, no chain knowledge beyond "does the parent exist")
// ---------------------------------------------------------------------------

fn check_number_map(value: &Value, path: &str, names: &[String], issues: &mut Vec<ThemeIssue>, range: Option<(f64, f64)>) {
    let Some(obj) = value.as_object() else {
        issues.push(issue(path, "expected an object of token values"));
        return;
    };
    for (name, v) in obj {
        let at = format!("{path}.{name}");
        if !names_include(names, name) {
            issues.push(issue(at, format!("unknown token; known: {}", known(names))));
        } else if let Some(n) = is_finite_number(v) {
            if let Some((lo, hi)) = range {
                if n < lo || n > hi {
                    issues.push(issue(at, format!("expected {}–{}", json::number_to_string(lo), json::number_to_string(hi))));
                }
            }
        } else {
            issues.push(issue(at, "expected a number"));
        }
    }
}

fn check_easing_map(value: &Value, path: &str, names: &[String], issues: &mut Vec<ThemeIssue>) {
    let Some(obj) = value.as_object() else {
        issues.push(issue(path, "expected an object of easing curves"));
        return;
    };
    for (name, v) in obj {
        let at = format!("{path}.{name}");
        if !names_include(names, name) {
            issues.push(issue(at, format!("unknown token; known: {}", known(names))));
        } else if !v.as_array().is_some_and(|a| a.len() == 4 && a.iter().all(|n| is_finite_number(n).is_some())) {
            issues.push(issue(at, "expected [x1, y1, x2, y2]"));
        }
    }
}

fn check_shadow(value: &Value, path: &str, issues: &mut Vec<ThemeIssue>) {
    let Some(layers) = value.as_array() else {
        issues.push(issue(path, "expected a list of shadow layers"));
        return;
    };
    for (i, layer) in layers.iter().enumerate() {
        let at = format!("{path}[{i}]");
        let Some(layer) = layer.as_object() else {
            issues.push(issue(at, "expected {x, y, blur, spread, color}"));
            continue;
        };
        for k in ["x", "y", "blur", "spread"] {
            if !layer.get(k).is_some_and(Value::is_number) {
                issues.push(issue(format!("{at}.{k}"), "expected a number"));
            }
        }
        if !layer.get("color").is_some_and(is_theme_hex) {
            issues.push(issue(format!("{at}.color"), "expected #rrggbb or #rrggbbaa"));
        }
    }
}

fn check_mode(value: &Value, path: &str, issues: &mut Vec<ThemeIssue>) {
    let Some(obj) = value.as_object() else {
        issues.push(issue(path, "expected {color, shadow}"));
        return;
    };
    for (group, entries) in obj {
        if !MODE_GROUPS.contains(&group.as_str()) {
            issues.push(issue(format!("{path}.{group}"), "unknown mode group; known: color|shadow"));
            continue;
        }
        let Some(entries) = entries.as_object() else {
            issues.push(issue(format!("{path}.{group}"), "expected an object of token values"));
            continue;
        };
        let names = token_names(group).map(Vec::as_slice).unwrap_or(&[]);
        for (name, v) in entries {
            let at = format!("{path}.{group}.{name}");
            if !names_include(names, name) {
                issues.push(issue(at, format!("unknown token; known: {}", known(names))));
            } else if group == "color" && !is_theme_hex(v) {
                issues.push(issue(at, "expected #rrggbb or #rrggbbaa (lowercase); the builder converts oklch/hsl on import"));
            } else if group == "shadow" {
                check_shadow(v, &at, issues);
            }
        }
    }
}

fn check_modes(value: &Value, path: &str, issues: &mut Vec<ThemeIssue>) {
    let Some(modes) = value.as_object() else {
        issues.push(issue(path, "expected {light, dark}"));
        return;
    };
    for (mode, v) in modes {
        if Mode::parse(mode).is_none() {
            issues.push(issue(format!("{path}.{mode}"), "unknown mode; known: light|dark"));
        } else {
            check_mode(v, &format!("{path}.{mode}"), issues);
        }
    }
}

fn check_tokens(value: &Value, path: &str, issues: &mut Vec<ThemeIssue>) {
    let Some(obj) = value.as_object() else {
        issues.push(issue(path, "expected the token groups"));
        return;
    };
    for (group, entries) in obj {
        let at = format!("{path}.{group}");
        if group == "type" {
            let Some(subs) = entries.as_object() else {
                issues.push(issue(at, "expected {size, lineHeight, weight, family}"));
                continue;
            };
            for (sub, map) in subs {
                let g = format!("type.{sub}");
                let Some(names) = token_names(&g) else {
                    issues.push(issue(format!("{at}.{sub}"), "unknown type group; known: size|lineHeight|weight|family"));
                    continue;
                };
                if STRING_GROUPS.contains(&g.as_str()) {
                    let Some(map) = map.as_object() else {
                        issues.push(issue(format!("{at}.{sub}"), "expected an object of family names"));
                        continue;
                    };
                    for (name, v) in map {
                        if !names_include(names, name) {
                            issues.push(issue(format!("{at}.{sub}.{name}"), format!("unknown token; known: {}", known(names))));
                        } else if v.as_str().is_none_or(str::is_empty) {
                            issues.push(issue(format!("{at}.{sub}.{name}"), "expected a font family name"));
                        }
                    }
                } else {
                    check_number_map(map, &format!("{at}.{sub}"), names, issues, None);
                }
            }
            continue;
        }
        if MODE_GROUPS.contains(&group.as_str()) {
            issues.push(issue(at, format!("{group} values live under modes.light / modes.dark")));
            continue;
        }
        let Some(names) = token_names(group) else {
            let groups: Vec<&str> = token_groups_in_order()
                .map(|(g, _)| g)
                .filter(|g| !g.starts_with("type.") && !MODE_GROUPS.contains(g))
                .collect();
            issues.push(issue(at, format!("unknown token group; known: {}|type", known(&groups))));
            continue;
        };
        if TUPLE_GROUPS.contains(&group.as_str()) {
            check_easing_map(entries, &at, names, issues);
            continue;
        }
        let range = match group.as_str() {
            "opacity" => Some((0.0, 1.0)),
            "density" => Some((0.5, 2.0)),
            _ => None,
        };
        check_number_map(entries, &at, names, issues, range);
    }
}

fn check_fonts(value: &Value, path: &str, issues: &mut Vec<ThemeIssue>) {
    let Some(obj) = value.as_object() else {
        issues.push(issue(path, "expected an object keyed by family name"));
        return;
    };
    for (family, spec) in obj {
        let at = format!("{path}.{family}");
        let Some(spec) = spec.as_object() else {
            issues.push(issue(at, "expected {fallback?, weights?, source?}"));
            continue;
        };
        for key in spec.keys() {
            if !["fallback", "weights", "source"].contains(&key.as_str()) {
                issues.push(issue(format!("{at}.{key}"), "unknown font key; known: fallback|weights|source"));
            }
        }
        if spec.get("fallback").is_some_and(|v| !v.is_string()) {
            issues.push(issue(format!("{at}.fallback"), "expected a string"));
        }
        if let Some(w) = spec.get("weights") {
            if !w.as_array().is_some_and(|a| a.iter().all(Value::is_number)) {
                issues.push(issue(format!("{at}.weights"), "expected a list of numbers"));
            }
        }
        if let Some(s) = spec.get("source") {
            if s.as_str() != Some("system") && s.as_str() != Some("host") {
                issues.push(issue(format!("{at}.source"), "expected system|host"));
            }
        }
    }
}

fn check_recipe_value(key: &str, value: &Value, path: &str, issues: &mut Vec<ThemeIssue>) {
    if key == "native" {
        if !value.is_boolean() {
            issues.push(issue(path, "expected true|false"));
        }
        return;
    }
    if ENUM_KEYS.contains(&key) {
        let allowed = style_key_enum(key);
        if !allowed.iter().any(|a| json::strict_eq(a, value)) {
            let names: Vec<String> = allowed.iter().map(json::to_js_string).collect();
            issues.push(issue(path, format!("expected {}", known(&names))));
        }
        return;
    }
    if key == "transform" {
        if !value.as_str().is_some_and(crate::style_check::is_transform) {
            issues.push(issue(path, "expected translate(Xpx, Ypx), scale(N) and/or rotate(Ndeg)"));
        }
        return;
    }
    let groups = key_groups(key);
    if let Some((group, name)) = parse_token_ref(value) {
        if !groups.contains(&group.as_str()) {
            if groups.is_empty() {
                issues.push(issue(path, "expected a number, not a token"));
            } else {
                issues.push(issue(path, format!("expected a ${} token, got ${group}", groups.join("|$"))));
            }
        } else if !token_names(&group).is_some_and(|names| names_include(names, &name)) {
            let names = token_names(&group).map(Vec::as_slice).unwrap_or(&[]);
            issues.push(issue(path, format!("unknown token {}; known: {}", value.as_str().unwrap_or(""), known(names))));
        }
        return;
    }
    if COLOR_KEYS.contains(&key) {
        if !is_theme_hex(value) {
            issues.push(issue(path, "expected #rrggbb, #rrggbbaa or $color.<name>"));
        }
        return;
    }
    if TOKEN_ONLY_KEYS.contains(&key) {
        issues.push(issue(path, format!("expected a ${}.<name> token", groups.first().copied().unwrap_or(""))));
        return;
    }
    let weights = style_key_enum("fontWeight");
    match is_finite_number(value) {
        None if groups.is_empty() => issues.push(issue(path, "expected a number")),
        None => issues.push(issue(path, format!("expected a number or a ${} token", groups.join("|$")))),
        Some(n) if key == "opacity" && !(0.0..=1.0).contains(&n) => issues.push(issue(path, "expected 0–1")),
        Some(n) if key == "fontWeight" && !weights.iter().any(|w| w.as_f64() == Some(n)) => {
            let names: Vec<String> = weights.iter().map(json::to_js_string).collect();
            issues.push(issue(path, format!("expected {} or $type.weight.<name>", known(&names))))
        }
        Some(_) => {}
    }
}

fn check_rule(rule: &Value, path: &str, spec: &PartSpec, issues: &mut Vec<ThemeIssue>) {
    let Some(rule) = rule.as_object() else {
        issues.push(issue(path, "expected {when?, style}"));
        return;
    };
    for key in rule.keys() {
        if key != "when" && key != "style" {
            issues.push(issue(format!("{path}.{key}"), "unknown rule key; known: when|style"));
        }
    }
    if let Some(when) = rule.get("when") {
        match when.as_object() {
            None => issues.push(issue(format!("{path}.when"), "expected an object of recipe props")),
            Some(when) => {
                for (k, v) in when {
                    let at = format!("{path}.when.{k}");
                    if k == "state" {
                        let states: Vec<&Value> = match v {
                            Value::Array(a) => a.iter().collect(),
                            other => vec![other],
                        };
                        for s in states {
                            if !s.as_str().is_some_and(|s| RECIPE_STATES.contains(&s)) {
                                issues.push(issue(at.clone(), format!("unknown state; known: {}", known(RECIPE_STATES))));
                            }
                        }
                    } else if !names_include(&spec.props, k) {
                        let mut names = vec!["state".to_string()];
                        names.extend(spec.props.iter().cloned());
                        issues.push(issue(at, format!("not a recipe prop of this part; known: {}", known(&names))));
                    } else {
                        let ok = |x: &Value| x.is_string() || x.is_number() || x.is_boolean();
                        if !(ok(v) || v.as_array().is_some_and(|a| a.iter().all(ok))) {
                            issues.push(issue(at, "expected a value or a list of values"));
                        }
                    }
                }
            }
        }
    }
    let Some(style) = rule.get("style").and_then(Value::as_object) else {
        issues.push(issue(format!("{path}.style"), "expected a style object"));
        return;
    };
    for (key, value) in style {
        let at = format!("{path}.style.{key}");
        if !RECIPE_KEYS.contains(&key.as_str()) {
            issues.push(issue(at, format!("not a recipe key; known: {}", known(RECIPE_KEYS))));
        } else {
            check_recipe_value(key, value, &at, issues);
        }
    }
}

fn check_recipes(value: &Value, path: &str, parts: &IndexMap<String, PartSpec>, issues: &mut Vec<ThemeIssue>) {
    let Some(obj) = value.as_object() else {
        issues.push(issue(path, "expected an object keyed by component"));
        return;
    };
    for (component, by_part) in obj {
        let at = format!("{path}.{component}");
        let Some(spec) = parts.get(component) else {
            issues.push(issue(at, "unknown component"));
            continue;
        };
        let Some(by_part) = by_part.as_object() else {
            issues.push(issue(at, format!("expected an object keyed by part; known: {}", known(&spec.parts))));
            continue;
        };
        for (part, rules) in by_part {
            let at_part = format!("{at}.{part}");
            if !names_include(&spec.parts, part) {
                issues.push(issue(at_part, format!("unknown part; known: {}", known(&spec.parts))));
                continue;
            }
            let Some(rules) = rules.as_array() else {
                issues.push(issue(at_part, "expected a list of rules [{when?, style}]"));
                continue;
            };
            for (i, rule) in rules.iter().enumerate() {
                check_rule(rule, &format!("{at_part}[{i}]"), spec, issues);
            }
        }
    }
}

/// The recipe index for the options' view (the cached core one when the
/// view is the core).
fn index_for(view: &CatalogView) -> Arc<RecipeIndex> {
    if std::ptr::eq(view, &**CORE_VIEW) {
        RecipeIndex::core()
    } else {
        Arc::new(RecipeIndex::new(view))
    }
}

/// Every problem in ONE theme file, each with a path and a readable
/// message. Never panics: a non-object is one issue.
pub fn validate_theme(source: &Value, options: &ThemeOptions) -> Vec<ThemeIssue> {
    let mut issues = Vec::new();
    let Some(src) = source.as_object() else {
        return vec![issue("theme", "expected a theme object {id, name, modes, tokens, recipes}")];
    };
    if !src.get("id").and_then(Value::as_str).is_some_and(is_kebab_id) {
        issues.push(issue("id", "expected a kebab-case id"));
    }
    if src.get("name").and_then(Value::as_str).is_none_or(str::is_empty) {
        issues.push(issue("name", "expected a name"));
    }
    for key in src.keys() {
        if !THEME_KEYS.contains(&key.as_str()) {
            issues.push(issue(key.clone(), "unknown theme key; known: id|name|extends|modes|contrast|tokens|fonts|recipes"));
        }
    }
    if let Some(ext) = src.get("extends") {
        let parents: Vec<&str> = options.themes.iter().map(|t| t.id().unwrap_or("")).collect();
        match ext.as_str() {
            None => issues.push(issue("extends", "expected a theme id")),
            Some(e) if !parents.contains(&e) => issues.push(issue("extends", format!("unknown theme \"{e}\"; known: {}", known(&parents)))),
            Some(e) if src.get("id").and_then(Value::as_str) == Some(e) => issues.push(issue("extends", "a theme cannot extend itself")),
            Some(_) => {}
        }
    }
    if let Some(modes) = src.get("modes") {
        check_modes(modes, "modes", &mut issues);
    }
    if let Some(contrast) = src.get("contrast") {
        check_modes(contrast, "contrast", &mut issues);
    }
    if let Some(tokens) = src.get("tokens") {
        check_tokens(tokens, "tokens", &mut issues);
    }
    if let Some(fonts) = src.get("fonts") {
        check_fonts(fonts, "fonts", &mut issues);
    }
    if let Some(recipes) = src.get("recipes") {
        check_recipes(recipes, "recipes", &index_for(options.view).parts, &mut issues);
    }
    issues
}

// ---------------------------------------------------------------------------
// Resolution: flatten the chain, check completeness
// ---------------------------------------------------------------------------

fn empty_theme(source: &Map<String, Value>) -> ResolvedTheme {
    ResolvedTheme {
        id: source.get("id").and_then(Value::as_str).unwrap_or_default().to_string(),
        name: source.get("name").and_then(Value::as_str).unwrap_or_default().to_string(),
        ..ResolvedTheme::default()
    }
}

fn overlay_mode_source(tm: &mut ThemeMode, m: Option<&Value>) {
    let Some(m) = m else { return };
    for (name, v) in m.get("color").and_then(Value::as_object).into_iter().flatten() {
        tm.color.insert(name.clone(), v.as_str().unwrap_or_default().to_string());
    }
    for (name, v) in m.get("shadow").and_then(Value::as_object).into_iter().flatten() {
        let layers: Vec<Shadow> = serde_json::from_value(v.clone()).unwrap_or_default();
        tm.shadow.insert(name.clone(), layers);
    }
}

fn overlay_mode(tm: &mut ThemeMode, from: &ThemeMode) {
    for (k, v) in &from.color {
        tm.color.insert(k.clone(), v.clone());
    }
    for (k, v) in &from.shadow {
        tm.shadow.insert(k.clone(), v.clone());
    }
}

/// Merge a VALIDATED source over the target (TS `overlay`).
fn overlay(target: &mut ResolvedTheme, source: &Value) {
    for mode in Mode::ALL {
        overlay_mode_source(target.modes.get_mut(mode), source.get("modes").and_then(|ms| ms.get(mode.as_str())));
        overlay_mode_source(target.contrast.get_mut(mode), source.get("contrast").and_then(|ms| ms.get(mode.as_str())));
    }
    if let Some(t) = source.get("tokens") {
        for group in NUMBER_GROUPS {
            let table = target.tokens.numbers_mut(group).expect("number group");
            for (name, v) in t.get(group).and_then(Value::as_object).into_iter().flatten() {
                table.insert(name.clone(), v.as_f64().unwrap_or_default());
            }
        }
        for (name, v) in t.get("ease").and_then(Value::as_object).into_iter().flatten() {
            let curve: Vec<f64> = v.as_array().map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
            target.tokens.ease.insert(name.clone(), curve);
        }
        for sub in TYPE_SUBS {
            let entries = t.get("type").and_then(|ty| ty.get(sub)).and_then(Value::as_object);
            for (name, v) in entries.into_iter().flatten() {
                if sub == "family" {
                    target.tokens.r#type.family.insert(name.clone(), v.as_str().unwrap_or_default().to_string());
                } else if let Some(table) = target.tokens.r#type.numbers_mut(sub) {
                    table.insert(name.clone(), v.as_f64().unwrap_or_default());
                }
            }
        }
    }
    for (family, spec) in source.get("fonts").and_then(Value::as_object).into_iter().flatten() {
        let spec: FontSpec = serde_json::from_value(spec.clone()).unwrap_or_default();
        target.fonts.insert(family.clone(), spec);
    }
    for (component, by_part) in source.get("recipes").and_then(Value::as_object).into_iter().flatten() {
        let parts = target.recipes.entry(component.clone()).or_default();
        for (part, rules) in by_part.as_object().into_iter().flatten() {
            let rules: Vec<RecipeRule> = serde_json::from_value(rules.clone()).unwrap_or_default();
            parts.entry(part.clone()).or_default().extend(rules);
        }
    }
}

fn overlay_resolved(target: &mut ResolvedTheme, parent: &ResolvedTheme) {
    for mode in Mode::ALL {
        overlay_mode(target.modes.get_mut(mode), parent.modes.get(mode));
        overlay_mode(target.contrast.get_mut(mode), parent.contrast.get(mode));
    }
    target.tokens = parent.tokens.clone();
    target.fonts = parent.fonts.clone();
    target.recipes = parent.recipes.clone();
}

/// The token names the merged theme still lacks, as issues.
fn completeness(theme: &ResolvedTheme) -> Vec<ThemeIssue> {
    let mut issues = Vec::new();
    for (group, names) in token_groups_in_order() {
        for name in names {
            if MODE_GROUPS.contains(&group) {
                for mode in Mode::ALL {
                    let m = theme.modes.get(mode);
                    let present = if group == "color" { m.color.contains_key(name) } else { m.shadow.contains_key(name) };
                    if !present {
                        issues.push(issue(format!("modes.{}.{group}.{name}", mode.as_str()), format!("missing value for ${group}.{name}")));
                    }
                }
                continue;
            }
            let present = if group == "type.family" {
                theme.tokens.r#type.family.contains_key(name)
            } else if group == "ease" {
                theme.tokens.ease.contains_key(name)
            } else {
                theme.tokens.numbers(group).is_some_and(|t| t.contains_key(name))
            };
            if !present {
                issues.push(issue(format!("tokens.{group}.{name}"), format!("missing value for ${group}.{name}")));
            }
        }
    }
    for family in theme.tokens.r#type.family.values() {
        if !theme.fonts.contains_key(family) {
            issues.push(issue(format!("fonts.{family}"), "family named by tokens.type.family has no fonts entry"));
        }
    }
    issues
}

enum Link<'a> {
    Source(&'a Value),
    Resolved(&'a ResolvedTheme),
}

/// The links a chain needs, root first; an issue when a link is missing.
fn chain_of<'a>(source: &'a Value, options: &ThemeOptions<'a>) -> Result<Vec<Link<'a>>, ThemeIssue> {
    let id_of = |v: &Value| v.get("id").and_then(Value::as_str).unwrap_or("").to_string();
    let mut chain = vec![Link::Source(source)];
    let mut seen = vec![id_of(source)];
    let mut current = source;
    while let Some(parent_id) = current.get("extends").and_then(Value::as_str).filter(|s| !s.is_empty()) {
        let Some(parent) = options.themes.iter().find(|t| t.id() == Some(parent_id)) else {
            return Err(issue("extends", format!("unknown theme \"{parent_id}\"")));
        };
        let pid = parent.id().unwrap_or("").to_string();
        if seen.contains(&pid) {
            seen.push(pid);
            return Err(issue("extends", format!("cycle: {}", seen.join(" → "))));
        }
        seen.push(pid);
        match parent {
            ThemeRef::Resolved(t) => {
                chain.insert(0, Link::Resolved(t));
                break;
            }
            ThemeRef::Source(v) => {
                chain.insert(0, Link::Source(v));
                current = v;
            }
        }
    }
    Ok(chain)
}

/// Validate + flatten, returning issues instead of failing loudly.
pub fn try_load_theme(source: &Value, options: &ThemeOptions) -> Result<ResolvedTheme, Vec<ThemeIssue>> {
    let issues = validate_theme(source, options);
    if !issues.is_empty() {
        return Err(issues);
    }
    let src = source.as_object().expect("validated");
    let chain = chain_of(source, options).map_err(|i| vec![i])?;
    let mut theme = empty_theme(src);
    let last = chain.len() - 1;
    for (i, link) in chain.iter().enumerate() {
        match link {
            Link::Resolved(parent) => {
                overlay_resolved(&mut theme, parent);
                theme.chain.extend(parent.chain.iter().cloned());
            }
            Link::Source(v) => {
                let id = v.get("id").and_then(Value::as_str).unwrap_or("").to_string();
                if i != last {
                    let parent_issues = validate_theme(v, options);
                    if !parent_issues.is_empty() {
                        return Err(parent_issues
                            .into_iter()
                            .map(|p| issue(format!("extends({id}).{}", p.path), p.message))
                            .collect());
                    }
                }
                overlay(&mut theme, v);
                theme.chain.push(id);
            }
        }
    }
    let missing = completeness(&theme);
    if !missing.is_empty() {
        return Err(missing);
    }
    Ok(theme)
}

/// A theme file → the flat theme a painter uses, or ONE error listing every issue.
pub fn load_theme(source: &Value, options: &ThemeOptions) -> Result<ResolvedTheme, ThemeError> {
    try_load_theme(source, options).map_err(|issues| ThemeError {
        id: source.get("id").and_then(Value::as_str).unwrap_or("?").to_string(),
        issues,
    })
}

// ---------------------------------------------------------------------------
// Resolving values for one mode
// ---------------------------------------------------------------------------

fn numbers_value(m: &IndexMap<String, f64>) -> Value {
    Value::Object(m.iter().map(|(k, v)| (k.clone(), json::number(*v))).collect())
}

/// `$color.primary` in `dark` → `"#e5e5e5"`; `$spacing.md` → `12`;
/// `$shadow.sm` → the layers; `$type.family.sans` → `"Inter"`. `None` when
/// not a token (or not one this theme has).
pub fn resolve_token(theme: &ResolvedTheme, reference: &Value, mode: Mode) -> Option<Value> {
    let (group, name) = parse_token_ref(reference)?;
    let m = theme.modes.get(mode);
    match group.as_str() {
        "color" => m.color.get(&name).map(|c| Value::String(c.clone())),
        "shadow" => m.shadow.get(&name).map(|l| serde_json::to_value(l).expect("shadow")),
        "type.family" => theme.tokens.r#type.family.get(&name).map(|f| Value::String(f.clone())),
        "ease" => theme.tokens.ease.get(&name).map(|c| Value::Array(c.iter().map(|n| json::number(*n)).collect())),
        // `$type.size` names a whole sub-table, like the TS lookup does.
        "type" => match name.as_str() {
            "family" => serde_json::to_value(&theme.tokens.r#type.family).ok(),
            sub => theme.tokens.r#type.numbers(sub).map(numbers_value),
        },
        g => theme.tokens.numbers(g).and_then(|t| t.get(&name)).map(|v| json::number(*v)),
    }
}

/// A media key with its `$breakpoint.<name>` replaced by the theme's px
/// (`resolveConditionKey`); any other key passes.
pub fn resolve_condition_key(theme: &ResolvedTheme, key: &str) -> String {
    let Some(condition) = crate::conditions::parse_media_condition(key) else { return key.to_string() };
    let Some(name) = crate::conditions::breakpoint_ref(&condition.value) else { return key.to_string() };
    match theme.tokens.breakpoint.get(name) {
        Some(px) => key.replace(&condition.value, &format!("{}px", json::number_to_string(*px))),
        None => key.to_string(),
    }
}

fn resolve_style_value(theme: &ResolvedTheme, value: &Value, mode: Mode) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.iter().map(|v| resolve_style_value(theme, v, mode)).collect()),
        Value::Object(nested) => Value::Object(resolve_style_values(theme, nested, mode)),
        Value::String(s) if s.starts_with('$') => resolve_token(theme, value, mode).unwrap_or_else(|| value.clone()),
        other => other.clone(),
    }
}

/// Every token reference in a style object replaced by its value for the
/// mode (nested condition blocks, gradients and arrays included, and the
/// `$breakpoint` tokens inside media keys); other values pass through.
pub fn resolve_style_values(theme: &ResolvedTheme, style: &Props, mode: Mode) -> Props {
    let mut out = Props::new();
    for (key, value) in style {
        out.insert(resolve_condition_key(theme, key), resolve_style_value(theme, value, mode));
    }
    out
}

// ---------------------------------------------------------------------------
// Surface settings: mode, density, contrast (round 1)
// ---------------------------------------------------------------------------

/// What a host asks for: a mode, or `system` = the platform's preference.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModeSetting {
    Light,
    Dark,
    #[default]
    System,
}

impl ModeSetting {
    pub fn parse(s: &str) -> Option<ModeSetting> {
        match s {
            "light" => Some(ModeSetting::Light),
            "dark" => Some(ModeSetting::Dark),
            "system" => Some(ModeSetting::System),
            _ => None,
        }
    }
}

/// `system` → what the platform prefers; a mode stays itself.
pub fn resolve_mode(setting: ModeSetting, system_prefers_dark: bool) -> Mode {
    match setting {
        ModeSetting::Light => Mode::Light,
        ModeSetting::Dark => Mode::Dark,
        ModeSetting::System => {
            if system_prefers_dark {
                Mode::Dark
            } else {
                Mode::Light
            }
        }
    }
}

/// The surface density; `default` = the tokens as written.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Density {
    Compact,
    #[default]
    Default,
    Comfortable,
}

impl Density {
    pub fn parse(s: &str) -> Option<Density> {
        match s {
            "compact" => Some(Density::Compact),
            "default" => Some(Density::Default),
            "comfortable" => Some(Density::Comfortable),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Density::Compact => "compact",
            Density::Default => "default",
            Density::Comfortable => "comfortable",
        }
    }
}

fn scaled(table: &IndexMap<String, f64>, factor: f64) -> IndexMap<String, f64> {
    table.iter().map(|(k, v)| (k.clone(), crate::expr::js_round(v * factor))).collect()
}

/// The theme a surface of this density runs with: `control` and `spacing`
/// scaled by the theme's `$density.<name>` multiplier and rounded to whole
/// px; `default` (or a theme without the multiplier) returns it unchanged.
/// Done once per surface, so every resolver and fixture stays
/// density-agnostic.
pub fn apply_density(theme: &ResolvedTheme, density: Density) -> ResolvedTheme {
    let Some(factor) = (density != Density::Default).then(|| theme.tokens.density.get(density.as_str()).copied()).flatten() else {
        return theme.clone();
    };
    let mut out = theme.clone();
    out.tokens.control = scaled(&theme.tokens.control, factor);
    out.tokens.spacing = scaled(&theme.tokens.spacing, factor);
    out
}

/// The theme with its high-contrast overlays merged into the modes (the
/// platform asked for more contrast). A theme without overlays is unchanged.
pub fn apply_contrast(theme: &ResolvedTheme) -> ResolvedTheme {
    let mut out = theme.clone();
    for mode in Mode::ALL {
        overlay_mode(out.modes.get_mut(mode), theme.contrast.get(mode));
    }
    out
}

/// An easing as CSS `cubic-bezier(…)`.
pub fn easing_css(curve: &[f64]) -> String {
    format!("cubic-bezier({})", curve.iter().map(|n| json::number_to_string(*n)).collect::<Vec<_>>().join(", "))
}

fn matches(rule: &RecipeRule, props: &Props, states: &[String]) -> bool {
    let Some(when) = &rule.when else { return true };
    for (key, want) in when {
        if key == "state" {
            let needed: Vec<&Value> = match want {
                Value::Array(a) => a.iter().collect(),
                other => vec![other],
            };
            if !needed.iter().all(|s| s.as_str().is_some_and(|s| states.iter().any(|x| x == s))) {
                return false;
            }
            continue;
        }
        let ok = match props.get(key) {
            None => false,
            Some(actual) => match want {
                Value::Array(list) => list.iter().any(|w| json::strict_eq(w, actual)),
                w => json::strict_eq(w, actual),
            },
        };
        if !ok {
            return false;
        }
    }
    true
}

/// The matching rules' styles merged in order (token refs kept).
/// How specific a rule is: one point per `when` condition (a `state` list
/// counts each state). VAPP-90: rules merge in SPECIFICITY order, like the
/// CSS the web renderer compiles them to — a base rule (no `when`) never
/// shadows a `checked`/`focus`/`variant` rule that sits before it, which is
/// what a child theme's appended base override used to do under plain
/// source order. Ties keep source order (later wins). Mirrors
/// `ruleSpecificity` in `packages/exponential-ui/src/theme.ts`.
pub fn rule_specificity(rule: &RecipeRule) -> usize {
    let Some(when) = &rule.when else { return 0 };
    when.iter()
        .map(|(key, want)| match (key.as_str(), want) {
            ("state", Value::Array(list)) => list.len(),
            _ => 1,
        })
        .sum()
}

pub fn recipe_style(theme: &ResolvedTheme, query: &RecipeQuery) -> Props {
    let mut out = Props::new();
    let Some(rules) = theme.recipes.get(&query.component).and_then(|p| p.get(&query.part)) else { return out };
    let mut ordered: Vec<(usize, &RecipeRule)> = rules.iter().enumerate().collect();
    // A stable sort: ties keep source order.
    ordered.sort_by_key(|(_, r)| rule_specificity(r));
    for (_, rule) in ordered {
        if matches(rule, &query.props, &query.states) {
            for (k, v) in &rule.style {
                out.insert(k.clone(), v.clone());
            }
        }
    }
    out
}

/// Does the query's recipe change under interaction `state` (`hover`)? True
/// when a rule names the state in `when.state` and every other `when` key
/// matches the query's props (the rule's OTHER states are not required: a
/// `[hover, checked]` rule counts while unchecked). Painters ask it to know
/// which nodes to track the pointer over.
pub fn recipe_reacts_to(theme: &ResolvedTheme, query: &RecipeQuery, state: &str) -> bool {
    let Some(rules) = theme.recipes.get(&query.component).and_then(|p| p.get(&query.part)) else { return false };
    rules.iter().any(|rule| {
        let Some(when) = &rule.when else { return false };
        let names_state = match when.get("state") {
            Some(Value::Array(list)) => list.iter().any(|s| s.as_str() == Some(state)),
            Some(v) => v.as_str() == Some(state),
            None => false,
        };
        names_state
            && when.iter().filter(|(k, _)| k.as_str() != "state").all(|(key, want)| match query.props.get(key) {
                None => false,
                Some(actual) => match want {
                    Value::Array(list) => list.iter().any(|w| json::strict_eq(w, actual)),
                    w => json::strict_eq(w, actual),
                },
            })
    })
}

/// A part's concrete visuals for one mode (`theme-recipes.json` locks it).
pub fn resolve_recipe(theme: &ResolvedTheme, query: &RecipeQuery, mode: Mode) -> Props {
    resolve_style_values(theme, &recipe_style(theme, query), mode)
}

/// The recipe query a NORMALIZED node answers to: its macro part, or the
/// native's root with its own recipe props.
pub fn node_recipe_query(node: &UiNode, index: &RecipeIndex) -> RecipeQuery {
    if let Some(r) = &node.recipe {
        return RecipeQuery::new(r.macro_.clone(), r.part.clone(), r.props.clone(), Vec::new());
    }
    RecipeQuery::new(node.component.clone(), "root", index.native_recipe_props(&node.component, &node.props), Vec::new())
}

/// What a painter paints a node with: native recipe < node style < macro
/// part recipe, token references resolved, condition blocks kept nested.
pub fn resolve_node_style(theme: &ResolvedTheme, node: &UiNode, mode: Mode, states: &[String], index: &RecipeIndex) -> Props {
    let own_query = RecipeQuery::new(node.component.clone(), "root", index.native_recipe_props(&node.component, &node.props), states.to_vec());
    let mut out = resolve_recipe(theme, &own_query, mode);
    if let Some(style) = &node.style {
        out.extend(resolve_style_values(theme, style, mode));
    }
    if node.recipe.is_some() {
        let mut q = node_recipe_query(node, index);
        q.states = states.to_vec();
        out.extend(resolve_recipe(theme, &q, mode));
    }
    out
}

/// The shadow layers as one CSS `box-shadow` string (for web painters).
pub fn shadow_css(layers: &[Shadow]) -> String {
    if layers.is_empty() {
        return "none".into();
    }
    let n = json::number_to_string;
    layers
        .iter()
        .map(|l| format!("{}px {}px {}px {}px {}", n(l.x), n(l.y), n(l.blur), n(l.spread), l.color))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    /// VAPP-90: a conditioned rule wins over a LATER base rule (a child
    /// theme's appended override used to shadow every `checked`/`focus`
    /// rule under plain source order); ties keep source order.
    #[test]
    fn recipe_rules_merge_by_specificity() {
        use serde_json::json;
        let theme = crate::themes::builtin_theme("exponential").unwrap();
        let props = |checked: bool| crate::types::Props::from_iter([("checked".to_string(), json!(checked)), ("disabled".to_string(), json!(false))]);
        let on = super::resolve_recipe(&theme, &super::RecipeQuery::new("Switch", "track", props(true), vec!["checked".into()]), super::Mode::Dark);
        let off = super::resolve_recipe(&theme, &super::RecipeQuery::new("Switch", "track", props(false), vec![]), super::Mode::Dark);
        let color = |name: &str| theme.modes.dark.color.get(name).cloned().map(serde_json::Value::String);
        assert_eq!(on.get("backgroundColor").cloned(), color("primary"));
        assert_eq!(off.get("backgroundColor").cloned(), color("input"));
        let playful = crate::themes::builtin_theme("playful").unwrap();
        let track = super::resolve_recipe(&playful, &super::RecipeQuery::new("Switch", "track", props(true), vec!["checked".into()]), super::Mode::Dark);
        assert_eq!(track.get("width").and_then(serde_json::Value::as_f64), Some(44.0));
        assert_eq!(super::rule_specificity(&super::RecipeRule { when: Some(serde_json::Map::from_iter([("state".to_string(), json!(["hover", "focus"])), ("variant".to_string(), json!("ghost"))])), style: Default::default() }), 3);
    }

    use super::*;
    use crate::themes::{builtin_refs, builtin_theme, builtin_themes, BUILTIN_THEME_IDS};
    use serde_json::json;

    fn q(component: &str, part: &str, props: Value, states: &[&str]) -> RecipeQuery {
        RecipeQuery::new(component, part, props.as_object().cloned().unwrap_or_default(), states.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn the_three_built_ins_load_neutral_is_the_root() {
        assert_eq!(BUILTIN_THEME_IDS, ["neutral", "exponential", "playful"]);
        for theme in builtin_themes() {
            assert_eq!(theme.chain[0], "neutral");
            assert_eq!(theme.modes.light.color.len(), theme.modes.dark.color.len());
        }
        assert_eq!(builtin_theme("playful").unwrap().chain, ["neutral", "playful"]);
        assert_eq!(builtin_theme("exponential").unwrap().chain, ["neutral", "exponential"]);
    }

    #[test]
    fn the_exponential_theme_carries_the_apps_values() {
        let t = builtin_theme("exponential").unwrap();
        assert_eq!(t.tokens.spacing["md"], 12.0);
        assert_eq!(t.tokens.radius["lg"], 12.0);
        assert_eq!(t.tokens.r#type.family["sans"], "Inter");
        assert_eq!(t.tokens.control["buttonMd"], 32.0);
        assert_eq!(t.modes.dark.color["background"], "#0a0a0a");
        assert_eq!(t.modes.light.color["background"], "#ffffff");
        let card = &t.modes.dark.color["card"];
        assert!(card.starts_with("#ffffff") && card.len() == 9);
    }

    #[test]
    fn playful_differs_from_neutral_where_it_says_so_and_inherits_the_rest() {
        let neutral = builtin_theme("neutral").unwrap();
        let playful = builtin_theme("playful").unwrap();
        assert_ne!(playful.tokens.radius["md"], neutral.tokens.radius["md"]);
        assert_eq!(playful.tokens.spacing, neutral.tokens.spacing);
        assert_eq!(playful.tokens.r#type.family["sans"], "Nunito");
        let button = |id: &str| resolve_recipe(&builtin_theme(id).unwrap(), &q("Button", "root", json!({"variant": "default", "size": "default"}), &[]), Mode::Light);
        assert_eq!(button("playful")["borderRadius"], json!(9999));
        assert_eq!(button("neutral")["borderRadius"], json!(8));
        assert_eq!(button("playful")["backgroundColor"], json!("#7c3aed"));
    }

    #[test]
    fn tokens_resolve_per_mode() {
        let n = builtin_theme("neutral").unwrap();
        assert_eq!(resolve_token(&n, &json!("$color.primary"), Mode::Light), Some(json!("#171717")));
        assert_eq!(resolve_token(&n, &json!("$color.primary"), Mode::Dark), Some(json!("#e5e5e5")));
        assert_eq!(resolve_token(&n, &json!("$spacing.md"), Mode::Light), Some(json!(12)));
        assert_eq!(resolve_token(&n, &json!("$type.family.sans"), Mode::Light), Some(json!("Geist")));
        assert_eq!(resolve_token(&n, &json!("$shadow.none"), Mode::Light), Some(json!([])));
        assert_eq!(resolve_token(&n, &json!("nope"), Mode::Light), None);
    }

    fn has(style: &Props, expected: Value) {
        for (k, v) in expected.as_object().unwrap() {
            assert!(style.get(k).is_some_and(|s| json::equal(s, v)), "{k}: {:?} != {v}", style.get(k));
        }
    }

    #[test]
    fn rules_merge_in_order_variant_size_and_state_compose() {
        let n = builtin_theme("neutral").unwrap();
        let r = |props: Value, states: &[&str]| resolve_recipe(&n, &q("Button", "root", props, states), Mode::Light);
        has(&r(json!({"variant": "default", "size": "default"}), &[]), json!({"height": 36, "backgroundColor": "#171717", "color": "#fafafa", "borderRadius": 8}));
        has(&r(json!({"variant": "outline", "size": "sm"}), &[]), json!({"height": 32, "borderWidth": 1, "borderColor": "#e5e5e5", "backgroundColor": "#ffffff"}));
        has(&r(json!({"variant": "outline", "size": "sm"}), &["hover"]), json!({"backgroundColor": "#f5f5f5"}));
        has(&r(json!({"variant": "ghost", "size": "icon"}), &[]), json!({"width": 36, "height": 36, "backgroundColor": "#00000000", "boxShadow": []}));
        has(&r(json!({"variant": "default", "size": "default"}), &["disabled"]), json!({"opacity": 0.5}));
        has(&r(json!({"variant": "default", "size": "default"}), &["hover"]), json!({"opacity": 0.9}));
    }

    #[test]
    fn a_state_rule_needs_every_listed_state_a_list_value_matches_any() {
        let n = builtin_theme("neutral").unwrap();
        let style = resolve_recipe(&n, &q("Badge", "icon", json!({"variant": "outline"}), &[]), Mode::Dark);
        assert_eq!(style["color"], json!("#fafafa"));
    }

    #[test]
    fn node_precedence_native_recipe_then_node_style_then_macro_part_recipe() {
        // The Badge label node as the reducer emits it (Text + the Badge/label part).
        let n = builtin_theme("neutral").unwrap();
        let index = RecipeIndex::core();
        let mut label = UiNode::new("b.label", "Text");
        label.props = json!({"text": "New", "variant": "caption"}).as_object().cloned().unwrap();
        label.recipe = Some(crate::types::Recipe { macro_: "Badge".into(), part: "label".into(), props: json!({"variant": "destructive"}).as_object().cloned().unwrap() });
        let query = node_recipe_query(&label, &index);
        assert_eq!(query, q("Badge", "label", json!({"variant": "destructive"}), &[]));
        let style = resolve_node_style(&n, &label, Mode::Light, &[], &index);
        assert_eq!(style["fontSize"], json!(12));
        assert_eq!(style["color"], json!(n.modes.light.color["destructiveForeground"]));
        assert_eq!(style["fontWeight"], json!(500));
    }

    #[test]
    fn an_authors_style_on_a_plain_native_wins_over_its_recipe() {
        let n = builtin_theme("neutral").unwrap();
        let mut text = UiNode::new("t", "Text");
        text.props = json!({"text": "x", "variant": "muted"}).as_object().cloned().unwrap();
        text.style = json!({"color": "#ff0000"}).as_object().cloned();
        assert_eq!(resolve_node_style(&n, &text, Mode::Light, &[], &RecipeIndex::core())["color"], json!("#ff0000"));
    }

    #[test]
    fn a_theme_that_changes_primary_and_the_button_radius_works_everywhere() {
        let refs = builtin_refs();
        let brand = load_theme(
            &json!({"id": "brand", "name": "Brand", "extends": "neutral", "modes": {"light": {"color": {"primary": "#2563eb"}}}, "recipes": {"Button": {"root": [{"style": {"borderRadius": "$radius.full"}}]}}}),
            &ThemeOptions::core(&refs),
        )
        .unwrap();
        assert_eq!(brand.chain, ["neutral", "brand"]);
        assert_eq!(brand.modes.light.color["primary"], "#2563eb");
        assert_eq!(brand.modes.dark.color["primary"], "#e5e5e5");
        assert_eq!(brand.modes.light.color["secondary"], "#f5f5f5");
        has(&resolve_recipe(&brand, &q("Button", "root", json!({"variant": "default", "size": "sm"}), &[]), Mode::Light), json!({"backgroundColor": "#2563eb", "borderRadius": 9999, "height": 32}));
        assert_eq!(resolve_recipe(&brand, &q("Badge", "root", json!({"variant": "default"}), &[]), Mode::Light)["backgroundColor"], json!("#2563eb"));
        assert_eq!(resolve_recipe(&brand, &q("Text", "root", json!({"variant": "title", "tone": "primary"}), &[]), Mode::Light)["color"], json!("#2563eb"));
    }

    #[test]
    fn chains_of_chains_and_resolved_parents() {
        let refs = [ThemeRef::Resolved(builtin_theme("playful").unwrap())];
        let child = load_theme(&json!({"id": "c", "name": "C", "extends": "playful", "tokens": {"spacing": {"md": 20}}}), &ThemeOptions::core(&refs)).unwrap();
        assert_eq!(child.chain, ["neutral", "playful", "c"]);
        assert_eq!(child.tokens.spacing["md"], 20.0);
        assert_eq!(child.tokens.radius["md"], 14.0);
    }

    #[test]
    fn source_chains_and_cycles() {
        let refs = [
            ThemeRef::Source(json!({"id": "a", "name": "A", "extends": "b"})),
            ThemeRef::Source(json!({"id": "b", "name": "B", "extends": "a"})),
        ];
        let err = try_load_theme(&json!({"id": "c", "name": "C", "extends": "a"}), &ThemeOptions::core(&refs)).unwrap_err();
        assert_eq!(err, [issue("extends", "cycle: c → a → b → a")]);
    }

    #[test]
    fn an_invalid_theme_fails_with_every_issue_listed_never_a_crash() {
        let refs = builtin_refs();
        let opts = ThemeOptions::core(&refs);
        let e = load_theme(&Value::Null, &opts).unwrap_err();
        assert_eq!(e.id, "?");
        assert!(load_theme(&json!(42), &ThemeOptions::default()).unwrap_err().to_string().contains("expected a theme object"));
        let bad = json!({"id": "bad", "name": "Bad", "extends": "neutral", "tokens": {"spacing": {"huge": 1}}, "recipes": {"Button": {"root": [{"style": {"display": "flex"}}]}}});
        let e = load_theme(&bad, &opts).unwrap_err();
        let paths: Vec<&str> = e.issues.iter().map(|i| i.path.as_str()).collect();
        assert_eq!(paths, ["tokens.spacing.huge", "recipes.Button.root[0].style.display"]);
        let msg = e.to_string();
        assert!(msg.contains("Theme \"bad\" is invalid:"));
        assert!(msg.contains("not a recipe key"));
        let err = try_load_theme(&json!({"id": "x", "name": "X", "extends": "nope"}), &opts).unwrap_err();
        assert!(err[0].message.contains("unknown theme \"nope\""));
        assert_eq!(validate_theme(&json!("neutral"), &ThemeOptions::default()).len(), 1);
    }

    #[test]
    fn a_root_theme_must_give_every_token_a_value() {
        let issues = try_load_theme(&json!({"id": "bare", "name": "Bare", "modes": {}, "tokens": {}, "recipes": {}}), &ThemeOptions::default()).unwrap_err();
        assert!(issues.len() > 100);
        assert!(issues[0].message.starts_with("missing value for $color."));
    }

    #[test]
    fn shadow_css_prints_js_numbers() {
        let n = builtin_theme("neutral").unwrap();
        assert_eq!(shadow_css(&n.modes.light.shadow["none"]), "none");
        assert_eq!(shadow_css(&n.modes.light.shadow["sm"]), "0px 1px 2px 0px #0000000d");
    }

    #[test]
    fn hex_and_id_checks() {
        assert!(is_theme_hex(&json!("#a1b2c3")));
        assert!(is_theme_hex(&json!("#a1b2c3d4")));
        assert!(!is_theme_hex(&json!("#A1B2C3")));
        assert!(!is_theme_hex(&json!("#abc")));
        assert!(is_kebab_id("a-1"));
        assert!(!is_kebab_id("1a"));
        assert!(!is_kebab_id(""));
    }
}
