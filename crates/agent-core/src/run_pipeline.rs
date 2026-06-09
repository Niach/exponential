//! The per-issue pipeline I/O stages — a port of `buildIssuePipeline` /
//! `producePlanStage` / `codeStage` from `pipeline.ts`. This is where the pure
//! brain (`decide_stage`, prompt builders, `parse_driver_output`) drives the
//! real I/O modules: MCP (read issue / submit plan / comment / status), git
//! (clone / worktree / push), the agent CLI (`agent_run`), and GitHub
//! (repo + PR). It assembles a `dispatcher::PipelineFn`.
//!
//! With `Config::interactive` (the default for the claude driver) BOTH the plan
//! and the code stage run as live sessions in the host's embedded terminal —
//! the user watches, steers, and answers claude's permission prompts. The plan
//! is delivered out-of-band via the MCP plan-submit tool (verified afterwards
//! through `mcp::get_issue`); after a code session exits the CORE checks for
//! commits, pushes the branch, and opens the PR. Codex (`codex exec`) and
//! `interactive: false` keep the fully headless in-core path.
//!
//! Not unit-tested here (it's the integration of already-tested parts + live
//! I/O); the pure decision logic is covered in `pipeline.rs`.

use crate::agent_run::{self, RunRequest, EXIT_CANCELLED};
use crate::dispatcher::PipelineFn;
use crate::pipeline::{
    build_code_user_prompt, build_interactive_plan_user_prompt, build_plan_user_prompt,
    decide_stage, format_thread_for_prompt, latest_approved_plan_text, latest_plan_text,
    parse_driver_output, DriverOutputKind, IssueDetail, Stage, CODE_SYSTEM_PROMPT,
    INTERACTIVE_PLAN_SYSTEM_PROMPT, PLAN_SYSTEM_PROMPT, PLAN_REVISION_CAP,
};
use crate::state::{IssuePatch, IssueRow, IssueSeed, State};
use crate::{git, mcp, mcp_config, trpc};
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
    /// Wall-clock cap for headless runs (`runTimeoutS`, default 30min).
    /// Interactive sessions are untimed — the user owns them; Cancel is the
    /// escape hatch.
    pub run_timeout_s: u64,
    /// Route the claude driver's plan/code stages through the host's embedded
    /// terminal as live sessions (default). Codex ignores this (no interactive
    /// exec mode).
    pub interactive: bool,
}

impl Config {
    fn mcp_url(&self) -> String {
        format!("{}/api/mcp", self.base_url.trim_end_matches('/'))
    }
    fn wants_interactive(&self) -> bool {
        self.interactive && self.driver != "codex"
    }
}

