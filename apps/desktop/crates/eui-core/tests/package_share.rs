//! VAPP-241: `fixtures/package-share.json` replayed through the Rust core (the
//! TS reference replays it in `src/host/share.test.ts`).

mod support;

use eui_core::host::share::validate_package_shares;
use support::fixture;

#[test]
fn package_share_fixture() {
    let f = fixture("package-share.json");
    for c in f["cases"].as_array().unwrap() {
        let mut pkg = f["package"].as_object().unwrap().clone();
        pkg.insert("share".into(), c["share"].clone());
        assert_eq!(serde_json::to_value(validate_package_shares(&pkg)).unwrap(), c["issues"], "{}", c["name"].as_str().unwrap());
    }
}
