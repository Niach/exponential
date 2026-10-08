//! The built-in themes, compiled in (VAPP-92; mirrors `src/themes.ts`).
//! `neutral` is stock shadcn and the ROOT the others extend; `exponential`
//! is generated from the app's design tokens; `playful` is the deliberately
//! different test theme. The generator embeds them already RESOLVED
//! (`BUILTIN_THEME_JSON`), so they are parsed once, never re-flattened.

use std::sync::{Arc, LazyLock};

use crate::generated::themes as g;
use crate::theme::{ResolvedTheme, ThemeRef};

/// Source order = resolution order.
pub const BUILTIN_THEME_IDS: &[&str] = g::BUILTIN_THEME_IDS;
pub const BUILTIN_THEME_NAMES: &[&str] = g::BUILTIN_THEME_NAMES;
pub const DEFAULT_THEME_ID: &str = g::DEFAULT_THEME_ID;

static BUILTINS: LazyLock<Vec<Arc<ResolvedTheme>>> = LazyLock::new(|| {
    g::BUILTIN_THEME_JSON
        .iter()
        .map(|src| Arc::new(serde_json::from_str::<ResolvedTheme>(src).expect("built-in theme JSON")))
        .collect()
});

/// A built-in, parsed once per process; `None` for an unknown id.
pub fn builtin_theme(id: &str) -> Option<Arc<ResolvedTheme>> {
    BUILTINS.iter().find(|t| t.id == id).cloned()
}

/// Every built-in, in source order.
pub fn builtin_themes() -> Vec<Arc<ResolvedTheme>> {
    BUILTINS.clone()
}

/// The registry a host passes as `ThemeOptions::themes` to extend a built-in.
pub fn builtin_refs() -> Vec<ThemeRef> {
    BUILTINS.iter().cloned().map(ThemeRef::Resolved).collect()
}

/// The default theme (`exponential`).
pub fn default_theme() -> Arc<ResolvedTheme> {
    builtin_theme(DEFAULT_THEME_ID).expect("default theme is a built-in")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_parse_in_source_order() {
        let ids: Vec<String> = builtin_themes().iter().map(|t| t.id.clone()).collect();
        assert_eq!(ids, BUILTIN_THEME_IDS);
        let names: Vec<String> = builtin_themes().iter().map(|t| t.name.clone()).collect();
        assert_eq!(names, BUILTIN_THEME_NAMES);
        assert_eq!(default_theme().id, "exponential");
        assert!(builtin_theme("nope").is_none());
    }

    #[test]
    fn builtins_reserialize_to_the_embedded_json() {
        // Equal as JSON, numbers in the JS model (`1`, never `1.0`). Byte
        // order differs only inside recipe `when`/`style` (sorted maps).
        for (src, theme) in g::BUILTIN_THEME_JSON.iter().zip(builtin_themes()) {
            let out = serde_json::to_string(&*theme).unwrap();
            let embedded: serde_json::Value = serde_json::from_str(src).unwrap();
            let again: serde_json::Value = serde_json::from_str(&out).unwrap();
            assert_eq!(again, embedded, "{}", theme.id);
            assert_eq!(out.len(), src.len(), "{}", theme.id);
        }
    }
}
