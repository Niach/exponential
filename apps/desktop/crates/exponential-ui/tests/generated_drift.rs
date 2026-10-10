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
}

#[test]
fn the_embedded_catalog_parses_and_the_built_ins_load() {
    use exponential_ui::catalog::{CORE, CORE_MACROS, TOKEN_GROUPS};
    assert_eq!(CORE.components.len(), 72);
    assert_eq!(CORE_MACROS.len(), 26);
    assert_eq!(TOKEN_GROUPS.keys().map(String::as_str).collect::<Vec<_>>(), exponential_ui::generated::catalog::TOKEN_GROUPS);
    for id in exponential_ui::themes::BUILTIN_THEME_IDS {
        assert!(exponential_ui::themes::builtin_theme(id).is_some(), "{id}");
    }
}
