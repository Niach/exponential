//! Claude tasks — the reusable "let Claude handle it" primitive (masterplan v4
//! §4.9). A one-shot **interactive** `claude --model <model>
//! --dangerously-skip-permissions <prompt>` spawned into a terminal-dock tab
//! ([`terminal::TabKind::ClaudeTask`]).
//!
//! Deliberately NOT [`crate::launch`] (DNR L5, invariant 5): a Claude task has
//! **no MCP config, no `PROMPT.md`, no `coding_sessions` row** (not
//! issue-bound: no steer room, no plan-limit charge), and creates no
//! branch/worktree — it runs where it is pointed (`cwd`). Always visible,
//! always steerable by typing; never a hidden background job.
//!
//! Every task passes `--strict-mcp-config` (EXP-83): strict mode ENFORCES the
//! MCP-less posture instead of merely assuming it — nothing the cwd's repo
//! carries can connect. It does NOT suppress claude's project-approval dialog
//! (EXP-98: that startup scan of a cwd `.mcp.json` is unconditional), which
//! is why the coding session's config is named `.exp-mcp.json`
//! ([`crate::mcp_json`]) and why callers pointing a task at a possibly-stale
//! worktree should [`crate::mcp_json::remove_stale_legacy_mcp_json`] first.
//! The one MCP-enabled task ([`claude_task_with_mcp`]) passes that file
//! explicitly via `--mcp-config`, which connects trusted without prompting.
//!
//! The prompt is passed as the positional argument (not typed into the PTY),
//! and `--model` is ALWAYS explicit (DNR invariant 10 — never the CLI default,
//! which may be a scarcer model).
//!
//! v4 callers (§4.9 table): trunk conflict mode → [`fix_conflicts_prompt`];
//! issue Changes tab "Resolve PR conflicts" / "Update from main" →
//! [`resolve_pr_prompt`].

use std::path::Path;

use terminal::pty::SpawnSpec;

use crate::settings::Settings;

/// A ready-to-open Claude task: the argv-direct [`SpawnSpec`] plus the tab
/// title. The UI opens it via the §06 `TerminalManager` with
/// `TabKind::ClaudeTask` (v4 §4.9).
#[derive(Debug, Clone)]
pub struct ClaudeTask {
    pub spawn: SpawnSpec,
    pub tab_title: String,
}

/// Build a Claude task (v4 §4.9): `claude --model <model> --strict-mcp-config
/// --dangerously-skip-permissions <prompt>`, cwd = `cwd`. `label` names the
/// terminal tab (e.g. `Fix conflicts · EXP-42`). Program + model come from
/// [`Settings`] (same source as [`crate::launch`], so the two paths stay in
/// lockstep on the configured `claude` binary and model).
pub fn claude_task(settings: &Settings, cwd: &Path, prompt: &str, label: &str) -> ClaudeTask {
    let spawn = SpawnSpec::new(&settings.resolved_claude_path())
        .args([
            "--model",
            settings.claude_model.as_str(),
            "--strict-mcp-config",
            "--dangerously-skip-permissions",
            prompt,
        ])
        .cwd(cwd);
    ClaudeTask {
        spawn,
        tab_title: label.to_string(),
    }
}

/// [`claude_task`] plus the scoped MCP config the caller wrote into `cwd` —
/// the actions "Describe with Claude" creator (EXP-253 / L24, the ONE
/// MCP-enabled task). The file is passed explicitly (`--mcp-config`, resolved
/// against the spawn cwd) and connects trusted; its non-`.mcp.json` name
/// keeps it out of claude's project-approval dialog scan (EXP-98).
/// `--strict-mcp-config` rides along like every task.
pub fn claude_task_with_mcp(
    settings: &Settings,
    cwd: &Path,
    prompt: &str,
    label: &str,
) -> ClaudeTask {
    let spawn = SpawnSpec::new(&settings.resolved_claude_path())
        .args([
            "--model",
            settings.claude_model.as_str(),
            "--mcp-config",
            crate::mcp_json::MCP_JSON_FILE,
            "--strict-mcp-config",
            "--dangerously-skip-permissions",
            prompt,
        ])
        .cwd(cwd);
    ClaudeTask {
        spawn,
        tab_title: label.to_string(),
    }
}

