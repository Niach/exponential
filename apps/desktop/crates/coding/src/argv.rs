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
//!   when skipping. `--full-auto` is deprecated and never used. Every codex
//!   argv also disables the startup update prompt (EXP-389 — it parks an
//!   unattended session; the directory-trust screen is handled separately by
//!   [`crate::codex_trust`], because `-c projects.….trust_level` cannot
//!   express paths containing dots).
//! - **pi** — no permission system exists; no flags either way.

use std::path::Path;

use crate::agent::CodingAgent;
use crate::mcp_json::MCP_JSON_FILE;
use crate::pi_bridge::{PI_BRIDGE_FILE, PI_OBSERVER_FILE, PI_PLAN_FILE};
use crate::settings::Settings;

/// The env var carrying the raw `expu_` key for codex + pi sessions (EXP-201)
/// — those agents get the MCP credential via the spawn environment instead of
/// a worktree file: codex reads it through `bearer_token_env_var`, the pi
/// bridge reads it directly. Never on argv (ps-visible), never on disk.
pub const MCP_TOKEN_ENV: &str = "EXP_MCP_TOKEN";

/// The env var carrying the `/api/mcp` URL for the pi bridge.
pub const MCP_URL_ENV: &str = "EXP_MCP_URL";

/// EXP-249 — the hooks sidecar's spawn env (mirrors `steer::hooks`'
/// `HOOK_PORT_ENV`/`HOOK_TOKEN_ENV`; the two crates cannot depend on each
/// other, §3.1). The `--settings` file's hook commands expand these at hook
/// time, so the file itself stays constant and secret-free.
pub const HOOK_PORT_ENV: &str = "EXP_HOOK_PORT";
pub const HOOK_TOKEN_ENV: &str = "EXP_HOOK_TOKEN";

/// Spawn-env vars the pi observer extension reads (EXP-383; mirror of
/// `steer::pi_observer::OBSERVER_{URL,TOKEN}_ENV` — the two crates cannot
/// depend on each other, §3.1).
pub const OBSERVER_URL_ENV: &str = "EXP_OBSERVER_URL";
pub const OBSERVER_TOKEN_ENV: &str = "EXP_OBSERVER_TOKEN";

/// Spawn-env gate for the pi plan-mode extension (EXP-441): the launcher
/// sets it to `1` on a pi launch with plan mode on. The extension file
/// itself rides `-e` unconditionally (like the observer) and is inert
/// without this value.
pub const PI_PLAN_MODE_ENV: &str = "EXP_PI_PLAN_MODE";

/// EXP-443: codex's per-spawn originator override — the value lands verbatim
/// in every rollout meta this spawn writes, giving the activity emitter a
/// discriminator against foreign codex processes sharing the cwd. NOT one of
/// the steer↔coding mirrored env pairs: steer never reads the env, it reads
/// the value back OUT of the rollout meta. The name is codex's own internal
/// override; if a codex build ignores it, discovery degrades to the pre-fix
/// cwd-only match (see `steer::codex_activity::find_live_rollout`).
pub const CODEX_ORIGINATOR_ENV: &str = "CODEX_INTERNAL_ORIGINATOR_OVERRIDE";

/// The originator stamped onto codex AGENT SHELLS (EXP-443): shells share the
/// trunk-clone cwd with action runs, so they must carry an originator that no
/// session emitter's strict pass can ever match.
pub const CODEX_SHELL_ORIGINATOR: &str = "exponential-shell";

/// The per-session codex originator: `exponential-<sid8>`, keyed by OUR
/// `coding_sessions` row id (codex's own rollout ids don't exist until it
/// boots). 8 chars is plenty — the value only has to differ between
/// concurrent same-cwd spawns on one machine.
pub fn codex_session_originator(session_id: &str) -> String {
    let sid8: String = session_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(8)
        .collect();
    format!("exponential-{sid8}")
}

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
    /// Launch-into-plan mode: claude natively (`--permission-mode plan`),
    /// pi via the injected `.exp-pi-plan.ts` extension gated on
    /// [`PI_PLAN_MODE_ENV`] (EXP-441). Never codex.
    pub plan_mode: bool,
    /// Full permission bypass (claude `--dangerously-skip-permissions` /
    /// codex `--dangerously-bypass-approvals-and-sandbox`). OFF = the
    /// agent's guarded auto mode. Inert for pi (always unguarded).
    pub skip_permissions: bool,
}

