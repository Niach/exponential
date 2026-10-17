//! EXP-1341: ONE session model ×4 — the Rust twin of
//! `packages/domain-contract/src/session-kind.ts` (iOS `SessionKind.swift`,
//! Android `domain/SessionKind.kt`), locked by `fixtures/session-kind.json`.
//! A thread and an agent run the same way; an agent is just `persistent`
//! (never ends: an exit or a refused launch reports idle, `sessions.end`
//! refuses it, its scratch dir is kept, and its device REVIVES it when it
//! comes back) and carries `triggers`. Branch on these traits, never on the
//! kind string.

use crate::contract::{SESSION_KIND_AGENT, SESSION_KIND_THREAD};

/// What a session kind does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SessionTraits {
    /// Never ends: exits idle, `sessions.end` refuses, revived by its device.
    pub persistent: bool,
    /// Carries triggers (schedules / events) its device fires as messages.
    pub triggers: bool,
}

/// `kinds.thread`.
pub const THREAD: SessionTraits = SessionTraits { persistent: false, triggers: false };
/// `kinds.agent`.
pub const AGENT: SessionTraits = SessionTraits { persistent: true, triggers: true };

/// The traits of `kind`; an unknown or absent kind = a thread.
pub fn session_traits(kind: Option<&str>) -> SessionTraits {
    match kind {
        Some(SESSION_KIND_AGENT) => AGENT,
        Some(SESSION_KIND_THREAD) | Some(_) | None => THREAD,
    }
}

/// A persistent session (an agent root): it idles instead of ending.
pub fn is_persistent(kind: Option<&str>) -> bool {
    session_traits(kind).persistent
}

/// Carries triggers its device fires.
pub fn has_triggers(kind: Option<&str>) -> bool {
    session_traits(kind).triggers
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/session-kind.json");

    fn traits(value: &Value) -> SessionTraits {
        SessionTraits {
            persistent: value["persistent"].as_bool().unwrap(),
            triggers: value["triggers"].as_bool().unwrap(),
        }
    }

    #[test]
    fn every_fixture_case_matches() {
        let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let kind = case["kind"].as_str();
            assert_eq!(session_traits(kind), traits(&case["expected"]), "{kind:?}");
            assert_eq!(is_persistent(kind), case["expected"]["persistent"].as_bool().unwrap());
        }
    }

    #[test]
    fn the_constants_match_the_fixture() {
        let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
        let kinds = fixture["kinds"].as_object().unwrap();
        assert_eq!(kinds.len(), 2);
        assert_eq!(traits(&kinds[SESSION_KIND_THREAD]), THREAD);
        assert_eq!(traits(&kinds[SESSION_KIND_AGENT]), AGENT);
    }
}
