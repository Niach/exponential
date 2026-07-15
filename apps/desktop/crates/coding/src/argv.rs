//! Spawn argv assembly for coding sessions — the ONE place the launcher's
//! `claude` flags are composed. `--model` is explicit-ALWAYS (never the
//! user's CLI default — §7.7, locked 2026-07-03), the seed prompt rides argv
//! positional-last (bytes typed into the PTY before the TUI enters raw mode
//! get swallowed, so the prompt must never ride stdin), and the permission
//! posture is native: `--permission-mode plan` for gated runs,
//! `--dangerously-skip-permissions` otherwise. The doctor's
//! [`crate::doctor::MIN_CLAUDE_VERSION`] gate guarantees every flag here.

use crate::mcp_json::MCP_JSON_FILE;
use crate::settings::Settings;

/// The MCP wiring of every coding argv: the launcher-written worktree
/// [`MCP_JSON_FILE`] (`.exp-mcp.json`) rides `--mcp-config` (resolved against
/// the spawn cwd = the worktree) and connects trusted, prompt-free.
///
/// The flags alone are NOT what suppresses claude's "New MCP server found in
/// this project" dialog — EXP-83 assumed they were, and the dialog kept
/// firing (EXP-98). Claude's interactive startup runs an unconditional
/// approval scan of the project-scope config (the literal `.mcp.json` in the
/// cwd) that ignores both `--mcp-config` and `--strict-mcp-config`; those
/// flags only gate which servers CONNECT. The actual fix is the file NAME:
/// `.exp-mcp.json` is invisible to that scan (see [`crate::mcp_json`]).
/// `--strict-mcp-config` still matters — it keeps any repo-carried MCP
/// config from connecting in a `--dangerously-skip-permissions` session.
pub fn mcp_config_args() -> Vec<String> {
    vec![
        "--mcp-config".into(),
        MCP_JSON_FILE.into(),
        "--strict-mcp-config".into(),
    ]
}

/// The permission tail of every coding argv: native plan mode, or the classic
/// skip flag. `--dangerously-skip-permissions` cannot ride NEXT TO plan mode
/// (both select the STARTING permission mode), so gated runs pass
/// `--allow-dangerously-skip-permissions` instead: the session still starts
/// in plan mode, and `bypassPermissions` joins the Shift+Tab mode cycle — one
/// keypress to full-auto after the plan is approved. Mutations during
/// planning prompt like Manual mode (a real gate, no allow-list holes).
pub fn permission_args(plan_mode: bool) -> Vec<String> {
    if plan_mode {
        vec![
            "--permission-mode".into(),
            "plan".into(),
            "--allow-dangerously-skip-permissions".into(),
        ]
    } else {
        vec!["--dangerously-skip-permissions".into()]
    }
}

/// The Start-coding dialog's choices — ONE shape for both run modes (a
/// single-issue session and a multi-issue batch session differ only in their
/// settings DEFAULTS, not in the flags they can carry).
#[derive(Clone, Debug)]
pub struct LaunchOptions {
    /// `--model` alias (fable/opus/sonnet).
    pub model: String,
    /// `--effort` level; blank = omit the flag. Ignored while ultracode is on
    /// (ultracode IS the effort level — `--effort ultracode`).
    pub effort: String,
    /// Dynamic workflows (`--effort ultracode`, CLI ≥2.1.203 —
    /// model-independent, no opus pin). Wins over `effort` while on.
    pub ultracode: bool,
    /// Native plan mode (`--permission-mode plan`).
    pub plan_mode: bool,
}

impl LaunchOptions {
    /// The settings-default options for a SINGLE-ISSUE run — what a relay
    /// start (no dialog) runs with.
    pub fn issue_defaults(settings: &Settings) -> Self {
        Self {
            model: settings.claude_model.clone(),
            effort: settings.claude_effort.clone(),
            ultracode: settings.issue_ultracode,
            plan_mode: settings.issue_plan_mode,
        }
    }

