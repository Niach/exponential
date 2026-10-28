//! `fixtures/screen-memory.json`, compiled in: every number and string the
//! crate uses comes from here, and the tests replay its cases.

use std::sync::OnceLock;

use serde_json::Value;

const FIXTURE: &str = include_str!("../../../../../packages/domain-contract/fixtures/screen-memory.json");

pub(crate) fn fixture() -> &'static Value {
    static FIXTURE_JSON: OnceLock<Value> = OnceLock::new();
    FIXTURE_JSON.get_or_init(|| serde_json::from_str(FIXTURE).expect("screen-memory.json parses"))
}

fn number(path: &[&str]) -> u64 {
    let mut value = fixture();
    for key in path {
        value = &value[*key];
    }
    value.as_u64().unwrap_or_else(|| panic!("screen-memory.json {} is a number", path.join(".")))
}

pub(crate) fn copy(key: &str) -> &'static str {
    fixture()["copy"][key].as_str().unwrap_or_else(|| panic!("screen-memory.json copy.{key}"))
}

pub(crate) fn interval_ms() -> u64 {
    number(&["capture", "intervalMs"])
}

pub(crate) fn focus_poll_ms() -> u64 {
    number(&["capture", "focusPollMs"])
}

pub(crate) fn max_distance() -> u32 {
    number(&["dedupe", "maxDistance"]) as u32
}

pub(crate) fn webp_quality() -> f32 {
    number(&["store", "webpQuality"]) as f32
}

pub(crate) fn retention_ms() -> i64 {
    number(&["retentionMs"]) as i64
}

pub(crate) fn tool() -> &'static Value {
    &fixture()["tool"]
}

pub(crate) fn tool_name() -> &'static str {
    tool()["name"].as_str().expect("screen-memory.json tool.name")
}

pub(crate) fn default_span_ms() -> i64 {
    number(&["tool", "defaultSpanSeconds"]) as i64 * 1000
}

pub(crate) fn max_frames() -> usize {
    number(&["tool", "maxFrames"]) as usize
}

pub(crate) fn max_edge() -> u32 {
    number(&["tool", "maxEdge"]) as u32
}

/// `tool.minCrop`: a smaller focused window keeps the whole frame.
pub(crate) fn min_crop() -> u32 {
    number(&["tool", "minCrop"]) as u32
}

/// `denylist.apps` / `denylist.titles`, lowercased.
pub(crate) fn denylist(key: &str) -> &'static [String] {
    static APPS: OnceLock<Vec<String>> = OnceLock::new();
    static TITLES: OnceLock<Vec<String>> = OnceLock::new();
    let cell = if key == "apps" { &APPS } else { &TITLES };
    cell.get_or_init(|| {
        fixture()["denylist"][key]
            .as_array()
            .unwrap_or_else(|| panic!("screen-memory.json denylist.{key}"))
            .iter()
            .map(|entry| entry.as_str().expect("a denylist entry").to_lowercase())
            .collect()
    })
}
