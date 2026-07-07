use std::env;

// Embed the white brand-logo icon into the Windows executable so exp-desktop.exe
// shows a real icon in Explorer, the taskbar and Alt-Tab instead of the blank
// default (EXP-20). embed-resource already rides in the tree as gpui's own
// build-dependency, and no-ops on every non-Windows target; the env guard keeps
// us from even touching the resource compiler off Windows. Nothing else in the
// packaging pipeline needs to change — the compiled resource lives inside the
// exe the existing Windows zip step ships.
fn main() {
    println!("cargo:rerun-if-changed=resources/app.rc");
    println!("cargo:rerun-if-changed=resources/app.ico");

    // CARGO_CFG_TARGET_OS reflects the *target* (build scripts compile for the
    // host, so a plain cfg!(windows) here would be wrong when cross-compiling).
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("resources/app.rc", embed_resource::NONE)
            .manifest_optional()
            .expect("failed to embed the Windows app icon (resources/app.rc)");
    }
}