/// Everything the core tells the host beyond log lines. `RunRequest` demands a
/// response (`agent_core_submit_run_result`); the rest are fire-and-forget UI
/// signals (toasts, run indicators, terminal teardown, structured errors).
pub enum HostEvent<'a> {
    RunRequest(&'a RunRequest),
    RunStarted { issue_id: &'a str, issue_identifier: &'a str, run_id: &'a str, mode: &'a str },
    RunFinished { issue_id: &'a str, run_id: &'a str, exit_code: i32, outcome: &'a str },
    RunCancelled { issue_id: &'a str, run_id: &'a str },
    AgentError { issue_id: &'a str, code: &'a str, message: &'a str },
}

pub type Emit = Arc<dyn Fn(&HostEvent) + Send + Sync>;

/// Stable machine codes for the blocked/error states the host renders
/// structured guidance for (the human message may change; these must not).
pub const ERROR_CODE_REPO_NOT_LINKED: &str = "repo_not_linked";
pub const ERROR_CODE_REPO_TOKEN_UNAVAILABLE: &str = "repo_token_unavailable";

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
    fn fresh_row(&self, id: &str) -> Option<IssueRow> {
        self.state.lock().unwrap().get_issue(id).ok().flatten()
    }
    fn emit_event(&self, ev: &HostEvent) {
        (self.emit)(ev);
    }
    /// needs_human, reporting the error event only on the first hit (dedup via
    /// lastError). The `code` is a stable machine string (also the dedup key);
    /// the message surfaces in the issue's Plan Panel error card + activity feed.
    fn needs_human(&self, issue: &IssueRow, code: &str, message: &str) {
        let already = issue.last_error.as_deref() == Some(code);
        self.set_status(&issue.id, "needs_human", Some(code));
        if !already {
            self.report_error(&issue.id, code, message);
        }
    }
    fn report_error(&self, issue_id: &str, code: &str, message: &str) {
        self.emit_event(&HostEvent::AgentError { issue_id, code, message });
        let _ = mcp::report_error(&self.config.base_url, &self.config.api_key, issue_id, message, self.config.timeout_s);
    }
}

/// Build the dispatcher's pipeline closure.
pub fn build_pipeline(config: Arc<Config>, state: Arc<Mutex<State>>, emit: Emit) -> PipelineFn {
    Arc::new(move |issue: IssueRow| {
        let ctx = Ctx { config: Arc::clone(&config), state: Arc::clone(&state), emit: Arc::clone(&emit) };
        if let Err(message) = run(&ctx, &issue) {
            ctx.set_status(&issue.id, "failed", Some(&message));
            let text = format!("Agent encountered an error: {}", &message[..message.len().min(1500)]);
            ctx.report_error(&issue.id, "pipeline_failed", &text);
        }
    })
}

/// Resolve the issue's repo + GitHub auth (shared by the headless + interactive
/// paths). Returns Err on a hard failure; the needs_human (blocked) cases are
/// surfaced by the caller. None = blocked (needs_human already posted).
fn resolve_handle(ctx: &Ctx, issue: &IssueRow) -> Result<Option<git::RepoHandle>, String> {
    let cfg = &ctx.config;
    let project = mcp::get_project(&cfg.base_url, &cfg.api_key, &issue.project_id, cfg.timeout_s)?;
    let owner_repo = match project.and_then(|p| p.github_repo) {
        Some(r) if !r.is_empty() => r,
        _ => {
            ctx.needs_human(issue, ERROR_CODE_REPO_NOT_LINKED, "No GitHub repo linked for this project. Link one in workspace settings.");
            return Ok(None);
        }
    };
    let token = repo_token(cfg, &owner_repo);
    if token.is_empty() {
        ctx.needs_human(issue, ERROR_CODE_REPO_TOKEN_UNAVAILABLE, "The Exponential GitHub App isn't installed on this repo. Install it from Account \u{2192} Integrations on the web app.");
        return Ok(None);
    }
    let repo_meta = github::get_repo(&token, &owner_repo, cfg.timeout_s)?;
    let handle = git::ensure_repo(&cfg.repos_root, &owner_repo, &repo_meta.default_branch, &token)?;
    Ok(Some(handle))
}

use crate::github;

/// A GitHub token for `repo` ("owner/name"): a fresh, short-lived **App
/// installation token** fetched from the server (agent.repoToken). Falls
/// back to a configured token during the transition. Empty if neither.
fn repo_token(cfg: &Config, repo: &str) -> String {
    if let Some(t) = trpc::repo_token(&cfg.base_url, &cfg.api_key, repo, cfg.timeout_s) {
        return t;
    }
    cfg.github_token.clone()
}

/// Host-triggered interactive plan run (the desktop "AI" button — works on any
/// issue, assigned or not). Launches `claude` interactively in the embedded
/// terminal; the plan is delivered out-of-band via MCP (no stdout parsing).
pub fn run_interactive_plan(config: Arc<Config>, state: Arc<Mutex<State>>, emit: Emit, issue_id: &str) {
    let ctx = Ctx { config, state, emit };
    if let Err(message) = run_interactive(&ctx, issue_id, false) {
        ctx.set_status(issue_id, "failed", Some(&message));
        ctx.report_error(issue_id, "interactive_failed", &format!("Interactive run failed: {message}"));
    }
}

/// Host-triggered interactive continue (the desktop "Approve & continue here",
/// for issues NOT assigned to the agent — assigned issues resume through the
/// dispatcher on the approval event). The host has already approved the plan
/// with the human's session; here we resume the same claude session in the
/// reused worktree to implement it.
pub fn run_interactive_continue(config: Arc<Config>, state: Arc<Mutex<State>>, emit: Emit, issue_id: &str) {
    let ctx = Ctx { config, state, emit };
    if let Err(message) = run_interactive(&ctx, issue_id, true) {
        ctx.set_status(issue_id, "failed", Some(&message));
        ctx.report_error(issue_id, "interactive_failed", &format!("Interactive continue failed: {message}"));
    }
}

fn run_interactive(ctx: &Ctx, issue_id: &str, continuing: bool) -> Result<(), String> {
    let cfg = &ctx.config;
    // One live run per issue — refuse a second AI-button press while a session
    // (or a dispatcher run) is in flight.
    if agent_run::run_for_issue(issue_id).is_some() {
        return Err("a run is already in flight for this issue".into());
    }
    let detail = mcp::get_issue(&cfg.base_url, &cfg.api_key, issue_id, cfg.timeout_s)?
        .ok_or_else(|| "issue not found".to_string())?;
    let issue = match ctx.fresh_row(issue_id) {
        Some(row) => row,
        // The "AI" button starts an interactive session on ANY issue, but the
        // dispatcher only syncs issues ASSIGNED to the agent into local state.
        // Hydrate an unassigned issue from the server (MCP) on first use.
        None => {
            ctx.state
                .lock()
                .unwrap()
                .upsert_issue(&IssueSeed {
                    id: issue_id,
                    identifier: &detail.identifier,
                    title: &detail.title,
                    project_id: &detail.project_id,
                    status: &detail.status,
                })
                .map_err(|e| format!("seed issue: {e}"))?;
            ctx.fresh_row(issue_id).ok_or_else(|| "issue not found after hydrate".to_string())?
        }
    };
    let Some(handle) = resolve_handle(ctx, &issue)? else {
        return Ok(());
    };
    if continuing {
        interactive_code_session(ctx, &issue, &detail, &handle)
    } else {
        interactive_plan_session(ctx, &issue, &detail, &handle)
    }
}

/// A live plan session in the host terminal. Claims a fresh worktree, pins the
/// claude session id, holds `interactive_owned` for exactly the session's
/// lifetime, and verifies the out-of-band MCP plan submission after exit.
fn interactive_plan_session(ctx: &Ctx, issue: &IssueRow, detail: &IssueDetail, handle: &git::RepoHandle) -> Result<(), String> {
    let cfg = &ctx.config;
    ctx.set_status(&issue.id, "planning", None);
    let claim = git::worktree_claim(&cfg.worktrees_root, &cfg.branch_prefix, &handle.repo_path, &handle.default_branch, &issue.identifier, &issue.title)?;
    // Mark interactive_owned BEFORE the run so racing `updated` events can't
    // start a second pipeline while the session is live.
    ctx.patch(&issue.id, &IssuePatch {
        worktree_path: Some(claim.worktree_path.clone()),
        branch: Some(claim.branch.clone()),
        repo_path: Some(claim.repo_path.clone()),
        driver: Some("claude".to_string()),
        interactive_owned: Some(1),
        ..Default::default()
    });
    let mcp_path = match mcp_config::write_claude_mcp_json(&claim.worktree_path, &cfg.mcp_url(), &cfg.api_key) {
        Ok(p) => p,
        Err(e) => {
            // Nothing ran yet — a fresh plan worktree holds no work. Clean up
            // fully so the failure leaves no on-disk or ownership residue.
            git::worktree_cleanup(&cfg.branch_prefix, &claim);
            ctx.patch(&issue.id, &IssuePatch { interactive_owned: Some(0), ..Default::default() });
            return Err(format!("mcp config: {e}"));
        }
    };
    let _ = mcp::mark_agent_plan_started(&cfg.base_url, &cfg.api_key, &issue.id, cfg.timeout_s);

    // Pin the session identity up-front so a later code stage can --resume it
    // deterministically (no log scraping).
    let session_id = agent_run::new_session_uuid();
    if let Some(sid) = &session_id {
        ctx.patch(&issue.id, &IssuePatch { claude_session_id: Some(sid.clone()), ..Default::default() });
    }

    let thread = format_thread_for_prompt(detail);
    let user_prompt = build_interactive_plan_user_prompt(
        &issue.id,
        &issue.identifier,
        &issue.title,
        &detail.description_text,
        &thread,
        latest_plan_text(detail).as_deref(),
    );
    let mut req = agent_run::build_claude_interactive_run(
        &claim.worktree_path,
        &mcp_path,
        INTERACTIVE_PLAN_SYSTEM_PROMPT,
        &user_prompt,
        "plan",
        None,
        session_id.as_deref(),
    );
    req.issue_id = issue.id.clone();
    req.issue_identifier = issue.identifier.clone();
    let run_id = req.run_id.clone();

    ctx.emit_event(&HostEvent::RunStarted { issue_id: &issue.id, issue_identifier: &issue.identifier, run_id: &run_id, mode: "plan" });
    let result = agent_run::request_run_for_issue(&issue.id, req, |r| ctx.emit_event(&HostEvent::RunRequest(r)));

    // Session-id fallback for platforms without /dev/urandom: recover from
    // claude's on-disk session log.
    if session_id.is_none() {
        if let Some(sid) = result.session_id.clone().or_else(|| crate::session::find_latest_session_id(&claim.worktree_path)) {
            ctx.patch(&issue.id, &IssuePatch { claude_session_id: Some(sid), ..Default::default() });
        }
    }
    // The session is over — release ownership unconditionally.
    ctx.patch(&issue.id, &IssuePatch { interactive_owned: Some(0), ..Default::default() });
    let outcome = if result.exit_code == EXIT_CANCELLED { "cancelled" } else if result.exit_code == 0 { "ok" } else { "failed" };
    ctx.emit_event(&HostEvent::RunFinished { issue_id: &issue.id, run_id: &run_id, exit_code: result.exit_code, outcome });

    if result.exit_code == EXIT_CANCELLED {
        ctx.set_status(&issue.id, "cancelled", Some("cancelled by user"));
        // Don't leave the Plan Panel hanging at "Planning…".
        let _ = mcp::reset_agent_plan(&cfg.base_url, &cfg.api_key, &issue.id, cfg.timeout_s);
        return Ok(());
    }

    // The plan arrives out-of-band (MCP plan-submit inside the session) — the
    // exit code alone proves nothing. Verify against the server.
    let refreshed = mcp::get_issue(&cfg.base_url, &cfg.api_key, &issue.id, cfg.timeout_s)?;
    let submitted = refreshed.as_ref().and_then(|d| d.agent_plan_state.as_deref().map(|s| (s.to_string(), d.agent_plan_revision)));
    match submitted {
        Some((state, revision)) if state == "awaiting_approval" || state == "awaiting_answer" => {
            ctx.patch(&issue.id, &IssuePatch {
                status: Some("awaiting_approval".to_string()),
                plan_revision: Some(revision),
                ..Default::default()
            });
            Ok(())
        }
        _ => {
            // Reset the server's "planning" marker so the panel offers a retry.
            let _ = mcp::reset_agent_plan(&cfg.base_url, &cfg.api_key, &issue.id, cfg.timeout_s);
            ctx.needs_human(issue, "plan_not_submitted", "The interactive plan session ended without submitting a plan. Re-run the agent to plan again.");
            Ok(())
        }
    }
}

/// A live code session in the host terminal, resuming the plan session's
/// conversation in the reused worktree. The session commits; the CORE then
/// verifies commits exist, pushes the branch, and opens the PR (deterministic
/// lifecycle regardless of how the session went).
fn interactive_code_session(ctx: &Ctx, issue: &IssueRow, detail: &IssueDetail, handle: &git::RepoHandle) -> Result<(), String> {
    let cfg = &ctx.config;
    ctx.set_status(&issue.id, "claimed", None);
    let _ = mcp::update_issue_status(&cfg.base_url, &cfg.api_key, &issue.id, "in_progress", cfg.timeout_s);

    // Reuse (no -B reset) so the plan session's working tree + any commits from
    // a prior attempt survive.
    let claim = git::worktree_reuse(&cfg.worktrees_root, &cfg.branch_prefix, &handle.repo_path, &handle.default_branch, &issue.identifier, &issue.title)?;
    ctx.patch(&issue.id, &IssuePatch {
        worktree_path: Some(claim.worktree_path.clone()),
        branch: Some(claim.branch.clone()),
        repo_path: Some(claim.repo_path.clone()),
        driver: Some("claude".to_string()),
        interactive_owned: Some(1),
        ..Default::default()
    });
    let mcp_path = match mcp_config::write_claude_mcp_json(&claim.worktree_path, &cfg.mcp_url(), &cfg.api_key) {
        Ok(p) => p,
        Err(e) => {
            // The worktree may hold plan-session work — never delete it here.
            ctx.patch(&issue.id, &IssuePatch { interactive_owned: Some(0), ..Default::default() });
            return Err(format!("mcp config: {e}"));
        }
    };

    let base = match latest_approved_plan_text(detail) {
        Some(plan) => build_code_user_prompt(&issue.identifier, &issue.title, &detail.description_text, &plan),
        None => format!("# Issue {}: {}\n\n## Description\n{}", issue.identifier, issue.title, detail.description_text),
    };
    let user_prompt = format!(
        "Your plan was approved — implement it now.\n\n{base}\n\nMake the changes, run the relevant checks, then stage and COMMIT locally with a descriptive message. Do NOT push and do NOT open a PR — the agent core pushes the branch and opens the PR after this session ends."
    );

    ctx.set_status(&issue.id, "coding", None);
    let _ = ctx.state.lock().unwrap().bump_attempts(&issue.id);

    // Resume the plan session when we know it; otherwise pin a fresh id.
    let resume = ctx.fresh_row(&issue.id).and_then(|r| r.claude_session_id).filter(|s| !s.is_empty());
    let new_session = if resume.is_none() { agent_run::new_session_uuid() } else { None };
    if let Some(sid) = &new_session {
        ctx.patch(&issue.id, &IssuePatch { claude_session_id: Some(sid.clone()), ..Default::default() });
    }
    let mut req = agent_run::build_claude_interactive_run(
        &claim.worktree_path,
        &mcp_path,
        CODE_SYSTEM_PROMPT,
        &user_prompt,
        "code",
        resume.as_deref(),
        new_session.as_deref(),
    );
    req.issue_id = issue.id.clone();
    req.issue_identifier = issue.identifier.clone();
    let run_id = req.run_id.clone();

    ctx.emit_event(&HostEvent::RunStarted { issue_id: &issue.id, issue_identifier: &issue.identifier, run_id: &run_id, mode: "code" });
    let result = agent_run::request_run_for_issue(&issue.id, req, |r| ctx.emit_event(&HostEvent::RunRequest(r)));

    ctx.patch(&issue.id, &IssuePatch { interactive_owned: Some(0), ..Default::default() });
    let outcome = if result.exit_code == EXIT_CANCELLED { "cancelled" } else if result.exit_code == 0 { "ok" } else { "failed" };
    ctx.emit_event(&HostEvent::RunFinished { issue_id: &issue.id, run_id: &run_id, exit_code: result.exit_code, outcome });

    if result.exit_code == EXIT_CANCELLED {
        ctx.set_status(&issue.id, "cancelled", Some("cancelled by user"));
        return Ok(());
    }

    // The session's job was to commit. No commits → nothing to push; hand back
    // to the human instead of opening an empty PR.
    if !git::branch_has_commits(&claim.worktree_path, &handle.default_branch) {
        ctx.needs_human(issue, "no_commits", "The code session ended without committing any changes. Approve the plan again (or re-assign) to retry — the worktree is preserved.");
        return Ok(());
    }

    ctx.set_status(&issue.id, "pushed", None);
    let push_token = repo_token(cfg, &format!("{}/{}", handle.owner, handle.repo));
    git::push_branch(&claim.repo_path, &handle.owner, &handle.repo, &claim.branch, &push_token)?;

    // The SERVER opens the PR with the owner's connected GitHub token + records
    // pr_* + pr_opened (the agent doesn't create the PR via the API itself).
    let url = mcp::open_pr(&cfg.base_url, &cfg.api_key, &issue.id, &claim.branch, &handle.default_branch, cfg.timeout_s)?;
    ctx.patch(&issue.id, &IssuePatch { pr_url: Some(url), ..Default::default() });
    ctx.set_status(&issue.id, "in_review", None);
    Ok(())
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
    let Some(handle) = resolve_handle(ctx, issue)? else {
        return Ok(());
    };

    match decision.stage {
        Stage::ProducePlan => produce_plan_stage(ctx, issue, &detail, &handle),
        Stage::Code => code_stage(ctx, issue, &detail, &handle),
        Stage::Noop => Ok(()),
    }
}

/// Claim a worktree, write the CLI's MCP config, and return (claim, run_request)
/// for the given mode (shared by both headless stages).
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

    let mut req = if cfg.driver == "codex" {
        agent_run::build_codex_run(&claim.worktree_path, mode, &cfg.mcp_url(), &cfg.api_key, system_prompt, user_prompt)
    } else {
        let mcp_path = mcp_config::write_claude_mcp_json(&claim.worktree_path, &cfg.mcp_url(), &cfg.api_key)?;
        agent_run::build_claude_run(&claim.worktree_path, mode, &mcp_path, system_prompt, user_prompt)
    };
    req.issue_id = issue.id.clone();
    req.issue_identifier = issue.identifier.clone();
    Ok((claim, req))
}

