//! The coding agents the launcher can spawn (EXP-201): Claude Code and the
//! OpenAI Codex CLI. One closed enum with per-agent capability
//! metadata — argv composition ([`crate::argv`]), settings normalization
//! ([`crate::settings`]), the doctor ([`crate::doctor`]), and every agent
//! picker key off these methods instead of scattering `match`es.
//!
//! The model/effort value sets mirror `packages/domain-contract/contract.json`
//! (`codingAgent`/`codingModel`/`codingEffort`/`codexModel`/`codexEffort`)
//! — the `coding` crate deliberately does not depend
//! on `domain`, so the parity check lives in `ui::coding_selects` tests like
//! the pre-existing model/effort ones.

use serde::{Deserialize, Serialize};

use crate::settings::{EFFORT_LEVELS, MODEL_ALIASES};

/// Codex `-m` slugs (mid-2026: the GPT-5.6 tiers — there is NO
/// `gpt-5.6-codex` variant). Blank ("CLI default", omit `-m`) is a valid
/// extra value everywhere these are consumed.
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

    /// The id `codingSessions.start` carries for this run. EXP-862 dropped
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

    /// A launch-into-plan mode. Claude Code natively (`--permission-mode
    /// plan`). Codex HAS an interactive plan mode (`/plan`) but no flag to
    /// start in it.
    pub fn supports_plan_mode(self) -> bool {
        matches!(self, CodingAgent::Claude)
    }

    /// The closed model set for this agent (blank "CLI default" is an extra
    /// valid value for Codex; Claude's `--model` is explicit-always).
    pub fn model_values(self) -> &'static [&'static str] {
        match self {
            CodingAgent::Claude => &MODEL_ALIASES,
            CodingAgent::Codex => &CODEX_MODELS,
        }
    }

    /// The closed effort/thinking set for this agent (blank = omit the flag).
    pub fn effort_values(self) -> &'static [&'static str] {
        match self {
            CodingAgent::Claude => &EFFORT_LEVELS,
            CodingAgent::Codex => &CODEX_EFFORTS,
        }
    }

    /// Whether a blank model (= omit the model flag, CLI default) is valid.
    /// Claude stays explicit-always (§7.7, locked 2026-07-03).
    pub fn allows_blank_model(self) -> bool {
        !matches!(self, CodingAgent::Claude)
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
/// [`MODEL_ALIASES`].
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
    MODEL_ALIASES.iter().copied().find(|alias| *alias == family)
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
        // Ultracode and launch-into-plan are Claude-only (EXP-849 dropped pi).
        // EXP-690: there is no skip-permissions capability any more — every
        // run bypasses.
        assert!(CodingAgent::Claude.supports_ultracode());
        assert!(CodingAgent::Claude.supports_plan_mode());
        assert!(!CodingAgent::Claude.allows_blank_model());
        for agent in [CodingAgent::Codex] {
            assert!(!agent.supports_ultracode(), "{agent}");
            assert!(agent.allows_blank_model(), "{agent}");
        }
        assert!(!CodingAgent::Codex.supports_plan_mode());
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
        ] {
            assert_eq!(claude_model_alias(reported), Some(alias), "{reported}");
        }
        for unknown in ["", "claude-3-5-haiku", "gpt-5.6-sol", "haiku", "claude-"] {
            assert_eq!(claude_model_alias(unknown), None, "{unknown}");
        }
    }

    #[test]
    fn value_sets_are_wired_per_agent() {
        assert_eq!(CodingAgent::Claude.model_values(), &MODEL_ALIASES);
        assert_eq!(CodingAgent::Claude.effort_values(), &EFFORT_LEVELS);
        assert_eq!(CodingAgent::Codex.model_values(), &CODEX_MODELS);
        assert_eq!(CodingAgent::Codex.effort_values(), &CODEX_EFFORTS);
    }
}
