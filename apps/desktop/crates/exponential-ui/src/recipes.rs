//! The recipe CONTRACT as code (VAPP-92): which parts each component has and
//! which props a theme's `when` may key on. Natives are listed by hand in
//! `catalog/recipes.json`; macro parts are derived from their templates in
//! `macros.json` (every `part`, the macro's `recipeProps` + each part's
//! `$recipe` keys), so the two files cannot disagree. Mirrors `src/recipes.ts`.
//!
//! The TS recomputes the part table per call; here a [`RecipeIndex`] is built
//! ONCE per catalog view ([`RecipeIndex::core`] is cached).

use std::sync::{Arc, LazyLock};

use indexmap::IndexMap;
use crate::catalog::CatalogView;
use crate::generated::{catalog as gc, themes as gt};
use crate::types::{MacroDef, MacroTemplate, TemplateChild};
pub use crate::types::{PartSpec, Props};

/// The interaction states a rule's `when.state` may name.
pub const RECIPE_STATES: &[&str] = gt::RECIPE_STATES;
/// The style keys a recipe may set.
pub const RECIPE_KEYS: &[&str] = gt::RECIPE_KEYS;

#[derive(serde::Deserialize)]
struct RecipesFile {
    native: IndexMap<String, PartSpec>,
}

/// `catalog/recipes.json` `native`, in source order.
static NATIVE_PARTS: LazyLock<IndexMap<String, PartSpec>> =
    LazyLock::new(|| serde_json::from_str::<RecipesFile>(gc::RECIPES_JSON).expect("recipes.json").native);

fn push_unique(list: &mut Vec<String>, value: &str) {
    if !list.iter().any(|v| v == value) {
        list.push(value.to_string());
    }
}

fn walk_template(tpl: &MacroTemplate, parts: &mut Vec<String>, props: &mut Vec<String>) {
    push_unique(parts, &tpl.part);
    if let Some(recipe) = &tpl.recipe {
        for key in recipe.keys() {
            push_unique(props, key);
        }
    }
    for child in tpl.children.iter().flatten() {
        if let TemplateChild::Node(node) = child {
            walk_template(node, parts, props);
        }
    }
}

/// A macro's parts and recipe props, read off its template.
pub fn macro_parts(def: &MacroDef) -> PartSpec {
    let mut parts = Vec::new();
    let mut props = Vec::new();
    for p in &def.recipe_props {
        push_unique(&mut props, p);
    }
    walk_template(&def.root, &mut parts, &mut props);
    PartSpec { parts, props }
}

/// Every component of the view → its parts and `when` props. An extension's
/// natives declare `recipe: {parts, props}` on their component def; without
/// it a native gets `root` and its enum/boolean props.
pub fn recipe_parts(view: &CatalogView) -> IndexMap<String, PartSpec> {
    let mut out = IndexMap::new();
    for (name, def) in &view.components {
        if def.is_macro() {
            if let Some(m) = view.macros.get(name) {
                out.insert(name.clone(), macro_parts(m));
            }
            continue;
        }
        let spec = if let Some(native) = NATIVE_PARTS.get(name) {
            native.clone()
        } else if let Some(declared) = &def.recipe {
            declared.clone()
        } else {
            PartSpec {
                parts: vec!["root".into()],
                props: def
                    .props
                    .iter()
                    .filter(|(_, p)| p.type_ == "enum" || p.type_ == "boolean")
                    .map(|(k, _)| k.clone())
                    .collect(),
            }
        };
        out.insert(name.clone(), spec);
    }
    out
}

/// The part table of one catalog view plus the catalog defaults of every
/// recipe prop: what the validator and the node resolver look up.
#[derive(Debug, Clone)]
pub struct RecipeIndex {
    pub parts: IndexMap<String, PartSpec>,
    /// Component → (recipe prop → catalog default), only props with a default.
    defaults: IndexMap<String, Props>,
    /// The components the view defines (a native query needs a def).
    known: IndexMap<String, ()>,
}

