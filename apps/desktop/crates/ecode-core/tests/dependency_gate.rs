//! #EXP-1613: eCode's core stays pure and reusable (Rerusted, #RUST-9). It
//! depends on serde and serde_json only (no path or git dependency, nothing
//! of Exponential), its sources do no I/O, read no clock, spawn nothing, run
//! no async or unsafe code, and the crate stays under a size budget.

const ALLOWED: &[&str] = &["serde", "serde_json"];
const DEV_ALLOWED: &[&str] = &["toml"];
const FORBIDDEN_IN_SRC: &[&str] = &[
    "std::fs", "std::net", "std::io", "std::process", "std::thread", "std::env", "std::time", "SystemTime", "Instant",
    "async ", ".await", "unsafe ", "unsafe{", "extern crate",
];
/// The source budget in bytes: raise it only with a reason in the PR.
const SRC_BUDGET_BYTES: usize = 160 * 1024;

const SOURCES: &[(&str, &str)] = &[
    ("lib.rs", include_str!("../src/lib.rs")),
    ("contract.generated.rs", include_str!("../src/contract.generated.rs")),
    ("js.rs", include_str!("../src/js.rs")),
    ("checks.rs", include_str!("../src/checks.rs")),
    ("forge.rs", include_str!("../src/forge.rs")),
    ("pipeline.rs", include_str!("../src/pipeline.rs")),
    ("remote.rs", include_str!("../src/remote.rs")),
    ("runner.rs", include_str!("../src/runner.rs")),
    ("train.rs", include_str!("../src/train.rs")),
    ("workflow.rs", include_str!("../src/workflow.rs")),
];

#[test]
fn depends_on_serde_only() {
    let manifest: toml::Table = include_str!("../Cargo.toml").parse().expect("Cargo.toml parses");
    for (section, allowed) in [("dependencies", ALLOWED), ("build-dependencies", &[][..]), ("dev-dependencies", DEV_ALLOWED)] {
        let Some(deps) = manifest.get(section).and_then(|v| v.as_table()) else { continue };
        for (name, spec) in deps {
            assert!(
                allowed.contains(&name.as_str()) && spec.get("path").is_none() && spec.get("git").is_none(),
                "ecode-core must not depend on `{name}` ({section}): it is a pure, reusable core (README.md)"
            );
        }
    }
    assert!(manifest.get("features").is_none(), "ecode-core has no features: one shape everywhere");
}

#[test]
fn sources_are_pure_and_small() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut on_disk: Vec<String> =
        std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    on_disk.sort();
    let mut listed: Vec<String> = SOURCES.iter().map(|(file, _)| file.to_string()).collect();
    listed.sort();
    assert_eq!(on_disk, listed, "every source file in ecode-core/src must be listed in SOURCES here");
    let mut total = 0;
    for (file, src) in SOURCES {
        total += src.len();
        for needle in FORBIDDEN_IN_SRC {
            assert!(!src.contains(needle), "ecode-core/src/{file} uses `{needle}`: the core does no I/O and reads no clock");
        }
    }
    assert!(total <= SRC_BUDGET_BYTES, "ecode-core/src is {total} bytes, over the {SRC_BUDGET_BYTES}-byte budget");
}
