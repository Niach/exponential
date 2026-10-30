//! Round 1 (docs/round-1-contract.md §2): STYLE CONDITIONS — the one media
//! grammar (`catalog/style.json` `conditions.media`, hand-parsed: no regex
//! engine), the state keys, and `resolveConditions`, the reference flattening
//! of `src/style.ts`: the base keys, then every MATCHING `@media` block in
//! SOURCE order (later wins), then `:hover`, `:focus-visible`, `:pressed` in
//! that order (pressed wins). Sizes are the SURFACE box; `min-*` = size >= N,
//! `max-*` = size < N (strict); a `$breakpoint.<name>` resolves through the
//! context's breakpoints (an unknown token never matches); orientation
//! portrait = height >= width (landscape when the height is unknown).
//! `fixtures/style-conditions.json` locks it.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::generated::catalog as g;

/// The state keys in the order they apply (the last wins).
pub const STYLE_STATES: &[&str] = g::STYLE_STATES;

/// The size features of the grammar.
const SIZE_FEATURES: [&str; 4] = ["min-width", "max-width", "min-height", "max-height"];

/// One parsed `@media (feature: value)` key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaCondition {
    /// `min-width | max-width | min-height | max-height | orientation | hover | prefers-reduced-motion`
    pub feature: String,
    /// The px value (`600px`) or `$breakpoint.<name>` for the size features;
    /// the keyword for the others.
    pub value: String,
}

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

/// `\$breakpoint\.[a-zA-Z0-9]+` → the name.
pub fn breakpoint_ref(value: &str) -> Option<&str> {
    let name = value.strip_prefix("$breakpoint.")?;
    (!name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric())).then_some(name)
}

/// `"@media (min-width: $breakpoint.md)"` → `{feature, value}`; `None` when
/// the key is not a media condition of the grammar (`STYLE_MEDIA_PATTERN`).
pub fn parse_media_condition(key: &str) -> Option<MediaCondition> {
    let inner = key.strip_prefix("@media (")?.strip_suffix(')')?;
    let (feature, value) = inner.split_once(": ")?;
    let ok = match feature {
        f if SIZE_FEATURES.contains(&f) => value.strip_suffix("px").is_some_and(decimal) || breakpoint_ref(value).is_some(),
        "orientation" => matches!(value, "portrait" | "landscape"),
        "hover" => matches!(value, "hover" | "none"),
        "prefers-reduced-motion" => matches!(value, "reduce" | "no-preference"),
        _ => false,
    };
    ok.then(|| MediaCondition { feature: feature.to_string(), value: value.to_string() })
}

/// A `@media` key of the grammar.
pub fn is_media_key(key: &str) -> bool {
    parse_media_condition(key).is_some()
}

/// A `@media` or state condition key (`isConditionKey`).
pub fn is_condition_key(key: &str) -> bool {
    is_media_key(key) || STYLE_STATES.contains(&key)
}

/// What a client knows when it flattens a style: the SURFACE box, the
/// platform's pointer and motion preference, the node's interaction states
/// and the theme's breakpoint values (px).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConditionContext {
    pub width: f32,
    #[serde(default)]
    pub height: Option<f32>,
    /// A hover-capable pointer (mouse, trackpad); false on touch.
    #[serde(default)]
    pub hover: bool,
    /// The platform asks for reduced motion.
    #[serde(default)]
    pub reduced_motion: bool,
    /// The node's active states, the state keys without the colon
    /// (`hover`, `focus-visible`, `pressed`).
    #[serde(default)]
    pub states: Vec<String>,
    /// `$breakpoint.<name>` → px.
    #[serde(default)]
    pub breakpoints: IndexMap<String, f32>,
}