    /// The settings-default options for a BATCH (multi-issue) run.
    pub fn batch_defaults(settings: &Settings) -> Self {
        Self {
            model: settings.claude_model.clone(),
            effort: settings.claude_effort.clone(),
            ultracode: settings.batch_ultracode,
            plan_mode: settings.batch_plan_mode,
        }
    }
}

/// The coding-session argv:
/// `--model <m> [--effort ultracode|<e>] <mcp_config_args> <permission_args>
/// <positional>`.
pub fn session_args(opts: &LaunchOptions, positional: &str) -> Vec<String> {
    let mut args = vec!["--model".to_string(), opts.model.clone()];
    let effort = if opts.ultracode {
        Some("ultracode".to_string())
    } else {
        let trimmed = opts.effort.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    };
    if let Some(effort) = effort {
        args.push("--effort".to_string());
        args.push(effort);
    }
    args.extend(mcp_config_args());
    args.extend(permission_args(opts.plan_mode));
    args.push(positional.to_string());
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_args_split_on_plan_mode() {
        // Gated: plan START mode + bypass ALLOWED (Shift+Tab reachable) but
        // never `--dangerously-skip-permissions` itself — that flag IS a
        // starting mode and would erase the gate.
        assert_eq!(
            permission_args(true),
            vec![
                "--permission-mode".to_string(),
                "plan".to_string(),
                "--allow-dangerously-skip-permissions".to_string(),
            ]
        );
        assert_eq!(
            permission_args(false),
            vec!["--dangerously-skip-permissions".to_string()]
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
    fn session_args_matrix() {
        // Plan mode ON (the issue default), no effort, no ultracode.
        let opts = LaunchOptions {
            model: "fable".to_string(),
            effort: "".to_string(),
            ultracode: false,
            plan_mode: true,
        };
        assert_eq!(
            session_args(&opts, "do the thing"),
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

        // Plan mode OFF + effort set: skip flag, effort before the
        // MCP + permission tail, positional last.
        let opts = LaunchOptions {
            model: "opus".to_string(),
            effort: "xhigh".to_string(),
            ultracode: false,
            plan_mode: false,
        };
        assert_eq!(
            session_args(&opts, "prompt"),
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

        // Ultracode WINS over a set effort (`--effort ultracode`,
        // model-independent — the chosen model stays).
        let opts = LaunchOptions {
            model: "fable".to_string(),
            effort: "high".to_string(),
            ultracode: true,
            plan_mode: false,
        };
        assert_eq!(
            session_args(&opts, "seed")[..4],
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
            ultracode: false,
            plan_mode: false,
        };
        let args = session_args(&opts, "p");
        assert!(!args.iter().any(|arg| arg == "--effort"));
        assert!(!args.iter().any(|arg| arg == "--agents"));
        assert_eq!(args.last().map(String::as_str), Some("p"));
    }

    #[test]
    fn defaults_map_model_effort_and_per_mode_toggles() {
        let mut settings = Settings::default();
        settings.claude_model = "sonnet".to_string();
        settings.claude_effort = "high".to_string();
        settings.issue_plan_mode = false;
        let opts = LaunchOptions::issue_defaults(&settings);
        assert_eq!(opts.model, "sonnet");
        assert_eq!(opts.effort, "high");
        assert!(!opts.plan_mode);
        assert!(!opts.ultracode);

        // The stock defaults: issue runs — plan mode ON, ultracode OFF;
        // batch runs — ultracode ON, plan mode OFF. Same model/effort pair.
        let issue = LaunchOptions::issue_defaults(&Settings::default());
        assert_eq!(issue.model, "fable");
        assert_eq!(issue.effort, "");
        assert!(issue.plan_mode);
        assert!(!issue.ultracode);

        let batch = LaunchOptions::batch_defaults(&Settings::default());
        assert_eq!(batch.model, "fable");
        assert_eq!(batch.effort, "");
        assert!(!batch.plan_mode);
        assert!(batch.ultracode);
    }
}
