//! #EXP-1504: eCore stays small and plain. It depends on serde and
//! serde_json only (never a path dependency, so never another module), its
//! sources do no I/O, spawn nothing and run no async code, and the whole
//! crate stays under a size budget.

const ALLOWED: &[&str] = &["serde", "serde_json"];
const DEV_ALLOWED: &[&str] = &["toml"];
const FORBIDDEN_IN_SRC: &[&str] = &[
    "std::fs", "std::net", "std::io", "std::process", "std::thread", "std::env", "async ", ".await", "unsafe ",
    "extern crate",
];
/// The source budget in bytes: raise it only with a reason in the PR.
const SRC_BUDGET_BYTES: usize = 32 * 1024;

const SOURCES: &[(&str, &str)] = &[
    ("lib.rs", include_str!("../src/lib.rs")),
    ("contract.generated.rs", include_str!("../src/contract.generated.rs")),
];

#[test]
fn depends_on_serde_only() {
    let manifest: toml::Table = include_str!("../Cargo.toml").parse().expect("Cargo.toml parses");
    for (section, allowed) in [("dependencies", ALLOWED), ("build-dependencies", &[][..]), ("dev-dependencies", DEV_ALLOWED)] {
        let Some(deps) = manifest.get(section).and_then(|v| v.as_table()) else { continue };
        for (name, spec) in deps {
            assert!(
                allowed.contains(&name.as_str()) && spec.get("path").is_none() && spec.get("git").is_none(),
                "ecore must not depend on `{name}` ({section}): eCore is types only (README.md)"
            );
        }
    }
    assert!(manifest.get("features").is_none(), "ecore has no features: one shape everywhere");
}

#[test]
fn sources_are_pure_and_small() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let on_disk = std::fs::read_dir(&dir).unwrap().count();
    assert_eq!(on_disk, SOURCES.len(), "a new source file in ecore/src must be listed in SOURCES here");
    let mut total = 0;
    for (file, src) in SOURCES {
        total += src.len();
        for needle in FORBIDDEN_IN_SRC {
            assert!(!src.contains(needle), "ecore/src/{file} uses `{needle}`: eCore does no I/O and runs nothing");
        }
    }
    assert!(total <= SRC_BUDGET_BYTES, "ecore/src is {total} bytes, over the {SRC_BUDGET_BYTES}-byte budget");
}
