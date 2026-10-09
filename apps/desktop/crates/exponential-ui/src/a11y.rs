//! Round 1 (docs/round-1-contract.md §6): the accessibility contract as data
//! (`catalog/a11y.json`, embedded): per component its role and keyboard
//! expectations, the rules every component keeps, and the commands a host
//! may send a surface (`surface::SurfaceCommand`). Mirrors `src/a11y.ts`.

use std::sync::LazyLock;

use indexmap::IndexMap;
use serde::Deserialize;

use crate::generated::catalog as g;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ComponentA11y {
    /// The platform role it maps to (ARIA names).
    pub role: String,
    /// Keyboard expectations, one line per key group.
    #[serde(default)]
    pub keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct A11yCommand {
    pub args: Vec<String>,
    pub description: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct A11yContract {
    pub rules: Vec<String>,
    pub commands: IndexMap<String, A11yCommand>,
    pub components: IndexMap<String, ComponentA11y>,
}

pub static A11Y: LazyLock<A11yContract> = LazyLock::new(|| serde_json::from_str(g::A11Y_JSON).expect("a11y.json"));

/// A component's entry, if it has one (extensions declare their own).
pub fn component_a11y(name: &str) -> Option<&'static ComponentA11y> {
    A11Y.components.get(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_core_component_has_a_role_and_the_four_commands_exist() {
        for name in g::COMPONENT_NAMES {
            assert!(component_a11y(name).is_some_and(|a| !a.role.is_empty()), "{name}");
        }
        assert_eq!(A11Y.commands.keys().map(String::as_str).collect::<Vec<_>>(), ["focus", "announce", "scrollIntoView", "scrollToIndex"]);
        assert!(!A11Y.rules.is_empty());
    }
}
