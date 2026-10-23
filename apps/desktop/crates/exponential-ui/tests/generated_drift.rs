//! The crate's copies of the generated constants must equal the package's
//! (`packages/exponential-ui/generated/*.rs`): the generator writes both,
//! `generate.test.ts` gates the package side, this gates the crate side.

const PKG: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../packages/exponential-ui/generated");
const CRATE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src/generated");

fn same(a: &str, b: &str) {
    let x = std::fs::read_to_string(a).unwrap_or_else(|e| panic!("{a}: {e}"));
    let y = std::fs::read_to_string(b).unwrap_or_else(|e| panic!("{b}: {e}"));
    assert!(x == y, "{b} differs from {a}: run `bun run --filter @exponential-at/ui generate`");
}

#[test]
fn the_generated_copies_match_the_package() {
    same(&format!("{PKG}/catalog.generated.rs"), &format!("{CRATE}/catalog.rs"));
    same(&format!("{PKG}/catalog.themes.generated.rs"), &format!("{CRATE}/themes.rs"));
    same(&format!("{PKG}/catalog.descriptions.generated.rs"), &format!("{CRATE}/descriptions.rs"));
}

/// VAPP-106: the embedded catalog is the RUNTIME one: no description or
/// example anywhere (the text is behind the `descriptions` feature).
#[test]
fn the_embedded_catalog_carries_no_text() {
    use exponential_ui::catalog::CORE;
    for (name, def) in &CORE.components {
        assert!(def.description.is_empty(), "{name}: description embedded");
        assert!(def.example.is_none(), "{name}: example embedded");
        for (prop, schema) in &def.props {
            assert!(schema.description.is_empty(), "{name}.{prop}: description embedded");
        }
    }
    for (name, def) in &CORE.defs {
        assert!(def.description.is_empty(), "{name}: description embedded");
    }
}

#[cfg(feature = "descriptions")]
#[test]
fn the_descriptions_cover_every_component() {
    use exponential_ui::catalog::{descriptions, CORE};
    let d = descriptions();
    for name in CORE.components.keys() {
        let entry = &d["components"][name];
        assert!(entry["description"].as_str().is_some_and(|s| !s.is_empty()), "{name}");
    }
}

/// VAPP-106: without `builtin-themes` only the default theme is embedded.
#[cfg(not(feature = "builtin-themes"))]
#[test]
fn without_builtin_themes_only_the_default_is_embedded() {
    assert_eq!(exponential_ui::themes::BUILTIN_THEME_IDS, [exponential_ui::themes::DEFAULT_THEME_ID]);
    assert!(exponential_ui::themes::builtin_theme("playful").is_none());
    let (theme, issues) = exponential_ui::themes::theme_or_default(&serde_json::json!("playful"));
    assert_eq!(theme.id, exponential_ui::themes::DEFAULT_THEME_ID);
    assert!(!issues.is_empty());
}

#[test]
fn the_embedded_catalog_parses_and_the_built_ins_load() {
    use exponential_ui::catalog::{CORE, CORE_MACROS, TOKEN_GROUPS};
    assert_eq!(CORE.components.len(), 77, "EXP-1280: + ChoiceTiles; N-12: + DockCard");
    assert_eq!(CORE_MACROS.len(), 29);
    assert_eq!(TOKEN_GROUPS.keys().map(String::as_str).collect::<Vec<_>>(), exponential_ui::generated::catalog::TOKEN_GROUPS);
    for id in exponential_ui::themes::BUILTIN_THEME_IDS {
        assert!(exponential_ui::themes::builtin_theme(id).is_some(), "{id}");
    }
}