/// Prompt for the trunk conflict-mode "Fix conflicts with Claude" action
/// (v4 §4.9 table, row 1): a `git pull --rebase` on `branch` stopped on
/// conflicts in `files`. Resolve preserving both sides' intent, continue the
/// rebase/merge, verify the build, and **do NOT push** (the human reviews the
/// trunk before pushing).
pub fn fix_conflicts_prompt(branch: &str, files: &[String]) -> String {
    let file_list = if files.is_empty() {
        "the conflicted files".to_string()
    } else {
        files.join(", ")
    };
    format!(
        "Please resolve a rebase conflict: a `git pull --rebase` on `{branch}` stopped on \
conflicts in {file_list}. Resolve them preserving both sides' intent, run `git rebase --continue` (or \
`git merge --continue`), verify the board still builds, and do NOT push."
    )
}

/// Prompt for the issue Changes tab "Resolve PR conflicts" / "Update from
/// main" actions (v4 §4.9 table, row 2): rebase the issue worktree's branch
/// onto `origin/<default_branch>`, resolve conflicts, verify the build, then
/// push with `--force-with-lease`.
pub fn resolve_pr_prompt(default_branch: &str) -> String {
    format!(
        "Please rebase this branch onto `origin/{default_branch}`, resolve any conflicts, \
verify the build, then push with `--force-with-lease`."
    )
}

