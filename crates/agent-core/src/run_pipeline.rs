//! The per-issue pipeline I/O stages — a port of `buildIssuePipeline` /
//! `producePlanStage` / `codeStage` from `pipeline.ts`. This is where the pure
//! brain (`decide_stage`, prompt builders, `parse_driver_output`) drives the
//! real I/O modules: MCP (read issue / submit plan / comment / status), git
//! (clone / worktree / push), the host-run agent CLI (`agent_run`), and GitHub
//! (repo + PR). It assembles a `dispatcher::PipelineFn`.
//!
//! The driver execution is delegated to the host via `agent_run::request_run`
//! (the core never spawns the CLI). Not unit-tested here (it's the integration
//! of already-tested parts + live I/O); the pure decision logic is covered in
//! `pipeline.rs`.

use crate::agent_run::{self, RunRequest};
use crate::dispatcher::PipelineFn;
use crate::pipeline::{
    self, build_code_user_prompt, build_plan_user_prompt, decide_stage, format_thread_for_prompt,
    latest_approved_plan_text, latest_plan_text, parse_driver_output, DriverOutputKind, IssueDetail, Stage,
    CODE_SYSTEM_PROMPT, PLAN_SYSTEM_PROMPT, PLAN_REVISION_CAP,
};
use crate::state::{IssuePatch, IssueRow, State};
use crate::{git, github, mcp, mcp_config};
use std::sync::{Arc, Mutex};

/// Everything a pipeline run needs. `api_key` is the agent `expk_` (MCP +
/// heartbeat); `github_token` is the GitHub user token (git push + REST).
pub struct Config {
    pub base_url: String,
    pub api_key: String,
    pub bot_user_id: String,
    pub github_token: String,
    pub repos_root: String,
    pub worktrees_root: String,
    pub branch_prefix: String,
    pub driver: String, // "claude" | "codex"
    pub timeout_s: u64,
}

impl Config {
    fn mcp_url(&self) -> String {
        format!("{}/api/mcp", self.base_url.trim_end_matches('/'))
    }
}

pub type Emit = Arc<dyn Fn(&RunRequest) + Send + Sync>;

struct Ctx {
    config: Arc<Config>,
    state: Arc<Mutex<State>>,
    emit: Emit,
}

impl Ctx {
    fn set_status(&self, id: &str, status: &str, error: Option<&str>) {
        let _ = self.state.lock().unwrap().set_issue_status(id, status, error);
    }
    fn patch(&self, id: &str, patch: &IssuePatch) {
        let _ = self.state.lock().unwrap().patch_issue(id, patch);
    }
    fn comment(&self, issue_id: &str, body: &str, kind: Option<&str>) {
        let _ = mcp::create_comment(&self.config.base_url, &self.config.api_key, issue_id, body, kind, self.config.timeout_s);
    }
    /// needs_human, posting the comment only on the first hit (dedup via lastError).
    fn needs_human(&self, issue: &IssueRow, reason: &str, comment: &str) {
        let already = issue.last_error.as_deref() == Some(reason);
        self.set_status(&issue.id, "needs_human", Some(reason));
        if !already {
            self.comment(&issue.id, comment, None);
        }
    }
}

/// Build the dispatcher's pipeline closure.
pub fn build_pipeline(config: Arc<Config>, state: Arc<Mutex<State>>, emit: Emit) -> PipelineFn {
    Arc::new(move |issue: IssueRow| {
        let ctx = Ctx { config: Arc::clone(&config), state: Arc::clone(&state), emit: Arc::clone(&emit) };
        if let Err(message) = run(&ctx, &issue) {
            ctx.set_status(&issue.id, "failed", Some(&message));
            ctx.comment(&issue.id, &format!("Agent encountered an error: {}", &message[..message.len().min(1500)]), None);
        }
    })
}

