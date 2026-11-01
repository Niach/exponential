//! #EXP-1509: the hub core stays a pure function. It depends on eCore,
//! serde and serde_json only (eCore its one path dependency), its sources do
//! no I/O, read no clock, spawn nothing and run no async code, and the crate
//! stays under a size budget.

const ALLOWED: &[&str] = &["ecore", "serde", "serde_json"];
const PATH_ALLOWED: &[&str] = &["ecore"];
const DEV_ALLOWED: &[&str] = &["toml"];
const FORBIDDEN_IN_SRC: &[&str] = &[
    "std::fs", "std::net", "std::io", "std::process", "std::thread", "std::env", "std::time", "SystemTime", "Instant",
    "async ", ".await", "unsafe ", "extern crate",
];
/// The source budget in bytes: raise it only with a reason in the PR.
const SRC_BUDGET_BYTES: usize = 128 * 1024;

const SOURCES: &[(&str, &str)] = &[
    ("lib.rs", include_str!("../src/lib.rs")),
    ("model.rs", include_str!("../src/model.rs")),
    ("copy.rs", include_str!("../src/copy.rs")),
    ("js.rs", include_str!("../src/js.rs")),
    ("reduce.rs", include_str!("../src/reduce.rs")),
];

#[test]
fn depends_on_ecore_and_serde_only() {
    let manifest: toml::Table = include_str!("../Cargo.toml").parse().expect("Cargo.toml parses");
    for (section, allowed) in [("dependencies", ALLOWED), ("build-dependencies", &[][..]), ("dev-dependencies", DEV_ALLOWED)] {
        let Some(deps) = manifest.get(section).and_then(|v| v.as_table()) else { continue };
        for (name, spec) in deps {
            assert!(
                allowed.contains(&name.as_str())
                    && (spec.get("path").is_none() || PATH_ALLOWED.contains(&name.as_str()))
                    && spec.get("git").is_none(),
                "eagent-hub-core must not depend on `{name}` ({section}): it is a pure reducer (README.md)"
            );
        }
    }
    assert!(manifest.get("features").is_none(), "eagent-hub-core has no features: one shape everywhere");
}

#[test]
fn sources_are_pure_and_small() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let on_disk = std::fs::read_dir(&dir).unwrap().count();
    assert_eq!(on_disk, SOURCES.len(), "a new source file in eagent-hub-core/src must be listed in SOURCES here");
    let mut total = 0;
    for (file, src) in SOURCES {
        total += src.len();
        for needle in FORBIDDEN_IN_SRC {
            assert!(!src.contains(needle), "eagent-hub-core/src/{file} uses `{needle}`: the core does no I/O and reads no clock");
        }
    }
    assert!(total <= SRC_BUDGET_BYTES, "eagent-hub-core/src is {total} bytes, over the {SRC_BUDGET_BYTES}-byte budget");
}
