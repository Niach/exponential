//! EXP-1426: eagent stays server-free. Every Exponential app crate and
//! every UI toolkit is refused as a dependency, so the coupling the extraction
//! removed cannot come back through a convenience import. The one path
//! dependency allowed is eCore (VAPP-264), the serde-only types every module
//! may link, as for emcp.

const FORBIDDEN: &[&str] = &[
    "api", "steer", "domain", "coding", "engine", "terminal", "ui", "app", "cli", "sync",
    "voice", "talk", "screen", "computer", "codemode", "updater", "theme", "gpui",
    "gpui-component", "gpui_platform", "observability", "sentry",
];

/// The path dependencies eagent may take: eCore only.
const PATH_ALLOWED: &[&str] = &["ecore"];

#[test]
fn depends_on_no_app_crate_and_no_ui_toolkit() {
    let manifest: toml::Table = include_str!("../Cargo.toml").parse().expect("Cargo.toml parses");
    for section in ["dependencies", "build-dependencies", "dev-dependencies"] {
        let Some(deps) = manifest.get(section).and_then(|v| v.as_table()) else { continue };
        for name in deps.keys() {
            let path_dep = deps[name].get("path").is_some() && !PATH_ALLOWED.contains(&name.as_str());
            assert!(
                !FORBIDDEN.contains(&name.as_str()) && !path_dep,
                "eagent must not depend on `{name}` ({section}): server and UI coupling \
                 stays in the hosts (README.md)"
            );
        }
    }
}
