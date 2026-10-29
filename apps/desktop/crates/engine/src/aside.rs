//! EXP-1330 — a `/btw` aside run: a forked claude run of its PARENT's
//! conversation (`--resume <parent> --fork-session`, read-only tools, the
//! parent's worktree), launched by `coding::aside`. The engine half: the
//! fork + tool posture on the adapter ([`ASIDE_DISALLOWED_TOOLS`] and NO MCP
//! servers, [`ASIDE_MCP_CONFIG`]) and the one-shot end — after its FIRST
//! reply the aside ends (the reply itself posts like any turn's), unless a
//! person spoke in it meanwhile: then it is an ordinary session.
//!
//! NEW-8: nothing launches an aside any more (`/btw` is gone and the pickup
//! never routes one); a run record left by an older build still resumes
//! under this posture, never with the full tool set.

use std::sync::atomic::{AtomicBool, Ordering};

// EXP-1426 part 4c: the aside's tool posture is the claude adapter's, so
// it moved with it into the agent runtime.
pub use exponential_agent_runtime::engine::launch::{ASIDE_DISALLOWED_TOOLS, ASIDE_MCP_CONFIG};

/// The aside's one-shot state on its [`crate::host::SessionCtx`].
#[derive(Debug, Default)]
pub(crate) struct AsideState {
    /// A turn has started (the seed question went to the agent): the idle
    /// edges before it (the handshake) are no answer.
    turn_started: AtomicBool,
    /// A person's message was handed to the run.
    person_spoke: AtomicBool,
    /// The first turn ended: decided once, never again.
    settled: AtomicBool,
}

/// What the idle edge decided.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum AsideEnd {
    /// Not an answer edge (no turn yet, already settled).
    Nothing,
    /// A person spoke: the run carries on as an ordinary session.
    KeepGoing,
    /// End the run.
    End,
}

impl AsideState {
    pub(crate) fn turn_started(&self) {
        self.turn_started.store(true, Ordering::SeqCst);
    }

    pub(crate) fn person_spoke(&self) {
        self.person_spoke.store(true, Ordering::SeqCst);
    }

    /// The idle edge: the first turn's end settles the aside.
    pub(crate) fn turn_ended(&self) -> AsideEnd {
        if !self.turn_started.load(Ordering::SeqCst) || self.settled.swap(true, Ordering::SeqCst) {
            return AsideEnd::Nothing;
        }
        if self.person_spoke.load(Ordering::SeqCst) {
            return AsideEnd::KeepGoing;
        }
        AsideEnd::End
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_answer_ends_the_aside() {
        let aside = AsideState::default();
        assert_eq!(aside.turn_ended(), AsideEnd::Nothing, "the handshake's idle edge");
        aside.turn_started();
        assert_eq!(aside.turn_ended(), AsideEnd::End);
        assert_eq!(aside.turn_ended(), AsideEnd::Nothing, "settled once");
    }

    #[test]
    fn the_aside_mcp_config_names_no_server() {
        let parsed: serde_json::Value = serde_json::from_str(ASIDE_MCP_CONFIG).expect("JSON");
        assert_eq!(parsed, serde_json::json!({ "mcpServers": {} }));
        assert!(!ASIDE_MCP_CONFIG.contains("exponential"));
    }

    #[test]
    fn a_person_reply_keeps_the_aside_running() {
        let aside = AsideState::default();
        aside.turn_started();
        aside.person_spoke();
        assert_eq!(aside.turn_ended(), AsideEnd::KeepGoing);
    }
}
