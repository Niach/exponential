//! Spawn argv assembly for coding sessions — the ONE place the launcher's
//! `claude` flags are composed. `--model` is explicit-ALWAYS (never the
//! user's CLI default — §7.7, locked 2026-07-03), the seed prompt rides argv
//! positional-last (bytes typed into the PTY before the TUI enters raw mode
//! get swallowed, so the prompt must never ride stdin), and the permission
//! posture is native: `--permission-mode plan` for gated runs,
//! `--dangerously-skip-permissions` otherwise. The doctor's
//! [`crate::doctor::MIN_CLAUDE_VERSION`] gate guarantees every flag here.

use crate::mcp_json::MCP_JSON_FILE;
use crate::release_launcher::ReleaseLaunchOptions;
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

/// The Start-coding dialog's choices for a SINGLE-ISSUE run.
#[derive(Clone, Debug)]
pub struct IssueLaunchOptions {
    /// `--model` alias (fable/opus/sonnet).
    pub model: String,
    /// `--effort` level; blank = omit the flag.
    pub effort: String,
    /// Native plan mode (`--permission-mode plan`).
    pub plan_mode: bool,
}

impl IssueLaunchOptions {
    /// The settings-default options — what a relay start (no dialog) runs
    /// with.
    pub fn from_settings(settings: &Settings) -> Self {
        Self {
            model: settings.claude_model.clone(),
            effort: settings.claude_effort.clone(),
            plan_mode: settings.issue_plan_mode,
        }
    }
}

/// Single-issue argv:
/// `--model <m> [--effort <e>] <mcp_config_args> <permission_args>
/// <positional>`.
pub fn issue_args(opts: &IssueLaunchOptions, positional: &str) -> Vec<String> {
    let mut args = vec!["--model".to_string(), opts.model.clone()];
    let effort = opts.effort.trim();
    if !effort.is_empty() {
        args.push("--effort".to_string());
        args.push(effort.to_string());
    }
    args.extend(mcp_config_args());
    args.extend(permission_args(opts.plan_mode));
    args.push(positional.to_string());
    args
}

/// Release-orchestrator argv:
/// `--model <m> [--effort ultracode|<e>] --agents <json> <mcp_config_args>
/// <permission_args> <positional>`. Ultracode IS an effort level (`--effort ultracode`,
/// CLI ≥2.1.203 — model-independent, no opus pin) and wins over the main
/// effort while engaged.
pub fn release_args(
    opts: &ReleaseLaunchOptions,
    agents_json: &str,
    positional: &str,
) -> Vec<String> {
    let mut args = vec!["--model".to_string(), opts.main_model.clone()];
    let effort = if opts.ultracode {
        Some("ultracode".to_string())
    } else {
        opts.main_effort
            .as_deref()
            .map(str::trim)
            .filter(|effort| !effort.is_empty())
            .map(str::to_string)
    };
    if let Some(effort) = effort {
        args.push("--effort".to_string());
        args.push(effort);
    }
    args.push("--agents".to_string());
    args.push(agents_json.to_string());
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
    fn issue_args_matrix() {
        // Plan mode ON (the issue default), no effort.
        let opts = IssueLaunchOptions {
            model: "fable".to_string(),
            effort: "".to_string(),
            plan_mode: true,
        };
        assert_eq!(
            issue_args(&opts, "do the thing"),
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
        let opts = IssueLaunchOptions {
            model: "opus".to_string(),
            effort: "xhigh".to_string(),
            plan_mode: false,
        };
        assert_eq!(
            issue_args(&opts, "prompt"),
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

        // Whitespace effort → omitted.
        let opts = IssueLaunchOptions {
            model: "sonnet".to_string(),
            effort: "  ".to_string(),
            plan_mode: false,
        };
        assert!(!issue_args(&opts, "p").iter().any(|arg| arg == "--effort"));
    }

    #[test]
    fn from_settings_maps_model_effort_and_issue_plan_mode() {
        let mut settings = Settings::default();
        settings.claude_model = "sonnet".to_string();
        settings.claude_effort = "high".to_string();
        settings.issue_plan_mode = false;
        let opts = IssueLaunchOptions::from_settings(&settings);
        assert_eq!(opts.model, "sonnet");
        assert_eq!(opts.effort, "high");
        assert!(!opts.plan_mode);

        // The stock defaults: fable, no effort, plan mode ON.
        let opts = IssueLaunchOptions::from_settings(&Settings::default());
        assert_eq!(opts.model, "fable");
        assert_eq!(opts.effort, "");
        assert!(opts.plan_mode);
    }

    fn release_options() -> ReleaseLaunchOptions {
        ReleaseLaunchOptions {
            main_model: "fable".to_string(),
            main_effort: Some("high".to_string()),
            subagent_model: "opus".to_string(),
            subagent_effort: Some("high".to_string()),
            ultracode: true,
            plan_mode: false,
        }
    }

    #[test]
    fn release_args_matrix() {
        // Ultracode: `--effort ultracode` (model-independent — fable stays),
        // main effort ignored, agents + skip flag + positional.
        assert_eq!(
            release_args(&release_options(), r#"{"exp-42":{}}"#, "seed"),
            vec![
                "--model",
                "fable",
                "--effort",
                "ultracode",
                "--agents",
                r#"{"exp-42":{}}"#,
                "--mcp-config",
                ".exp-mcp.json",
                "--strict-mcp-config",
                "--dangerously-skip-permissions",
                "seed",
            ]
        );

        // Ultracode OFF: the main effort applies.
        let mut opts = release_options();
        opts.ultracode = false;
        assert_eq!(
            release_args(&opts, "{}", "seed"),
            vec![
                "--model",
                "fable",
                "--effort",
                "high",
                "--agents",
                "{}",
                "--mcp-config",
                ".exp-mcp.json",
                "--strict-mcp-config",
                "--dangerously-skip-permissions",
                "seed",
            ]
        );

        // Ultracode OFF + blank effort → no --effort at all.
        let mut opts = release_options();
        opts.ultracode = false;
        opts.main_effort = Some("  ".to_string());
        assert!(!release_args(&opts, "{}", "seed").iter().any(|arg| arg == "--effort"));

        // Plan mode ON rides the native permission args.
        let mut opts = release_options();
        opts.plan_mode = true;
        let args = release_args(&opts, "{}", "seed");
        assert!(args.windows(2).any(|pair| pair == ["--permission-mode", "plan"]));
        assert!(!args.iter().any(|arg| arg == "--dangerously-skip-permissions"));
        assert_eq!(args.last().map(String::as_str), Some("seed"));
    }
}