fn run(ctx: &Ctx, issue: &IssueRow) -> Result<(), String> {
    let cfg = &ctx.config;
    let mut detail = mcp::get_issue(&cfg.base_url, &cfg.api_key, &issue.id, cfg.timeout_s)?
        .ok_or_else(|| "mcp.get_issue returned null".to_string())?;

    // Hard-reset: dispatcher zeroed our plan revision (re-assignment) but the
    // server still has stale plan state → reset + refetch.
    if issue.plan_revision == 0 && detail.agent_plan_state.is_some() {
        let _ = mcp::reset_agent_plan(&cfg.base_url, &cfg.api_key, &issue.id, cfg.timeout_s);
        if let Ok(Some(refreshed)) = mcp::get_issue(&cfg.base_url, &cfg.api_key, &issue.id, cfg.timeout_s) {
            detail = refreshed;
        }
    }

    let decision = decide_stage(&detail, issue.plan_revision);
    if decision.stage == Stage::Noop {
        return Ok(());
    }

    // Both stages need the repo + GitHub auth.
    let project = mcp::get_project(&cfg.base_url, &cfg.api_key, &issue.project_id, cfg.timeout_s)?;
    let owner_repo = match project.and_then(|p| p.github_repo) {
        Some(r) if !r.is_empty() => r,
        _ => {
            ctx.needs_human(issue, "no github repo linked", "No GitHub repo linked for this project. Link one in workspace settings.");
            return Ok(());
        }
    };
    if cfg.github_token.is_empty() {
        ctx.needs_human(issue, "no github authentication", "The desktop agent is not authenticated to GitHub. Connect GitHub in the desktop app.");
        return Ok(());
    }

    let repo_meta = github::get_repo(&cfg.github_token, &owner_repo, cfg.timeout_s)?;
    let handle = git::ensure_repo(&cfg.repos_root, &owner_repo, &repo_meta.default_branch, &cfg.github_token)?;

    match decision.stage {
        Stage::ProducePlan => produce_plan_stage(ctx, issue, &detail, &handle),
        Stage::Code => code_stage(ctx, issue, &detail, &handle),
        Stage::Noop => Ok(()),
    }
}

/// Claim a worktree, write the CLI's MCP config, and return (claim, run_request)
/// for the given mode (shared by both stages).
fn prepare_run(ctx: &Ctx, issue: &IssueRow, handle: &git::RepoHandle, mode: &str, system_prompt: &str, user_prompt: &str) -> Result<(git::WorktreeClaim, RunRequest), String> {
    let cfg = &ctx.config;
    let claim = git::worktree_claim(&cfg.worktrees_root, &cfg.branch_prefix, &handle.repo_path, &handle.default_branch, &issue.identifier, &issue.title)?;
    ctx.patch(&issue.id, &IssuePatch {
        worktree_path: Some(claim.worktree_path.clone()),
        branch: Some(claim.branch.clone()),
        repo_path: Some(claim.repo_path.clone()),
        driver: Some(cfg.driver.clone()),
        ..Default::default()
    });

    let req = if cfg.driver == "codex" {
        agent_run::build_codex_run(&claim.worktree_path, mode, &cfg.mcp_url(), &cfg.api_key, system_prompt, user_prompt)
    } else {
        let mcp_path = mcp_config::write_claude_mcp_json(&claim.worktree_path, &cfg.mcp_url(), &cfg.api_key)?;
        agent_run::build_claude_run(&claim.worktree_path, mode, &mcp_path, system_prompt, user_prompt)
    };
    Ok((claim, req))
}

fn produce_plan_stage(ctx: &Ctx, issue: &IssueRow, detail: &IssueDetail, handle: &git::RepoHandle) -> Result<(), String> {
    if detail.agent_plan_revision >= PLAN_REVISION_CAP {
        ctx.needs_human(
            issue,
            "plan revision cap reached",
            &format!("The agent has revised the plan {PLAN_REVISION_CAP} times without approval. Stopping to avoid a runaway loop — please review and either approve, request changes, or unassign the agent."),
        );
        return Ok(());
    }

    ctx.set_status(&issue.id, "planning", None);
    let thread = format_thread_for_prompt(detail);
    let user_prompt = build_plan_user_prompt(&issue.identifier, &issue.title, &detail.description_text, &thread, latest_plan_text(detail).as_deref());
    let (_claim, req) = prepare_run(ctx, issue, handle, "plan", PLAN_SYSTEM_PROMPT, &user_prompt)?;

    // Tell the server the agent started (best-effort; no-op when already non-null).
    let cfg = &ctx.config;
    let _ = mcp::mark_agent_plan_started(&cfg.base_url, &cfg.api_key, &issue.id, cfg.timeout_s);

    let result = agent_run::request_run(req, |r| (ctx.emit)(r));
    let parsed = parse_driver_output(&result.final_text);

    match parsed.kind {
        DriverOutputKind::Questions => {
            ctx.comment(&issue.id, &parsed.body, Some("question"));
            mcp::submit_agent_plan(&cfg.base_url, &cfg.api_key, &issue.id, "", "awaiting_answer", cfg.timeout_s)?;
        }
        DriverOutputKind::Plan => {
            mcp::submit_agent_plan(&cfg.base_url, &cfg.api_key, &issue.id, &parsed.body, "awaiting_approval", cfg.timeout_s)?;
        }
    }
    ctx.patch(&issue.id, &IssuePatch {
        status: Some("awaiting_approval".to_string()),
        plan_revision: Some(detail.agent_plan_revision + 1),
        ..Default::default()
    });
    Ok(())
}

