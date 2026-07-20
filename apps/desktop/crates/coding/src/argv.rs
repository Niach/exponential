//! Spawn argv assembly for coding sessions — the ONE place the agent CLI
//! flags are composed (EXP-201: `claude`, `codex`, or `pi`). The model flag
//! is explicit for Claude (never the user's CLI default — §7.7, locked
//! 2026-07-03; codex/pi allow blank = their own default), the seed prompt
//! rides argv positional-last (bytes typed into the PTY before the TUI
//! enters raw mode get swallowed, so the prompt must never ride stdin), and
//! the permission posture is per-agent:
//!
//! - **claude** — guarded AUTO mode by default (`--permission-mode auto`,
//!   verified v2.1.215), plan mode when gated, or the classic
//!   `--dangerously-skip-permissions` when the skip checkbox is on. The
//!   doctor's [`crate::doctor::MIN_CLAUDE_VERSION`] gate guarantees every
//!   claude flag here.
//! - **codex** — the TUI's own "Auto" preset (`--sandbox workspace-write
//!   --ask-for-approval on-request`, plus the network override so `git push`
//!   works inside the sandbox), or `--dangerously-bypass-approvals-and-sandbox`
//!   when skipping. `--full-auto` is deprecated and never used.
//! - **pi** — no permission system exists; no flags either way.

use crate::agent::CodingAgent;
use crate::mcp_json::MCP_JSON_FILE;
use crate::pi_bridge::PI_BRIDGE_FILE;
use crate::settings::Settings;

/// The env var carrying the raw `expu_` key for codex + pi sessions (EXP-201)
/// — those agents get the MCP credential via the spawn environment instead of
/// a worktree file: codex reads it through `bearer_token_env_var`, the pi
/// bridge reads it directly. Never on argv (ps-visible), never on disk.
pub const MCP_TOKEN_ENV: &str = "EXP_MCP_TOKEN";

/// The env var carrying the `/api/mcp` URL for the pi bridge.
pub const MCP_URL_ENV: &str = "EXP_MCP_URL";

/// The MCP wiring of every CLAUDE coding argv: the launcher-written worktree
/// [`MCP_JSON_FILE`] (`.exp-mcp.json`) rides `--mcp-config` (resolved against
/// the spawn cwd = the worktree) and connects trusted, prompt-free.
///
/// The flags alone are NOT what suppresses claude's "New MCP server found in
/// this project" dialog — EXP-83 assumed they were, and the dialog kept
/// firing (EXP-98). Claude's interactive startup runs an unconditional
/// approval scan of the project-scope config (the literal `.mcp.json` in the
/// cwd) that ignores both `--mcp-config`/`--strict-mcp-config`; those
/// flags only gate which servers CONNECT. The actual fix is the file NAME:
/// `.exp-mcp.json` is invisible to that scan (see [`crate::mcp_json`]).
/// `--strict-mcp-config` still matters — it keeps any repo-carried MCP
/// config from connecting in an unattended session.
pub fn mcp_config_args() -> Vec<String> {
    vec![
        "--mcp-config".into(),
        MCP_JSON_FILE.into(),
        "--strict-mcp-config".into(),
    ]
}

/// The permission tail of every CLAUDE coding argv (EXP-201 posture):
///
/// - Plan mode wins the STARTING mode: `--permission-mode plan` +
///   `--allow-dangerously-skip-permissions` (the skip flag cannot ride NEXT
///   TO a starting mode — both select one; the allow flag instead puts
///   `bypassPermissions` in the Shift+Tab cycle, one keypress to full-auto
///   after the plan is approved).
/// - Skip checkbox on: the classic `--dangerously-skip-permissions`
///   (≡ `--permission-mode bypassPermissions`).
/// - Otherwise: guarded AUTO mode (`--permission-mode auto` — a classifier
///   approves routine actions and prompts on risky ones; v2.1.215) with the
///   bypass reachable via Shift+Tab.
pub fn permission_args(plan_mode: bool, skip_permissions: bool) -> Vec<String> {
    if plan_mode {
        vec![
            "--permission-mode".into(),
            "plan".into(),
            "--allow-dangerously-skip-permissions".into(),
        ]
    } else if skip_permissions {
        vec!["--dangerously-skip-permissions".into()]
    } else {
        vec![
            "--permission-mode".into(),
            "auto".into(),
            "--allow-dangerously-skip-permissions".into(),
        ]
    }
}

