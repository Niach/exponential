//! Sub-parts the core does not synthesize (a Checkbox `check`, a Switch
//! `thumb`, a Select `trigger`, Button `icon`, Ring `track`/`fill`…) are
//! painted from their recipe: [`part_visual`] resolves one exactly like the
//! core resolves a synthetic part (`native_recipe_props` of the owner + the
//! states), so a theme styles them the same on every painter.

use exponential_ui::recipes;
use exponential_ui::style::{self, BoxKind, Visual};
use exponential_ui::theme::{self, Mode, RecipeQuery, ResolvedTheme};
use gpui::Hsla;
use serde_json::{Map, Value};

use super::color::parse_hex;

/// The resolved flat style of `owner_component/part` (token refs resolved).
pub fn part_props(theme: Option<&ResolvedTheme>, mode: Mode, owner_component: &str, part: &str, owner_props: &Map<String, Value>, states: &[String]) -> Map<String, Value> {
    let Some(theme) = theme else { return Map::new() };
    let query = RecipeQuery::new(owner_component, part, recipes::native_recipe_props(owner_component, owner_props), states.to_vec());
    theme::resolve_recipe(theme, &query, mode)
}

/// The painted visual of `owner_component/part` (empty in geometry mode).
pub fn part_visual(theme: Option<&ResolvedTheme>, mode: Mode, owner_component: &str, part: &str, owner_props: &Map<String, Value>, states: &[String]) -> Visual {
    style::visual(&part_props(theme, mode, owner_component, part, owner_props, states), BoxKind::Leaf)
}

/// A px number of a resolved flat style (`width`, `height`, `borderWidth`…).
pub fn px_prop(props: &Map<String, Value>, key: &str) -> Option<f32> {
    props.get(key).and_then(style::px)
}

/// A colour token of the theme for the mode (`foreground`, `chart1`…).
pub fn theme_color(theme: Option<&ResolvedTheme>, mode: Mode, name: &str) -> Option<Hsla> {
    theme.and_then(|t| t.modes.get(mode).color.get(name)).and_then(|c| parse_hex(c))
}

/// The surface's default text colour (CSS: `.xui-surface{color:foreground}`).
pub fn default_ink(theme: Option<&ResolvedTheme>, mode: Mode) -> Hsla {
    theme_color(theme, mode, "foreground").unwrap_or_else(|| match mode {
        Mode::Dark => parse_hex("#fafafa").expect("hex"),
        Mode::Light => parse_hex("#0a0a0a").expect("hex"),
    })
}

/// The theme's `sans` family (the surface default), else `None`.
pub fn default_family(theme: Option<&ResolvedTheme>) -> Option<String> {
    theme.and_then(|t| t.tokens.r#type.family.get("sans").cloned())
}

/// The theme's `mono` family.
pub fn mono_family(theme: Option<&ResolvedTheme>) -> Option<String> {
    theme.and_then(|t| t.tokens.r#type.family.get("mono").cloned())
}

/// A spacing token in px (geometry mode: the core's fallback table).
pub fn spacing(theme: Option<&ResolvedTheme>, name: &str) -> f32 {
    match theme {
        Some(t) => t.tokens.spacing.get(name).copied().unwrap_or(0.0) as f32,
        None => match name {
            "xxs" => 2.0,
            "xs" => 4.0,
            "sm" => 8.0,
            "md" => 12.0,
            "lg" => 16.0,
            "xl" => 24.0,
            _ => 0.0,
        },
    }
}

/// A control token in px (`iconSm`, `slider`…), with a fallback.
pub fn control(theme: Option<&ResolvedTheme>, name: &str, fallback: f32) -> f32 {
    theme.and_then(|t| t.tokens.control.get(name).copied()).map(|v| v as f32).unwrap_or(fallback)
}

/// `props` with `key` replaced (the owner props a mirrored control paints with).
pub fn with_prop(props: &Map<String, Value>, key: &str, value: Value) -> Map<String, Value> {
    let mut out = props.clone();
    out.insert(key.to_string(), value);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use exponential_ui::themes::builtin_theme;
    use serde_json::json;

    #[test]
    fn a_select_trigger_resolves_from_the_owner_recipe() {
        let theme = builtin_theme("neutral").unwrap();
        let props = json!({"options": []}).as_object().unwrap().clone();
        let v = part_visual(Some(&theme), Mode::Light, "Select", "trigger", &props, &[]);
        assert_eq!(v.border_width, Some(1.0));
        assert!(v.background_color.is_some());
        let p = part_props(Some(&theme), Mode::Light, "Select", "trigger", &props, &[]);
        assert_eq!(px_prop(&p, "height"), Some(36.0));
    }

    #[test]
    fn states_and_props_select_rules() {
        let theme = builtin_theme("neutral").unwrap();
        let off = part_visual(Some(&theme), Mode::Light, "Checkbox", "box", &json!({"checked": false}).as_object().unwrap().clone(), &[]);
        let on = part_visual(Some(&theme), Mode::Light, "Checkbox", "box", &json!({"checked": true}).as_object().unwrap().clone(), &[]);
        assert_ne!(off.background_color, on.background_color);
        assert!(part_visual(None, Mode::Light, "Checkbox", "box", &Map::new(), &[]).background_color.is_none());
    }
}
