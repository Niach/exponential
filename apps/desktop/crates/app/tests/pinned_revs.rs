//! EXP-519: the zed/gpui pin lives in Cargo.lock, not Cargo.toml.
//!
//! Upstream gpui-component FLOATS its zed git deps (their Cargo.lock is their
//! only pin), and cargo refuses both a rev-pinned manifest next to a floating
//! transitive (two gpui package ids splitting every gpui type) and a same-URL
//! `[patch]`. So the workspace manifest floats too and the committed Cargo.lock
//! carries the one true rev — which means a stray `cargo update` (without
//! `--precise`) would silently move the entire gpui stack to zed main HEAD.
//! This test pins the lockfile itself: bump the constants here together with
//! the revs (see the workspace Cargo.toml comment for the update recipe).

/// The zed rev gpui-component's own Cargo.lock locks at [`GPUI_COMPONENT_REV`].
const ZED_REV: &str = "cc053a4a6fa2fd0e8793201ed9099466af1be0b1";
/// The gpui-component rev the workspace Cargo.toml pins.
const GPUI_COMPONENT_REV: &str = "da4f93696dc2b2b4d91bcc42412b9053a3d24de8";

#[test]
fn lockfile_pins_the_expected_git_revs() {
    let lock = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../Cargo.lock"
    ))
    .expect("workspace Cargo.lock");

    let mut zed_sources = 0usize;
    let mut component_sources = 0usize;
    for line in lock.lines() {
        let Some(source) = line.strip_prefix("source = \"git+") else {
            continue;
        };
        if let Some(rest) = source.strip_prefix("https://github.com/zed-industries/zed") {
            zed_sources += 1;
            let expected = format!("#{ZED_REV}\"");
            assert!(
                rest == expected,
                "zed git source drifted off the pin: {line}\n\
                 expected every zed source to end in {expected}"
            );
        } else if let Some(rest) =
            source.strip_prefix("https://github.com/longbridge/gpui-component")
        {
            component_sources += 1;
            let expected = format!("?rev={GPUI_COMPONENT_REV}#{GPUI_COMPONENT_REV}\"");
            assert!(
                rest == expected,
                "gpui-component git source drifted off the pin: {line}"
            );
        }
    }

    // A refactor that renamed/moved the sources entirely must fail loudly too.
    assert!(zed_sources > 0, "no zed git sources found in Cargo.lock");
    assert!(
        component_sources > 0,
        "no gpui-component git sources found in Cargo.lock"
    );
}