impl LaunchOptions {
    /// The settings-default options (the local Start-coding dialog's seed
    /// values). EXP-206: ONE set of defaults — a single-issue run and a
    /// multi-issue batch run seed identically; the per-run-mode settings
    /// pairs are gone.
    pub fn defaults(settings: &Settings) -> Self {
        Self::defaults_for(settings, settings.default_agent)
    }

    /// The settings-default options for an EXPLICIT agent pick (EXP-325 —
    /// the terminal dock's "+" menu launches whichever installed agent was
    /// clicked, with that agent's persisted model/effort/toggles and the
    /// usual capability masking).
    pub fn defaults_for(settings: &Settings, agent: CodingAgent) -> Self {
        Self {
            agent,
            model: settings.model_for(agent).to_string(),
            effort: settings.effort_for(agent).to_string(),
            ultracode: settings.claude_ultracode && agent.supports_ultracode(),
            plan_mode: settings.plan_mode_for(agent) && agent.supports_plan_mode(),
            skip_permissions: settings.skip_permissions_for(agent)
                && agent.supports_skip_permissions(),
        }
    }

    /// The shared RELAY-start normalization (EXP-149/EXP-201) — one form for
    /// issue and batch starts alike (EXP-206): the remote client's
    /// Start-coding choices normalized against the AGENT's closed sets, over
    /// settings defaults for anything it didn't send.
    ///
    /// - Absent/unknown `agent` → **Claude** (an option-less legacy frame
    ///   must behave exactly as before EXP-201 — never the local default
    ///   agent, or an old phone's claude vocabulary could land on a codex
    ///   launch).
    /// - `effort: Some("")` is an explicit "CLI default" and beats a
    ///   non-blank settings effort; same for a blank codex/pi model.
    /// - Absent ultracode/skip fall to the settings defaults (skip is
    ///   per-AGENT — [`Settings::skip_permissions_for`] of the RESOLVED
    ///   agent); plan mode defaults OFF when absent (F7 — an option-less
    ///   start must never park an unattended desktop at the plan-approval
    ///   TUI); a remote client sending `plan_mode: true` opted in knowingly.
    /// - Capabilities mask everything: a non-claude agent can never carry
    ///   ultracode, codex never carries plan, pi never carries skip.
    #[allow(clippy::too_many_arguments)]
    pub fn remote(
        settings: &Settings,
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
            ultracode: ultracode.unwrap_or(settings.claude_ultracode)
                && agent.supports_ultracode(),
            plan_mode: plan_mode.unwrap_or(false) && agent.supports_plan_mode(),
            skip_permissions: skip_permissions.unwrap_or(settings.skip_permissions_for(agent))
                && agent.supports_skip_permissions(),
        }
    }
}

