//! The MEASURE contract a painter (built in or a host's override) must keep
//! for a control (VAPP-92; mirrors `src/geometry.ts`): its box comes from the
//! theme's tokens through the recipe of the part that IS the control, never
//! from the painter. `control-geometry.json` records every built-in theme ×
//! control × size; [`check_geometry`] is what the conformance suite runs an
//! override through.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::json;
use crate::theme::{resolve_recipe, Mode, RecipeQuery, ResolvedTheme};
use crate::types::Props;

/// The part whose recipe sizes each control.
pub const CONTROL_PARTS: &[(&str, &str)] = &[
    ("Button", "root"),
    ("Toggle", "root"),
    ("ToggleGroup", "item"),
    ("Input", "field"),
    ("Textarea", "field"),
    ("Select", "trigger"),
    ("DatePicker", "trigger"),
    ("Checkbox", "box"),
    ("Radio", "item"),
    ("Switch", "track"),
    ("Slider", "thumb"),
    ("Avatar", "root"),
    ("Icon", "root"),
    ("Spinner", "root"),
    ("Ring", "root"),
    ("Tabs", "tab"),
];

pub const GEOMETRY_KEYS: &[&str] =
    &["width", "height", "minWidth", "minHeight", "paddingHorizontal", "paddingVertical", "padding", "gap", "borderWidth", "borderRadius"];

/// The sizing part of a control (`root` for anything not listed).
pub fn control_part(component: &str) -> &'static str {
    CONTROL_PARTS.iter().find(|(c, _)| *c == component).map(|(_, p)| *p).unwrap_or("root")
}

/// The numeric box of a control for a theme (mode-independent), in
/// [`GEOMETRY_KEYS`] order, numeric values only.
pub fn control_geometry(theme: &ResolvedTheme, component: &str, props: &Props, states: &[String]) -> IndexMap<String, f64> {
    let query = RecipeQuery::new(component, control_part(component), props.clone(), states.to_vec());
    let style = resolve_recipe(theme, &query, Mode::Light);
    let mut out = IndexMap::new();
    for key in GEOMETRY_KEYS {
        if let Some(v) = style.get(*key).and_then(|v| v.as_f64()) {
            out.insert(key.to_string(), v);
        }
    }
    out
}

/// What a painter reports after measuring the control it drew.
#[derive(Clone, Copy, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeasuredBox {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub padding_horizontal: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub padding_vertical: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border_width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border_radius: Option<f64>,
}

/// A host's replacement painter for one native kind: it may draw anything,
/// but `measure` must return the theme's box.
pub trait PainterOverride {
    fn component(&self) -> &str;
    fn measure(&self, theme: &ResolvedTheme, props: &Props, states: &[String]) -> MeasuredBox;
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct GeometryIssue {
    pub key: String,
    pub expected: f64,
    pub actual: Option<f64>,
}

/// Every geometry key the theme fixes that the measured box misses or
/// changes (a key the theme leaves open is the painter's).
pub fn check_geometry(expected: &IndexMap<String, f64>, measured: &MeasuredBox, tolerance: f64) -> Vec<GeometryIssue> {
    let mut issues = Vec::new();
    let mut probe = |key: &str, actual: Option<f64>| {
        let Some(&want) = expected.get(key) else { return };
        if actual.is_none_or(|a| (a - want).abs() > tolerance) {
            issues.push(GeometryIssue { key: key.into(), expected: want, actual });
        }
    };
    probe("width", measured.width);
    // A control with a minimum height (Textarea) grows from it: the recipe's
    // `height` is its one-line base, so only the floor is checked (VAPP-91).
    if !expected.contains_key("minHeight") {
        probe("height", measured.height);
    }
    probe("paddingHorizontal", measured.padding_horizontal);
    probe("paddingVertical", measured.padding_vertical);
    probe("borderWidth", measured.border_width);
    probe("borderRadius", measured.border_radius);
    if let Some(&min) = expected.get("minHeight") {
        if measured.height.is_none_or(|h| h + tolerance < min) {
            issues.push(GeometryIssue { key: "minHeight".into(), expected: min, actual: measured.height });
        }
    }
    issues
}

/// The tolerance the TS defaults to.
pub const DEFAULT_TOLERANCE: f64 = 0.5;

/// One fixture case an override is run through.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct GeometryCase {
    #[serde(default)]
    pub props: Props,
    #[serde(default)]
    pub states: Vec<String>,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct GeometryFailure {
    pub name: String,
    pub issues: Vec<GeometryIssue>,
}

/// Run an override through every case the fixture holds for its kind; only
/// the failing cases come back (named `k=v,…` or `default`).
pub fn verify_painter_geometry(override_: &dyn PainterOverride, theme: &ResolvedTheme, cases: &[GeometryCase]) -> Vec<GeometryFailure> {
    cases
        .iter()
        .map(|c| {
            let name = c.props.iter().map(|(k, v)| format!("{k}={}", json::to_js_string(v))).collect::<Vec<_>>().join(",");
            let expected = control_geometry(theme, override_.component(), &c.props, &c.states);
            let issues = check_geometry(&expected, &override_.measure(theme, &c.props, &c.states), DEFAULT_TOLERANCE);
            GeometryFailure { name: if name.is_empty() { "default".into() } else { name }, issues }
        })
        .filter(|r| !r.issues.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_geometry_tolerates_half_a_point() {
        let expected: IndexMap<String, f64> = [("height".to_string(), 20.0)].into_iter().collect();
        assert!(check_geometry(&expected, &MeasuredBox { height: Some(20.4), ..Default::default() }, DEFAULT_TOLERANCE).is_empty());
        let min: IndexMap<String, f64> = [("minHeight".to_string(), 20.0)].into_iter().collect();
        assert_eq!(check_geometry(&min, &MeasuredBox::default(), DEFAULT_TOLERANCE).len(), 1);
    }
}
