//! #EXP-1615: ecode is server-free and reusable (eDE, Spezl, Rerusted
//! #RUST-9): it links no Exponential app crate, and its one path dependency
//! is ecode-core (carried by the same git checkout).

const FORBIDDEN: &[&str] = &[
    "api", "steer", "domain", "coding", "engine", "terminal", "ui", "app", "cli", "sync", "codemode", "updater",
    "theme", "exp-http", "observability", "elocal", "epilot", "epilot-screen", "eui-core", "eui-gpui", "etalk",
    "eagent", "exponential-agent-runtime", "eengine", "emcp",
];

/// The path dependencies ecode may take.
const PATH_ALLOWED: &[&str] = &["ecode-core"];

#[test]
fn depends_on_no_app_crate() {
    let manifest: toml::Table = include_str!("../Cargo.toml").parse().expect("Cargo.toml parses");
    for section in ["dependencies", "build-dependencies", "dev-dependencies"] {
        let Some(deps) = manifest.get(section).and_then(|v| v.as_table()) else { continue };
        for (name, spec) in deps {
            assert!(!FORBIDDEN.contains(&name.as_str()), "ecode must not depend on `{name}` ({section}): app coupling stays in the host");
            assert!(
                PATH_ALLOWED.contains(&name.as_str()) || spec.get("path").is_none(),
                "ecode's dependency `{name}` ({section}) is a path: a git consumer would not have it"
            );
        }
    }
}

#[test]
fn gix_stays_minimal() {
    let manifest: toml::Table = include_str!("../Cargo.toml").parse().unwrap();
    let gix = &manifest["dependencies"]["gix"];
    assert_eq!(gix["default-features"].as_bool(), Some(false), "gix without its defaults (network, status, blob merge)");
    let features: Vec<&str> = gix["features"].as_array().unwrap().iter().filter_map(|f| f.as_str()).collect();
    assert_eq!(features, ["sha1", "revision"], "widen gix's features only with a size note in docs/third-party-licences.md");
}
