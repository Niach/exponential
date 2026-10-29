//! The coding agents a host can spawn (EXP-201): Claude Code and the
//! OpenAI Codex CLI. One closed enum with per-agent capability
//! metadata — argv composition, settings normalization, the doctor
//! ([`crate::detect`]), the login ([`crate::login`]) and every agent picker
//! key off these methods instead of scattering `match`es.
//!
//! EXP-1426: moved out of `coding` (`coding::CodingAgent` re-exports it).
//! The model/effort value sets mirror `packages/domain-contract/contract.json`
//! (`codingAgent`/`codingModel`/`codingEffort`/`codexModel`/`codexEffort`);
//! this crate depends on no app crate, so the claude vocabulary is spelled
//! here and `coding::agent` pins it to `domain::contract` in a test. The
//! fresh-install DEFAULT model is a product setting and stays in `coding`
//! (`coding::CodingAgentExt::default_model`).

use serde::{Deserialize, Serialize};

/// THE claude model vocabulary (contract `codingModel.values`): Claude
/// Code's family aliases, resolved by the CLI to the newest of each.
pub const CLAUDE_MODEL_ALIASES: &[&str] = &["fable", "opus", "sonnet", "haiku"];

/// The claude `--effort` levels the CLI accepts (blank = omit the flag).
pub const CLAUDE_EFFORT_LEVELS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

/// Codex `-m` slugs (mid-2026: the GPT-5.6 tiers — there is NO
/// `gpt-5.6-codex` variant). Every model is explicit: no blank "CLI
/// default".
pub const CODEX_MODELS: [&str; 3] = ["gpt-5.6-sol", "gpt-5.6-terra", "gpt-5.6-luna"];

/// Codex `model_reasoning_effort` levels (no `max`); blank = omit.
pub const CODEX_EFFORTS: [&str; 5] = ["minimal", "low", "medium", "high", "xhigh"];

/// The coding agent CLIs the desktop can launch. `id()` strings are the wire
/// vocabulary (contract `codingAgent`, steer frames, settings.json).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CodingAgent {
    #[default]
    Claude,
    Codex,
}

impl CodingAgent {
    pub const ALL: [CodingAgent; 2] = [CodingAgent::Claude, CodingAgent::Codex];

    /// The wire/settings id (`claude`/`codex`).
    pub fn id(self) -> &'static str {
        match self {
            CodingAgent::Claude => "claude",
            CodingAgent::Codex => "codex",
        }
    }

    /// Human label for pickers.
    pub fn label(self) -> &'static str {
        match self {
            CodingAgent::Claude => "Claude Code",
            CodingAgent::Codex => "Codex",
        }
    }

    /// The id a session row's `cli` carries for this agent. EXP-862 dropped
    /// external ACP agents, so every launch names a builtin and the wire id
    /// is always there (it was `Option` while `AgentKind::External` existed).
    pub fn wire_id(self) -> Option<&'static str> {
        Some(self.id())
    }

    /// Lenient id parse (`"Codex "` → `Codex`); `None` for anything unknown —
    /// callers pick their own fallback (settings → Claude, remote starts →
    /// Claude so an option-less legacy frame behaves exactly as before).
    pub fn parse(raw: &str) -> Option<CodingAgent> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "claude" => Some(CodingAgent::Claude),
            "codex" => Some(CodingAgent::Codex),
            _ => None,
        }
    }

    /// The default binary name (also the settings-path placeholder).
    pub fn default_binary(self) -> &'static str {
        self.id()
    }

    /// Dynamic workflows (`--effort ultracode`) — Claude Code only.
    pub fn supports_ultracode(self) -> bool {
        matches!(self, CodingAgent::Claude)
    }

    /// EXP-981: a pinnable model for the agent's SUBAGENTS — Claude Code
    /// only (`CLAUDE_CODE_SUBAGENT_MODEL`).
    pub fn supports_subagent_model(self) -> bool {
        matches!(self, CodingAgent::Claude)
    }

    /// EXP-1280: plan mode is Exponential's (the runbook's `## Plan` + the
    /// server's `pr_open` gate), never the CLI's, so EVERY agent supports it.
    /// Always `true`; kept only so `crates/ui` compiles until it drops it.
    pub fn supports_plan_mode(self) -> bool {
        true
    }

    /// The closed model set for this agent. Every launch passes one of
    /// these explicitly (no blank "CLI default" for either agent).
    pub fn model_values(self) -> &'static [&'static str] {
        match self {
            CodingAgent::Claude => CLAUDE_MODEL_ALIASES,
            CodingAgent::Codex => &CODEX_MODELS,
        }
    }

    /// The closed effort/thinking set for this agent (blank = omit the flag).
    pub fn effort_values(self) -> &'static [&'static str] {
        match self {
            CodingAgent::Claude => &CLAUDE_EFFORT_LEVELS,
            CodingAgent::Codex => &CODEX_EFFORTS,
        }
    }

    /// The effort concept's UI label.
    pub fn effort_label(self) -> &'static str {
        match self {
            CodingAgent::Claude => "Effort",
            CodingAgent::Codex => "Reasoning",
        }
    }
}

