//! Claude tasks — the reusable "let Claude handle it" primitive (masterplan v4
//! §4.9). A one-shot **interactive** `claude --model <model>
//! --dangerously-skip-permissions <prompt>` spawned into a terminal-dock tab
//! ([`terminal::TabKind::ClaudeTask`]).
//!
//! Deliberately NOT [`crate::launch`] (DNR L5, invariant 5): a Claude task has
//! **no `.mcp.json`, no `PROMPT.md`, no `coding_sessions` row** (not
//! issue-bound: no steer room, no plan-limit charge), and creates no
//! branch/worktree — it runs where it is pointed (`cwd`). Always visible,
//! always steerable by typing; never a hidden background job.
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

/// Build a Claude task (v4 §4.9): `claude --model <model>
/// --dangerously-skip-permissions <prompt>`, cwd = `cwd`. `label` names the
/// terminal tab (e.g. `Fix conflicts · EXP-42`). Program + model come from
/// [`Settings`] (same source as [`crate::launch`], so the two paths stay in
/// lockstep on the configured `claude` binary and model).
pub fn claude_task(settings: &Settings, cwd: &Path, prompt: &str, label: &str) -> ClaudeTask {
    let spawn = SpawnSpec::new(&settings.claude_path)
        .args([
            "--model",
            settings.claude_model.as_str(),
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
        "A `git pull --rebase` on `{branch}` stopped on conflicts in {file_list}. \
Resolve them preserving both sides' intent, run `git rebase --continue` (or \
`git merge --continue`), verify the project still builds, and do NOT push."
    )
}

/// Prompt for the issue Changes tab "Resolve PR conflicts" / "Update from
/// main" actions (v4 §4.9 table, row 2): rebase the issue worktree's branch
/// onto `origin/<default_branch>`, resolve conflicts, verify the build, then
/// push with `--force-with-lease`.
pub fn resolve_pr_prompt(default_branch: &str) -> String {
    format!(
        "Rebase this branch onto `origin/{default_branch}`, resolve any conflicts, \
verify the build, then push with `--force-with-lease`."
    )
}

/// Prompt for the run-configs editor's "Create with Claude" action (v5 §7.3 /
/// L24): the ONE MCP-enabled Claude task. It runs in the project's trunk clone
/// alongside a scoped `.mcp.json` that exposes the `exponential_run_configs_*`
/// MCP tools, and asks Claude to inspect the repo and create run configs for
/// `project_id`. Unlike the conflict prompts it does NOT touch git — it only
/// reads the repo and calls the MCP tools.
pub fn create_run_configs_prompt(project_id: &str) -> String {
    format!(
        "Inspect this repository — its README, package.json, Cargo.toml, Makefile, \
justfile, docker-compose, and scripts — to learn how it is developed, built, run, \
tested, and linted. Then create a small set of useful run configurations for the \
Exponential project with id `{project_id}` using the `exponential_run_configs_create` \
MCP tool (one per common task, e.g. dev server, build, test, lint). Each config's \
argv is spawned directly with NO shell, so argv[0] must be the program and the rest \
its arguments; set cwd (repo-relative, no \"..\") and env only when needed. Call \
`exponential_run_configs_list` for that project first so you don't create duplicates. \
Do not commit, push, or change any files — only call the run-config MCP tools."
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
        }
    }

    #[test]
    fn task_is_argv_direct_model_explicit_prompt_positional() {
        let cwd = PathBuf::from("/repos/acme/web");
        let task = claude_task(&settings(), &cwd, "do the thing", "Fix conflicts · EXP-42");
        // program = the configured claude binary, verbatim (never a shell).
        assert_eq!(task.spawn.program, "/opt/homebrew/bin/claude");
        // Exactly: --model <model> --dangerously-skip-permissions <prompt>.
        // `--model` is explicit-ALWAYS (never the CLI default) and the prompt
        // is the positional arg (not typed into the PTY).
        assert_eq!(
            task.spawn.args,
            vec![
                "--model".to_string(),
                "opus".to_string(),
                "--dangerously-skip-permissions".to_string(),
                "do the thing".to_string(),
            ]
        );
        assert_eq!(task.spawn.cwd.as_deref(), Some(cwd.as_path()));
        assert_eq!(task.tab_title, "Fix conflicts · EXP-42");
    }

    #[test]
    fn task_sets_no_env_and_carries_no_session_state() {
        // A Claude task is NOT a coding launch (v4 §4.9): no `.mcp.json`, no
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
            .any(|a| a.contains(".mcp.json") || a.contains("PROMPT.md")));
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
            "A `git pull --rebase` on `main` stopped on conflicts in src/app.rs, Cargo.lock. \
Resolve them preserving both sides' intent, run `git rebase --continue` (or \
`git merge --continue`), verify the project still builds, and do NOT push."
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
    fn create_run_configs_prompt_targets_the_project_and_the_mcp_tools() {
        let prompt = create_run_configs_prompt("proj-123");
        // Names the exact project so Claude passes the right projectId.
        assert!(prompt.contains("proj-123"));
        // Points at the run-config MCP tools (the scoped .mcp.json exposes them).
        assert!(prompt.contains("exponential_run_configs_create"));
        assert!(prompt.contains("exponential_run_configs_list"));
        // No-shell posture is spelled out for the argv-direct spawner.
        assert!(prompt.contains("NO shell"));
        // Read-only w.r.t. the tree — this task must not commit or push.
        assert!(prompt.contains("Do not commit, push"));
    }

    #[test]
    fn resolve_pr_prompt_matches_the_table_and_force_pushes() {
        let prompt = resolve_pr_prompt("main");
        assert_eq!(
            prompt,
            "Rebase this branch onto `origin/main`, resolve any conflicts, \
verify the build, then push with `--force-with-lease`."
        );
        // The worktree prompt DOES push (Claude owns its branch — v4 §4.9 row 2).
        assert!(prompt.contains("--force-with-lease"));
    }
}
