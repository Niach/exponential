//! The built-in themes, compiled in (VAPP-92; mirrors `src/themes.ts`).
//! `neutral` is stock shadcn and the ROOT the others extend; `exponential`
//! is generated from the app's design tokens; `playful` is the deliberately
//! different test theme. The generator embeds them already RESOLVED
//! (`BUILTIN_THEME_JSON`), so they are parsed once, never re-flattened.

use std::sync::{Arc, LazyLock};

use serde_json::Value;

use crate::generated::themes as g;
use crate::theme::{try_load_theme, ResolvedTheme, ThemeIssue, ThemeOptions, ThemeRef};

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

/// Round 4 (VAPP-103): the theme a host paints with, NEVER a failure: a
/// built-in id (a JSON string) or a theme file over the built-ins; anything
/// unusable (an unknown id, a bad `$schema`, an unknown token, a missing
/// value) falls back to [`default_theme`] with the issues saying why (TS
/// `themeOrDefault`, Swift/Kotlin `ThemeHandle.loadOrDefault`).
pub fn theme_or_default(input: &Value) -> (Arc<ResolvedTheme>, Vec<ThemeIssue>) {
    if let Some(id) = input.as_str() {
        return match builtin_theme(id) {
            Some(t) => (t, vec![]),
            None => (default_theme(), vec![ThemeIssue { path: "theme".into(), message: format!("unknown built-in theme \"{id}\"; known: {}", BUILTIN_THEME_IDS.join("|")) }]),
        };
    }
    let refs = builtin_refs();
    match try_load_theme(input, &ThemeOptions::core(&refs)) {
        Ok(t) => (Arc::new(t), vec![]),
        Err(issues) => (default_theme(), issues),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_or_default_never_fails() {
        let (t, issues) = theme_or_default(&serde_json::json!("nope"));
        assert_eq!(t.id, DEFAULT_THEME_ID);
        assert_eq!(issues[0].path, "theme");
        let (t, issues) = theme_or_default(&serde_json::json!({"id": "x", "name": "X", "extends": "neutral"}));
        assert_eq!(t.id, DEFAULT_THEME_ID);
        assert_eq!(issues[0].path, "$schema");
        let (t, issues) = theme_or_default(&serde_json::json!({"$schema": crate::theme::THEME_SCHEMA_ID, "id": "x", "name": "X", "extends": "neutral"}));
        assert_eq!((t.id.as_str(), issues.len()), ("x", 0));
        assert_eq!(theme_or_default(&serde_json::json!("playful")).0.id, "playful");
    }

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