/// Per-agent MCP wiring, resolved by the launcher (step 4) and consumed by
/// [`session_args`]:
///
/// - Claude: the worktree `.exp-mcp.json` file (rides [`mcp_config_args`]).
/// - Codex: `-c mcp_servers.*` CLI overrides pointing at `url`, with the
///   bearer token read from [`MCP_TOKEN_ENV`] in the spawn env — the key
///   never lands on disk or argv for codex.
/// - Pi: the launcher-written [`PI_BRIDGE_FILE`] extension (rides `-e`); the
///   bridge reads [`MCP_URL_ENV`] + [`MCP_TOKEN_ENV`] from the env.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentMcp {
    ClaudeFile,
    CodexOverrides { url: String },
    PiExtension,
}

/// The Start-coding dialog's choices — ONE shape for both run modes (a
/// single-issue session and a multi-issue batch session differ only in their
/// settings DEFAULTS, not in the flags they can carry).
#[derive(Clone, Debug)]
pub struct LaunchOptions {
    /// Which agent CLI to spawn (EXP-201).
    pub agent: CodingAgent,
    /// Model choice within the agent's closed set. Blank = omit the model
    /// flag (valid for codex/pi only; claude is explicit-always).
    pub model: String,
    /// Effort/reasoning/thinking level; blank = omit the flag. Ignored while
    /// ultracode is on (ultracode IS the effort level — `--effort ultracode`).
    pub effort: String,
    /// Dynamic workflows (`--effort ultracode`, CLI ≥2.1.203 —
    /// model-independent, no opus pin). Claude-only; wins over `effort`.
    pub ultracode: bool,
    /// Native plan mode (`--permission-mode plan`). Claude-only.
    pub plan_mode: bool,
    /// Full permission bypass (claude `--dangerously-skip-permissions` /
    /// codex `--dangerously-bypass-approvals-and-sandbox`). OFF = the
    /// agent's guarded auto mode. Inert for pi (always unguarded).
    pub skip_permissions: bool,
}

impl LaunchOptions {
    /// The settings-default options for a SINGLE-ISSUE run (the local
    /// Start-coding dialog's seed values).
    pub fn issue_defaults(settings: &Settings) -> Self {
        let agent = settings.default_agent;
        Self {
            agent,
            model: settings.model_for(agent).to_string(),
            effort: settings.effort_for(agent).to_string(),
            ultracode: settings.issue_ultracode && agent.supports_ultracode(),
            plan_mode: settings.issue_plan_mode && agent.supports_plan_mode(),
            skip_permissions: settings.issue_skip_permissions
                && agent.supports_skip_permissions(),
        }
    }

    /// The settings-default options for a BATCH (multi-issue) run.
    pub fn batch_defaults(settings: &Settings) -> Self {
        let agent = settings.default_agent;
        Self {
            agent,
            model: settings.model_for(agent).to_string(),
            effort: settings.effort_for(agent).to_string(),
            ultracode: settings.batch_ultracode && agent.supports_ultracode(),
            plan_mode: settings.batch_plan_mode && agent.supports_plan_mode(),
            skip_permissions: settings.batch_skip_permissions
                && agent.supports_skip_permissions(),
        }
    }

