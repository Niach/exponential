//! Theme loading on arbitrary JSON: `try_load_theme` over the built-in
//! registry and `theme_or_default`. Never a panic; the default theme is
//! always a fallback.
#![no_main]

use exponential_ui::theme::{try_load_theme, validate_theme, ThemeOptions};
use exponential_ui::themes::{builtin_refs, theme_or_default};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(data) else { return };
    let refs = builtin_refs();
    let options = ThemeOptions::core(&refs);
    let issues = validate_theme(&value, &options);
    let loaded = try_load_theme(&value, &options);
    assert_eq!(issues.is_empty(), loaded.is_ok(), "validate and load agree");
    let (theme, _) = theme_or_default(&value);
    assert!(!theme.id.is_empty());
});
