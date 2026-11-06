//! #EDE-55: elocal is the local store every host shares (eDE now, the IDE
//! after the parity gate, the Engine later), so it links no UI toolkit, no
//! app crate and not the Engine. The path dependencies it may take: the
//! server-free client crates it answers for.

const FORBIDDEN: &[&str] = &[
    "steer", "coding", "engine", "terminal", "ui", "app", "cli", "voice", "talk", "screen",
    "computer", "codemode", "updater", "theme", "gpui", "gpui-component", "gpui_platform",
    "eengine", "eengine-desktop", "eengine-runner", "eengine-ffi", "ede-shell", "uicore-gpui",
];

/// The path dependencies elocal may take.
const PATH_ALLOWED: &[&str] = &["api", "sync", "domain", "ecore"];

#[test]
fn depends_on_no_app_crate_no_ui_toolkit_and_not_the_engine() {
    let manifest: toml::Table = include_str!("../Cargo.toml").parse().expect("Cargo.toml parses");
    for section in ["dependencies", "build-dependencies", "dev-dependencies"] {
        let Some(deps) = manifest.get(section).and_then(|v| v.as_table()) else { continue };
        for name in deps.keys() {
            let path_dep = deps[name].get("path").is_some() && !PATH_ALLOWED.contains(&name.as_str());
            assert!(
                !FORBIDDEN.contains(&name.as_str()) && !path_dep,
                "elocal must not depend on `{name}` ({section}): hosts stay above it, the Engine comes later"
            );
        }
    }
    let sync = manifest["dependencies"]["sync"].as_table().expect("sync is a table");
    assert_eq!(sync.get("default-features").and_then(|v| v.as_bool()), Some(false), "sync's gpui feature stays off");
}
