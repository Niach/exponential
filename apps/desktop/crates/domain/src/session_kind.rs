//! EXP-1341: ONE session model ×4 — the Rust twin of
//! `packages/domain-contract/src/session-kind.ts` (iOS `SessionKind.swift`,
//! Android `domain/SessionKind.kt`), locked by `fixtures/session-kind.json`.
//! A thread and an agent run the same way; an agent is just `persistent`
//! (never ends: an exit or a refused launch reports idle, `sessions.end`
//! refuses it, its scratch dir is kept, and its device REVIVES it when it
//! comes back). NEW-2: the kind derives from `agent_id` (an agent's
//! conversation session); triggers live on the agent definition and the
//! SERVER fires them. Branch on these traits, never on the kind string.

use crate::contract::{SESSION_KIND_AGENT, SESSION_KIND_THREAD};

/// What a session kind does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SessionTraits {
    /// Never ends: exits idle, `sessions.end` refuses, revived by its device.
    pub persistent: bool,
    /// The kind whose definition carries triggers (the server fires them).
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

/// NEW-2: the kind of a session row: `agent` when it is an agent's
/// conversation session (`agent_id` set), else `thread`.
pub fn session_kind_of(agent_id: Option<&str>) -> &'static str {
    match agent_id {
        Some(id) if !id.is_empty() => SESSION_KIND_AGENT,
        _ => SESSION_KIND_THREAD,
    }
}

/// A persistent session (an agent's conversation): it idles instead of
/// ending. Keyed on `agent_id`.
pub fn is_persistent(agent_id: Option<&str>) -> bool {
    session_traits(Some(session_kind_of(agent_id))).persistent
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
        }
    }

    /// NEW-2: `kindOfCases`: the kind derives from `agentId`, never `kind`.
    #[test]
    fn every_kind_of_case_matches() {
        let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
        let cases = fixture["kindOfCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let agent_id = case["session"]["agentId"].as_str();
            let expected = case["expected"].as_str().unwrap();
            assert_eq!(session_kind_of(agent_id), expected, "{case}");
            assert_eq!(is_persistent(agent_id), expected == SESSION_KIND_AGENT, "{case}");
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
