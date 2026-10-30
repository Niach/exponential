//! EXP-1420: etalk-gpui is consumed outside this repo (spezl) as a git
//! dependency, so it links no Exponential app crate, not even the app's
//! `theme`: colours come from the host's gpui-component theme and eUI's
//! default theme (`eui_core::themes`). Path dependencies are eTalk's own
//! crates only; the other shared libraries come in through their release tags
//! (#EXP-1460), so a git consumer pinning the same tags gets ONE copy of each.

const FORBIDDEN: &[&str] = &[
    "api", "steer", "domain", "coding", "engine", "terminal", "ui", "app", "cli", "sync", "codemode",
    "updater", "theme",
];
const OWN: &[&str] = &["etalk"];
/// The other libs, each with the tag prefix it must be pinned through.
const TAGGED: &[(&str, &str)] = &[
    ("eui-core", "eui-v"),
    ("eui-gpui", "eui-v"),
    ("epilot", "epilot-v"),
    ("epilot-screen", "epilot-v"),
];

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
        if let Some((_, prefix)) = TAGGED.iter().find(|(lib, _)| *lib == name) {
            let pinned = format!("git = \"https://github.com/Niach/exponential\", tag = \"{prefix}");
            assert!(spec.contains(&pinned), "etalk-gpui must take `{name}` through its release tag ({pinned}…), never by path: a git consumer would get two copies");
        } else if spec.contains("path") {
            assert!(OWN.contains(&name), "etalk-gpui's path dependency `{name}` ({section}) is not one of eTalk's crates");
        }
    }
}