fn produce_plan_stage(ctx: &Ctx, issue: &IssueRow, detail: &IssueDetail, handle: &git::RepoHandle) -> Result<(), String> {
    if detail.agent_plan_revision >= PLAN_REVISION_CAP {
        ctx.needs_human(
            issue,
            "plan_revision_cap",
            &format!("The agent has revised the plan {PLAN_REVISION_CAP} times without approval. Stopping to avoid a runaway loop — please review and either approve, request changes, or unassign the agent."),
        );
        return Ok(());
    }
    let cfg = &ctx.config;
    if cfg.wants_interactive() {
        return interactive_plan_session(ctx, issue, detail, handle);
    }

    ctx.set_status(&issue.id, "planning", None);
    let thread = format_thread_for_prompt(detail);
    let user_prompt = build_plan_user_prompt(&issue.identifier, &issue.title, &detail.description_text, &thread, latest_plan_text(detail).as_deref());
    let (_claim, req) = prepare_run(ctx, issue, handle, "plan", PLAN_SYSTEM_PROMPT, &user_prompt)?;

    // Tell the server the agent started (best-effort; no-op when already non-null).
    let _ = mcp::mark_agent_plan_started(&cfg.base_url, &cfg.api_key, &issue.id, cfg.timeout_s);

    let run_id = req.run_id.clone();
    ctx.emit_event(&HostEvent::RunStarted { issue_id: &issue.id, issue_identifier: &issue.identifier, run_id: &run_id, mode: "plan" });
    let result = agent_run::run_headless_for_issue(&issue.id, &req, cfg.run_timeout_s);
    let outcome = if result.exit_code == EXIT_CANCELLED { "cancelled" } else if result.exit_code == 0 { "ok" } else { "failed" };
    ctx.emit_event(&HostEvent::RunFinished { issue_id: &issue.id, run_id: &run_id, exit_code: result.exit_code, outcome });

    if result.exit_code == EXIT_CANCELLED {
        ctx.set_status(&issue.id, "cancelled", Some("cancelled by user"));
        let _ = mcp::reset_agent_plan(&cfg.base_url, &cfg.api_key, &issue.id, cfg.timeout_s);
        return Ok(());
    }
    if result.exit_code != 0 {
        return Err(format!("plan run exited with code {}", result.exit_code));
    }
    let parsed = parse_driver_output(&result.final_text);

    match parsed.kind {
        DriverOutputKind::Questions => {
            // Questions go through the structured field now — not a comment.
            mcp::submit_agent_plan(&cfg.base_url, &cfg.api_key, &issue.id, "", "awaiting_answer", Some(&parsed.body), cfg.timeout_s)?;
        }
        DriverOutputKind::Plan => {
            mcp::submit_agent_plan(&cfg.base_url, &cfg.api_key, &issue.id, &parsed.body, "awaiting_approval", None, cfg.timeout_s)?;
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
    if cfg.wants_interactive() {
        return interactive_code_session(ctx, issue, detail, handle);
    }

    let approved = match latest_approved_plan_text(detail) {
        Some(p) => p,
        None => {
            ctx.set_status(&issue.id, "needs_human", Some("plan approved but no plan text found"));
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
        let mut req = if cfg.driver == "codex" {
            agent_run::build_codex_run(&claim.worktree_path, "code", &cfg.mcp_url(), &cfg.api_key, CODE_SYSTEM_PROMPT, &prompt)
        } else {
            let mcp_path = format!("{}/.mcp.json", claim.worktree_path);
            agent_run::build_claude_run(&claim.worktree_path, "code", &mcp_path, CODE_SYSTEM_PROMPT, &prompt)
        };
        req.issue_id = issue.id.clone();
        req.issue_identifier = issue.identifier.clone();
        let run_id = req.run_id.clone();
        ctx.emit_event(&HostEvent::RunStarted { issue_id: &issue.id, issue_identifier: &issue.identifier, run_id: &run_id, mode: "code" });
        let result = agent_run::run_headless_for_issue(&issue.id, &req, cfg.run_timeout_s);
        let outcome = if result.exit_code == EXIT_CANCELLED { "cancelled" } else if result.exit_code == 0 { "ok" } else { "failed" };
        ctx.emit_event(&HostEvent::RunFinished { issue_id: &issue.id, run_id: &run_id, exit_code: result.exit_code, outcome });
        if result.exit_code == EXIT_CANCELLED {
            ctx.set_status(&issue.id, "cancelled", Some("cancelled by user"));
            return Ok(());
        }
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
    let push_token = repo_token(cfg, &format!("{}/{}", handle.owner, handle.repo));
    git::push_branch(&claim.repo_path, &handle.owner, &handle.repo, &claim.branch, &push_token)?;

    // The SERVER opens the PR with the owner's connected GitHub token + records
    // pr_* + pr_opened (the agent no longer creates the PR via the API itself).
    let url = mcp::open_pr(&cfg.base_url, &cfg.api_key, &issue.id, &claim.branch, &handle.default_branch, cfg.timeout_s)?;

    ctx.patch(&issue.id, &IssuePatch { pr_url: Some(url.clone()), ..Default::default() });
    ctx.set_status(&issue.id, "in_review", None);
    // The server already recorded a pr_opened event; no comment needed.
    let _ = url;
    Ok(())
}
