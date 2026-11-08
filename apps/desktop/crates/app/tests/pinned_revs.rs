//! EXP-519: the gpui stack is pinned exactly, and the lock must agree.
//!
//! gpui comes from the `gpui-pre-*` crates.io snapshots of zed's crates at the
//! exact snapshot gpui-kit's `gpui-component` release builds against (gpui-kit
//! pins `=` versions; any snapshot may change gpui's API). Every `gpui-pre*`
//! package in Cargo.lock sits on [`GPUI_PRE`], every gpui-kit package on
//! [`GPUI_COMPONENT`], and neither upstream may come back as a git source
//! (a second gpui package id splits every gpui type). Bump the constants here
//! together with the workspace Cargo.toml (see its comment for the recipe).

/// The gpui-pre snapshot gpui-component [`GPUI_COMPONENT`] pins.
const GPUI_PRE: &str = "0.3.11";
/// The gpui-component release the workspace Cargo.toml pins.
const GPUI_COMPONENT: &str = "0.9.0";
/// gpui-kit's packages that release together with gpui-component.
const GPUI_KIT_PACKAGES: &[&str] = &[
    "gpui-component",
    "gpui-base",
    "gpui-kit-assets",
    "gpui-component-macros",
];

#[test]
fn lockfile_pins_the_expected_gpui_versions() {
    let lock = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../Cargo.lock"
    ))
    .expect("workspace Cargo.lock");

    let mut gpui_pre = 0usize;
    let mut component = 0usize;
    for package in lock.split("[[package]]").skip(1) {
        let field = |key: &str| {
            package.lines().find_map(|line| {
                line.strip_prefix(&format!("{key} = \""))
                    .and_then(|rest| rest.strip_suffix('"'))
            })
        };
        let (Some(name), Some(version)) = (field("name"), field("version")) else {
            continue;
        };
        if let Some(source) = field("source") {
            assert!(
                !source.contains("github.com/zed-industries/zed")
                    && !source.contains("github.com/longbridge/gpui-"),
                "{name} comes from a git source ({source}): gpui is pinned to crates.io"
            );
        }
        // The snapshot's reqwest fork keeps reqwest's own version line.
        if (name == "gpui-pre" || name.starts_with("gpui-pre-")) && name != "gpui-pre-reqwest" {
            gpui_pre += 1;
            assert_eq!(version, GPUI_PRE, "{name} drifted off the gpui-pre pin");
        } else if GPUI_KIT_PACKAGES.contains(&name) {
            component += 1;
            assert_eq!(version, GPUI_COMPONENT, "{name} drifted off the gpui-component pin");
        }
    }

    // A refactor that renamed/moved the packages entirely must fail loudly too.
    assert!(gpui_pre > 0, "no gpui-pre packages found in Cargo.lock");
    assert!(
        component >= GPUI_KIT_PACKAGES.len(),
        "not every gpui-kit package is in Cargo.lock"
    );
}