fn code_stage(ctx: &Ctx, issue: &IssueRow, detail: &IssueDetail, handle: &git::RepoHandle) -> Result<(), String> {
    let cfg = &ctx.config;
    let approved = latest_approved_plan_text(detail)
        .ok_or(()) // mirrors the "no plan-kind comment" needs_human path
        ;
    let approved = match approved {
        Ok(p) => p,
        Err(_) => {
            ctx.set_status(&issue.id, "needs_human", Some("plan approved but no plan-kind comment found"));
            return Ok(());
        }
    };

    ctx.set_status(&issue.id, "claimed", None);
    mcp::update_issue_status(&cfg.base_url, &cfg.api_key, &issue.id, "in_progress", cfg.timeout_s)?;

    let user_prompt = build_code_user_prompt(&issue.identifier, &issue.title, &detail.description_text, &approved);
    let (claim, _req0) = prepare_run(ctx, issue, handle, "code", CODE_SYSTEM_PROMPT, &user_prompt)?;

    // Up to two attempts (mirrors runDriverWithRetry); rebuild the request each
    // attempt so the run_id is fresh and the retry note is appended.
    let mut last_err = String::new();
    let mut ok = false;
    for attempt in 1..=2 {
        ctx.set_status(&issue.id, "coding", None);
        let _ = ctx.state.lock().unwrap().bump_attempts(&issue.id);
        let prompt = if attempt == 1 {
            user_prompt.clone()
        } else {
            format!("{user_prompt}\n\n## Retry {attempt}\n\nPrevious attempt failed. Pay attention to the error and try a different approach.")
        };
        let req = if cfg.driver == "codex" {
            agent_run::build_codex_run(&claim.worktree_path, "code", &cfg.mcp_url(), &cfg.api_key, CODE_SYSTEM_PROMPT, &prompt)
        } else {
            let mcp_path = format!("{}/.mcp.json", claim.worktree_path);
            agent_run::build_claude_run(&claim.worktree_path, "code", &mcp_path, CODE_SYSTEM_PROMPT, &prompt)
        };
        let result = agent_run::request_run(req, |r| (ctx.emit)(r));
        if result.exit_code == 0 {
            ok = true;
            break;
        }
        last_err = format!("agent run exited with code {}", result.exit_code);
    }
    if !ok {
        return Err(if last_err.is_empty() { "agent run failed".to_string() } else { last_err });
    }

    ctx.set_status(&issue.id, "pushed", None);
    git::push_branch(&claim.repo_path, &handle.owner, &handle.repo, &claim.branch, &cfg.github_token)?;

    let (url, _number) = github::create_pull_request(
        &cfg.github_token,
        &handle.owner,
        &handle.repo,
        &claim.branch,
        &handle.default_branch,
        &format!("[{}] {}", issue.identifier, issue.title),
        &pipeline::pr_body(&issue.identifier),
        cfg.timeout_s,
    )?;

    ctx.patch(&issue.id, &IssuePatch { pr_url: Some(url.clone()), ..Default::default() });
    ctx.set_status(&issue.id, "in_review", None);
    ctx.comment(&issue.id, &format!("PR opened: {url}"), None);
    Ok(())
}
