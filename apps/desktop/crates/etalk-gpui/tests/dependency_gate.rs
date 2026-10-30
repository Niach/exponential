//! EXP-1420: etalk-gpui is consumed outside this repo (spezl) as a git
//! dependency, so it links no Exponential app crate, not even the app's
//! `theme`: colours come from the host's gpui-component theme and eUI's
//! default theme (`eui_core::themes`). Path dependencies are the other
//! shared libraries only.

const FORBIDDEN: &[&str] = &[
    "api", "steer", "domain", "coding", "engine", "terminal", "ui", "app", "cli", "sync", "codemode",
    "updater", "theme",
];
const SHARED: &[&str] = &["etalk", "eui-core", "eui-gpui", "epilot", "epilot-screen"];

#[test]
fn depends_on_no_app_crate() {
    let mut section = String::new();
    for line in include_str!("../Cargo.toml").lines().map(str::trim) {
        if line.starts_with('[') {
            section = line.to_string();
            continue;
        }
        if !section.ends_with("dependencies]") || line.starts_with('#') {
            continue;
        }
        let Some((name, spec)) = line.split_once('=') else { continue };
        let name = name.trim();
        assert!(!FORBIDDEN.contains(&name), "etalk-gpui must not depend on `{name}` ({section}): app coupling stays in the host");
        if spec.contains("path") {
            assert!(SHARED.contains(&name), "etalk-gpui's path dependency `{name}` ({section}) is not a shared library");
        }
    }
}