    /// The shared RELAY-start normalization (EXP-149/EXP-201): the remote
    /// client's Start-coding choices normalized against the AGENT's closed
    /// sets, over settings defaults for anything it didn't send.
    ///
    /// - Absent/unknown `agent` → **Claude** (an option-less legacy frame
    ///   must behave exactly as before EXP-201 — never the local default
    ///   agent, or an old phone's claude vocabulary could land on a codex
    ///   launch).
    /// - `effort: Some("")` is an explicit "CLI default" and beats a
    ///   non-blank settings effort; same for a blank codex/pi model.
    /// - Absent ultracode/skip fall to the run mode's settings defaults;
    ///   plan mode defaults OFF when absent (F7 — an option-less start must
    ///   never park an unattended desktop at the plan-approval TUI); a
    ///   remote client sending `plan_mode: true` opted in knowingly.
    /// - Capabilities mask everything: a non-claude agent can never carry
    ///   ultracode/plan, pi never carries skip.
    #[allow(clippy::too_many_arguments)]
    fn remote(
        settings: &Settings,
        default_ultracode: bool,
        default_skip: bool,
        agent: Option<&str>,
        model: Option<&str>,
        effort: Option<&str>,
        ultracode: Option<bool>,
        plan_mode: Option<bool>,
        skip_permissions: Option<bool>,
    ) -> Self {
        use crate::settings::normalize_choice;
        let agent = agent
            .and_then(CodingAgent::parse)
            .unwrap_or(CodingAgent::Claude);
        let model_fallback = if agent == settings.default_agent {
            settings.model_for(agent)
        } else if agent.allows_blank_model() {
            ""
        } else {
            crate::settings::DEFAULT_CLAUDE_MODEL
        };
        let model = match model {
            Some(model) => {
                let normalized = normalize_choice(model, agent.model_values(), "");
                if normalized.is_empty() && !agent.allows_blank_model() {
                    model_fallback.to_string()
                } else {
                    normalized
                }
            }
            None => model_fallback.to_string(),
        };
        let effort = match effort {
            Some(effort) => normalize_choice(effort, agent.effort_values(), ""),
            None if agent == settings.default_agent => settings.effort_for(agent).to_string(),
            None => String::new(),
        };
        Self {
            agent,
            model,
            effort,
            ultracode: ultracode.unwrap_or(default_ultracode) && agent.supports_ultracode(),
            plan_mode: plan_mode.unwrap_or(false) && agent.supports_plan_mode(),
            skip_permissions: skip_permissions.unwrap_or(default_skip)
                && agent.supports_skip_permissions(),
        }
    }

    /// RELAY-triggered SINGLE-ISSUE start (EXP-149): absent ultracode/skip
    /// fall to the ISSUE settings defaults.
    #[allow(clippy::too_many_arguments)]
    pub fn remote_issue(
        settings: &Settings,
        agent: Option<&str>,
        model: Option<&str>,
        effort: Option<&str>,
        ultracode: Option<bool>,
        plan_mode: Option<bool>,
        skip_permissions: Option<bool>,
    ) -> Self {
        Self::remote(
            settings,
            settings.issue_ultracode,
            settings.issue_skip_permissions,
            agent,
            model,
            effort,
            ultracode,
            plan_mode,
            skip_permissions,
        )
    }

    /// RELAY-triggered BATCH start: absent ultracode/skip fall to the BATCH
    /// settings defaults; plan mode stays OFF unless the remote client
    /// explicitly opted in (F7 — an unattended desktop must never park at a
    /// plan-approval menu).
    #[allow(clippy::too_many_arguments)]
    pub fn remote_batch(
        settings: &Settings,
        agent: Option<&str>,
        model: Option<&str>,
        effort: Option<&str>,
        ultracode: Option<bool>,
        plan_mode: Option<bool>,
        skip_permissions: Option<bool>,
    ) -> Self {
        Self::remote(
            settings,
            settings.batch_ultracode,
            settings.batch_skip_permissions,
            agent,
            model,
            effort,
            ultracode,
            plan_mode,
            skip_permissions,
        )
    }
}

