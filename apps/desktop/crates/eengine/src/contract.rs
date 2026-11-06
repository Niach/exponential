//! The Engine's contract (`packages/eengine/fixtures`), compiled in. Each
//! fixture's `$comment` is the spec; [`crate::lifecycle`], [`crate::broker`],
//! [`crate::consent`], [`crate::providers`], [`crate::outbox`], [`crate::tools`] and
//! [`crate::pdf`] replay them
//! (`tests/fixtures.rs`), as the TS twin `@exp/eengine` does.

use std::sync::LazyLock;

use serde_json::Value;

pub const LIFECYCLE_JSON: &str = include_str!("../../../../../packages/eengine/fixtures/lifecycle.json");
pub const CONSENT_JSON: &str = include_str!("../../../../../packages/eengine/fixtures/consent.json");
pub const CAPABILITIES_JSON: &str = include_str!("../../../../../packages/eengine/fixtures/capabilities.json");
pub const PROVIDERS_JSON: &str = include_str!("../../../../../packages/eengine/fixtures/providers.json");
pub const OUTBOX_JSON: &str = include_str!("../../../../../packages/eengine/fixtures/outbox.json");
/// O1's outbox lives in `app-records.json` `outbox`; `outbox.json` references it.
pub const APP_RECORDS_JSON: &str = include_str!("../../../../../packages/domain-contract/fixtures/app-records.json");

fn parse(text: &str, name: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|e| panic!("{name} is JSON: {e}"))
}

pub static LIFECYCLE: LazyLock<Value> = LazyLock::new(|| parse(LIFECYCLE_JSON, "lifecycle.json"));
pub static CONSENT: LazyLock<Value> = LazyLock::new(|| parse(CONSENT_JSON, "consent.json"));
pub static CAPABILITIES: LazyLock<Value> = LazyLock::new(|| parse(CAPABILITIES_JSON, "capabilities.json"));
pub static PROVIDERS: LazyLock<Value> = LazyLock::new(|| parse(PROVIDERS_JSON, "providers.json"));
pub static OUTBOX: LazyLock<Value> = LazyLock::new(|| parse(OUTBOX_JSON, "outbox.json"));
pub static APP_RECORDS: LazyLock<Value> = LazyLock::new(|| parse(APP_RECORDS_JSON, "app-records.json"));

/// A list of strings at `pointer` in a fixture.
pub(crate) fn strings(fixture: &Value, pointer: &str) -> Vec<String> {
    fixture.pointer(pointer).and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default()
}
