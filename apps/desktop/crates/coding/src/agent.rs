//! The **AgentAdapter** (masterplan v5 "codex-support") — everything the
//! Start-coding launcher varies per coding agent, in ONE place:
//!
//! - the binary to spawn (+ its doctor [`Tool`]),
//! - the fixed argv and how the seed prompt rides it,
//! - whether the agent consumes the `.mcp.json` mechanism at all,
//! - which `PROMPT.md` flavor to seed,
//! - the env overlay (none for either agent today — the seam exists so a
//!   future agent needing one changes this file only).
//!
//! [`Agent::Claude`] is the DEFAULT and byte-for-byte the pre-adapter
//! behavior — same program resolution, same argv, same `.mcp.json`, same
//! prompt. [`Agent::Codex`] (OpenAI Codex CLI) is **EXPERIMENTAL** and
//! strictly opt-in via the `codingAgent` setting (§7.7 settings pane) —
//! anything other than the literal `codex` falls back to Claude, so a stale
//! or hand-edited settings file can never silently switch agents.
//!
//! A closed enum (like [`Tool`] / `TabKind`), not a trait object: the agent
//! set is small and known, and matching keeps every per-agent decision
//! greppable from one type.

use crate::doctor::Tool;
use crate::prompt::{render_prompt, render_prompt_no_mcp, SEED_LINE};
use crate::settings::Settings;

/// EXPERIMENTAL — the fixed Codex CLI argv, before the positional seed
/// prompt. Centralized as ONE constant because the exact invocation is NOT
/// verified against a pinned codex release; the assumptions:
///
/// - `codex [OPTIONS] [PROMPT]` — the interactive TUI takes its initial
///   prompt as a positional argument (the same delivery claude uses: bytes
///   typed into the PTY before a TUI enters raw mode get swallowed, so the
///   prompt must never ride stdin);
/// - `--full-auto` is codex's documented low-friction preset
///   (workspace-write sandbox, auto-executes commands) — the closest analog
///   to claude's `--dangerously-skip-permissions` that keeps codex's own
///   sandbox on;
/// - NO model flag: `Settings::claude_model` values (opus/sonnet/…) are
///   Claude model names — codex runs with the user's own configured default.
///
/// If a codex release changes its flag surface, fix it HERE — nothing else
/// in the launcher encodes codex specifics.
pub const CODEX_CODING_ARGS: &[&str] = &["--full-auto"];

/// Which coding agent "Start coding" drives. Default: [`Agent::Claude`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Agent {
    /// Claude Code — the default, byte-for-byte the pre-adapter launcher.
    #[default]
    Claude,
    /// OpenAI Codex CLI — EXPERIMENTAL, opt-in via the `codingAgent` setting.
    Codex,
}

impl Agent {
    /// Parse the `codingAgent` setting. Only the literal `codex`
    /// (case-insensitive, trimmed) opts into the experimental adapter;
    /// everything else — including blank and unknown values — is Claude.
    pub fn from_setting(raw: &str) -> Agent {
        if raw.trim().eq_ignore_ascii_case("codex") {
            Agent::Codex
        } else {
            Agent::Claude
        }
    }

    /// The active agent per the resolved [`Settings`].
    pub fn from_settings(settings: &Settings) -> Agent {
        Agent::from_setting(&settings.coding_agent)
    }

