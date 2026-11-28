//! The genUI specimens (`packages/exponential-ui/fixtures/specimens.json`):
//! one nested tree per component + its forced interaction states, the same
//! entries `shots/exponential-ui-<name>/` pictures.

use indexmap::IndexMap;
use serde_json::Value;

/// The 20 components of #EDE-279's criterion 1, as specimen ids.
pub const TRIAL: [&str; 20] = [
    "exponential-ui-text",
    "exponential-ui-heading",
    "exponential-ui-button",
    "exponential-ui-badge",
    "exponential-ui-row",
    "exponential-ui-list",
    "exponential-ui-card",
    "exponential-ui-stack",
    "exponential-ui-grid",
    "exponential-ui-input",
    "exponential-ui-textarea",
    "exponential-ui-checkbox",
    "exponential-ui-switch",
    "exponential-ui-select",
    "exponential-ui-tabs",
    "exponential-ui-dialog",
    "exponential-ui-markdown",
    "exponential-ui-code-block",
    "exponential-ui-progress",
    "exponential-ui-icon",
];

pub struct Specimen {
    pub node: eui_core::types::NestedNode,
    pub states: IndexMap<String, Vec<String>>,
}

pub fn load(id: &str) -> Specimen {
    let raw = std::fs::read_to_string(crate::repo().join("packages/exponential-ui/fixtures/specimens.json")).expect("specimens.json");
    let all: Value = serde_json::from_str(&raw).expect("specimens parse");
    let e = all["specimens"].as_array().and_then(|s| s.iter().find(|e| e["id"] == id)).unwrap_or_else(|| panic!("no specimen {id}"));
    let node = serde_json::from_value(e["node"].clone()).expect("a nested node");
    let states = e.get("states").and_then(|s| serde_json::from_value(s.clone()).ok()).unwrap_or_default();
    Specimen { node, states }
}
