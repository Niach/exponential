//! The coding agents the launcher can spawn (EXP-201): Claude Code, OpenAI
//! Codex CLI, and pi (pi.dev). One closed enum with per-agent capability
//! metadata — argv composition ([`crate::argv`]), settings normalization
//! ([`crate::settings`]), the doctor ([`crate::doctor`]), and every agent
//! picker key off these methods instead of scattering `match`es.
//!
//! The model/effort value sets mirror `packages/domain-contract/contract.json`
//! (`codingAgent`/`codingModel`/`codingEffort`/`codexModel`/`codexEffort`/
//! `piModel`/`piThinking`) — the `coding` crate deliberately does not depend
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

/// pi `--model` patterns (fuzzy-resolved by pi itself: `fable` →
/// `claude-fable-5`); blank = omit (pi's own default model).
pub const PI_MODELS: [&str; 7] = [
    "fable",
    "opus",
    "sonnet",
    "gpt-5.6-sol",
    "gpt-5.6-terra",
    "gpt-5.6-luna",
    "grok-4.5",
];

/// pi `--thinking` levels; blank = omit.
pub const PI_THINKING: [&str; 7] = ["off", "minimal", "low", "medium", "high", "xhigh", "max"];

/// The coding agent CLIs the desktop can launch. `id()` strings are the wire
/// vocabulary (contract `codingAgent`, steer frames, settings.json).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CodingAgent {
    #[default]
    Claude,
    Codex,
    Pi,
}

impl CodingAgent {
    pub const ALL: [CodingAgent; 3] = [CodingAgent::Claude, CodingAgent::Codex, CodingAgent::Pi];

    /// The wire/settings id (`claude`/`codex`/`pi`).
    pub fn id(self) -> &'static str {
        match self {
            CodingAgent::Claude => "claude",
            CodingAgent::Codex => "codex",
            CodingAgent::Pi => "pi",
        }
    }

    /// Human label for pickers.
    pub fn label(self) -> &'static str {
        match self {
            CodingAgent::Claude => "Claude Code",
            CodingAgent::Codex => "Codex",
            CodingAgent::Pi => "pi",
        }
    }

    /// Lenient id parse (`"Codex "` → `Codex`); `None` for anything unknown —
    /// callers pick their own fallback (settings → Claude, remote starts →
    /// Claude so an option-less legacy frame behaves exactly as before).
    pub fn parse(raw: &str) -> Option<CodingAgent> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "claude" => Some(CodingAgent::Claude),
            "codex" => Some(CodingAgent::Codex),
            "pi" => Some(CodingAgent::Pi),
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

    /// A launch-into-plan mode — Claude Code only (`--permission-mode plan`).
    /// Codex HAS an interactive plan mode (`/plan`) but no flag to start in
    /// it; pi has none at all.
    pub fn supports_plan_mode(self) -> bool {
        matches!(self, CodingAgent::Claude)
    }

    /// Whether the "Skip permissions" (full-bypass) checkbox applies: pi has
    /// no permission system at all — it always runs unguarded, so there is
    /// nothing to toggle.
    pub fn supports_skip_permissions(self) -> bool {
        !matches!(self, CodingAgent::Pi)
    }

    /// Native conversation resume in a reused worktree cwd: `--continue`
    /// resumes the latest conversation FOR THE SPAWN CWD, so the
    /// one-issue-one-worktree layout is the whole key (no session id to
    /// capture). Claude documents the flag; pi has the same `-c`/`--continue`
    /// undocumented. Codex's `resume --last` is global-latest (not cwd-scoped
    /// — it would happily resume an unrelated conversation), so codex
    /// resumes via a fresh session seeded with the resume prompt instead
    /// ([`crate::prompt::render_resume_prompt`]).
    pub fn supports_native_resume(self) -> bool {
        !matches!(self, CodingAgent::Codex)
    }

    /// The closed model set for this agent (blank "CLI default" is an extra
    /// valid value for Codex and pi; Claude's `--model` is explicit-always).
    pub fn model_values(self) -> &'static [&'static str] {
        match self {
            CodingAgent::Claude => &MODEL_ALIASES,
            CodingAgent::Codex => &CODEX_MODELS,
            CodingAgent::Pi => &PI_MODELS,
        }
    }

    /// The closed effort/thinking set for this agent (blank = omit the flag).
    pub fn effort_values(self) -> &'static [&'static str] {
        match self {
            CodingAgent::Claude => &EFFORT_LEVELS,
            CodingAgent::Codex => &CODEX_EFFORTS,
            CodingAgent::Pi => &PI_THINKING,
        }
    }

    /// Whether a blank model (= omit the model flag, CLI default) is valid.
    /// Claude stays explicit-always (§7.7, locked 2026-07-03).
    pub fn allows_blank_model(self) -> bool {
        !matches!(self, CodingAgent::Claude)
    }

    /// The effort concept's UI label ("Thinking" is pi's own vocabulary).
    pub fn effort_label(self) -> &'static str {
        match self {
            CodingAgent::Claude => "Effort",
            CodingAgent::Codex => "Reasoning",
            CodingAgent::Pi => "Thinking",
        }
    }
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
    }

    #[test]
    fn serde_uses_the_lowercase_ids() {
        assert_eq!(serde_json::to_string(&CodingAgent::Pi).unwrap(), "\"pi\"");
        assert_eq!(
            serde_json::from_str::<CodingAgent>("\"codex\"").unwrap(),
            CodingAgent::Codex
        );
    }

    #[test]
    fn capability_matrix() {
        // Ultracode + native plan mode are Claude-only; the skip-permissions
        // checkbox exists everywhere but pi (pi is always unguarded).
        assert!(CodingAgent::Claude.supports_ultracode());
        assert!(CodingAgent::Claude.supports_plan_mode());
        assert!(CodingAgent::Claude.supports_skip_permissions());
        assert!(CodingAgent::Claude.supports_native_resume());
        assert!(!CodingAgent::Claude.allows_blank_model());
        for agent in [CodingAgent::Codex, CodingAgent::Pi] {
            assert!(!agent.supports_ultracode(), "{agent}");
            assert!(!agent.supports_plan_mode(), "{agent}");
            assert!(agent.allows_blank_model(), "{agent}");
        }
        // Native `--continue` resume: claude (documented) + pi (undocumented
        // but real); codex's `resume --last` is not cwd-scoped → prompt-based.
        assert!(CodingAgent::Pi.supports_native_resume());
        assert!(!CodingAgent::Codex.supports_native_resume());
        assert!(CodingAgent::Codex.supports_skip_permissions());
        assert!(!CodingAgent::Pi.supports_skip_permissions());
    }

    #[test]
    fn value_sets_are_wired_per_agent() {
        assert_eq!(CodingAgent::Claude.model_values(), &MODEL_ALIASES);
        assert_eq!(CodingAgent::Claude.effort_values(), &EFFORT_LEVELS);
        assert_eq!(CodingAgent::Codex.model_values(), &CODEX_MODELS);
        assert_eq!(CodingAgent::Codex.effort_values(), &CODEX_EFFORTS);
        assert_eq!(CodingAgent::Pi.model_values(), &PI_MODELS);
        assert_eq!(CodingAgent::Pi.effort_values(), &PI_THINKING);
    }
}