static CORE_INDEX: LazyLock<Arc<RecipeIndex>> = LazyLock::new(|| Arc::new(RecipeIndex::new(&CatalogView::core())));

impl RecipeIndex {
    pub fn new(view: &CatalogView) -> RecipeIndex {
        let parts = recipe_parts(view);
        let mut defaults = IndexMap::new();
        let mut known = IndexMap::new();
        for (name, def) in &view.components {
            known.insert(name.clone(), ());
            let Some(spec) = parts.get(name) else { continue };
            let mut d = Props::new();
            for prop in &spec.props {
                if let Some(v) = def.props.get(prop).and_then(|p| p.default.as_ref()) {
                    if !v.is_null() {
                        d.insert(prop.clone(), v.clone());
                    }
                }
            }
            defaults.insert(name.clone(), d);
        }
        RecipeIndex { parts, defaults, known }
    }

    /// The core catalog's index, built once per process.
    pub fn core() -> Arc<RecipeIndex> {
        CORE_INDEX.clone()
    }

    pub fn part_spec(&self, component: &str) -> Option<&PartSpec> {
        self.parts.get(component)
    }

    /// The recipe props a NATIVE node answers to: each spec prop's authored
    /// value, else the catalog default, skipping undefined (TS
    /// `nativeRecipeProps`; `null` counts as missing like `??`).
    pub fn native_recipe_props(&self, component: &str, props: &Props) -> Props {
        let mut out = Props::new();
        let (Some(spec), true) = (self.parts.get(component), self.known.contains_key(component)) else {
            return out;
        };
        let defaults = self.defaults.get(component);
        for name in &spec.props {
            let value = match props.get(name) {
                Some(v) if !v.is_null() => Some(v),
                _ => defaults.and_then(|d| d.get(name)),
            };
            if let Some(v) = value {
                out.insert(name.clone(), v.clone());
            }
        }
        out
    }
}

/// [`RecipeIndex::native_recipe_props`] against the core catalog.
pub fn native_recipe_props(component: &str, props: &Props) -> Props {
    CORE_INDEX.native_recipe_props(component, props)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    #[test]
    fn every_component_has_parts_macro_parts_come_from_the_templates() {
        let index = RecipeIndex::core();
        let parts = &index.parts;
        assert_eq!(parts["Badge"].parts, ["root", "icon", "label"]);
        assert_eq!(parts["Badge"].props, ["variant"]);
        assert_eq!(parts["Meter"].props, ["tone"]);
        assert_eq!(parts["ButtonGroup"].props, ["size", "selected"]);
        assert!(parts["Switch"].parts.iter().any(|p| p == "track"));
        assert_eq!(parts["Button"].props, ["variant", "size", "disabled", "loading"]);
        assert_eq!(parts.len(), 61);
    }

    #[test]
    fn recipe_keys_are_a_subset_of_the_box_visual_keys_plus_the_box_keys_and_native() {
        let style: Value = serde_json::from_str(gc::STYLE_JSON).unwrap();
        let mut allowed: Vec<String> = style["visual"].as_object().unwrap().keys().cloned().collect();
        for k in ["padding", "paddingHorizontal", "paddingVertical", "gap", "width", "height", "minWidth", "minHeight", "native"] {
            allowed.push(k.into());
        }
        for key in RECIPE_KEYS {
            assert!(allowed.iter().any(|a| a == key), "{key}");
        }
        assert!(RECIPE_STATES.contains(&"hover"));
    }

    #[test]
    fn native_props_fill_catalog_defaults() {
        let index = RecipeIndex::core();
        let props = json!({"text": "x", "variant": "muted"});
        let out = index.native_recipe_props("Text", props.as_object().unwrap());
        assert_eq!(out.get("variant"), Some(&json!("muted")));
        assert!(out.get("text").is_none());
        assert!(index.native_recipe_props("Nope", &Props::new()).is_empty());
    }
}
