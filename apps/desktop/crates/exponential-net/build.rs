// `wasm_browser` = wasm32-unknown-unknown (the browser), the same alias iroh uses.
fn main() {
    println!("cargo::rustc-check-cfg=cfg(wasm_browser)");
    let family = std::env::var("CARGO_CFG_TARGET_FAMILY").unwrap_or_default();
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if family.split(',').any(|f| f == "wasm") && os == "unknown" {
        println!("cargo::rustc-cfg=wasm_browser");
    }
}