/// The built-in breakpoints (every built-in theme's, Tailwind's).
pub fn default_breakpoints() -> IndexMap<String, f32> {
    [("sm", 640.0), ("md", 768.0), ("lg", 1024.0), ("xl", 1280.0)].into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

fn size_value(value: &str, breakpoints: &IndexMap<String, f32>) -> Option<f32> {
    if let Some(name) = breakpoint_ref(value) {
        return breakpoints.get(name).copied();
    }
    value.strip_suffix("px").unwrap_or(value).parse::<f32>().ok()
}

/// Does a media condition hold for the context?
pub fn media_matches(condition: &MediaCondition, ctx: &ConditionContext) -> bool {
    match condition.feature.as_str() {
        f if SIZE_FEATURES.contains(&f) => {
            let Some(n) = size_value(&condition.value, &ctx.breakpoints) else { return false };
            let size = if f.ends_with("width") { Some(ctx.width) } else { ctx.height };
            let Some(size) = size else { return false };
            if f.starts_with("min") {
                size >= n
            } else {
                size < n
            }
        }
        "orientation" => {
            let portrait = ctx.height.is_some_and(|h| h >= ctx.width);
            condition.value == if portrait { "portrait" } else { "landscape" }
        }
        "hover" => condition.value == if ctx.hover { "hover" } else { "none" },
        "prefers-reduced-motion" => condition.value == if ctx.reduced_motion { "reduce" } else { "no-preference" },
        _ => false,
    }
}

/// Flatten a style for one pass: the base keys, then every matching `@media`
/// block in source order, then the matching state blocks in [`STYLE_STATES`]
/// order. Token values pass through untouched (the theme resolves them).
pub fn resolve_conditions(style: &Map<String, Value>, ctx: &ConditionContext) -> Map<String, Value> {
    let mut out = Map::new();
    for (k, v) in style {
        if !is_condition_key(k) {
            out.insert(k.clone(), v.clone());
        }
    }
    for (k, v) in style {
        let Some(condition) = parse_media_condition(k) else { continue };
        let Some(block) = v.as_object() else { continue };
        if media_matches(&condition, ctx) {
            for (bk, bv) in block {
                out.insert(bk.clone(), bv.clone());
            }
        }
    }
    for key in STYLE_STATES {
        let state = &key[1..];
        if !ctx.states.iter().any(|s| s == state) {
            continue;
        }
        if let Some(block) = style.get(*key).and_then(Value::as_object) {
            for (bk, bv) in block {
                out.insert(bk.clone(), bv.clone());
            }
        }
    }
    out
}

/// The breakpoint a surface of this width is in: the LAST `$breakpoint`
/// name (ascending) whose px is <= width; `None` below the first (= `base`).
pub fn active_breakpoint(width: f32, breakpoints: &IndexMap<String, f32>) -> Option<String> {
    let mut out = None;
    for name in crate::macros::BREAKPOINTS.iter() {
        if breakpoints.get(name).is_some_and(|px| width >= *px) {
            out = Some(name.clone());
        }
    }
    out
}

/// Does any style in the subtree depend on the surface HEIGHT (a height or
/// orientation media block)? A height change restyles only then.
pub fn depends_on_height(style: &Map<String, Value>) -> bool {
    style.keys().filter_map(|k| parse_media_condition(k)).any(|c| c.feature.ends_with("height") || c.feature == "orientation")
}

/// Does the style carry a state block (`:hover`, …)? Only such nodes (and
/// those with state recipes) restyle on a state change.
pub fn has_state_blocks(style: &Map<String, Value>) -> bool {
    STYLE_STATES.iter().any(|k| style.contains_key(*k))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_grammar() {
        for key in [
            "@media (min-width: 600px)",
            "@media (max-width: 37.5px)",
            "@media (min-height: $breakpoint.md)",
            "@media (orientation: portrait)",
            "@media (hover: none)",
            "@media (prefers-reduced-motion: reduce)",
        ] {
            assert!(is_media_key(key), "{key}");
        }
        for key in ["@media (min-width: 600)", "@media (width: 600px)", "@media (orientation: up)", "@media (min-width: $spacing.md)", "@media screen", ":focus"] {
            assert!(!is_media_key(key), "{key}");
        }
        assert!(is_condition_key(":hover") && is_condition_key(":focus-visible") && is_condition_key(":pressed"));
    }

    #[test]
    fn max_is_strict_and_unknown_tokens_never_match() {
        let style = json!({"gap": 4, "@media (max-width: $breakpoint.md)": {"gap": 1}, "@media (min-width: $breakpoint.huge)": {"gap": 99}});
        let ctx = |w: f32| ConditionContext { width: w, breakpoints: default_breakpoints(), ..Default::default() };
        assert_eq!(resolve_conditions(style.as_object().unwrap(), &ctx(767.0))["gap"], json!(1));
        assert_eq!(resolve_conditions(style.as_object().unwrap(), &ctx(768.0))["gap"], json!(4));
        assert_eq!(resolve_conditions(style.as_object().unwrap(), &ctx(5000.0))["gap"], json!(4));
        assert_eq!(active_breakpoint(700.0, &default_breakpoints()).as_deref(), Some("sm"));
        assert_eq!(active_breakpoint(500.0, &default_breakpoints()), None);
        assert_eq!(active_breakpoint(1280.0, &default_breakpoints()).as_deref(), Some("xl"));
    }
}
