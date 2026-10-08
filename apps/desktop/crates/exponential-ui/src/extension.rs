//! Extension catalogs: their own catalog id, `extends` the core, their
//! components (natives a host paints, macros expanded from their own
//! templates) and no name that shadows a core one. Mirrors `src/extension.ts`.

use crate::catalog::{CORE, CORE_CATALOG_ID};
use crate::types::ExtensionDef;

/// `^https?:\/\/[^\s]+$`
fn is_catalog_id(id: &str) -> bool {
    id.strip_prefix("https://")
        .or_else(|| id.strip_prefix("http://"))
        .is_some_and(|rest| !rest.is_empty() && !rest.chars().any(char::is_whitespace))
}

/// `^[A-Z][A-Za-z0-9]*$`
fn is_pascal_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_uppercase()) && chars.all(|c| c.is_ascii_alphanumeric())
}

/// Every problem with the definition (empty = valid).
pub fn validate_extension(def: &ExtensionDef) -> Vec<String> {
    let core = &*CORE;
    let mut errors = Vec::new();
    if !is_catalog_id(&def.id) {
        errors.push("id must be a URL-shaped catalog id".to_string());
    }
    if def.extends != CORE_CATALOG_ID {
        errors.push(format!("extends must be {CORE_CATALOG_ID}"));
    }
    if def.name.is_empty() {
        errors.push("name is required".to_string());
    }
    if def.components.is_empty() {
        errors.push("an extension defines at least one component".to_string());
    }
    let ext_enums = def.enums.as_ref();
    let ext_defs = def.defs.as_ref();
    for (name, component) in &def.components {
        if !is_pascal_name(name) {
            errors.push(format!("{name}: component names are PascalCase"));
        }
        if core.components.contains_key(name) {
            errors.push(format!("{name}: shadows a core component"));
        }
        if component.description.is_empty() {
            errors.push(format!("{name}: description is required"));
        }
        for (prop, schema) in &component.props {
            if schema.description.is_empty() {
                errors.push(format!("{name}.{prop}: description is required"));
            }
            if let Some(e) = schema.enum_.as_deref().filter(|e| !e.is_empty()) {
                if schema.type_ == "enum" && !core.enums.contains_key(e) && !ext_enums.is_some_and(|m| m.contains_key(e)) {
                    errors.push(format!("{name}.{prop}: enum {e} is not defined"));
                }
            }
            if let Some(s) = schema.shape.as_deref().filter(|s| !s.is_empty()) {
                if schema.type_ == "object" && !core.defs.contains_key(s) && !ext_defs.is_some_and(|m| m.contains_key(s)) {
                    errors.push(format!("{name}.{prop}: shape {s} is not defined"));
                }
            }
        }
        if component.is_macro() && !def.macros.as_ref().is_some_and(|m| m.contains_key(name)) {
            errors.push(format!("{name}: a macro needs a template in macros"));
        }
    }
    for name in def.macros.iter().flat_map(|m| m.keys()) {
        if !def.components.get(name).is_some_and(|c| c.is_macro()) {
            errors.push(format!("macros.{name}: no macro component of that name"));
        }
    }
    for name in def.enums.iter().flat_map(|m| m.keys()) {
        if core.enums.contains_key(name) {
            errors.push(format!("enums.{name}: shadows a core enum"));
        }
    }
    errors
}

/// The definition, checked; `Err` lists every problem.
pub fn define_extension(def: ExtensionDef) -> Result<ExtensionDef, String> {
    let errors = validate_extension(&def);
    if errors.is_empty() {
        Ok(def)
    } else {
        Err(format!("extension {}:\n{}", def.id, errors.join("\n")))
    }
}

/// Parse an extension catalog from JSON and check it.
pub fn parse_extension(json: &str) -> Result<ExtensionDef, String> {
    let def: ExtensionDef = serde_json::from_str(json).map_err(|e| format!("extension: {e}"))?;
    define_extension(def)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example() -> ExtensionDef {
        let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/fixtures/catalog-extension.json"))
            .expect("catalog-extension.json");
        let file: serde_json::Value = serde_json::from_str(&text).unwrap();
        serde_json::from_value(file["extension"].clone()).unwrap()
    }

    #[test]
    fn define_extension_accepts_the_example_extension() {
        let ex = example();
        assert_eq!(validate_extension(&ex), Vec::<String>::new());
        assert_eq!(define_extension(ex.clone()), Ok(ex.clone()));
        let json = serde_json::to_string(&ex).unwrap();
        assert_eq!(parse_extension(&json), Ok(ex));
    }

    #[test]
    fn define_extension_refuses_a_core_name_a_missing_macro_template_a_wrong_base_and_a_bad_id() {
        let ex = example();
        let mut shadow = ex.clone();
        shadow.components.insert("Button".into(), ex.components["Sparkline"].clone());
        assert!(validate_extension(&shadow).contains(&"Button: shadows a core component".to_string()));
        let mut no_template = ex.clone();
        no_template.macros = Some(Default::default());
        assert!(validate_extension(&no_template).contains(&"StatCard: a macro needs a template in macros".to_string()));
        let mut wrong_base = ex.clone();
        wrong_base.extends = "https://example.com/other".into();
        assert!(validate_extension(&wrong_base).contains(&format!("extends must be {CORE_CATALOG_ID}")));
        let mut bad_id = ex.clone();
        bad_id.id = "example".into();
        assert!(validate_extension(&bad_id).contains(&"id must be a URL-shaped catalog id".to_string()));
        assert!(define_extension(bad_id).unwrap_err().contains("catalog id"));
        assert!(parse_extension("{").is_err());
    }
}
