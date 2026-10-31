//! EXP-1492: emcp is consumed outside this repo (spezl) as a git dependency
//! on its `emcp-v*` tag and is the base the other libs build their MCP
//! endpoints on, so it links no Exponential app crate and no other lib:
//! the hosts depend on emcp, never the reverse.

const FORBIDDEN: &[&str] = &[
    "api", "steer", "domain", "coding", "engine", "terminal", "ui", "app", "cli", "sync", "codemode", "updater",
    "theme", "epilot", "epilot-screen", "eui-core", "eui-gpui", "etalk", "exponential-agent-runtime",
];

#[test]
fn depends_on_no_app_crate_and_no_path() {
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
        assert!(!FORBIDDEN.contains(&name), "emcp must not depend on `{name}` ({section}): app coupling stays in the host");
        assert!(!spec.contains("path"), "emcp's dependency `{name}` ({section}) is a path: a git consumer would not have it");
    }
}
