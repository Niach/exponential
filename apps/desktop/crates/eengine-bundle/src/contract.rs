//! `packages/eengine/fixtures/bundle.json`, compiled in.

use std::sync::LazyLock;

use serde_json::Value;

pub const BUNDLE_JSON: &str = include_str!("../../../../../packages/eengine/fixtures/bundle.json");

pub static BUNDLE: LazyLock<Value> = LazyLock::new(|| serde_json::from_str(BUNDLE_JSON).expect("bundle.json parses"));

/// The strings at a JSON pointer of the fixture.
pub fn strings(pointer: &str) -> Vec<String> {
    BUNDLE.pointer(pointer).and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default()
}