/// The coding-session argv for `opts.agent`, prompt positional-LAST always:
///
/// - claude: `--model <m> [--effort ultracode|<e>] <mcp_config_args>
///   <permission_args> <positional>`
/// - codex: `[-m <m>] [-c model_reasoning_effort=<e>] <mcp -c overrides>
///   <sandbox/approval flags> <positional>`
/// - pi: `[--model <m>] [--thinking <t>] -e ./<bridge> <positional>`
pub fn session_args(opts: &LaunchOptions, mcp: &AgentMcp, positional: &str) -> Vec<String> {
    let trimmed_model = opts.model.trim();
    let trimmed_effort = opts.effort.trim();
    let mut args: Vec<String> = Vec::new();
    match opts.agent {
        CodingAgent::Claude => {
            args.push("--model".into());
            args.push(if trimmed_model.is_empty() {
                // Claude is explicit-always; a blank here is a caller bug —
                // degrade to the spec default rather than the user's CLI one.
                crate::settings::DEFAULT_CLAUDE_MODEL.to_string()
            } else {
                trimmed_model.to_string()
            });
            let effort = if opts.ultracode {
                Some("ultracode".to_string())
            } else {
                (!trimmed_effort.is_empty()).then(|| trimmed_effort.to_string())
            };
            if let Some(effort) = effort {
                args.push("--effort".into());
                args.push(effort);
            }
            args.extend(mcp_config_args());
            args.extend(permission_args(opts.plan_mode, opts.skip_permissions));
        }
        CodingAgent::Codex => {
            if !trimmed_model.is_empty() {
                args.push("-m".into());
                args.push(trimmed_model.to_string());
            }
            if !trimmed_effort.is_empty() {
                args.push("-c".into());
                args.push(format!("model_reasoning_effort=\"{trimmed_effort}\""));
            }
            if let AgentMcp::CodexOverrides { url } = mcp {
                // Streamable-HTTP MCP via -c overrides (codex has no
                // --mcp-config flag); the token rides MCP_TOKEN_ENV in the
                // spawn env — never argv, never disk. The rmcp toggle is
                // defensive for older builds where HTTP MCP was feature-gated
                // (harmless on current ones).
                args.push("-c".into());
                args.push(format!("mcp_servers.exponential.url=\"{url}\""));
                args.push("-c".into());
                args.push(format!(
                    "mcp_servers.exponential.bearer_token_env_var=\"{MCP_TOKEN_ENV}\""
                ));
                args.push("-c".into());
                args.push("experimental_use_rmcp_client=true".into());
            }
            if opts.skip_permissions {
                args.push("--dangerously-bypass-approvals-and-sandbox".into());
            } else {
                // The TUI's own "Auto" preset, made explicit (--full-auto is
                // deprecated), plus the network override: workspace-write
                // blocks network by default and the session must `git push`.
                args.push("--sandbox".into());
                args.push("workspace-write".into());
                args.push("--ask-for-approval".into());
                args.push("on-request".into());
                args.push("-c".into());
                args.push("sandbox_workspace_write.network_access=true".into());
            }
        }
        CodingAgent::Pi => {
            if !trimmed_model.is_empty() {
                args.push("--model".into());
                args.push(trimmed_model.to_string());
            }
            if !trimmed_effort.is_empty() {
                args.push("--thinking".into());
                args.push(trimmed_effort.to_string());
            }
            // The MCP bridge extension (pi has no native MCP). `-e` loads it
            // independent of pi's project-trust prompt; never pass
            // -a/--approve (it would auto-trust repo-carried extensions).
            args.push("-e".into());
            args.push(format!("./{PI_BRIDGE_FILE}"));
        }
    }
    args.push(positional.to_string());
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claude_opts() -> LaunchOptions {
        LaunchOptions {
            agent: CodingAgent::Claude,
            model: "fable".to_string(),
            effort: "".to_string(),
            ultracode: false,
            plan_mode: false,
            skip_permissions: false,
        }
    }

    #[test]
    fn permission_args_split_on_plan_and_skip() {
        // Gated: plan START mode + bypass ALLOWED (Shift+Tab reachable) but
        // never `--dangerously-skip-permissions` itself — that flag IS a
        // starting mode and would erase the gate.
        assert_eq!(
            permission_args(true, false),
            vec![
                "--permission-mode".to_string(),
                "plan".to_string(),
                "--allow-dangerously-skip-permissions".to_string(),
            ]
        );
        // Plan wins the starting mode even with skip checked.
        assert_eq!(permission_args(true, true), permission_args(true, false));
        assert_eq!(
            permission_args(false, true),
            vec!["--dangerously-skip-permissions".to_string()]
        );
        // EXP-201: the new default — guarded auto mode, bypass reachable.
        assert_eq!(
            permission_args(false, false),
            vec![
                "--permission-mode".to_string(),
                "auto".to_string(),
                "--allow-dangerously-skip-permissions".to_string(),
            ]
        );
    }

    #[test]
    fn mcp_config_args_pass_the_worktree_file_explicitly_and_strictly() {
        // Explicit --mcp-config on the non-discoverable name (EXP-98 — the
        // dialog scan only sees `.mcp.json`) + strict mode (repo-carried MCP
        // config never connects).
        assert_eq!(
            mcp_config_args(),
            vec![
                "--mcp-config".to_string(),
                ".exp-mcp.json".to_string(),
                "--strict-mcp-config".to_string(),
            ]
        );
    }

    #[test]
    fn claude_session_args_matrix() {
        // Plan mode ON (the issue default), no effort, no ultracode.
        let opts = LaunchOptions {
            plan_mode: true,
            ..claude_opts()
        };
        assert_eq!(
            session_args(&opts, &AgentMcp::ClaudeFile, "do the thing"),
            vec![
                "--model",
                "fable",
                "--mcp-config",
                ".exp-mcp.json",
                "--strict-mcp-config",
                "--permission-mode",
                "plan",
                "--allow-dangerously-skip-permissions",
                "do the thing",
            ]
        );

        // Plan OFF + skip ON + effort set: the classic skip flag, effort
        // before the MCP + permission tail, positional last.
        let opts = LaunchOptions {
            model: "opus".to_string(),
            effort: "xhigh".to_string(),
            skip_permissions: true,
            ..claude_opts()
        };
        assert_eq!(
            session_args(&opts, &AgentMcp::ClaudeFile, "prompt"),
            vec![
                "--model",
                "opus",
                "--effort",
                "xhigh",
                "--mcp-config",
                ".exp-mcp.json",
                "--strict-mcp-config",
                "--dangerously-skip-permissions",
                "prompt",
            ]
        );

        // Plan OFF + skip OFF (EXP-201 default): guarded auto mode.
        let args = session_args(&claude_opts(), &AgentMcp::ClaudeFile, "p");
        assert_eq!(
            args[args.len() - 4..],
            [
                "--permission-mode".to_string(),
                "auto".to_string(),
                "--allow-dangerously-skip-permissions".to_string(),
                "p".to_string(),
            ]
        );

        // Ultracode WINS over a set effort (`--effort ultracode`,
        // model-independent — the chosen model stays).
        let opts = LaunchOptions {
            effort: "high".to_string(),
            ultracode: true,
            ..claude_opts()
        };
        assert_eq!(
            session_args(&opts, &AgentMcp::ClaudeFile, "seed")[..4],
            [
                "--model".to_string(),
                "fable".to_string(),
                "--effort".to_string(),
                "ultracode".to_string(),
            ]
        );

        // Whitespace effort + no ultracode → no --effort at all; never
        // an --agents flag.
        let opts = LaunchOptions {
            model: "sonnet".to_string(),
            effort: "  ".to_string(),
            ..claude_opts()
        };
        let args = session_args(&opts, &AgentMcp::ClaudeFile, "p");
        assert!(!args.iter().any(|arg| arg == "--effort"));
        assert!(!args.iter().any(|arg| arg == "--agents"));
        assert_eq!(args.last().map(String::as_str), Some("p"));
    }

    #[test]
    fn codex_session_args_matrix() {
        let mcp = AgentMcp::CodexOverrides {
            url: "https://app.exponential.at/api/mcp".to_string(),
        };
        // Auto mode (skip OFF): explicit workspace-write + on-request + the
        // network override; MCP via -c overrides with the env-var token.
        let opts = LaunchOptions {
            agent: CodingAgent::Codex,
            model: "gpt-5.6-sol".to_string(),
            effort: "high".to_string(),
            ultracode: false,
            plan_mode: false,
            skip_permissions: false,
        };
        assert_eq!(
            session_args(&opts, &mcp, "prompt"),
            vec![
                "-m",
                "gpt-5.6-sol",
                "-c",
                "model_reasoning_effort=\"high\"",
                "-c",
                "mcp_servers.exponential.url=\"https://app.exponential.at/api/mcp\"",
                "-c",
                "mcp_servers.exponential.bearer_token_env_var=\"EXP_MCP_TOKEN\"",
                "-c",
                "experimental_use_rmcp_client=true",
                "--sandbox",
                "workspace-write",
                "--ask-for-approval",
                "on-request",
                "-c",
                "sandbox_workspace_write.network_access=true",
                "prompt",
            ]
        );

        // Skip ON: the yolo flag replaces the sandbox/approval tail; blank
        // model + effort omit their flags entirely (codex's own defaults).
        let opts = LaunchOptions {
            agent: CodingAgent::Codex,
            model: "".to_string(),
            effort: "".to_string(),
            ultracode: false,
            plan_mode: false,
            skip_permissions: true,
        };
        let args = session_args(&opts, &mcp, "prompt");
        assert_eq!(
            args,
            vec![
                "-c",
                "mcp_servers.exponential.url=\"https://app.exponential.at/api/mcp\"",
                "-c",
                "mcp_servers.exponential.bearer_token_env_var=\"EXP_MCP_TOKEN\"",
                "-c",
                "experimental_use_rmcp_client=true",
                "--dangerously-bypass-approvals-and-sandbox",
                "prompt",
            ]
        );
        // The raw key must NEVER ride argv (ps-visible) — only the env-var
        // NAME appears.
        assert!(!args.iter().any(|arg| arg.contains("expu_")));
    }

    #[test]
    fn pi_session_args_matrix() {
        // pi: model/thinking flags + the bridge extension; no permission
        // flags exist (pi is YOLO by design), prompt positional-last.
        let opts = LaunchOptions {
            agent: CodingAgent::Pi,
            model: "grok-4.5".to_string(),
            effort: "high".to_string(),
            ultracode: false,
            plan_mode: false,
            skip_permissions: false,
        };
        assert_eq!(
            session_args(&opts, &AgentMcp::PiExtension, "prompt"),
            vec![
                "--model",
                "grok-4.5",
                "--thinking",
                "high",
                "-e",
                "./.exp-pi-mcp.ts",
                "prompt",
            ]
        );

        // Blank model + thinking: only the bridge + prompt; never -a (that
        // would auto-trust repo-carried extensions).
        let opts = LaunchOptions {
            agent: CodingAgent::Pi,
            model: "".to_string(),
            effort: "".to_string(),
            ultracode: false,
            plan_mode: false,
            skip_permissions: true, // inert for pi
        };
        let args = session_args(&opts, &AgentMcp::PiExtension, "p");
        assert_eq!(args, vec!["-e", "./.exp-pi-mcp.ts", "p"]);
        assert!(!args.iter().any(|arg| arg == "-a" || arg == "--approve"));
    }

    #[test]
    fn defaults_map_model_effort_and_per_mode_toggles() {
        let mut settings = Settings::default();
        settings.claude_model = "sonnet".to_string();
        settings.claude_effort = "high".to_string();
        settings.issue_plan_mode = false;
        let opts = LaunchOptions::issue_defaults(&settings);
        assert_eq!(opts.agent, CodingAgent::Claude);
        assert_eq!(opts.model, "sonnet");
        assert_eq!(opts.effort, "high");
        assert!(!opts.plan_mode);
        assert!(!opts.ultracode);
        assert!(!opts.skip_permissions);

        // The stock defaults: issue runs — plan mode ON, ultracode OFF;
        // batch runs — ultracode ON, plan mode OFF. Same model/effort pair,
        // skip OFF everywhere (guarded auto is the default posture).
        let issue = LaunchOptions::issue_defaults(&Settings::default());
        assert_eq!(issue.model, "fable");
        assert_eq!(issue.effort, "");
        assert!(issue.plan_mode);
        assert!(!issue.ultracode);
        assert!(!issue.skip_permissions);

        let batch = LaunchOptions::batch_defaults(&Settings::default());
        assert_eq!(batch.model, "fable");
        assert_eq!(batch.effort, "");
        assert!(!batch.plan_mode);
        assert!(batch.ultracode);
        assert!(!batch.skip_permissions);
    }

    /// EXP-201: a non-claude default agent seeds ITS model/effort pair and
    /// masks the claude-only toggles even when their settings are on.
    #[test]
    fn defaults_follow_the_default_agent_and_mask_capabilities() {
        let mut settings = Settings::default();
        settings.default_agent = CodingAgent::Codex;
        settings.codex_model = "gpt-5.6-terra".to_string();
        settings.codex_effort = "xhigh".to_string();
        settings.issue_ultracode = true; // claude-only — must mask
        settings.issue_plan_mode = true; // claude-only — must mask
        settings.issue_skip_permissions = true; // codex supports skip
        let opts = LaunchOptions::issue_defaults(&settings);
        assert_eq!(opts.agent, CodingAgent::Codex);
        assert_eq!(opts.model, "gpt-5.6-terra");
        assert_eq!(opts.effort, "xhigh");
        assert!(!opts.ultracode);
        assert!(!opts.plan_mode);
        assert!(opts.skip_permissions);

        settings.default_agent = CodingAgent::Pi;
        settings.pi_model = "grok-4.5".to_string();
        settings.pi_thinking = "max".to_string();
        let opts = LaunchOptions::batch_defaults(&settings);
        assert_eq!(opts.agent, CodingAgent::Pi);
        assert_eq!(opts.model, "grok-4.5");
        assert_eq!(opts.effort, "max");
        assert!(!opts.skip_permissions, "pi has no permission system");
    }

    #[test]
    fn remote_issue_all_absent_matches_pre_options_relay_behavior() {
        // The F7 baseline: settings model/effort/ultracode, plan mode OFF —
        // exactly what an option-less relay start ran before EXP-149.
        let mut settings = Settings::default();
        settings.claude_model = "opus".to_string();
        settings.claude_effort = "high".to_string();
        settings.issue_ultracode = true;
        settings.issue_plan_mode = true; // must NOT leak into a remote start
        let opts = LaunchOptions::remote_issue(&settings, None, None, None, None, None, None);
        assert_eq!(opts.agent, CodingAgent::Claude);
        assert_eq!(opts.model, "opus");
        assert_eq!(opts.effort, "high");
        assert!(opts.ultracode);
        assert!(!opts.plan_mode);
        assert!(!opts.skip_permissions);
    }

    #[test]
    fn remote_issue_applies_and_normalizes_sent_options() {
        let mut settings = Settings::default();
        settings.claude_effort = "high".to_string();

        let opts = LaunchOptions::remote_issue(
            &settings,
            Some("Claude"),
            Some("Sonnet"),
            Some("max"),
            Some(false),
            Some(true),
            Some(true),
        );
        assert_eq!(opts.agent, CodingAgent::Claude);
        assert_eq!(opts.model, "sonnet", "case-normalized");
        assert_eq!(opts.effort, "max");
        assert!(!opts.ultracode);
        assert!(opts.plan_mode, "explicit remote opt-in");
        assert!(opts.skip_permissions, "explicit remote opt-in");

        // Bogus model falls back to the settings model, never to a crash or
        // a raw pass-through to the CLI argv.
        let opts =
            LaunchOptions::remote_issue(&settings, None, Some("gpt-6"), None, None, None, None);
        assert_eq!(opts.model, "fable");

        // Explicit blank effort = "CLI default" and beats the settings value.
        let opts = LaunchOptions::remote_issue(&settings, None, None, Some(""), None, None, None);
        assert_eq!(opts.effort, "");
        // Bogus effort also degrades to blank (omit --effort).
        let opts =
            LaunchOptions::remote_issue(&settings, None, None, Some("extreme"), None, None, None);
        assert_eq!(opts.effort, "");
    }

    /// EXP-201: a remote CODEX start normalizes against codex sets and can
    /// never carry the claude-only toggles; an unknown agent degrades to
    /// claude (legacy behavior, never the local default agent).
    #[test]
    fn remote_normalizes_per_agent_and_masks_capabilities() {
        let mut settings = Settings::default();
        settings.issue_ultracode = true; // must not leak onto codex

        let opts = LaunchOptions::remote_issue(
            &settings,
            Some("codex"),
            Some("gpt-5.6-luna"),
            Some("minimal"),
            Some(true), // ultracode — claude-only, must mask
            Some(true), // plan — claude-only, must mask
            None,
        );
        assert_eq!(opts.agent, CodingAgent::Codex);
        assert_eq!(opts.model, "gpt-5.6-luna");
        assert_eq!(opts.effort, "minimal");
        assert!(!opts.ultracode);
        assert!(!opts.plan_mode);
        assert!(!opts.skip_permissions);

        // A claude model on a codex start is bogus → blank (codex default).
        let opts = LaunchOptions::remote_issue(
            &settings,
            Some("codex"),
            Some("fable"),
            None,
            None,
            None,
            None,
        );
        assert_eq!(opts.model, "");

        // pi: thinking set, skip masked off.
        let opts = LaunchOptions::remote_issue(
            &settings,
            Some("pi"),
            Some("grok-4.5"),
            Some("xhigh"),
            None,
            None,
            Some(true),
        );
        assert_eq!(opts.agent, CodingAgent::Pi);
        assert_eq!(opts.model, "grok-4.5");
        assert_eq!(opts.effort, "xhigh");
        assert!(!opts.skip_permissions);

        // Unknown agent string → claude with claude normalization.
        let opts = LaunchOptions::remote_issue(
            &settings,
            Some("cursor"),
            Some("sonnet"),
            None,
            None,
            None,
            None,
        );
        assert_eq!(opts.agent, CodingAgent::Claude);
        assert_eq!(opts.model, "sonnet");
    }

    /// A non-default remote agent with NO model/effort sent uses ITS blank
    /// CLI defaults — never the default agent's persisted pair.
    #[test]
    fn remote_non_default_agent_falls_to_blank_not_foreign_settings() {
        let mut settings = Settings::default();
        settings.claude_model = "opus".to_string();
        settings.claude_effort = "high".to_string();
        let opts =
            LaunchOptions::remote_issue(&settings, Some("codex"), None, None, None, None, None);
        assert_eq!(opts.agent, CodingAgent::Codex);
        assert_eq!(opts.model, "", "claude's opus must not leak onto codex");
        assert_eq!(opts.effort, "");

        // And when codex IS the default agent, its persisted pair applies.
        settings.default_agent = CodingAgent::Codex;
        settings.codex_model = "gpt-5.6-sol".to_string();
        settings.codex_effort = "high".to_string();
        let opts =
            LaunchOptions::remote_issue(&settings, Some("codex"), None, None, None, None, None);
        assert_eq!(opts.model, "gpt-5.6-sol");
        assert_eq!(opts.effort, "high");
    }

    #[test]
    fn remote_batch_all_absent_uses_batch_ultracode_and_plan_off() {
        // All-absent: settings model/effort + the BATCH ultracode default,
        // plan mode OFF even when the batch plan-mode setting is ON (F7).
        let mut settings = Settings::default();
        settings.claude_model = "opus".to_string();
        settings.claude_effort = "high".to_string();
        settings.batch_ultracode = true;
        settings.batch_plan_mode = true; // must NOT leak into a remote start
        settings.batch_skip_permissions = true; // batch default applies
        let opts = LaunchOptions::remote_batch(&settings, None, None, None, None, None, None);
        assert_eq!(opts.model, "opus");
        assert_eq!(opts.effort, "high");
        assert!(opts.ultracode);
        assert!(!opts.plan_mode);
        assert!(opts.skip_permissions);
    }

    #[test]
    fn remote_batch_applies_and_normalizes_sent_options() {
        let mut settings = Settings::default();
        settings.claude_effort = "high".to_string();
        settings.batch_ultracode = true;

        let opts = LaunchOptions::remote_batch(
            &settings,
            None,
            Some("Sonnet"),
            Some("max"),
            Some(false),
            Some(true),
            None,
        );
        assert_eq!(opts.model, "sonnet", "case-normalized");
        assert_eq!(opts.effort, "max");
        assert!(!opts.ultracode, "explicit remote off beats the batch default");
        assert!(opts.plan_mode, "explicit remote opt-in");

        // Bogus model falls back to the settings model.
        let opts =
            LaunchOptions::remote_batch(&settings, None, Some("gpt-6"), None, None, None, None);
        assert_eq!(opts.model, "fable");

        // Explicit blank effort = "CLI default"; bogus effort also degrades.
        let opts = LaunchOptions::remote_batch(&settings, None, None, Some(""), None, None, None);
        assert_eq!(opts.effort, "");
        let opts =
            LaunchOptions::remote_batch(&settings, None, None, Some("extreme"), None, None, None);
        assert_eq!(opts.effort, "");
    }
}
