// EXP-1196: the linked cua driver's ScreenCaptureKit bridge is Swift and
// finds the Swift runtime through @rpath. A transitive crate's link args
// never reach the final binary, so the rpath is baked into `exponential`
// here (the IDE's build.rs does the same).
fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
    }
}