/// Prompt for the actions panel's "Describe with Claude" creator (EXP-253 /
/// L24): the ONE MCP-enabled Claude task. It runs in a scratch dir alongside
/// a scoped `.exp-mcp.json` that exposes the `exponential_actions_*` MCP
/// tools, and asks Claude to author ONE action for `team_id` from the
/// user's one-line `description` (optionally seeded by a `template` body).
/// Unlike the conflict prompts it must NOT touch git or files — it only
/// calls the MCP tools.
pub fn create_action_prompt(team_id: &str, description: &str, template: Option<&str>) -> String {
    let template_seed = template
        .map(|body| {
            format!(
                "\n\nUse this template as a starting point, adapting it to the description:\n\
---\n{body}\n---"
            )
        })
        .unwrap_or_default();
    format!(
        "Please create ONE new action for the Exponential team with id `{team_id}`. An \
action is a reusable markdown prompt that a team member later runs as an interactive \
Claude session on their own desktop (the exponential MCP tools are available to that \
run). The user described the action they want as:\n\n\"{description}\"\n\n\
Write a clear, focused markdown body for it: state the goal, the concrete steps, \
which exponential MCP tools to use (e.g. exponential_issues_list / \
exponential_issues_create / exponential_labels_list), and what to report at the end. \
Call `exponential_actions_list` for the team first so the name doesn't collide. \
Leave `repositoryId` unset unless the description clearly needs repository access \
(then pick the right repo id from `exponential_repositories_list`). Create the \
action with `exponential_actions_create` (teamId, a short name, a one-line \
description, the markdown body). Do not commit, push, or change any files — only \
call the MCP tools.{template_seed}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn settings() -> Settings {
        Settings {
            claude_path: "/opt/homebrew/bin/claude".to_string(),
            repos_root: "~/repos".to_string(),
            branch_prefix: "exp/".to_string(),
            claude_model: "opus".to_string(),
            ..Settings::default()
        }
    }

    #[test]
    fn task_is_argv_direct_model_explicit_prompt_positional() {
        let cwd = PathBuf::from("/repos/acme/web");
        let task = claude_task(&settings(), &cwd, "do the thing", "Fix conflicts · EXP-42");
        // program = the configured claude binary, verbatim (never a shell).
        assert_eq!(task.spawn.program, "/opt/homebrew/bin/claude");
        // Exactly: --model <model> --strict-mcp-config
        // --dangerously-skip-permissions <prompt>. `--model` is
        // explicit-ALWAYS (never the CLI default), strict MCP enforces the
        // task's MCP-less posture even in a worktree carrying a session
        // `.exp-mcp.json` (EXP-83/EXP-98), and the prompt is the positional
        // arg (not typed into the PTY).
        assert_eq!(
            task.spawn.args,
            vec![
                "--model".to_string(),
                "opus".to_string(),
                "--strict-mcp-config".to_string(),
                "--dangerously-skip-permissions".to_string(),
                "do the thing".to_string(),
            ]
        );
        assert_eq!(task.spawn.cwd.as_deref(), Some(cwd.as_path()));
        assert_eq!(task.tab_title, "Fix conflicts · EXP-42");
    }

    #[test]
    fn task_sets_no_env_and_carries_no_session_state() {
        // A Claude task is NOT a coding launch (v4 §4.9): no MCP config, no
        // `PROMPT.md`, no `coding_sessions` row. The spec surfaces as: an empty
        // env overlay (PATH augmentation is the terminal layer's §6.12 job, not
        // ours) and a struct that carries only the spawn + tab title — there is
        // no session id / worktree / branch field to bind a room or charge a
        // plan limit.
        let task = claude_task(&settings(), &PathBuf::from("/tmp"), "p", "T");
        assert!(task.spawn.env.is_empty());
        // argv references none of the launch-only seed files.
        assert!(!task
            .spawn
            .args
            .iter()
            .any(|a| a.contains("mcp.json") || a.contains("PROMPT.md")));
    }

    #[test]
    fn mcp_task_passes_the_scoped_config_explicitly_and_strictly() {
        // The ONE MCP-enabled task (the actions creator): the caller-written
        // `.exp-mcp.json` rides `--mcp-config` (connects trusted; the name is
        // invisible to claude's project-approval scan — EXP-98) with
        // `--strict-mcp-config` so repo-carried MCP config never connects.
        let cwd = PathBuf::from("/repos/acme/web");
        let task = claude_task_with_mcp(&settings(), &cwd, "make an action", "New action");
        assert_eq!(task.spawn.program, "/opt/homebrew/bin/claude");
        assert_eq!(
            task.spawn.args,
            vec![
                "--model".to_string(),
                "opus".to_string(),
                "--mcp-config".to_string(),
                ".exp-mcp.json".to_string(),
                "--strict-mcp-config".to_string(),
                "--dangerously-skip-permissions".to_string(),
                "make an action".to_string(),
            ]
        );
        assert_eq!(task.spawn.cwd.as_deref(), Some(cwd.as_path()));
    }

    #[test]
    fn task_uses_the_configured_model_verbatim() {
        let mut settings = settings();
        settings.claude_model = "sonnet".to_string();
        let task = claude_task(&settings, &PathBuf::from("/tmp"), "p", "T");
        assert_eq!(task.spawn.args[1], "sonnet");
    }

    #[test]
    fn fix_conflicts_prompt_matches_the_table_with_named_files() {
        let prompt = fix_conflicts_prompt("main", &["src/app.rs".into(), "Cargo.lock".into()]);
        assert_eq!(
            prompt,
            "Please resolve a rebase conflict: a `git pull --rebase` on `main` stopped on \
conflicts in src/app.rs, Cargo.lock. Resolve them preserving both sides' intent, run `git rebase --continue` (or \
`git merge --continue`), verify the board still builds, and do NOT push."
        );
        // The trunk conflict prompt must never instruct a push (the human
        // reviews the trunk before pushing — v4 §4.9 row 1).
        assert!(!prompt.contains("push with"));
        assert!(prompt.contains("do NOT push"));
    }

    #[test]
    fn fix_conflicts_prompt_falls_back_when_no_files_named() {
        let prompt = fix_conflicts_prompt("develop", &[]);
        assert!(prompt.contains("stopped on conflicts in the conflicted files."));
        assert!(prompt.contains("on `develop`"));
    }

    #[test]
    fn create_action_prompt_targets_the_team_and_the_mcp_tools() {
        let prompt = create_action_prompt("team-123", "review the backlog weekly", None);
        // Names the exact team so Claude passes the right teamId.
        assert!(prompt.contains("team-123"));
        // Carries the user's one-line description verbatim.
        assert!(prompt.contains("review the backlog weekly"));
        // Points at the actions MCP tools (the scoped .exp-mcp.json exposes them).
        assert!(prompt.contains("exponential_actions_create"));
        assert!(prompt.contains("exponential_actions_list"));
        // Read-only w.r.t. the tree — this task must not commit or push.
        assert!(prompt.contains("Do not commit, push"));
        // No template → no seed section.
        assert!(!prompt.contains("starting point"));
    }

    #[test]
    fn create_action_prompt_seeds_the_template_body() {
        let prompt = create_action_prompt(
            "team-123",
            "code review",
            Some("# Code review\nScan the repo."),
        );
        assert!(prompt.contains("starting point"));
        assert!(prompt.contains("# Code review\nScan the repo."));
    }

    #[test]
    fn resolve_pr_prompt_matches_the_table_and_force_pushes() {
        let prompt = resolve_pr_prompt("main");
        assert_eq!(
            prompt,
            "Please rebase this branch onto `origin/main`, resolve any conflicts, \
verify the build, then push with `--force-with-lease`."
        );
        // The worktree prompt DOES push (Claude owns its branch — v4 §4.9 row 2).
        assert!(prompt.contains("--force-with-lease"));
    }
}