/// What ends a coding-session argv (EXP-202): the seed prompt as the
/// positional, or the agent's NATIVE resume with no prompt at all.
///
/// - `Continue` — claude/pi append `--continue` (cwd-scoped: resumes the
///   latest conversation for the spawn cwd = the reused worktree; pi's flag
///   is undocumented but real). A caller bug on codex (which has no
///   cwd-scoped continue — `resume --last` is global-latest) degrades to a
///   flagless spawn.
/// - `CodexResume(id)` — codex only: the `resume <SESSION_ID>` subcommand
///   form, resuming the EXACT session the launcher recovered for this
///   worktree from codex's rollout metas ([`crate::codex_sessions`]).
///   `resume` rides argv-FIRST (it is a subcommand, verified to accept the
///   same `-m`/`-c`/sandbox/approval flags as a fresh spawn).
/// - `None` — a FRESH interactive session with no seed prompt (EXP-325: the
///   terminal dock's "+" agent launch). Appends nothing on every agent —
///   deliberately not `Continue`, which would resume the cwd's latest
///   conversation instead of starting empty.
#[derive(Clone, Copy, Debug)]
pub enum SessionTail<'a> {
    Prompt(&'a str),
    Continue,
    CodexResume(&'a str),
    None,
}

/// The coding-session argv for `opts.agent`, tail LAST always (the prompt
/// positional, or `--continue` on a claude/pi resume; a codex resume instead
/// PREPENDS `resume <SESSION_ID>` as the subcommand):
///
/// - claude: `--model <m> [--effort ultracode|<e>] [--settings <file>]
///   [--session-id <uuid>] <mcp_config_args> <permission_args> <tail>`
/// - codex: `[resume <session-id>] [-m <m>] [-c model_reasoning_effort=<e>]
///   <mcp -c overrides> <sandbox/approval flags> [<positional>]`
/// - pi: `[--model <m>] [--thinking <t>] -e ./<bridge> <tail>`
///
/// `claude_settings` is the EXP-249 hooks-sidecar settings file (an absolute
/// path OUTSIDE the worktree — see `launcher::HookSetup`). `None` = no
/// sidecar for this run: claude then uses its own settings chain and the
/// session degrades to grid-only detection. Ignored for codex/pi, which have
/// no hooks system.
///
/// `claude_session_id` (EXP-443) is the launcher-minted UUID a FRESH claude
/// session is told to use (`--session-id`), so the transcript pin exists
/// before the first hook fires. `None` on resume (the conversation keeps its
/// original id — the SessionStart hook seeds the pin instead) and on every
/// other agent.
pub fn session_args(
    opts: &LaunchOptions,
    mcp: &AgentMcp,
    claude_settings: Option<&Path>,
    claude_session_id: Option<&str>,
    tail: SessionTail<'_>,
) -> Vec<String> {
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
            if let Some(settings) = claude_settings {
                args.push("--settings".into());
                args.push(settings.to_string_lossy().into_owned());
            }
            if let Some(id) = claude_session_id {
                args.push("--session-id".into());
                args.push(id.to_string());
            }
            args.extend(mcp_config_args());
            args.extend(permission_args(opts.plan_mode, opts.skip_permissions));
        }
        CodingAgent::Codex => {
            // EXP-389: codex's startup update prompt ("Update now / Skip …
            // Press enter to continue") blocks an unattended session exactly
            // like the trust screen — a remote start would park on it with
            // nothing visible on the phone. Session-scoped override, the
            // user's own config/interactive runs keep their update checks.
            args.push("-c".into());
            args.push("check_for_update_on_startup=false".into());
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
            // The observer extension (EXP-383): reports activity to the
            // loopback sidecar and applies remote steers via
            // pi.sendUserMessage. `-e` is repeatable; the file is inert
            // without the EXP_OBSERVER_* env, so it rides unconditionally.
            args.push("-e".into());
            args.push(format!("./{PI_OBSERVER_FILE}"));
            // The plan-mode extension (EXP-441): blocks mutating tools until
            // the user approves a plan via the exit_plan_mode tool. Inert
            // without [`PI_PLAN_MODE_ENV`], so it rides unconditionally too.
            args.push("-e".into());
            args.push(format!("./{PI_PLAN_FILE}"));
        }
    }
    match tail {
        SessionTail::Prompt(positional) => args.push(positional.to_string()),
        // Only claude + pi have a cwd-scoped continue flag; on codex this
        // variant is a caller bug — degrade to a flagless spawn rather than
        // panic or hand codex an unknown flag.
        SessionTail::Continue if !matches!(opts.agent, CodingAgent::Codex) => {
            args.push("--continue".into())
        }
        SessionTail::Continue => {}
        // The subcommand must lead the argv; every flag above is accepted by
        // `codex resume` too. On any other agent this is a caller bug —
        // degrade like Continue does.
        SessionTail::CodexResume(id) if opts.agent == CodingAgent::Codex => {
            args.insert(0, "resume".into());
            args.insert(1, id.to_string());
        }
        SessionTail::CodexResume(_) => {}
        // A fresh promptless interactive session — nothing to append.
        SessionTail::None => {}
    }
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
            session_args(&opts, &AgentMcp::ClaudeFile, None, None, SessionTail::Prompt("do the thing")),
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
            session_args(&opts, &AgentMcp::ClaudeFile, None, None, SessionTail::Prompt("prompt")),
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
        let args = session_args(&claude_opts(), &AgentMcp::ClaudeFile, None, None, SessionTail::Prompt("p"));
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
            session_args(&opts, &AgentMcp::ClaudeFile, None, None, SessionTail::Prompt("seed"))[..4],
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
        let args = session_args(&opts, &AgentMcp::ClaudeFile, None, None, SessionTail::Prompt("p"));
        assert!(!args.iter().any(|arg| arg == "--effort"));
        assert!(!args.iter().any(|arg| arg == "--agents"));
        assert_eq!(args.last().map(String::as_str), Some("p"));
    }

    /// EXP-249: the hooks-sidecar settings file rides `--settings` between
    /// the model/effort pair and the MCP flags — never near the tail, which
    /// stays the prompt positional.
    #[test]
    fn claude_session_args_carry_the_hook_settings_file() {
        let settings = Path::new("/home/u/.local/share/exponential/claude-hooks/sess-1.settings.json");
        let opts = LaunchOptions {
            effort: "high".to_string(),
            ..claude_opts()
        };
        assert_eq!(
            session_args(
                &opts,
                &AgentMcp::ClaudeFile,
                Some(settings),
                None,
                SessionTail::Prompt("prompt")
            ),
            vec![
                "--model",
                "fable",
                "--effort",
                "high",
                "--settings",
                "/home/u/.local/share/exponential/claude-hooks/sess-1.settings.json",
                "--mcp-config",
                ".exp-mcp.json",
                "--strict-mcp-config",
                "--permission-mode",
                "auto",
                "--allow-dangerously-skip-permissions",
                "prompt",
            ]
        );
        // A resume keeps it too (the sidecar is per-RUN, not per-prompt).
        let args = session_args(
            &claude_opts(),
            &AgentMcp::ClaudeFile,
            Some(settings),
            None,
            SessionTail::Continue,
        );
        assert!(args.contains(&"--settings".to_string()));
        assert_eq!(args.last().map(String::as_str), Some("--continue"));

        // codex and pi have no hooks system — the file never reaches them.
        let codex = LaunchOptions {
            agent: CodingAgent::Codex,
            ..claude_opts()
        };
        let mcp = AgentMcp::CodexOverrides {
            url: "https://app.exponential.at/api/mcp".to_string(),
        };
        let args = session_args(&codex, &mcp, Some(settings), None, SessionTail::Prompt("p"));
        assert!(!args.iter().any(|arg| arg == "--settings"));
        let pi = LaunchOptions {
            agent: CodingAgent::Pi,
            ..claude_opts()
        };
        let args = session_args(&pi, &AgentMcp::PiExtension, Some(settings), None, SessionTail::Prompt("p"));
        assert!(!args.iter().any(|arg| arg == "--settings"));
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
            session_args(&opts, &mcp, None, None, SessionTail::Prompt("prompt")),
            vec![
                "-c",
                "check_for_update_on_startup=false",
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
        let args = session_args(&opts, &mcp, None, None, SessionTail::Prompt("prompt"));
        assert_eq!(
            args,
            vec![
                "-c",
                "check_for_update_on_startup=false",
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
            session_args(&opts, &AgentMcp::PiExtension, None, None, SessionTail::Prompt("prompt")),
            vec![
                "--model",
                "grok-4.5",
                "--thinking",
                "high",
                "-e",
                "./.exp-pi-mcp.ts",
                "-e",
                "./.exp-pi-observer.ts",
                "-e",
                "./.exp-pi-plan.ts",
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
        let args = session_args(&opts, &AgentMcp::PiExtension, None, None, SessionTail::Prompt("p"));
        assert_eq!(
            args,
            vec![
                "-e",
                "./.exp-pi-mcp.ts",
                "-e",
                "./.exp-pi-observer.ts",
                "-e",
                "./.exp-pi-plan.ts",
                "p"
            ]
        );
        assert!(!args.iter().any(|arg| arg == "-a" || arg == "--approve"));
    }

    /// EXP-202: the resume tails. Claude + pi end with `--continue` (their
    /// cwd-scoped native resume) and carry NO positional prompt, with every
    /// other flag intact; codex resumes via the `resume <SESSION_ID>`
    /// subcommand PREPENDED to the same flag set. Cross-agent variants are
    /// caller bugs and degrade to a flagless spawn.
    #[test]
    fn resume_tail_matrix() {
        // Claude: full flag set preserved, `--continue` last, no prompt.
        let args = session_args(&claude_opts(), &AgentMcp::ClaudeFile, None, None, SessionTail::Continue);
        assert_eq!(args.last().map(String::as_str), Some("--continue"));
        assert!(args.contains(&"--mcp-config".to_string()));
        assert!(args.contains(&"--permission-mode".to_string()));

        // pi: `-c`/`--continue` exists but is undocumented — the bridge
        // extension still loads, `--continue` last, no prompt.
        let opts = LaunchOptions {
            agent: CodingAgent::Pi,
            model: "fable".to_string(),
            effort: "".to_string(),
            ultracode: false,
            plan_mode: false,
            skip_permissions: false,
        };
        assert_eq!(
            session_args(&opts, &AgentMcp::PiExtension, None, None, SessionTail::Continue),
            vec![
                "--model",
                "fable",
                "-e",
                "./.exp-pi-mcp.ts",
                "-e",
                "./.exp-pi-observer.ts",
                "-e",
                "./.exp-pi-plan.ts",
                "--continue"
            ]
        );

        // codex: the exact recovered session id rides the `resume`
        // subcommand FIRST; the MCP overrides + permission posture stay.
        let opts = LaunchOptions {
            agent: CodingAgent::Codex,
            model: "".to_string(),
            effort: "".to_string(),
            ultracode: false,
            plan_mode: false,
            skip_permissions: true,
        };
        let mcp = AgentMcp::CodexOverrides {
            url: "https://app.exponential.at/api/mcp".to_string(),
        };
        let args = session_args(&opts, &mcp, None, None, SessionTail::CodexResume("019f-abc"));
        assert_eq!(args[..2], ["resume".to_string(), "019f-abc".to_string()]);
        assert!(args.contains(&"--dangerously-bypass-approvals-and-sandbox".to_string()));
        assert!(args
            .contains(&"mcp_servers.exponential.bearer_token_env_var=\"EXP_MCP_TOKEN\"".to_string()));

        // Cross-agent tails are caller bugs and must DEGRADE, never panic or
        // pass an unknown flag: Continue on codex, CodexResume on claude.
        let args = session_args(&opts, &mcp, None, None, SessionTail::Continue);
        assert!(!args.iter().any(|arg| arg == "--continue"));
        assert_eq!(
            args.last().map(String::as_str),
            Some("--dangerously-bypass-approvals-and-sandbox")
        );
        let args =
            session_args(&claude_opts(), &AgentMcp::ClaudeFile, None, None, SessionTail::CodexResume("x"));
        assert!(!args.iter().any(|arg| arg == "resume" || arg == "x"));
    }

    /// EXP-325: the promptless tail — a fresh interactive session appends
    /// NOTHING on any agent (never `--continue`, which would resume).
    #[test]
    fn none_tail_appends_nothing_on_every_agent() {
        let args = session_args(&claude_opts(), &AgentMcp::ClaudeFile, None, None, SessionTail::None);
        assert_eq!(
            args.last().map(String::as_str),
            Some("--allow-dangerously-skip-permissions")
        );
        assert!(!args.iter().any(|arg| arg == "--continue"));

        let codex = LaunchOptions {
            agent: CodingAgent::Codex,
            model: "".to_string(),
            effort: "".to_string(),
            ultracode: false,
            plan_mode: false,
            skip_permissions: false,
        };
        let mcp = AgentMcp::CodexOverrides {
            url: "https://app.exponential.at/api/mcp".to_string(),
        };
        let args = session_args(&codex, &mcp, None, None, SessionTail::None);
        assert_eq!(
            args.last().map(String::as_str),
            Some("sandbox_workspace_write.network_access=true")
        );

        let pi = LaunchOptions {
            agent: CodingAgent::Pi,
            model: "".to_string(),
            effort: "".to_string(),
            ultracode: false,
            plan_mode: false,
            skip_permissions: false,
        };
        let args = session_args(&pi, &AgentMcp::PiExtension, None, None, SessionTail::None);
        assert_eq!(
            args,
            vec![
                "-e",
                "./.exp-pi-mcp.ts",
                "-e",
                "./.exp-pi-observer.ts",
                "-e",
                "./.exp-pi-plan.ts"
            ]
        );
    }

    /// EXP-325: an explicit agent pick seeds THAT agent's persisted pair and
    /// masks capabilities — regardless of the default agent.
    #[test]
    fn defaults_for_follow_the_picked_agent() {
        let mut settings = Settings::default();
        settings.default_agent = CodingAgent::Claude;
        settings.claude_ultracode = true; // claude-only — must mask on codex
        settings.claude_plan_mode = true;
        settings.codex_model = "gpt-5.6-terra".to_string();
        settings.codex_effort = "xhigh".to_string();
        settings.codex_skip_permissions = true;

        let opts = LaunchOptions::defaults_for(&settings, CodingAgent::Codex);
        assert_eq!(opts.agent, CodingAgent::Codex);
        assert_eq!(opts.model, "gpt-5.6-terra");
        assert_eq!(opts.effort, "xhigh");
        assert!(!opts.ultracode);
        assert!(!opts.plan_mode);
        assert!(opts.skip_permissions, "codex's own skip default");

        // `defaults` stays the default-agent shorthand.
        let via_default = LaunchOptions::defaults(&settings);
        let via_for = LaunchOptions::defaults_for(&settings, settings.default_agent);
        assert_eq!(via_default.agent, via_for.agent);
        assert_eq!(via_default.model, via_for.model);
        assert_eq!(via_default.effort, via_for.effort);

        let opts = LaunchOptions::defaults_for(&settings, CodingAgent::Pi);
        assert_eq!(opts.agent, CodingAgent::Pi);
        assert!(!opts.skip_permissions, "pi has no permission system");
        assert!(opts.plan_mode, "pi seeds its OWN plan default (EXP-441)");

        // The pi plan default is its own field — independent of claude's.
        settings.pi_plan_mode = false;
        assert!(!LaunchOptions::defaults_for(&settings, CodingAgent::Pi).plan_mode);
        assert!(LaunchOptions::defaults_for(&settings, CodingAgent::Claude).plan_mode);
    }

    #[test]
    fn defaults_map_model_effort_and_toggles() {
        let mut settings = Settings::default();
        settings.claude_model = "sonnet".to_string();
        settings.claude_effort = "high".to_string();
        settings.claude_plan_mode = false;
        let opts = LaunchOptions::defaults(&settings);
        assert_eq!(opts.agent, CodingAgent::Claude);
        assert_eq!(opts.model, "sonnet");
        assert_eq!(opts.effort, "high");
        assert!(!opts.plan_mode);
        assert!(!opts.ultracode);
        assert!(!opts.skip_permissions);

        // The stock defaults (EXP-206 — ONE set, no issue/batch split):
        // plan mode ON, ultracode OFF, skip OFF (guarded auto posture).
        let opts = LaunchOptions::defaults(&Settings::default());
        assert_eq!(opts.model, "fable");
        assert_eq!(opts.effort, "");
        assert!(opts.plan_mode);
        assert!(!opts.ultracode);
        assert!(!opts.skip_permissions);
    }

    /// EXP-201: a non-claude default agent seeds ITS model/effort pair and
    /// masks the claude-only toggles even when their settings are on.
    #[test]
    fn defaults_follow_the_default_agent_and_mask_capabilities() {
        let mut settings = Settings::default();
        settings.default_agent = CodingAgent::Codex;
        settings.codex_model = "gpt-5.6-terra".to_string();
        settings.codex_effort = "xhigh".to_string();
        settings.claude_ultracode = true; // claude-only — must mask
        settings.claude_plan_mode = true; // claude-only — must mask
        settings.codex_skip_permissions = true; // codex's OWN skip default
        let opts = LaunchOptions::defaults(&settings);
        assert_eq!(opts.agent, CodingAgent::Codex);
        assert_eq!(opts.model, "gpt-5.6-terra");
        assert_eq!(opts.effort, "xhigh");
        assert!(!opts.ultracode);
        assert!(!opts.plan_mode);
        assert!(opts.skip_permissions);

        // Claude's skip default never leaks onto another agent (EXP-206:
        // skip is per-agent) — codex OFF stays OFF with claude ON.
        settings.claude_skip_permissions = true;
        settings.codex_skip_permissions = false;
        assert!(!LaunchOptions::defaults(&settings).skip_permissions);

        settings.default_agent = CodingAgent::Pi;
        settings.pi_model = "grok-4.5".to_string();
        settings.pi_thinking = "max".to_string();
        let opts = LaunchOptions::defaults(&settings);
        assert_eq!(opts.agent, CodingAgent::Pi);
        assert_eq!(opts.model, "grok-4.5");
        assert_eq!(opts.effort, "max");
        assert!(!opts.skip_permissions, "pi has no permission system");
    }

    #[test]
    fn remote_all_absent_matches_pre_options_relay_behavior() {
        // The F7 baseline: settings model/effort/ultracode, plan mode OFF —
        // exactly what an option-less relay start ran before EXP-149.
        let mut settings = Settings::default();
        settings.claude_model = "opus".to_string();
        settings.claude_effort = "high".to_string();
        settings.claude_ultracode = true;
        settings.claude_plan_mode = true; // must NOT leak into a remote start
        let opts = LaunchOptions::remote(&settings, None, None, None, None, None, None);
        assert_eq!(opts.agent, CodingAgent::Claude);
        assert_eq!(opts.model, "opus");
        assert_eq!(opts.effort, "high");
        assert!(opts.ultracode);
        assert!(!opts.plan_mode);
        assert!(!opts.skip_permissions);
    }

    #[test]
    fn remote_applies_and_normalizes_sent_options() {
        let mut settings = Settings::default();
        settings.claude_effort = "high".to_string();

        let opts = LaunchOptions::remote(
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
        let opts = LaunchOptions::remote(&settings, None, Some("gpt-6"), None, None, None, None);
        assert_eq!(opts.model, "fable");

        // Explicit blank effort = "CLI default" and beats the settings value.
        let opts = LaunchOptions::remote(&settings, None, None, Some(""), None, None, None);
        assert_eq!(opts.effort, "");
        // Bogus effort also degrades to blank (omit --effort).
        let opts = LaunchOptions::remote(&settings, None, None, Some("extreme"), None, None, None);
        assert_eq!(opts.effort, "");
    }

    /// EXP-201: a remote CODEX start normalizes against codex sets and can
    /// never carry the claude-only toggles; an unknown agent degrades to
    /// claude (legacy behavior, never the local default agent).
    #[test]
    fn remote_normalizes_per_agent_and_masks_capabilities() {
        let mut settings = Settings::default();
        settings.claude_ultracode = true; // must not leak onto codex

        let opts = LaunchOptions::remote(
            &settings,
            Some("codex"),
            Some("gpt-5.6-luna"),
            Some("minimal"),
            Some(true), // ultracode — claude-only, must mask
            Some(true), // plan — claude/pi-only, must mask on codex
            None,
        );
        assert_eq!(opts.agent, CodingAgent::Codex);
        assert_eq!(opts.model, "gpt-5.6-luna");
        assert_eq!(opts.effort, "minimal");
        assert!(!opts.ultracode);
        assert!(!opts.plan_mode);
        assert!(!opts.skip_permissions);

        // A claude model on a codex start is bogus → blank (codex default).
        let opts = LaunchOptions::remote(
            &settings,
            Some("codex"),
            Some("fable"),
            None,
            None,
            None,
            None,
        );
        assert_eq!(opts.model, "");

        // pi: thinking set, skip masked off; an explicit plan opt-in passes
        // through (EXP-441 — pi plans via the injected extension).
        let opts = LaunchOptions::remote(
            &settings,
            Some("pi"),
            Some("grok-4.5"),
            Some("xhigh"),
            None,
            Some(true),
            Some(true),
        );
        assert_eq!(opts.agent, CodingAgent::Pi);
        assert_eq!(opts.model, "grok-4.5");
        assert_eq!(opts.effort, "xhigh");
        assert!(opts.plan_mode, "explicit remote opt-in (EXP-441)");
        assert!(!opts.skip_permissions);

        // F7 holds for pi too: an option-less start must never park an
        // unattended desktop at the plan gate.
        let opts = LaunchOptions::remote(&settings, Some("pi"), None, None, None, None, None);
        assert!(!opts.plan_mode, "absent plan defaults OFF");

        // Unknown agent string → claude with claude normalization.
        let opts = LaunchOptions::remote(
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
    /// CLI defaults — never the default agent's persisted pair. An absent
    /// skip falls to the RESOLVED agent's own setting (EXP-206).
    #[test]
    fn remote_non_default_agent_falls_to_blank_not_foreign_settings() {
        let mut settings = Settings::default();
        settings.claude_model = "opus".to_string();
        settings.claude_effort = "high".to_string();
        settings.claude_skip_permissions = true; // must not leak onto codex
        let opts = LaunchOptions::remote(&settings, Some("codex"), None, None, None, None, None);
        assert_eq!(opts.agent, CodingAgent::Codex);
        assert_eq!(opts.model, "", "claude's opus must not leak onto codex");
        assert_eq!(opts.effort, "");
        assert!(!opts.skip_permissions, "skip default is per-agent");

        // And when codex IS the default agent, its persisted pair applies —
        // as does its own skip default.
        settings.default_agent = CodingAgent::Codex;
        settings.codex_model = "gpt-5.6-sol".to_string();
        settings.codex_effort = "high".to_string();
        settings.codex_skip_permissions = true;
        let opts = LaunchOptions::remote(&settings, Some("codex"), None, None, None, None, None);
        assert_eq!(opts.model, "gpt-5.6-sol");
        assert_eq!(opts.effort, "high");
        assert!(opts.skip_permissions);
    }

    /// EXP-443: a fresh claude spawn carries the launcher-minted session id
    /// so the transcript pin exists before the first hook.
    #[test]
    fn claude_fresh_argv_carries_the_minted_session_id() {
        let args = session_args(
            &claude_opts(),
            &AgentMcp::ClaudeFile,
            None,
            Some("0d9f7f6e-8e1c-4b62-9a6e-2f1c9b3d4e5f"),
            SessionTail::Prompt("p"),
        );
        let at = args.iter().position(|a| a == "--session-id").expect("flag");
        assert_eq!(args[at + 1], "0d9f7f6e-8e1c-4b62-9a6e-2f1c9b3d4e5f");
        // Positional stays last — the flag must never trail the prompt.
        assert_eq!(args.last().map(String::as_str), Some("p"));
    }

    /// Resume passes no id (the conversation keeps its own — the SessionStart
    /// hook seeds the pin instead), and non-claude agents ignore the param.
    #[test]
    fn session_id_is_omitted_on_resume_and_non_claude_agents() {
        let args = session_args(
            &claude_opts(),
            &AgentMcp::ClaudeFile,
            None,
            None,
            SessionTail::Continue,
        );
        assert!(!args.iter().any(|a| a == "--session-id"));

        let codex = LaunchOptions {
            agent: CodingAgent::Codex,
            ..claude_opts()
        };
        let mcp = AgentMcp::CodexOverrides {
            url: "https://app.exponential.at/api/mcp".to_string(),
        };
        let args = session_args(&codex, &mcp, None, Some("sid"), SessionTail::Prompt("p"));
        assert!(!args.iter().any(|a| a == "--session-id"));
        let pi = LaunchOptions {
            agent: CodingAgent::Pi,
            ..claude_opts()
        };
        let args = session_args(
            &pi,
            &AgentMcp::PiExtension,
            None,
            Some("sid"),
            SessionTail::Prompt("p"),
        );
        assert!(!args.iter().any(|a| a == "--session-id"));
    }

    /// EXP-443: the per-session codex originator is stable, filesystem-safe
    /// and distinct from the agent-shell one.
    #[test]
    fn codex_session_originator_takes_eight_alphanumerics() {
        assert_eq!(
            codex_session_originator("0d9f7f6e-8e1c-4b62-9a6e-2f1c9b3d4e5f"),
            "exponential-0d9f7f6e"
        );
        assert_eq!(codex_session_originator("ab"), "exponential-ab");
        assert_ne!(codex_session_originator("deadbeef"), CODEX_SHELL_ORIGINATOR);
    }
}
