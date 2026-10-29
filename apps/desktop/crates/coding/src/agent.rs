//! The coding agents the launcher can spawn (EXP-201). EXP-1426: the enum and
//! its pure metadata moved to `exponential_agent_runtime::agent`; this module
//! re-exports them (so `coding::CodingAgent` and `crate::agent::*` keep
//! working) and keeps the one settings-backed method, the fresh-install
//! default model, as [`CodingAgentExt`].

pub use exponential_agent_runtime::agent::*;

/// What a [`CodingAgent`] answers from Exponential's settings / contract
/// defaults rather than from its own vocabulary.
pub trait CodingAgentExt {
    /// The model a launch of this agent falls back to when nothing valid
    /// was picked (contract `deviceAgentDefaults`).
    fn default_model(self) -> &'static str;
}

impl CodingAgentExt for CodingAgent {
    fn default_model(self) -> &'static str {
        match self {
            CodingAgent::Claude => crate::settings::DEFAULT_CLAUDE_MODEL,
            CodingAgent::Codex => crate::settings::DEFAULT_CODEX_MODEL,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{EFFORT_LEVELS, MODEL_ALIASES};

    /// The runtime spells the claude vocabulary itself (it depends on no
    /// app crate); it must stay the contract's.
    #[test]
    fn the_runtime_vocabulary_is_the_contracts() {
        assert_eq!(CLAUDE_MODEL_ALIASES, MODEL_ALIASES);
        assert_eq!(CLAUDE_MODEL_ALIASES, domain::contract::CODING_MODEL_VALUES);
        assert_eq!(CLAUDE_EFFORT_LEVELS, EFFORT_LEVELS);
    }

    #[test]
    fn value_sets_are_wired_per_agent() {
        assert_eq!(CodingAgent::Claude.model_values(), MODEL_ALIASES);
        assert_eq!(CodingAgent::Claude.effort_values(), &EFFORT_LEVELS);
    }

    #[test]
    fn each_agents_default_model_is_in_its_own_set() {
        // Every model is explicit: each agent's fallback is in its own set.
        for agent in CodingAgent::ALL {
            assert!(agent.model_values().contains(&agent.default_model()), "{agent}");
        }
    }
}
