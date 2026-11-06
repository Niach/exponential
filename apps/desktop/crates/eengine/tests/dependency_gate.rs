//! VAPP-261: the Engine is consumed outside this repo (the softcoded app,
//! eDE, Spezl, exported runners) as a git dependency on its `eengine-v*` tag.
//! It links no Exponential app crate and takes no path dependency: other
//! modules come in through their release tags (the workspace patches them
//! to the local crates). Modules never depend on the Engine.

const FORBIDDEN: &[&str] = &[
    "api", "steer", "domain", "coding", "engine", "terminal", "ui", "app", "cli", "sync", "codemode", "updater", "theme", "gpui", "gpui-component", "gpui_platform",
];

#[test]
fn depends_on_no_app_crate_and_no_path() {
    let manifest: toml::Table = include_str!("../Cargo.toml").parse().expect("Cargo.toml parses");
    for section in ["dependencies", "build-dependencies", "dev-dependencies"] {
        let Some(deps) = manifest.get(section).and_then(|v| v.as_table()) else { continue };
        for (name, spec) in deps {
            assert!(!FORBIDDEN.contains(&name.as_str()), "eengine must not depend on `{name}` ({section}): app coupling stays in the host");
            assert!(spec.get("path").is_none(), "eengine's dependency `{name}` ({section}) is a path: a git consumer would not have it");
        }
    }
}

#[test]
fn host_config_has_no_telemetry_or_update_field() {
    let source = include_str!("../src/providers.rs");
    let start = source.find("pub struct HostConfig").expect("HostConfig");
    let body = &source[start..start + source[start..].find("\n}").expect("end of HostConfig")];
    for word in ["telemetry", "analytics", "crash", "update"] {
        assert!(!body.to_lowercase().contains(&format!("pub {word}")), "HostConfig grew a `{word}` field");
    }
}