/// EXP-877 — the alias a RESOLVED claude model id belongs to
/// (`claude-opus-4-6[1m]` → `opus`), or `None` for anything outside
/// [`CLAUDE_MODEL_ALIASES`].
///
/// The CLI reports whatever id it actually resolved (`system/init`, the
/// `PostModelSwitch` hook, a refusal fallback), and that id is not what the
/// picker offers — the picker's vocabulary is the contract's three aliases.
/// Folding the report back onto an alias is what lets the republished
/// `config_state` confirm a `/model <alias>` switch instead of showing a
/// value no menu entry matches. The `[1m]` suffix is a context window, not a
/// tier, so it folds onto the same alias.
pub fn claude_model_alias(reported: &str) -> Option<&'static str> {
    let lowered = reported.trim().to_ascii_lowercase();
    let bare = lowered.split('[').next().unwrap_or_default().trim();
    let family = match bare.strip_prefix("claude-") {
        Some(rest) => rest.split('-').next().unwrap_or_default(),
        None => bare,
    };
    CLAUDE_MODEL_ALIASES.iter().copied().find(|alias| *alias == family)
}

impl std::fmt::Display for CodingAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.id())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_and_parse_is_lenient() {
        for agent in CodingAgent::ALL {
            assert_eq!(CodingAgent::parse(agent.id()), Some(agent));
        }
        assert_eq!(CodingAgent::parse(" Codex "), Some(CodingAgent::Codex));
        assert_eq!(CodingAgent::parse("CLAUDE"), Some(CodingAgent::Claude));
        assert_eq!(CodingAgent::parse("cursor"), None);
        assert_eq!(CodingAgent::parse(""), None);
        // EXP-862: every launch names a builtin, so the wire id is always
        // there (external ACP agents, which had none, are gone).
        for agent in CodingAgent::ALL {
            assert_eq!(agent.wire_id(), Some(agent.id()));
        }
    }

    #[test]
    fn serde_uses_the_lowercase_ids() {
        assert_eq!(
            serde_json::to_string(&CodingAgent::Claude).unwrap(),
            "\"claude\""
        );
        // EXP-849: pi is gone from the vocabulary; a legacy id no longer parses.
        assert!(CodingAgent::parse("pi").is_none());
        assert_eq!(
            serde_json::from_str::<CodingAgent>("\"codex\"").unwrap(),
            CodingAgent::Codex
        );
    }

    #[test]
    fn capability_matrix() {
        // Ultracode is Claude-only (EXP-849 dropped pi); EXP-1280: plan
        // mode is Exponential's, every agent's.
        // EXP-690: there is no skip-permissions capability any more — every
        // run bypasses.
        assert!(CodingAgent::Claude.supports_ultracode());
        // EXP-981: only claude pins a subagent model.
        assert!(CodingAgent::Claude.supports_subagent_model());
        assert!(!CodingAgent::Codex.supports_subagent_model());
        for agent in [CodingAgent::Codex] {
            assert!(!agent.supports_ultracode(), "{agent}");
        }
    }

    /// EXP-877: a resolved id folds back onto the alias the picker offers —
    /// the 1M suffix is a window, not a tier — and anything else is None so
    /// the composer falls back to printing the raw id.
    #[test]
    fn resolved_claude_ids_fold_back_onto_their_alias() {
        for (reported, alias) in [
            ("claude-fable-5-1", "fable"),
            ("claude-opus-4-6", "opus"),
            ("claude-opus-4-6[1m]", "opus"),
            ("claude-sonnet-4-5", "sonnet"),
            // The aliases themselves round-trip, whatever the casing.
            ("fable", "fable"),
            ("opus", "opus"),
            (" Sonnet ", "sonnet"),
            ("haiku", "haiku"),
            ("claude-haiku-4-5", "haiku"),
        ] {
            assert_eq!(claude_model_alias(reported), Some(alias), "{reported}");
        }
        for unknown in ["", "claude-3-5-haiku", "gpt-5.6-sol", "claude-"] {
            assert_eq!(claude_model_alias(unknown), None, "{unknown}");
        }
    }

    #[test]
    fn value_sets_are_wired_per_agent() {
        assert_eq!(CodingAgent::Claude.model_values(), CLAUDE_MODEL_ALIASES);
        assert_eq!(CodingAgent::Claude.effort_values(), &CLAUDE_EFFORT_LEVELS);
        assert_eq!(CodingAgent::Codex.model_values(), &CODEX_MODELS);
        assert_eq!(CodingAgent::Codex.effort_values(), &CODEX_EFFORTS);
    }
}
