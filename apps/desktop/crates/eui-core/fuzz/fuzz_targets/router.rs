//! The host router and package validation on arbitrary JSON: every message
//! routes to ops or a `send`, every package to a (possibly empty) issue
//! list. Never a panic.
#![no_main]

use eui_core::host::{validate_package, HostRouter};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(data) else { return };
    let mut router = HostRouter::new::<&str>(&[]);
    let _ = validate_package(&value, &router.supported_catalog_ids());
    let _ = router.install_package(&value);
    let messages: Vec<serde_json::Value> = match &value {
        serde_json::Value::Array(items) => items.clone(),
        other => vec![other.clone()],
    };
    for m in messages.iter().take(64) {
        for op in router.route(m) {
            assert!(op.get("op").and_then(|o| o.as_str()).is_some(), "every op names its kind");
        }
    }
});