    /// Tab-title / log label (`claude · EXP-42`).
    pub fn label(self) -> &'static str {
        match self {
            Agent::Claude => "claude",
            Agent::Codex => "codex",
        }
    }

    /// The doctor [`Tool`] for this agent's binary (§7.7 — the doctor checks
    /// the agent that will actually be spawned, never falsely blocking a
    /// codex user on a missing `claude`).
    pub fn tool(self) -> Tool {
        match self {
            Agent::Claude => Tool::Claude,
            Agent::Codex => Tool::Codex,
        }
    }

    /// The program to spawn and doctor-check. Claude keeps its per-user
    /// install probing ([`Settings::resolved_claude_path`]); codex uses the
    /// configured path verbatim (the terminal layer's §6.12 login-PATH
    /// augmentation covers npm-global / Homebrew installs for the bare name).
    pub fn program(self, settings: &Settings) -> String {
        match self {
            Agent::Claude => settings.resolved_claude_path(),
            Agent::Codex => settings.codex_path.clone(),
        }
    }

    /// Whether the launcher writes `.mcp.json` (and therefore mints/reads the
    /// hidden `expu_` personal key). The `.mcp.json` project file is a
    /// CLAUDE mechanism — codex configures MCP servers globally in its own
    /// `~/.codex/config.toml`, which the launcher must not touch, so the
    /// codex adapter declares "no MCP": no key mint, no `.mcp.json`, and a
    /// prompt that does not reference the `exponential_*` MCP tools.
    pub fn uses_mcp(self) -> bool {
        matches!(self, Agent::Claude)
    }

    /// The full spawn argv, seed prompt positional-last (§7.1 step 7 — never
    /// PTY stdin). Claude: explicit `--model` ALWAYS (never the CLI default,
    /// which may be a scarcer model — §7.7, locked 2026-07-03) plus the skip
    /// flag. Codex: [`CODEX_CODING_ARGS`].
    pub fn coding_args(self, settings: &Settings) -> Vec<String> {
        match self {
            Agent::Claude => vec![
                "--model".to_string(),
                settings.claude_model.clone(),
                "--dangerously-skip-permissions".to_string(),
                SEED_LINE.to_string(),
            ],
            Agent::Codex => CODEX_CODING_ARGS
                .iter()
                .map(|arg| (*arg).to_string())
                .chain([SEED_LINE.to_string()])
                .collect(),
        }
    }

    /// Env overlay for the spawn — empty for both agents today (the §6.12
    /// PATH augmentation is the terminal layer's job, not ours).
    pub fn env(self) -> Vec<(String, String)> {
        Vec::new()
    }

    /// The `PROMPT.md` flavor: Claude gets the MCP-tool plan-first template
    /// verbatim; codex gets the no-MCP variant (commit + push only — without
    /// `/api/mcp` access it cannot call `exponential_pr_open`, so the PR is
    /// opened on GitHub and linked to the issue by the branch-name webhook).
    pub fn render_prompt(
        self,
        identifier: &str,
        title: &str,
        description: Option<&str>,
    ) -> String {
        match self {
            Agent::Claude => render_prompt(identifier, title, description),
            Agent::Codex => render_prompt_no_mcp(identifier, title, description),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Settings {
        Settings {
            claude_path: "/opt/homebrew/bin/claude".to_string(),
            claude_model: "opus".to_string(),
            ..Settings::default()
        }
    }

    #[test]
    fn only_the_literal_codex_opts_in() {
        assert_eq!(Agent::from_setting("codex"), Agent::Codex);
        assert_eq!(Agent::from_setting("  Codex "), Agent::Codex);
        // Everything else — default, blank, typos, unknown agents — is
        // Claude: the experimental adapter can never engage by accident.
        for raw in ["claude", "", "   ", "codexx", "gpt", "CODEX2"] {
            assert_eq!(Agent::from_setting(raw), Agent::Claude, "for {raw:?}");
        }
        assert_eq!(Agent::default(), Agent::Claude);
    }

    #[test]
    fn claude_args_are_byte_identical_to_the_pre_adapter_launcher() {
        // The exact pre-adapter argv: --model <model>
        // --dangerously-skip-permissions <seed>. Behavior-preserving is the
        // adapter's contract for the default agent.
        assert_eq!(
            Agent::Claude.coding_args(&settings()),
            vec![
                "--model".to_string(),
                "opus".to_string(),
                "--dangerously-skip-permissions".to_string(),
                SEED_LINE.to_string(),
            ]
        );
        assert_eq!(
            Agent::Claude.program(&settings()),
            "/opt/homebrew/bin/claude"
        );
        assert!(Agent::Claude.uses_mcp());
        assert_eq!(Agent::Claude.tool(), Tool::Claude);
        assert_eq!(Agent::Claude.label(), "claude");
    }

    #[test]
    fn codex_args_are_the_centralized_constant_plus_the_seed() {
        let args = Agent::Codex.coding_args(&settings());
        let (fixed, seed) = args.split_at(CODEX_CODING_ARGS.len());
        assert_eq!(fixed, CODEX_CODING_ARGS);
        assert_eq!(seed, [SEED_LINE.to_string()]);
        // No Claude-specific flags leak into the codex invocation — the
        // model names in settings are Claude's, and the skip flag is not a
        // codex flag.
        assert!(!args.iter().any(|a| a == "--model" || a == "opus"));
        assert!(!args.iter().any(|a| a == "--dangerously-skip-permissions"));
    }

    #[test]
    fn codex_declares_no_mcp_and_uses_the_configured_path_verbatim() {
        let mut settings = settings();
        settings.codex_path = "/usr/local/bin/codex".to_string();
        assert!(!Agent::Codex.uses_mcp());
        assert_eq!(Agent::Codex.program(&settings), "/usr/local/bin/codex");
        assert_eq!(Agent::Codex.tool(), Tool::Codex);
        assert_eq!(Agent::Codex.label(), "codex");
    }

    #[test]
    fn env_overlay_is_empty_for_both_agents() {
        assert!(Agent::Claude.env().is_empty());
        assert!(Agent::Codex.env().is_empty());
    }

    #[test]
    fn prompts_split_on_mcp_tool_references() {
        let claude = Agent::Claude.render_prompt("EXP-1", "T", Some("body"));
        let codex = Agent::Codex.render_prompt("EXP-1", "T", Some("body"));
        // Claude's prompt is the untouched MCP template.
        assert_eq!(claude, render_prompt("EXP-1", "T", Some("body")));
        assert!(claude.contains("`exponential_pr_open`"));
        // Codex has no /api/mcp access — its prompt must not name MCP tools.
        assert!(!codex.contains("exponential_pr_open"));
        assert!(!codex.contains("exponential_issues_update_status"));
        // Both stay plan-first, carry the issue context, and ban `gh`.
        for prompt in [&claude, &codex] {
            assert!(prompt.contains("**EXP-1: T**"));
            assert!(prompt.contains("WAIT for explicit go-ahead"));
            assert!(prompt.contains("Do not use `gh`"));
            assert!(prompt.contains("body"));
        }
    }
}
