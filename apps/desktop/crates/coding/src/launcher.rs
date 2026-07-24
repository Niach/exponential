//! The Start-coding launcher (masterplan-v3 §7.1, DC-1) — ONE prepare
//! sequence for both launch shapes: a single-issue session and a
//! multi-issue batch session ([`PrepareRequest`]). A local dialog launch
//! and a relay `start_session` frame run the SAME sequence (§08 calls this
//! same entry point; there is no second "remote start" implementation).
//!
//! Split to match gpui's threading model while keeping one code path:
//!
//! 1. [`prepare`] — steps 0–6 (doctor → repo resolve → JIT token → git →
//!    `.exp-mcp.json` → prompt delivery → `codingSessions.start`). **Blocking
//!    network and git I/O, gpui-free** — run it on the background executor.
//!    Returns either a [`PreparedLaunch`] (the composed `claude` spawn spec)
//!    or a [`DisabledReason`] (never falsely block, always explain — none of
//!    these are errors/panics).
//! 2. [`spawn_prepared`] — steps 7–8 on the foreground: opens the Claude tab
//!    through the §06 `TerminalManager` (keyed by the `coding_sessions` id)
//!    and installs the one-shot exit hook that ends the session row
//!    (idempotent server-side) when the child dies. The prompt rides the
//!    spawn spec as claude's positional argument (never PTY stdin).
//!
//! The launcher never touches PTYs (§06 owns them) and never talks to the
//! steer relay (§3.1: `coding` does not depend on `steer`) — the app/ui layer
//! takes `LaunchOutcome::Spawned { session_id, .. }` and hands the same PTY
//! tee + session id to the steer publisher (§08).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use api::error::ApiError;
use api::token_store::TokenStore;
use api::trpc::TrpcClient;
use api::{coding_sessions, issues, repositories, users};
use gpui::App;
use terminal::pty::SpawnSpec;
use terminal::tab::{TabId, TabKind};
use terminal::TerminalManager;

use crate::agent::CodingAgent;
use crate::argv::{session_args, AgentMcp, LaunchOptions, SessionTail, MCP_TOKEN_ENV, MCP_URL_ENV};
use crate::action_prompt::render_action_prompt;
use crate::batch_launcher::{batch_branch_name, BatchLaunchRequest, RepoGroup};
use domain::IssueStatus;
use crate::batch_prompt::{render_batch_prompt, BatchPromptArgs};
use crate::doctor::{run_doctor, ToolCheck};
use crate::pi_bridge::write_pi_bridge;
use crate::git_credentials;
use crate::git_worktree::{
    branch_name, clone_path, create_worktree, ensure_clone, fetch_base,
    shared_cargo_target_dir, GitError, TokenUrl,
};
use crate::mcp_json::write_mcp_json;
use crate::prompt::{deliver_prompt, render_prompt, render_resume_prompt, PROMPT_FILE};
use crate::settings::Settings;

/// Cadence of the `codingSessions.heartbeat` liveness ping while the claude
/// child is alive. Must stay well inside the server's staleness window
/// (`CODING_SESSION_STALE_HOURS` = 2h in `@exp/db-schema/domain`, measured
/// from the row's `updated_at`) so that several pings would have to fail
/// back-to-back before a live session's row could be swept.
const SESSION_HEARTBEAT_INTERVAL: std::time::Duration =
    std::time::Duration::from_secs(30 * 60);

/// Where the launch came from (§7.1). Both origins run the SAME sequence —
/// the variant exists for the session's audit surface, not for branching.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaunchOrigin {
    /// The Start-coding button on the issue-detail header.
    Local,
    /// A relay `start_session` frame (§08's control channel).
    Relay { device_id: String, claimant: String },
}

/// The machine's hostname — §7.1's `device_label` (also the server-side
/// `coding_sessions.device_label`). Env vars first (cheap), then the
/// ubiquitous `hostname` binary; never fails (falls back to a placeholder).
pub fn default_device_label() -> String {
    for var in ["HOSTNAME", "COMPUTERNAME", "HOST"] {
        if let Ok(value) = std::env::var(var) {
            let trimmed = value.trim().to_string();
            if !trimmed.is_empty() {
                return trimmed;
            }
        }
    }
    if let Ok(output) = std::process::Command::new("hostname").output() {
        let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !name.is_empty() {
            return name;
        }
    }
    "unknown-host".to_string()
}

/// §7.1's single-issue launch input.
#[derive(Clone, Debug)]
pub struct LaunchRequest {
    pub issue_id: String,
    /// e.g. `EXP-42` — becomes the branch name (`<prefix><IDENTIFIER>`).
    pub issue_identifier: String,
    /// Status snapshot at launch time — step 6.5 flips backlog/todo issues
    /// to `in_progress` (EXP-194).
    pub issue_status: IssueStatus,
    /// Hostname; also `coding_sessions.device_label`.
    pub device_label: String,
    pub origin: LaunchOrigin,
    /// The Start-coding dialog's model/effort/mode choices (settings
    /// defaults for relay starts — [`LaunchOptions::remote`]).
    pub options: LaunchOptions,
    /// EXP-202: reuse the issue's persisted worktree and CONTINUE the
    /// previous agent conversation instead of seeding a fresh prompt.
    /// Claude + pi resume natively (`--continue`, cwd-scoped); codex resumes
    /// the EXACT recorded session for the worktree (`resume <id>`, recovered
    /// from its rollout metas — [`crate::codex_sessions`]) and falls back to
    /// a fresh session seeded with the resume prompt when none is recorded.
    /// EXP-210: the worktree's recorded-agent marker
    /// ([`crate::worktree_agents`]) gates every native path — resuming with
    /// an agent that never coded in the worktree degrades to that same
    /// fresh-seeded fallback instead of letting `--continue` fail.
    /// Worktree creation is already idempotent either way — this only
    /// changes step 5 + the argv tail, and clamps `options.plan_mode` off
    /// (the plan already happened in the conversation being continued).
    pub resume: bool,
}

/// An action run's launch input (EXP-253): no worktree, no branch, no PR —
/// an interactive claude session on the repo's trunk clone (autopulled) or,
/// for a repo-less action, a scratch dir holding only the MCP config.
#[derive(Clone, Debug)]
pub struct ActionLaunchRequest {
    pub action_id: String,
    /// Display snapshot (tab title + heartbeat scope).
    pub action_name: String,
    /// The FRESH body — the caller fetched it via `actions.get` and passed
    /// the per-device trust gate on ITS hash; a cached/listed body must
    /// never reach here.
    pub body: String,
    /// `Some` = run in this repo's trunk clone on the default branch;
    /// `None` = repo-less (scratch dir). Local starts resolve it from the
    /// window resolver; relay starts carry it in the frame (batch precedent
    /// — the desktop syncs no repositories).
    pub repo: Option<RepoGroup>,
    pub device_label: String,
    pub origin: LaunchOrigin,
    /// Claude-only v1: `agent` is clamped to Claude and `plan_mode` off in
    /// [`prepare`] regardless of what rides in; model/effort are honored.
    pub options: LaunchOptions,
}

/// The three launch shapes ONE [`prepare`] serves.
#[derive(Clone, Debug)]
pub enum PrepareRequest {
    Issue(LaunchRequest),
    Batch(BatchLaunchRequest),
    Action(ActionLaunchRequest),
}

/// Issue text for the seed prompt, fetched by the caller from the sync store
/// (`coding` cannot depend on `sync` — §3.1 dependency direction).
#[derive(Clone, Debug)]
pub struct IssueSeed {
    pub title: String,
    pub description: Option<String>,
}

/// §7.1 step 3, injectable for tests: turn (repos_root, repo, branch, token)
/// into a ready worktree. The real impl is [`GitWorktrees`] (argv git).
/// `expires_at` is the token's real ISO-8601 expiry from the mint — the
/// ambient-auth install's no-downgrade stamp (EXP-73).
pub trait WorktreeProvider: Send + Sync {
    #[allow(clippy::too_many_arguments)]
    fn prepare(
        &self,
        repos_root: &Path,
        full_name: &str,
        default_branch: &str,
        branch: &str,
        url: &TokenUrl,
        expires_at: Option<&str>,
    ) -> Result<PathBuf, GitError>;
}

/// The real git path: `ensure_clone` → [`git_credentials::ensure`] (bare
/// origin + repo-local helper + downgrade-guarded token file, re-run EVERY
/// launch — EXP-73) → best-effort fetch of the base branch →
/// `create_worktree` (idempotent reuse) → repo-local excludes for the
/// credential-bearing seed file.
pub struct GitWorktrees;

impl WorktreeProvider for GitWorktrees {
    fn prepare(
        &self,
        repos_root: &Path,
        full_name: &str,
        default_branch: &str,
        branch: &str,
        url: &TokenUrl,
        expires_at: Option<&str>,
    ) -> Result<PathBuf, GitError> {
        let clone = ensure_clone(repos_root, full_name, url)?;
        git_credentials::ensure(&clone, url, expires_at)?;
        // Best-effort: a stale-but-present origin/<default> still yields a
        // valid worktree; only a truly missing base ref fails below.
        let _ = fetch_base(&clone, default_branch, url);
        let worktree =
            create_worktree(&clone, branch, &format!("origin/{default_branch}"), url)?;
        // .exp-mcp.json carries the raw expu_ key and the agent is told to
        // commit + push — keep it (and the pi MCP-bridge extension) out of
        // `git add -A` via the shared, never-committed `.git/info/exclude`
        // (best-effort by design; the PROMPT.md exclude rides
        // [`crate::prompt::deliver_prompt_file`]).
        let _ = crate::git_worktree::ensure_local_excludes(
            &clone,
            &[
                crate::mcp_json::MCP_JSON_FILE,
                crate::pi_bridge::PI_BRIDGE_FILE,
                crate::worktree_agents::AGENTS_FILE,
            ],
        );
        Ok(worktree)
    }
}

/// Issue title/description lookup for the seed prompt (sync-store backed; the
/// caller owns the store — §3.1: `coding` cannot depend on `sync`).
pub type IssueSeedFn = Arc<dyn Fn(&str) -> Option<IssueSeed> + Send + Sync>;

/// The injected collaborators (§7.1) — everything the sequence needs, so the
/// crate stays testable and both launch origins share one code path.
pub struct CodingDeps {
    /// Mutation client bound to the signed-in account (bearer at call time).
    pub trpc: Arc<TrpcClient>,
    /// The file-based secret store holding the hidden `expu_` key (§7.2).
    pub token_store: Arc<TokenStore>,
    /// The account the key/session belong to ([`api::accounts`] id form).
    pub account_id: String,
    /// Resolved coding settings (claude path, repos root, branch prefix).
    pub settings: Settings,
    /// Issue title/description lookup for the seed prompt (sync-store backed).
    pub issue_seed: IssueSeedFn,
    /// Git ops ([`GitWorktrees`] in production).
    pub worktrees: Arc<dyn WorktreeProvider>,
    /// EXP-202: where codex records its session rollouts, for exact-session
    /// resume recovery. `None` (production) = auto-detect
    /// [`crate::codex_sessions::default_codex_sessions_root`]; tests inject
    /// a fixture tree.
    pub codex_sessions_root: Option<PathBuf>,
    /// The app data dir — repo-less action runs execute in
    /// `<data_dir>/actions/<action id>/` (EXP-253).
    pub data_dir: PathBuf,
}

/// §7.1's non-fatal "why Start coding can't run" set — each renders as a
/// small inline error with a remediation link (never falsely block,
/// always explain). None of these panic and none are transport errors.
#[derive(Clone, Debug)]
pub enum DisabledReason {
    /// `repositories.forIssue` returned null.
    NoRepositoryLinked,
    /// The GitHub App is not installed on the repo (server 412) — install is
    /// web-only (§7.9); link out.
    GithubAppMissing { full_name: String, message: String },
    /// `claude` or `git` failed the doctor (§7.7) — names which tool.
    DoctorFailed(ToolCheck),
    /// The plan's concurrent-session cap (server 412 on start) — carries the
    /// server's upgrade copy.
    SessionLimit { message: String },
    /// The server refused to mint the installation token (401/403).
    TokenDenied { message: String },
}

impl DisabledReason {
    /// User-facing copy (§7.1: inline error / disabled-button helper text).
    pub fn message(&self) -> String {
        match self {
            DisabledReason::NoRepositoryLinked => {
                "Link a repository to this board in team settings.".to_string()
            }
            DisabledReason::GithubAppMissing { message, .. } => message.clone(),
            DisabledReason::DoctorFailed(check) => check
                .error
                .clone()
                .unwrap_or_else(|| format!("{} is not available", check.tool)),
            DisabledReason::SessionLimit { message } => message.clone(),
            DisabledReason::TokenDenied { message } => message.clone(),
        }
    }
}

/// A hard failure in the sequence (network/git/filesystem) — distinct from
/// [`DisabledReason`], which is the expected "can't run, here's why" surface.
#[derive(Debug)]
pub enum CodingError {
    Api(ApiError),
    Git(GitError),
    Io(String),
    Terminal(String),
}

impl std::fmt::Display for CodingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CodingError::Api(err) => write!(f, "api: {err}"),
            CodingError::Git(err) => write!(f, "git: {err}"),
            CodingError::Io(msg) => write!(f, "io: {msg}"),
            CodingError::Terminal(msg) => write!(f, "terminal: {msg}"),
        }
    }
}

impl std::error::Error for CodingError {}

impl From<ApiError> for CodingError {
    fn from(err: ApiError) -> Self {
        CodingError::Api(err)
    }
}

impl From<GitError> for CodingError {
    fn from(err: GitError) -> Self {
        CodingError::Git(err)
    }
}

/// Steps 0–6 done: everything the foreground needs to open the Claude tab.
#[derive(Debug)]
pub struct PreparedLaunch {
    /// The `coding_sessions` row id — keys the terminal tab (§06) and the
    /// steer session room (§08).
    pub session_id: String,
    /// The issue identifier (`EXP-42`) — or `batch-<id8>` for a batch
    /// session (it feeds the same log/registry surfaces).
    pub issue_identifier: String,
    pub worktree: PathBuf,
    /// The shared clone the worktree hangs off (`<repos_root>/<owner>/<name>`)
    /// — the token refresher's ambient-auth target (EXP-73: the credential
    /// file + helper config live in the clone's shared `.git`, so refreshing
    /// the clone covers every worktree).
    pub clone: PathBuf,
    /// The team `repositories` row id — re-mints the installation token
    /// mid-session (EXP-56 P9). `None` for a repo-less action run (nothing
    /// to refresh: no clone, no token).
    pub repository_id: Option<String>,
    /// The real git branch (keeps its `/`), e.g. `exp/EXP-42` — or the
    /// batch branch `exp/batch-<id8>`.
    pub branch: String,
    /// The claude invocation in the worktree (§7.1 step 7).
    pub spawn: SpawnSpec,
    /// Tab strip default title (`claude · EXP-42` / `claude · EXP-42 +2`).
    pub tab_title: String,
    /// Issue identity re-attached to live OSC titles (EXP-145): `EXP-42` /
    /// `EXP-42 +2` — claude's OSC titles replace the whole tab title, so the
    /// strip shows `EXP-42 · <claude's title>`.
    pub tab_title_prefix: String,
    /// The row's start scope, re-sent with every heartbeat (EXP-105): a ping
    /// that finds the row swept (suspend outlived the staleness window)
    /// re-creates it server-side under the same id.
    pub heartbeat_scope: coding_sessions::HeartbeatScope,
    /// Which tab kind the spawn opens: `Claude` for issue/batch sessions,
    /// `Action(id)` for action runs (EXP-253).
    pub tab_kind: TabKind,
}

/// [`prepare`]'s outcome: ready to spawn, or disabled-with-reason.
#[derive(Debug)]
pub enum Prepared {
    Ready(PreparedLaunch),
    Disabled(DisabledReason),
}

/// §7.1's `LaunchOutcome`, produced by [`spawn_prepared`] (or directly by
/// the caller when [`prepare`] returned [`Prepared::Disabled`]).
#[derive(Debug)]
pub enum LaunchOutcome {
    Spawned {
        session_id: String,
        terminal_tab: TabId,
        worktree: PathBuf,
        branch: String,
    },
    Disabled {
        reason: DisabledReason,
    },
}

/// The shared 412/401/403 mapping for `repositories.installationToken`.
fn map_token_error(err: ApiError, full_name: &str) -> Result<Prepared, CodingError> {
    match err {
        ApiError::Http { status: 412, message } => {
            Ok(Prepared::Disabled(DisabledReason::GithubAppMissing {
                full_name: full_name.to_string(),
                message,
            }))
        }
        ApiError::Http { status: status @ (401 | 403), message } => {
            Ok(Prepared::Disabled(DisabledReason::TokenDenied {
                message: format!("{message} (HTTP {status})"),
            }))
        }
        err => Err(err.into()),
    }
}

/// Steps 0–6 of §7.1 (blocking; run on the background executor) — ONE
/// skeleton for both launch shapes, per-shape only at the marked match
/// points:
///
/// 0. doctor — `claude` (incl. the minimum-version gate) AND `git` must
///    resolve (§7.7: a machine with git missing is blocked here, not allowed
///    to crash at clone);
/// 1. repo — Issue: `repositories.forIssue` (null ⇒
///    [`DisabledReason::NoRepositoryLinked`]); Batch: the dialog already
///    resolved the repo — trust it;
/// 2. `repositories.installationToken` — JIT, session-gated, never persisted;
/// 3. git: clone/worktree + branch (`<prefix><IDENTIFIER>` /
///    `exp/batch-<id8>`) + ambient-auth install (bare origin + repo-local
///    credential helper, EXP-73; the personal-key read/mint races this on a
///    side thread, §7.2);
/// 4. `.exp-mcp.json` (the ONLY place the raw `expu_` key lands on disk);
/// 5. prompt — Issue: the single-issue template; Batch: the multi-issue
///    template (all issues + the combined-PR contract). Both size-gated
///    (direct argv when small, PROMPT.md otherwise);
/// 6. `codingSessions.start` / `start_batch` — BEFORE spawn; its id keys
///    tab + steer room.
pub fn prepare(req: &PrepareRequest, deps: &CodingDeps) -> Result<Prepared, CodingError> {
    // Action runs share none of the worktree/branch/PR skeleton below —
    // they get their own sequence (EXP-253).
    if let PrepareRequest::Action(action_req) = req {
        return prepare_action(action_req, deps);
    }
    let resume_requested = matches!(req, PrepareRequest::Issue(issue_req) if issue_req.resume);
    let mut options = match req {
        PrepareRequest::Issue(issue_req) => issue_req.options.clone(),
        PrepareRequest::Batch(batch_req) => batch_req.options.clone(),
        PrepareRequest::Action(_) => unreachable!("dispatched above"),
    };
    // A resume NEVER re-enters plan mode (EXP-202): the plan already
    // happened in the conversation being continued. The dialog clamps this
    // too, but the invariant belongs here so every future caller (remote
    // resume is the seam) inherits it.
    options.plan_mode &= !resume_requested;
    let options = &options;
    let agent = options.agent;

    // Step 0 — the doctor gate, PER-AGENT (EXP-201: git + the SELECTED
    // agent must resolve — a missing pi never blocks a claude launch).
    // Cheap relative to clone/mint and structural: the relay origin has no
    // button whose disabled state could have gated this.
    let report = run_doctor(&deps.settings);
    if let Some(failed) = report.first_failure_for(agent) {
        return Ok(Prepared::Disabled(DisabledReason::DoctorFailed(
            failed.clone(),
        )));
    }

    // Step 1 — resolve the repository (the coding-first gate).
    let (repository_id, full_name) = match req {
        PrepareRequest::Issue(issue_req) => {
            let Some(repo) = repositories::for_issue(&deps.trpc, &issue_req.issue_id)? else {
                return Ok(Prepared::Disabled(DisabledReason::NoRepositoryLinked));
            };
            (repo.repository_id, repo.full_name)
        }
        PrepareRequest::Batch(batch_req) => (
            batch_req.repo.repository_id.clone(),
            batch_req.repo.full_name.clone(),
        ),
        PrepareRequest::Action(_) => unreachable!("dispatched above"),
    };

    // Step 2 — mint the JIT installation token (session-gated, ≤1 h real
    // TTL, never persisted/logged server-side — TokenUrl + scrubbed git
    // errors enforce that). Through the process-wide cache, deliberately:
    // this seeds it, so the refresher's first pass and the git-bar's next
    // sync are cache hits instead of duplicate mints (EXP-73). The margin is
    // the refresher's LEAD, not the smaller per-op one — the session's
    // ambient token must be born with enough life to reach the refresher's
    // first scheduled pass even if that pass is delayed.
    let minted = match crate::token_cache::token_cache().get_or_mint_with_margin(
        &deps.trpc,
        &repository_id,
        crate::token_refresh::REFRESH_LEAD,
    ) {
        Ok(minted) => minted,
        Err(err) => return map_token_error(err, &full_name),
    };

    // §7.2 — the personal-key read/mint races the git prep on a side thread;
    // only step 4 (.exp-mcp.json) needs the result.
    let key_handle = {
        let trpc = Arc::clone(&deps.trpc);
        let store = Arc::clone(&deps.token_store);
        let account_id = deps.account_id.clone();
        std::thread::spawn(move || users::ensure_personal_key(&trpc, &store, &account_id))
    };

    // Step 3 — git via argv (never gh): clone → token remote → worktree on
    // the per-shape branch.
    let branch = match req {
        PrepareRequest::Issue(issue_req) => {
            branch_name(&deps.settings.branch_prefix, &issue_req.issue_identifier)
        }
        PrepareRequest::Batch(batch_req) => batch_branch_name(&batch_req.batch_id),
        PrepareRequest::Action(_) => unreachable!("dispatched above"),
    };
    let url = minted.url.clone();
    let repos_root = deps.settings.repos_root_path();
    let worktree = deps.worktrees.prepare(
        &repos_root,
        url.full_name(),
        &minted.default_branch,
        &branch,
        &url,
        minted.expires_at.as_deref(),
    )?;
    // The clone path the worktree hangs off — the P9 token refresher's target.
    let clone = clone_path(&repos_root, url.full_name());

    // Step 4 — per-agent MCP wiring (authenticates the spawned agent as the
    // real user against /api/mcp):
    // - claude: the worktree `.exp-mcp.json` (any subagents it spawns
    //   inherit the session's MCP servers; NOT named .mcp.json — EXP-98,
    //   see `crate::mcp_json`) — the ONLY on-disk consumer of the raw key.
    // - codex: `-c mcp_servers.*` argv overrides; the raw key rides ONLY the
    //   spawn env (EXP_MCP_TOKEN) — never disk, never argv.
    // - pi: the launcher-written `.exp-pi-mcp.ts` bridge extension (pi has
    //   no native MCP); url + key ride the spawn env like codex.
    let personal_key = key_handle
        .join()
        .map_err(|_| CodingError::Io("personal-key thread panicked".to_string()))??;
    let mcp_url = format!(
        "{}/api/mcp",
        deps.trpc.base_url().trim_end_matches('/')
    );
    let agent_mcp = match agent {
        CodingAgent::Claude => {
            write_mcp_json(&worktree, deps.trpc.base_url(), &personal_key)
                .map_err(|e| CodingError::Io(format!("write .exp-mcp.json: {e}")))?;
            AgentMcp::ClaudeFile
        }
        CodingAgent::Codex => AgentMcp::CodexOverrides { url: mcp_url.clone() },
        CodingAgent::Pi => {
            write_pi_bridge(&worktree)
                .map_err(|e| CodingError::Io(format!("write .exp-pi-mcp.ts: {e}")))?;
            AgentMcp::PiExtension
        }
    };

    // Step 5 — the seed prompt (both shapes: direct argv delivery when
    // small, PROMPT.md + seed line otherwise). A NATIVE resume (EXP-202)
    // skips the prompt entirely — the conversation already carries the
    // context: claude/pi via cwd-scoped `--continue`, codex via the exact
    // session id recovered from its rollout metas for THIS worktree (its
    // `resume --last` is global-latest, so the id lookup is what keeps a
    // resume from reopening an unrelated conversation). Codex with no
    // recorded session for the worktree (coded by another agent, sessions
    // pruned) falls back to a fresh session seeded with the resume prompt.
    // Either native path runs stale-seed hygiene (same rationale as
    // `deliver_prompt`'s Direct path: a resumed session must never re-read
    // an earlier launch's PROMPT.md).
    //
    // EXP-210: the worktree's recorded-agent marker gates native resume the
    // same way — `--continue` in a worktree THIS agent never coded in dies
    // with "no conversation found to continue", so a resume request against
    // another agent's worktree degrades to the fresh-seeded resume prompt
    // (the dialog also hides the Resume offer in that case; this is the
    // backstop for stale dialogs and the future remote-resume seam). A
    // marker-less worktree predates the marker — its history is unknown, so
    // the legacy behavior (attempt the native resume) stands.
    let marker_allows_resume = crate::worktree_agents::worktree_agents(&worktree)
        .is_none_or(|recorded| recorded.contains(&agent));
    let codex_resume_id =
        (resume_requested && marker_allows_resume && agent == CodingAgent::Codex)
            .then(|| {
                deps.codex_sessions_root
                    .clone()
                    .or_else(crate::codex_sessions::default_codex_sessions_root)
                    .and_then(|root| {
                        crate::codex_sessions::find_latest_codex_session_id(&root, &worktree)
                    })
            })
            .flatten();
    let native_resume = resume_requested
        && marker_allows_resume
        && (agent != CodingAgent::Codex || codex_resume_id.is_some());
    let delivery = if native_resume {
        let _ = std::fs::remove_file(worktree.join(PROMPT_FILE));
        None
    } else {
        let rendered = match req {
            // Resume without a recoverable conversation (codex fallback): a
            // fresh session in the reused worktree, told to pick the
            // existing branch work back up.
            PrepareRequest::Issue(issue_req) if issue_req.resume => {
                let seed = (deps.issue_seed)(&issue_req.issue_id);
                let title = seed
                    .as_ref()
                    .map(|seed| seed.title.as_str())
                    .unwrap_or(issue_req.issue_identifier.as_str());
                render_resume_prompt(&issue_req.issue_identifier, title, &minted.default_branch)
            }
            PrepareRequest::Issue(issue_req) => {
                // Title/description from the sync store.
                let seed = (deps.issue_seed)(&issue_req.issue_id);
                let (title, description) = match &seed {
                    Some(seed) => (seed.title.as_str(), seed.description.as_deref()),
                    None => (issue_req.issue_identifier.as_str(), None),
                };
                render_prompt(&issue_req.issue_identifier, title, description)
            }
            PrepareRequest::Batch(batch_req) => render_batch_prompt(&BatchPromptArgs {
                default_branch: &minted.default_branch,
                branch: &branch,
                issues: &batch_req.issues,
            }),
            PrepareRequest::Action(_) => unreachable!("dispatched above"),
        };
        Some(
            deliver_prompt(&worktree, &clone, &rendered)
                .map_err(|e| CodingError::Io(format!("deliver prompt: {e}")))?,
        )
    };

    // Step 6 — the session row, BEFORE spawn (the id keys everything).
    let session = match req {
        PrepareRequest::Issue(issue_req) => coding_sessions::start(
            &deps.trpc,
            &issue_req.issue_id,
            Some(&issue_req.device_label),
        ),
        PrepareRequest::Batch(batch_req) => coding_sessions::start_batch(
            &deps.trpc,
            &batch_req.team_id,
            Some(&batch_req.device_label),
        ),
        PrepareRequest::Action(_) => unreachable!("dispatched above"),
    };
    let session = match session {
        Ok(session) => session,
        Err(ApiError::Http { status: 412, message }) => {
            return Ok(Prepared::Disabled(DisabledReason::SessionLimit { message }))
        }
        Err(err) => return Err(err.into()),
    };

    // Step 6.5 (EXP-194) — the LAUNCHER parks backlog/todo issues in
    // `in_progress`. Under plan mode the agent's MCP status call would only
    // land after plan approval, so without this the issue lingers in backlog
    // while visibly "coding now". After the session row so a Disabled
    // outcome never flips anything; best-effort — a failed write never
    // blocks the launch. Only backlog/todo flip: never downgrade
    // in_progress/in_review/done/cancelled/duplicate (client-side snapshot,
    // same guard the dialog's state hints use).
    let flip_ids: Vec<&str> = match req {
        PrepareRequest::Issue(issue_req) => matches!(
            issue_req.issue_status,
            IssueStatus::Backlog | IssueStatus::Todo
        )
        .then_some(issue_req.issue_id.as_str())
        .into_iter()
        .collect(),
        PrepareRequest::Batch(batch_req) => batch_req
            .issues
            .iter()
            .filter(|issue| {
                matches!(issue.status, IssueStatus::Backlog | IssueStatus::Todo)
            })
            .map(|issue| issue.issue_id.as_str())
            .collect(),
        PrepareRequest::Action(_) => unreachable!("dispatched above"),
    };
    for issue_id in flip_ids {
        let mut input = issues::IssuesUpdateInput::new(issue_id);
        input.status = Some(IssueStatus::InProgress);
        let _ = issues::issues_update(&deps.trpc, &input);
    }

    // EXP-210: stamp THIS agent into the worktree's recorded-agent marker
    // (after every can-still-fail step, so a Disabled outcome records
    // nothing; read above BEFORE this write, so the marker gate judged the
    // PREVIOUS launches). Best-effort: a failed write only costs a future
    // resume offer.
    let _ = crate::worktree_agents::record_worktree_agent(&worktree, agent);

    // Step 7's spawn spec — argv from [`crate::argv`]: explicit `--model`,
    // the native permission posture, and the prompt positional-last (bytes
    // typed into the PTY before the TUI enters raw mode get swallowed during
    // startup, so the prompt must never be delivered via stdin).
    let (issue_identifier, tab_title_prefix) = match req {
        PrepareRequest::Issue(issue_req) => (
            issue_req.issue_identifier.clone(),
            issue_req.issue_identifier.clone(),
        ),
        PrepareRequest::Batch(batch_req) => {
            let first = batch_req
                .issues
                .first()
                .map(|issue| issue.issue_identifier.as_str())
                .unwrap_or("batch");
            let extra = batch_req.issues.len().saturating_sub(1);
            (
                format!("batch-{}", batch_req.batch_id),
                format!("{first} +{extra}"),
            )
        }
        PrepareRequest::Action(_) => unreachable!("dispatched above"),
    };
    let tail = match (&delivery, &codex_resume_id) {
        (Some(delivery), _) => SessionTail::Prompt(delivery.positional()),
        // Native resume: no prompt at all — codex reopens the exact
        // recovered session, claude/pi `--continue` the worktree's latest
        // conversation.
        (None, Some(id)) => SessionTail::CodexResume(id),
        (None, None) => SessionTail::Continue,
    };
    let args = session_args(options, &agent_mcp, tail);
    let tab_title = format!("{} · {tab_title_prefix}", agent.id());
    let mut spawn = SpawnSpec::new(&deps.settings.resolved_path_for(agent))
        .args(args)
        .cwd(&worktree)
        // EXP-76 disk hygiene, both inherited by every cargo the session runs:
        // one shared build cache for ALL of this repo's session worktrees
        // (instead of a full cold tree per worktree — concurrent builds
        // serialize on cargo's lock, which warm caches more than repay), and
        // no incremental caches (session builds are few-shot; the per-worktree
        // incremental dirs were ~1GB of pure waste each). Inert for non-Rust
        // repos.
        .env(
            "CARGO_TARGET_DIR",
            shared_cargo_target_dir(&clone).to_string_lossy().into_owned(),
        )
        .env("CARGO_INCREMENTAL", "0");
    // The MCP credential for codex/pi rides the ENV (claude's rides
    // `.exp-mcp.json`): codex reads it through `bearer_token_env_var`, the
    // pi bridge reads url + token directly.
    match agent {
        CodingAgent::Claude => {}
        CodingAgent::Codex => {
            spawn = spawn.env(MCP_TOKEN_ENV, personal_key.as_str());
        }
        CodingAgent::Pi => {
            spawn = spawn
                .env(MCP_URL_ENV, mcp_url.as_str())
                .env(MCP_TOKEN_ENV, personal_key.as_str())
                // Embedded sessions must not block on pi's startup
                // update/network checks.
                .env("PI_SKIP_VERSION_CHECK", "1");
        }
    }

    let heartbeat_scope = match req {
        PrepareRequest::Issue(issue_req) => coding_sessions::HeartbeatScope {
            issue_id: Some(issue_req.issue_id.clone()),
            team_id: None,
            action_id: None,
            action_name: None,
            device_label: Some(issue_req.device_label.clone()),
        },
        PrepareRequest::Batch(batch_req) => coding_sessions::HeartbeatScope {
            issue_id: None,
            team_id: Some(batch_req.team_id.clone()),
            action_id: None,
            action_name: None,
            device_label: Some(batch_req.device_label.clone()),
        },
        PrepareRequest::Action(_) => unreachable!("dispatched above"),
    };

    Ok(Prepared::Ready(PreparedLaunch {
        session_id: session.id,
        issue_identifier,
        worktree,
        clone,
        repository_id: Some(repository_id),
        branch,
        spawn,
        tab_title,
        tab_title_prefix,
        heartbeat_scope,
        tab_kind: TabKind::Claude,
    }))
}

/// The action-run sequence (EXP-253; blocking, background executor) — the
/// deliberately SHORT sibling of the issue/batch skeleton above: no
/// worktree, no branch, no PR contract, no status flips.
///
/// 0. doctor — claude always (Claude-only v1); `git` only when repo-backed
///    (a repo-less action needs no git at all);
/// 1. cwd — repo-backed: mint the JIT token (cache-seeded like a session),
///    ensure the trunk clone + ambient auth, then a BEST-EFFORT autopull
///    (`clone_manager::auto_sync` — a dirty/diverged trunk still launches;
///    the trunk-sync engine surfaces that state); repo-less:
///    `<data_dir>/actions/<action id>/`, created on demand;
/// 2. `.exp-mcp.json` in the cwd (repo-backed also excludes it from git);
/// 3. prompt — [`render_action_prompt`] preamble + the fresh body;
/// 4. `codingSessions.start({actionId})` — BEFORE spawn; its id keys the tab
///    + steer room like any session;
/// 5. spawn spec — interactive claude with the session argv
///    (`--mcp-config .exp-mcp.json --strict-mcp-config`, model/effort).
fn prepare_action(req: &ActionLaunchRequest, deps: &CodingDeps) -> Result<Prepared, CodingError> {
    // Claude-only v1: clamp regardless of what rode in (the server validates
    // remote starts the same way; this covers every local caller too).
    let mut options = req.options.clone();
    options.agent = CodingAgent::Claude;
    options.plan_mode = false;

    // Step 0 — doctor: claude always; git only when a clone is involved.
    let report = run_doctor(&deps.settings);
    let claude_check = report.check_for(CodingAgent::Claude);
    if !claude_check.ok {
        return Ok(Prepared::Disabled(DisabledReason::DoctorFailed(
            claude_check.clone(),
        )));
    }
    if req.repo.is_some() {
        if let Some(failed) = report.first_failure_for(CodingAgent::Claude) {
            return Ok(Prepared::Disabled(DisabledReason::DoctorFailed(
                failed.clone(),
            )));
        }
    }

    // §7.2 — the personal key (the MCP credential), raced like a session's.
    let key_handle = {
        let trpc = Arc::clone(&deps.trpc);
        let store = Arc::clone(&deps.token_store);
        let account_id = deps.account_id.clone();
        std::thread::spawn(move || users::ensure_personal_key(&trpc, &store, &account_id))
    };

    // Step 1 — resolve the cwd.
    let (cwd, repository_id) = match &req.repo {
        Some(repo) => {
            // Repo-backed: JIT token via the cache (same refresher-lead
            // margin as a session — the run may outlive one token TTL).
            let minted = match crate::token_cache::token_cache().get_or_mint_with_margin(
                &deps.trpc,
                &repo.repository_id,
                crate::token_refresh::REFRESH_LEAD,
            ) {
                Ok(minted) => minted,
                Err(err) => return map_token_error(err, &repo.full_name),
            };
            let url = minted.url.clone();
            let repos_root = deps.settings.repos_root_path();
            let clone = crate::git_worktree::ensure_clone(&repos_root, url.full_name(), &url)?;
            git_credentials::ensure(&clone, &url, minted.expires_at.as_deref())?;
            // Autopull before spawn — BEST-EFFORT: Skipped (dirty/diverged/
            // conflicted trunk) and even transport errors must not block the
            // launch; the action runs on whatever state the trunk is in and
            // the trunk-sync engine keeps surfacing it.
            let _ = crate::clone_manager::auto_sync(&clone, &url);
            let _ = crate::git_worktree::ensure_local_excludes(
                &clone,
                &[crate::mcp_json::MCP_JSON_FILE],
            );
            (clone, Some(repo.repository_id.clone()))
        }
        // Repo-less: a scratch dir holding only the MCP config (+ PROMPT.md
        // when the body is large). No git, no token. The id is a server
        // UUID, but server data is untrusted here by design — sanitize the
        // path segment so a crafted id can never escape `<data_dir>/actions/`.
        None => {
            let segment: String = req
                .action_id
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect();
            if segment.is_empty() {
                return Err(CodingError::Io("empty action id".to_string()));
            }
            let scratch = deps.data_dir.join("actions").join(segment);
            std::fs::create_dir_all(&scratch)
                .map_err(|e| CodingError::Io(format!("create action scratch dir: {e}")))?;
            (scratch, None)
        }
    };

    // Step 2 — the MCP config (claude authenticates as the real user).
    let personal_key = key_handle
        .join()
        .map_err(|_| CodingError::Io("personal-key thread panicked".to_string()))??;
    write_mcp_json(&cwd, deps.trpc.base_url(), &personal_key)
        .map_err(|e| CodingError::Io(format!("write .exp-mcp.json: {e}")))?;

    // Step 3 — the prompt (size-gated like a session's; the PROMPT.md
    // exclude write no-ops without a `.git`).
    let rendered = render_action_prompt(&req.action_name, &req.body);
    let delivery = deliver_prompt(&cwd, &cwd, &rendered)
        .map_err(|e| CodingError::Io(format!("deliver prompt: {e}")))?;

    // Step 4 — the session row, BEFORE spawn.
    let session = match coding_sessions::start_action(
        &deps.trpc,
        &req.action_id,
        Some(&req.device_label),
    ) {
        Ok(session) => session,
        Err(ApiError::Http { status: 412, message }) => {
            return Ok(Prepared::Disabled(DisabledReason::SessionLimit { message }))
        }
        Err(err) => return Err(err.into()),
    };

    // Step 5 — the spawn spec: interactive claude, session argv.
    let args = session_args(
        &options,
        &AgentMcp::ClaudeFile,
        SessionTail::Prompt(delivery.positional()),
    );
    let tab_title = format!("action · {}", req.action_name);
    let mut spawn = SpawnSpec::new(&deps.settings.resolved_path_for(CodingAgent::Claude))
        .args(args)
        .cwd(&cwd);
    if req.repo.is_some() {
        // Same EXP-76 shared-cache posture as a session — inert repo-less.
        spawn = spawn
            .env(
                "CARGO_TARGET_DIR",
                shared_cargo_target_dir(&cwd).to_string_lossy().into_owned(),
            )
            .env("CARGO_INCREMENTAL", "0");
    }

    Ok(Prepared::Ready(PreparedLaunch {
        session_id: session.id,
        issue_identifier: req.action_name.clone(),
        worktree: cwd.clone(),
        clone: cwd,
        repository_id,
        // No branch — an action run never pushes; the empty string keeps
        // every branch-keyed registry lookup a miss.
        branch: String::new(),
        spawn,
        tab_title,
        tab_title_prefix: req.action_name.clone(),
        heartbeat_scope: coding_sessions::HeartbeatScope {
            issue_id: None,
            team_id: session.team_id.clone(),
            action_id: Some(req.action_id.clone()),
            action_name: Some(req.action_name.clone()),
            device_label: Some(req.device_label.clone()),
        },
        tab_kind: TabKind::Action(req.action_id.clone()),
    }))
}

/// Foreground follow-up to the child-exit edge (§7.5): the ui layer passes
/// one of these into [`spawn_prepared_with`] to flip its play↔stop state /
/// clear its local-session registry when the Claude child dies. Runs on the
/// gpui foreground AFTER the idempotent `codingSessions.end` fire-and-forget
/// thread is spawned. The `coding` crate itself never needs it —
/// [`spawn_prepared`] passes `None`.
pub type ExitNotify = Box<dyn FnOnce(&mut App) + 'static>;

/// Steps 7–8 of §7.1 (foreground; needs `&mut App`):
///
/// 7. open a Claude tab keyed by the `coding_sessions` id via the §06
///    `TerminalManager` — the prompt already rides the spawn spec as claude's
///    positional argument (stdin written before the TUI's raw mode is
///    swallowed);
/// 8. install the one-shot exit hook: when the child dies,
///    `codingSessions.end` fires from a plain thread (idempotent server-side,
///    so a relay-side kill that already ended the row is safe). The tab
///    itself stays open with the final scrollback + exit-code strip (§7.5).
pub fn spawn_prepared(
    prepared: PreparedLaunch,
    manager: &gpui::Entity<TerminalManager>,
    cx: &mut App,
    trpc: Arc<TrpcClient>,
) -> Result<LaunchOutcome, CodingError> {
    spawn_prepared_with(prepared, manager, cx, trpc, None)
}

/// [`spawn_prepared`] with the optional foreground [`ExitNotify`] — the seam
/// the §7.5 play/stop UI consumes (the hook itself stays owned here so both
/// entry points share ONE exit path).
pub fn spawn_prepared_with(
    prepared: PreparedLaunch,
    manager: &gpui::Entity<TerminalManager>,
    cx: &mut App,
    trpc: Arc<TrpcClient>,
    exit_notify: Option<ExitNotify>,
) -> Result<LaunchOutcome, CodingError> {
    let PreparedLaunch {
        session_id, worktree, branch, spawn, tab_title, tab_title_prefix, heartbeat_scope,
        tab_kind, ..
    } = prepared;

    // Liveness heartbeat: the server's staleness sweep deletes `running`
    // rows whose `updated_at` stopped advancing, so a long-lived session (an
    // IDE tab open over a weekend, a multi-issue batch run) must keep
    // pinging or it loses its badge and steerability. The stop sender rides
    // the exit hook: when the hook fires (child exited) or is dropped (spawn
    // failure, tab teardown) the channel disconnects and the thread ends.
    // Best-effort by design — a failed ping is at worst a swept badge, never
    // a killed process (the sweep deletes the row; it never flips it to
    // `ended`, which is the kill-switch signal).
    let (heartbeat_stop, heartbeat_stopped) = std::sync::mpsc::channel::<()>();
    {
        let trpc = Arc::clone(&trpc);
        let session_id = session_id.clone();
        std::thread::spawn(move || loop {
            match heartbeat_stopped.recv_timeout(SESSION_HEARTBEAT_INTERVAL) {
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    let _ = coding_sessions::heartbeat(&trpc, &session_id, Some(&heartbeat_scope));
                }
                _ => return,
            }
        });
    }

    let end_session_id = session_id.clone();
    let exit_trpc = Arc::clone(&trpc);
    let on_exit: terminal::ExitHook = Box::new(move |_tab, _exit, cx| {
        // Disconnect the heartbeat thread — the child is gone, so the row is
        // about to be ended and must stop being kept alive.
        drop(heartbeat_stop);
        // Blocking HTTP off the foreground; best-effort — the server also
        // reconciles (idempotent end), and a dead network here must never
        // take the exit-strip rendering down with it.
        let trpc = Arc::clone(&exit_trpc);
        let end_session_id = end_session_id.clone();
        std::thread::spawn(move || {
            let _ = coding_sessions::end(&trpc, &end_session_id);
        });
        if let Some(notify) = exit_notify {
            notify(cx);
        }
    });

    let tab_id = manager
        .update(cx, |manager, cx| -> Result<TabId, CodingError> {
            let tab_id = manager
                .open_tab(
                    tab_kind,
                    tab_title,
                    Some(tab_title_prefix.into()),
                    &spawn,
                    Some(on_exit),
                    cx,
                )
                .map_err(|e| CodingError::Terminal(format!("spawn claude tab: {e}")))?;
            Ok(tab_id)
        })
        .inspect_err(|_| {
            // Step 6 already created a `running` row; a spawn failure means no
            // child and therefore no exit hook will EVER fire — end the row
            // now (idempotent server-side) or the "coding now" badge ghosts
            // on every client forever.
            let trpc = Arc::clone(&trpc);
            let session_id = session_id.clone();
            std::thread::spawn(move || {
                let _ = coding_sessions::end(&trpc, &session_id);
            });
        })?;

    Ok(LaunchOutcome::Spawned {
        session_id,
        terminal_tab: tab_id,
        worktree,
        branch,
    })
}

/// Best-effort session end for teardown paths that bypass the exit hook
/// (app quit with a live child, relay kill). Idempotent server-side.
pub fn end_session_best_effort(trpc: &TrpcClient, session_id: &str) {
    let _ = coding_sessions::end(trpc, session_id);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::batch_launcher::{BatchIssueSpec, RepoGroup};
    use crate::prompt::{PROMPT_ARGV_MAX_BYTES, PROMPT_FILE, SEED_LINE};
    use crate::test_support::{
        canned_server, canned_server_recording, make_deps, temp_dir, FakeWorktrees, FOR_ISSUE_OK,
        MINT_OK, START_ACTION_OK, START_BATCH_OK, START_OK, TOKEN_OK, UPDATE_OK,
    };
    use api::token_store::SecretKind;
    use std::fs;

    fn request(identifier: &str) -> LaunchRequest {
        LaunchRequest {
            issue_id: "issue-1".to_string(),
            issue_identifier: identifier.to_string(),
            // Already in_progress ⇒ step 6.5 skips the flip, keeping the
            // canned-server sequences below one-to-one with steps 0–6.
            issue_status: IssueStatus::InProgress,
            device_label: "testbox".to_string(),
            origin: LaunchOrigin::Local,
            // The dialog defaults: claude, fable, no effort, no ultracode,
            // plan mode ON, no skip (auto posture).
            options: LaunchOptions {
                agent: CodingAgent::Claude,
                model: "fable".to_string(),
                effort: "".to_string(),
                ultracode: false,
                plan_mode: true,
                skip_permissions: false,
            },
            resume: false,
        }
    }

    fn batch_options() -> LaunchOptions {
        LaunchOptions {
            agent: CodingAgent::Claude,
            model: "opus".to_string(),
            effort: "high".to_string(),
            ultracode: true,
            plan_mode: false,
            skip_permissions: true,
        }
    }

    fn batch_request() -> BatchLaunchRequest {
        BatchLaunchRequest {
            batch_id: "a1b2c3d4".to_string(),
            team_id: "ws-1".to_string(),
            repo: RepoGroup {
                repository_id: "repo-1".to_string(),
                full_name: "acme/web".to_string(),
                default_branch: "main".to_string(),
            },
            issues: vec![
                BatchIssueSpec {
                    issue_id: "issue-1".to_string(),
                    issue_identifier: "EXP-42".to_string(),
                    title: "Fix login flicker".to_string(),
                    description: Some("Steps.".to_string()),
                    status: IssueStatus::InProgress,
                },
                BatchIssueSpec {
                    issue_id: "issue-2".to_string(),
                    issue_identifier: "EXP-43".to_string(),
                    title: "Add badge".to_string(),
                    description: None,
                    status: IssueStatus::InProgress,
                },
            ],
            device_label: "testbox".to_string(),
            origin: LaunchOrigin::Local,
            options: batch_options(),
        }
    }

    // ---- the disabled surfaces (explain, never crash) ----

    #[test]
    fn doctor_failure_disables_without_any_network_call() {
        let dir = temp_dir("doctor");
        let worktrees = Arc::new(FakeWorktrees {
            worktree: dir.0.join("wt"),
            seen: Default::default(),
        });
        // Unroutable base: any network call would error the launch — proving
        // the doctor gate fires first.
        let mut deps = make_deps("http://127.0.0.1:1", &dir.0, worktrees);
        deps.settings.claude_path = "definitely-not-a-real-binary-exp".to_string();

        match prepare(&PrepareRequest::Issue(request("EXP-42")), &deps).unwrap() {
            Prepared::Disabled(DisabledReason::DoctorFailed(check)) => {
                assert_eq!(check.tool, crate::doctor::Tool::Claude);
                assert_eq!(
                    check.error.as_deref(),
                    Some("claude not found on PATH — set an absolute path")
                );
            }
            other => panic!("expected DoctorFailed, got {other:?}"),
        }
    }

    #[test]
    fn null_repo_is_no_repository_linked() {
        let dir = temp_dir("no-repo");
        let base = canned_server(vec![(200, r#"{"result":{"data":null}}"#.to_string())]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree: dir.0.join("wt"),
            seen: Default::default(),
        });
        let deps = make_deps(&base, &dir.0, worktrees.clone());
        match prepare(&PrepareRequest::Issue(request("EXP-42")), &deps).unwrap() {
            Prepared::Disabled(reason @ DisabledReason::NoRepositoryLinked) => {
                // §7.1: the exact helper copy for the disabled button.
                assert_eq!(
                    reason.message(),
                    "Link a repository to this board in team settings."
                );
            }
            other => panic!("expected NoRepositoryLinked, got {other:?}"),
        }
        assert!(worktrees.seen.lock().unwrap().is_empty(), "no git on null repo");
    }

    #[test]
    fn app_missing_412_maps_to_github_app_missing() {
        let dir = temp_dir("app-missing");
        let base = canned_server(vec![
            (200, FOR_ISSUE_OK.to_string()),
            (412, r#"{"error":{"message":"The Exponential GitHub App is not installed on acme/web. Reconnect it in team settings.","code":-32012,"data":{"code":"PRECONDITION_FAILED","httpStatus":412}}}"#.to_string()),
        ]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree: dir.0.join("wt"),
            seen: Default::default(),
        });
        let deps = make_deps(&base, &dir.0, worktrees);
        match prepare(&PrepareRequest::Issue(request("EXP-42")), &deps).unwrap() {
            Prepared::Disabled(DisabledReason::GithubAppMissing { full_name, message }) => {
                assert_eq!(full_name, "acme/web");
                assert!(message.contains("not installed"));
            }
            other => panic!("expected GithubAppMissing, got {other:?}"),
        }
    }

    #[test]
    fn token_403_maps_to_token_denied() {
        let dir = temp_dir("denied");
        let base = canned_server(vec![
            (200, FOR_ISSUE_OK.to_string()),
            (403, r#"{"error":{"message":"You are not a member of this team","code":-32003,"data":{"code":"FORBIDDEN","httpStatus":403}}}"#.to_string()),
        ]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree: dir.0.join("wt"),
            seen: Default::default(),
        });
        let deps = make_deps(&base, &dir.0, worktrees);
        match prepare(&PrepareRequest::Issue(request("EXP-42")), &deps).unwrap() {
            Prepared::Disabled(DisabledReason::TokenDenied { message }) => {
                assert!(message.contains("not a member"));
            }
            other => panic!("expected TokenDenied, got {other:?}"),
        }
    }

    #[test]
    fn session_limit_412_maps_to_session_limit_with_upgrade_copy() {
        let dir = temp_dir("limit");
        let worktree = dir.0.join("wt");
        fs::create_dir_all(&worktree).unwrap();
        let base = canned_server(vec![
            (200, FOR_ISSUE_OK.to_string()),
            (200, TOKEN_OK.to_string()),
            (412, r#"{"error":{"message":"Concurrent coding session limit reached — upgrade to run more.","code":-32012,"data":{"code":"PRECONDITION_FAILED","httpStatus":412}}}"#.to_string()),
        ]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree,
            seen: Default::default(),
        });
        let deps = make_deps(&base, &dir.0, worktrees);
        match prepare(&PrepareRequest::Issue(request("EXP-42")), &deps).unwrap() {
            Prepared::Disabled(DisabledReason::SessionLimit { message }) => {
                assert!(message.contains("upgrade"));
            }
            other => panic!("expected SessionLimit, got {other:?}"),
        }
    }

    // ---- the issue happy path through steps 0–6 ----

    #[test]
    fn prepare_action_repo_less_runs_in_the_scratch_dir() {
        // EXP-253: a repo-less action needs NO git, NO token mint — one
        // request total (codingSessions.start with the actionId).
        let dir = temp_dir("action-scratch");
        let (base, captured) = canned_server_recording(vec![(200, START_ACTION_OK.to_string())]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree: dir.0.join("unused"),
            seen: Default::default(),
        });
        let deps = make_deps(&base, &dir.0, worktrees);

        let req = ActionLaunchRequest {
            action_id: "act-1".to_string(),
            action_name: "Code review".to_string(),
            body: "# Review\nScan the backlog.".to_string(),
            repo: None,
            device_label: "box".to_string(),
            origin: LaunchOrigin::Local,
            options: LaunchOptions {
                agent: CodingAgent::Claude,
                model: "fable".to_string(),
                effort: String::new(),
                ultracode: false,
                // Deliberately ON to prove the Claude-only clamp turns it off.
                plan_mode: true,
                skip_permissions: false,
            },
        };
        let prepared = match prepare(&PrepareRequest::Action(req), &deps).unwrap() {
            Prepared::Ready(prepared) => prepared,
            Prepared::Disabled(reason) => panic!("unexpectedly disabled: {reason:?}"),
        };

        // The scratch dir IS the cwd; it holds the MCP config.
        let scratch = dir.0.join("actions").join("act-1");
        assert_eq!(prepared.worktree, scratch);
        assert!(scratch.join(crate::mcp_json::MCP_JSON_FILE).exists());
        // No repo: nothing for the token refresher, no branch to track.
        assert_eq!(prepared.repository_id, None);
        assert_eq!(prepared.branch, "");
        assert_eq!(prepared.session_id, "sess-a");
        assert_eq!(prepared.tab_title, "action · Code review");
        assert_eq!(prepared.tab_kind, TabKind::Action("act-1".to_string()));

        // Claude session argv: explicit model + strict MCP config; plan mode
        // CLAMPED OFF (Claude-only v1 takes model/effort only); the prompt is
        // the preamble + body positional.
        assert!(prepared
            .spawn
            .args
            .windows(2)
            .any(|w| w == ["--mcp-config", crate::mcp_json::MCP_JSON_FILE]));
        assert!(prepared.spawn.args.contains(&"--strict-mcp-config".to_string()));
        assert!(!prepared.spawn.args.iter().any(|a| a == "plan"));
        let prompt = prepared.spawn.args.last().unwrap();
        assert!(prompt.contains("team action \"Code review\""));
        assert!(prompt.contains("Scan the backlog."));

        // Heartbeat scope: team (from the start response) + action id + the
        // client-held name snapshot — the deleted-action degrade contract.
        assert_eq!(prepared.heartbeat_scope.issue_id, None);
        assert_eq!(prepared.heartbeat_scope.team_id.as_deref(), Some("ws-1"));
        assert_eq!(prepared.heartbeat_scope.action_id.as_deref(), Some("act-1"));
        assert_eq!(
            prepared.heartbeat_scope.action_name.as_deref(),
            Some("Code review")
        );

        // Exactly one request — the session start; never a repo/token call.
        let requests = captured.lock().unwrap();
        assert_eq!(requests.len(), 1, "{requests:?}");
        assert!(requests[0].starts_with("POST /api/trpc/codingSessions.start"));
        assert!(requests[0].contains(r#""actionId":"act-1""#));
    }

    #[test]
    fn prepare_action_sanitizes_a_hostile_action_id() {
        // Server data is untrusted: a crafted id must stay under
        // <data_dir>/actions/ (defense-in-depth — real ids are UUIDs).
        let dir = temp_dir("action-traversal");
        let (base, _captured) = canned_server_recording(vec![(200, START_ACTION_OK.to_string())]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree: dir.0.join("unused"),
            seen: Default::default(),
        });
        let deps = make_deps(&base, &dir.0, worktrees);
        let req = ActionLaunchRequest {
            action_id: "../../escape".to_string(),
            action_name: "Evil".to_string(),
            body: "x".to_string(),
            repo: None,
            device_label: "box".to_string(),
            origin: LaunchOrigin::Local,
            options: LaunchOptions {
                agent: CodingAgent::Claude,
                model: "fable".to_string(),
                effort: String::new(),
                ultracode: false,
                plan_mode: false,
                skip_permissions: false,
            },
        };
        let prepared = match prepare(&PrepareRequest::Action(req), &deps).unwrap() {
            Prepared::Ready(prepared) => prepared,
            Prepared::Disabled(reason) => panic!("unexpectedly disabled: {reason:?}"),
        };
        assert!(prepared.worktree.starts_with(dir.0.join("actions")));
        assert!(!prepared.worktree.to_string_lossy().contains(".."));
    }

    #[test]
    fn prepare_action_412_maps_to_session_limit() {
        let dir = temp_dir("action-limit");
        let base = canned_server(vec![(
            412,
            r#"{"error":{"message":"limit","code":-32012,"data":{"httpStatus":412}}}"#.to_string(),
        )]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree: dir.0.join("unused"),
            seen: Default::default(),
        });
        let deps = make_deps(&base, &dir.0, worktrees);
        let req = ActionLaunchRequest {
            action_id: "act-1".to_string(),
            action_name: "Groom".to_string(),
            body: "do it".to_string(),
            repo: None,
            device_label: "box".to_string(),
            origin: LaunchOrigin::Local,
            options: LaunchOptions {
                agent: CodingAgent::Claude,
                model: "fable".to_string(),
                effort: String::new(),
                ultracode: false,
                plan_mode: false,
                skip_permissions: false,
            },
        };
        match prepare(&PrepareRequest::Action(req), &deps).unwrap() {
            Prepared::Disabled(DisabledReason::SessionLimit { message }) => {
                assert!(message.contains("limit"));
            }
            other => panic!("expected SessionLimit, got {other:?}"),
        }
    }

    #[test]
    fn prepare_issue_full_sequence() {
        let dir = temp_dir("happy");
        let worktree = dir.0.join("wt");
        fs::create_dir_all(&worktree).unwrap();
        // A stale PROMPT.md from an earlier launch must be REMOVED by the
        // direct delivery (claude would read the outdated copy otherwise).
        fs::write(worktree.join(PROMPT_FILE), "stale").unwrap();
        let base = canned_server(vec![
            (200, FOR_ISSUE_OK.to_string()),
            (200, TOKEN_OK.to_string()),
            (200, START_OK.to_string()),
        ]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree: worktree.clone(),
            seen: Default::default(),
        });
        let deps = make_deps(&base, &dir.0, worktrees.clone());

        let prepared = match prepare(&PrepareRequest::Issue(request("EXP-42")), &deps).unwrap() {
            Prepared::Ready(prepared) => prepared,
            Prepared::Disabled(reason) => panic!("unexpectedly disabled: {reason:?}"),
        };

        // Session id from codingSessions.start keys everything downstream.
        assert_eq!(prepared.session_id, "sess-1");
        assert_eq!(prepared.branch, "exp/EXP-42");
        assert_eq!(prepared.worktree, worktree);
        assert_eq!(prepared.tab_title, "claude · EXP-42");
        // EXP-145: the identifier rides along so live OSC titles keep it.
        assert_eq!(prepared.tab_title_prefix, "EXP-42");
        // P9 refresher inputs: the server-confirmed repo id + the clone path
        // under the repos root (independent of the fake worktree location).
        assert_eq!(prepared.repository_id.as_deref(), Some("repo-1"));
        assert_eq!(prepared.clone, dir.0.join("repos").join("acme").join("web"));

        // Step 7's spawn spec: configured program, explicit --model, the
        // explicit+strict MCP config (EXP-83: no project-discovery trust
        // dialog), the native plan-mode permission args (issue default ON),
        // the FULL rendered prompt as the positional (small prompt ⇒ direct
        // delivery), worktree cwd.
        assert_eq!(prepared.spawn.program, "git"); // test claude_path
        assert_eq!(
            prepared.spawn.args,
            vec![
                "--model".to_string(),
                "fable".to_string(),
                "--mcp-config".to_string(),
                ".exp-mcp.json".to_string(),
                "--strict-mcp-config".to_string(),
                "--permission-mode".to_string(),
                "plan".to_string(),
                "--allow-dangerously-skip-permissions".to_string(),
                render_prompt("EXP-42", "Fix login flicker", Some("Steps in the issue.")),
            ]
        );
        assert_eq!(prepared.spawn.cwd.as_deref(), Some(worktree.as_path()));

        // Step 3 got the server-confirmed repo + §7.1 branch name + the
        // mint's real expiry (the ambient-auth no-downgrade stamp).
        let seen = worktrees.seen.lock().unwrap();
        assert_eq!(
            seen.as_slice(),
            &[(
                "acme/web".to_string(),
                "main".to_string(),
                "exp/EXP-42".to_string(),
                Some("2026-07-03T12:55:00.000Z".to_string())
            )]
        );

        // Step 4: .exp-mcp.json carries the stored key + the instance /api/mcp.
        let mcp = fs::read_to_string(worktree.join(".exp-mcp.json")).unwrap();
        assert!(mcp.contains(&format!("{base}/api/mcp")));
        assert!(mcp.contains("Bearer expu_seeded"));

        // Step 5: direct delivery — NO PROMPT.md on disk (the stale copy is
        // gone, no fresh one written).
        assert!(!worktree.join(PROMPT_FILE).exists());
    }

    /// EXP-202: a CLAUDE resume skips the seed prompt entirely — the argv
    /// keeps model/MCP/permission flags but ends with `--continue` (no
    /// positional prompt), stale PROMPT.md is removed, and a fresh session
    /// row is still started (rows are lifecycle records).
    #[test]
    fn prepare_resume_claude_uses_continue_and_skips_the_prompt() {
        let dir = temp_dir("resume-claude");
        let worktree = dir.0.join("wt");
        fs::create_dir_all(&worktree).unwrap();
        fs::write(worktree.join(PROMPT_FILE), "stale from the first run").unwrap();
        let base = canned_server(vec![
            (200, FOR_ISSUE_OK.to_string()),
            (200, TOKEN_OK.to_string()),
            (200, START_OK.to_string()),
        ]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree: worktree.clone(),
            seen: Default::default(),
        });
        let deps = make_deps(&base, &dir.0, worktrees);
        let mut req = request("EXP-42");
        req.resume = true;

        let prepared = match prepare(&PrepareRequest::Issue(req), &deps).unwrap() {
            Prepared::Ready(prepared) => prepared,
            other => panic!("expected Ready, got {other:?}"),
        };

        // A NEW session row still keys the run.
        assert_eq!(prepared.session_id, "sess-1");
        // Same worktree/branch as a fresh launch — resume IS the reuse.
        assert_eq!(prepared.branch, "exp/EXP-42");
        assert_eq!(prepared.spawn.cwd.as_deref(), Some(worktree.as_path()));
        // Full flag set preserved (fresh MCP config included), tail is
        // `--continue`, and no prompt rides argv. The request carried
        // `plan_mode: true` (the fixture default) — a resume clamps it to
        // guarded auto: the plan already happened in the conversation being
        // continued.
        assert_eq!(
            prepared.spawn.args,
            vec![
                "--model".to_string(),
                "fable".to_string(),
                "--mcp-config".to_string(),
                ".exp-mcp.json".to_string(),
                "--strict-mcp-config".to_string(),
                "--permission-mode".to_string(),
                "auto".to_string(),
                "--allow-dangerously-skip-permissions".to_string(),
                "--continue".to_string(),
            ]
        );
        // The MCP config was re-minted fresh for the resumed session.
        assert!(worktree.join(".exp-mcp.json").exists());
        // Stale-seed hygiene: the resumed session must never re-read an
        // earlier launch's PROMPT.md.
        assert!(!worktree.join(PROMPT_FILE).exists());
        // EXP-210: the launch stamped claude into the recorded-agent marker.
        assert_eq!(
            crate::worktree_agents::worktree_agents(&worktree),
            Some(vec![CodingAgent::Claude])
        );
    }

    /// EXP-210: a CLAUDE resume against a worktree whose recorded-agent
    /// marker names only ANOTHER agent never emits `--continue` (claude
    /// would die with "no conversation found to continue") — it degrades to
    /// the same fresh-seeded resume-prompt fallback as codex-without-a-
    /// recorded-session, and the launch then adds claude to the marker.
    #[test]
    fn prepare_resume_claude_on_a_codex_worktree_seeds_the_resume_prompt() {
        let dir = temp_dir("resume-cross-agent");
        let worktree = dir.0.join("wt");
        fs::create_dir_all(&worktree).unwrap();
        crate::worktree_agents::record_worktree_agent(&worktree, CodingAgent::Codex).unwrap();
        let base = canned_server(vec![
            (200, FOR_ISSUE_OK.to_string()),
            (200, TOKEN_OK.to_string()),
            (200, START_OK.to_string()),
        ]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree: worktree.clone(),
            seen: Default::default(),
        });
        let deps = make_deps(&base, &dir.0, worktrees);
        let mut req = request("EXP-42");
        req.resume = true;

        let prepared = match prepare(&PrepareRequest::Issue(req), &deps).unwrap() {
            Prepared::Ready(prepared) => prepared,
            other => panic!("expected Ready, got {other:?}"),
        };

        // No native resume tail — the RESUME prompt rides positional-last.
        assert!(!prepared.spawn.args.iter().any(|arg| arg == "--continue"));
        assert_eq!(
            prepared.spawn.args.last().unwrap(),
            &render_resume_prompt("EXP-42", "Fix login flicker", "main")
        );
        // The marker now records both agents — a later CODEX resume stays
        // native, and a later claude one has a conversation to continue.
        assert_eq!(
            crate::worktree_agents::worktree_agents(&worktree),
            Some(vec![CodingAgent::Codex, CodingAgent::Claude])
        );
    }

    /// EXP-202: a CODEX resume with a recorded session for the worktree
    /// reopens EXACTLY that session — `resume <id>` leads the argv (codex's
    /// `--last` is global-latest, never used), no prompt rides at all.
    #[test]
    fn prepare_resume_codex_reopens_the_recorded_session() {
        let dir = temp_dir("resume-codex-native");
        let worktree = dir.0.join("wt");
        fs::create_dir_all(&worktree).unwrap();
        fs::write(worktree.join(PROMPT_FILE), "stale from the first run").unwrap();
        // A recorded rollout whose meta cwd IS the worktree.
        let sessions_root = dir.0.join("codex-sessions");
        let day = sessions_root.join("2026/07/20");
        fs::create_dir_all(&day).unwrap();
        fs::write(
            day.join("rollout-2026-07-20T10-00-00-sess-uuid-1.jsonl"),
            format!(
                "{}\n",
                serde_json::json!({
                    "type": "session_meta",
                    "payload": { "id": "sess-uuid-1", "cwd": worktree.to_string_lossy() },
                })
            ),
        )
        .unwrap();
        let base = canned_server(vec![
            (200, FOR_ISSUE_OK.to_string()),
            (200, TOKEN_OK.to_string()),
            (200, START_OK.to_string()),
        ]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree: worktree.clone(),
            seen: Default::default(),
        });
        let mut deps = make_deps(&base, &dir.0, worktrees);
        deps.settings.codex_path = "git".to_string(); // runnable stub
        deps.codex_sessions_root = Some(sessions_root);
        let mut req = request("EXP-42");
        req.resume = true;
        req.options = LaunchOptions {
            agent: CodingAgent::Codex,
            model: "gpt-5.6-sol".to_string(),
            effort: "".to_string(),
            ultracode: false,
            plan_mode: false,
            skip_permissions: false,
        };

        let prepared = match prepare(&PrepareRequest::Issue(req), &deps).unwrap() {
            Prepared::Ready(prepared) => prepared,
            other => panic!("expected Ready, got {other:?}"),
        };

        // The subcommand form leads; flags stay; NO prompt positional.
        assert_eq!(
            prepared.spawn.args[..2],
            ["resume".to_string(), "sess-uuid-1".to_string()]
        );
        assert!(!prepared.spawn.args.iter().any(|arg| arg == "--last"));
        assert!(!prepared.spawn.args.iter().any(|arg| arg.contains("EXP-42:")));
        assert!(prepared
            .spawn
            .args
            .contains(&"mcp_servers.exponential.bearer_token_env_var=\"EXP_MCP_TOKEN\"".to_string()));
        // Stale-seed hygiene + a fresh session row, same as claude.
        assert!(!worktree.join(PROMPT_FILE).exists());
        assert_eq!(prepared.session_id, "sess-1");
    }

    /// EXP-202: a CODEX resume with NO recorded session for the worktree
    /// (coded by another agent, rollouts pruned) falls back to a fresh
    /// session in the reused worktree seeded with the RESUME prompt
    /// (inspect existing work, continue, update the PR).
    #[test]
    fn prepare_resume_codex_seeds_the_resume_prompt() {
        let dir = temp_dir("resume-codex");
        let worktree = dir.0.join("wt");
        fs::create_dir_all(&worktree).unwrap();
        let base = canned_server(vec![
            (200, FOR_ISSUE_OK.to_string()),
            (200, TOKEN_OK.to_string()),
            (200, START_OK.to_string()),
        ]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree: worktree.clone(),
            seen: Default::default(),
        });
        let mut deps = make_deps(&base, &dir.0, worktrees);
        deps.settings.codex_path = "git".to_string(); // runnable stub
        // An empty recorded-sessions tree — hermetic (never the dev
        // machine's real ~/.codex).
        deps.codex_sessions_root = Some(dir.0.join("codex-sessions-empty"));
        let mut req = request("EXP-42");
        req.resume = true;
        req.options = LaunchOptions {
            agent: CodingAgent::Codex,
            model: "gpt-5.6-sol".to_string(),
            effort: "".to_string(),
            ultracode: false,
            plan_mode: false,
            skip_permissions: false,
        };

        let prepared = match prepare(&PrepareRequest::Issue(req), &deps).unwrap() {
            Prepared::Ready(prepared) => prepared,
            other => panic!("expected Ready, got {other:?}"),
        };

        // The positional is the RESUME prompt, not the seed prompt.
        let positional = prepared.spawn.args.last().unwrap();
        assert_eq!(
            positional,
            &render_resume_prompt("EXP-42", "Fix login flicker", "main")
        );
        assert!(positional.contains("git log origin/main..HEAD"));
        assert!(!positional.contains("## Issue context"));
        // Never a resume subcommand or `--continue` for the fallback, and
        // the codex MCP posture holds (env token, no on-disk config).
        assert_ne!(prepared.spawn.args[0], "resume");
        assert!(!prepared.spawn.args.iter().any(|arg| arg == "--continue"));
        assert!(!worktree.join(".exp-mcp.json").exists());
        assert!(prepared
            .spawn
            .env
            .contains(&("EXP_MCP_TOKEN".to_string(), "expu_seeded".to_string())));
    }

    /// Step 6.5 (EXP-194): a backlog/todo issue is flipped to `in_progress`
    /// by the LAUNCHER, after the session row (request order proves it), so
    /// the issue never lingers in backlog while plan mode holds the agent's
    /// MCP calls back.
    #[test]
    fn prepare_flips_a_todo_issue_to_in_progress() {
        let dir = temp_dir("flip");
        let worktree = dir.0.join("wt");
        fs::create_dir_all(&worktree).unwrap();
        let (base, requests) = canned_server_recording(vec![
            (200, FOR_ISSUE_OK.to_string()),
            (200, TOKEN_OK.to_string()),
            (200, START_OK.to_string()),
            (200, UPDATE_OK.to_string()),
        ]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree,
            seen: Default::default(),
        });
        let deps = make_deps(&base, &dir.0, worktrees);
        let mut req = request("EXP-42");
        req.issue_status = IssueStatus::Todo;

        match prepare(&PrepareRequest::Issue(req), &deps).unwrap() {
            Prepared::Ready(prepared) => assert_eq!(prepared.session_id, "sess-1"),
            other => panic!("expected Ready, got {other:?}"),
        }
        let seen = requests.lock().unwrap();
        let update = seen
            .iter()
            .find(|request| request.starts_with("POST /api/trpc/issues.update"))
            .expect("the launcher must send the in_progress flip");
        assert!(update.ends_with(r#"{"id":"issue-1","status":"in_progress"}"#));
        // After codingSessions.start — a Disabled outcome never flips.
        assert!(
            seen.iter()
                .position(|r| r.starts_with("POST /api/trpc/codingSessions.start"))
                < seen
                    .iter()
                    .position(|r| r.starts_with("POST /api/trpc/issues.update"))
        );
    }

    /// Step 6.5 only ever PROMOTES backlog/todo — an issue already
    /// in_progress (or beyond) is left alone.
    #[test]
    fn prepare_skips_the_flip_for_non_backlog_todo() {
        let dir = temp_dir("no-flip");
        let worktree = dir.0.join("wt");
        fs::create_dir_all(&worktree).unwrap();
        let (base, requests) = canned_server_recording(vec![
            (200, FOR_ISSUE_OK.to_string()),
            (200, TOKEN_OK.to_string()),
            (200, START_OK.to_string()),
        ]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree,
            seen: Default::default(),
        });
        let deps = make_deps(&base, &dir.0, worktrees);

        // request() snapshots in_progress — the default fixture is the guard.
        match prepare(&PrepareRequest::Issue(request("EXP-42")), &deps).unwrap() {
            Prepared::Ready(prepared) => assert_eq!(prepared.session_id, "sess-1"),
            other => panic!("expected Ready, got {other:?}"),
        }
        assert!(
            !requests
                .lock()
                .unwrap()
                .iter()
                .any(|request| request.contains("issues.update")),
            "an in_progress issue must not be re-flipped"
        );
    }

    /// Plan mode OFF rides the classic skip flag — the dialog's choice, not
    /// a launcher hardcode.
    #[test]
    fn plan_mode_off_uses_the_skip_flag() {
        let dir = temp_dir("skip-flag");
        let worktree = dir.0.join("wt");
        fs::create_dir_all(&worktree).unwrap();
        let base = canned_server(vec![
            (200, FOR_ISSUE_OK.to_string()),
            (200, TOKEN_OK.to_string()),
            (200, START_OK.to_string()),
        ]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree: worktree.clone(),
            seen: Default::default(),
        });
        let deps = make_deps(&base, &dir.0, worktrees);
        let mut req = request("EXP-42");
        req.options.plan_mode = false;
        req.options.skip_permissions = true;
        req.options.effort = "xhigh".to_string();

        match prepare(&PrepareRequest::Issue(req), &deps).unwrap() {
            Prepared::Ready(prepared) => {
                assert_eq!(
                    prepared.spawn.args[..8],
                    [
                        "--model".to_string(),
                        "fable".to_string(),
                        "--effort".to_string(),
                        "xhigh".to_string(),
                        "--mcp-config".to_string(),
                        ".exp-mcp.json".to_string(),
                        "--strict-mcp-config".to_string(),
                        "--dangerously-skip-permissions".to_string(),
                    ]
                );
                assert!(!prepared.spawn.args.iter().any(|arg| arg == "--permission-mode"));
            }
            other => panic!("expected Ready, got {other:?}"),
        }
    }

    /// EXP-201: a CODEX launch writes NO `.exp-mcp.json` (the raw key rides
    /// only the spawn env as EXP_MCP_TOKEN), composes the `-c mcp_servers.*`
    /// overrides + the explicit Auto preset, and titles the tab `codex · …`.
    #[test]
    fn prepare_codex_full_sequence() {
        let dir = temp_dir("codex-happy");
        let worktree = dir.0.join("wt");
        fs::create_dir_all(&worktree).unwrap();
        let base = canned_server(vec![
            (200, FOR_ISSUE_OK.to_string()),
            (200, TOKEN_OK.to_string()),
            (200, START_OK.to_string()),
        ]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree: worktree.clone(),
            seen: Default::default(),
        });
        let mut deps = make_deps(&base, &dir.0, worktrees);
        deps.settings.codex_path = "git".to_string(); // runnable stub
        let mut req = request("EXP-42");
        req.options = LaunchOptions {
            agent: CodingAgent::Codex,
            model: "gpt-5.6-sol".to_string(),
            effort: "high".to_string(),
            ultracode: false,
            plan_mode: false,
            skip_permissions: false,
        };

        let prepared = match prepare(&PrepareRequest::Issue(req), &deps).unwrap() {
            Prepared::Ready(prepared) => prepared,
            other => panic!("expected Ready, got {other:?}"),
        };

        assert_eq!(prepared.tab_title, "codex · EXP-42");
        assert_eq!(prepared.spawn.program, "git"); // the configured codex path
        // NO on-disk MCP config for codex — the key must not land in the tree.
        assert!(!worktree.join(".exp-mcp.json").exists());
        assert!(!worktree.join(".exp-pi-mcp.ts").exists());
        // The -c overrides point at the instance /api/mcp; the token itself
        // never rides argv…
        let args = &prepared.spawn.args;
        assert!(args.contains(&format!("mcp_servers.exponential.url=\"{base}/api/mcp\"")));
        assert!(args
            .contains(&"mcp_servers.exponential.bearer_token_env_var=\"EXP_MCP_TOKEN\"".to_string()));
        assert!(!args.iter().any(|arg| arg.contains("expu_")));
        // …it rides the spawn env.
        assert!(prepared
            .spawn
            .env
            .contains(&("EXP_MCP_TOKEN".to_string(), "expu_seeded".to_string())));
        // Auto preset (skip OFF): workspace-write + on-request + network.
        assert!(args.contains(&"--sandbox".to_string()));
        assert!(args.contains(&"workspace-write".to_string()));
        assert!(args.contains(&"sandbox_workspace_write.network_access=true".to_string()));
        assert!(!args.iter().any(|arg| arg == "--dangerously-bypass-approvals-and-sandbox"));
        // Prompt positional-last, model/effort flags present.
        assert_eq!(args[..2], ["-m".to_string(), "gpt-5.6-sol".to_string()]);
        assert!(args.contains(&"model_reasoning_effort=\"high\"".to_string()));
        assert!(args.last().unwrap().contains("EXP-42"));
    }

    /// EXP-201: a PI launch writes the `.exp-pi-mcp.ts` bridge (no
    /// `.exp-mcp.json`), loads it via `-e`, and carries url + token +
    /// PI_SKIP_VERSION_CHECK in the spawn env; tab titled `pi · …`.
    #[test]
    fn prepare_pi_full_sequence() {
        let dir = temp_dir("pi-happy");
        let worktree = dir.0.join("wt");
        fs::create_dir_all(&worktree).unwrap();
        let base = canned_server(vec![
            (200, FOR_ISSUE_OK.to_string()),
            (200, TOKEN_OK.to_string()),
            (200, START_OK.to_string()),
        ]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree: worktree.clone(),
            seen: Default::default(),
        });
        let mut deps = make_deps(&base, &dir.0, worktrees);
        deps.settings.pi_path = "git".to_string(); // runnable stub
        let mut req = request("EXP-42");
        req.options = LaunchOptions {
            agent: CodingAgent::Pi,
            model: "grok-4.5".to_string(),
            effort: "high".to_string(),
            ultracode: false,
            plan_mode: false,
            skip_permissions: false,
        };

        let prepared = match prepare(&PrepareRequest::Issue(req), &deps).unwrap() {
            Prepared::Ready(prepared) => prepared,
            other => panic!("expected Ready, got {other:?}"),
        };

        assert_eq!(prepared.tab_title, "pi · EXP-42");
        // The bridge is on disk (static, secret-free); no .exp-mcp.json.
        let bridge = fs::read_to_string(worktree.join(".exp-pi-mcp.ts")).unwrap();
        assert!(!bridge.contains("expu_"));
        assert!(!worktree.join(".exp-mcp.json").exists());
        let args = &prepared.spawn.args;
        assert_eq!(
            args[..6],
            [
                "--model".to_string(),
                "grok-4.5".to_string(),
                "--thinking".to_string(),
                "high".to_string(),
                "-e".to_string(),
                "./.exp-pi-mcp.ts".to_string(),
            ]
        );
        // pi has no permission flags; never -a (would auto-trust the repo).
        assert!(!args.iter().any(|arg| arg == "-a" || arg == "--approve"));
        for (key, value) in [
            ("EXP_MCP_URL", format!("{base}/api/mcp")),
            ("EXP_MCP_TOKEN", "expu_seeded".to_string()),
            ("PI_SKIP_VERSION_CHECK", "1".to_string()),
        ] {
            assert!(
                prepared.spawn.env.contains(&(key.to_string(), value.clone())),
                "missing env {key}={value}: {:?}",
                prepared.spawn.env
            );
        }
    }

    /// EXP-201 per-agent doctor gate: a missing codex blocks a CODEX launch
    /// with the codex copy — while claude (the settings stub) stays fine.
    #[test]
    fn missing_selected_agent_blocks_with_its_own_copy() {
        let dir = temp_dir("codex-missing");
        let worktrees = Arc::new(FakeWorktrees {
            worktree: dir.0.join("wt"),
            seen: Default::default(),
        });
        // Unroutable base: any network call would error the launch — proving
        // the doctor gate fires first.
        let mut deps = make_deps("http://127.0.0.1:1", &dir.0, worktrees);
        deps.settings.codex_path = "definitely-not-a-real-binary-exp".to_string();
        let mut req = request("EXP-42");
        req.options.agent = CodingAgent::Codex;

        match prepare(&PrepareRequest::Issue(req), &deps).unwrap() {
            Prepared::Disabled(DisabledReason::DoctorFailed(check)) => {
                assert_eq!(check.tool, crate::doctor::Tool::Codex);
                assert_eq!(
                    check.error.as_deref(),
                    Some("codex not found on PATH — set an absolute path")
                );
            }
            other => panic!("expected DoctorFailed, got {other:?}"),
        }
    }

    /// A >28KB rendered prompt cannot ride argv (Windows' 32,767-char command
    /// line cap): it falls back to PROMPT.md + the seed-line positional.
    #[test]
    fn oversized_description_falls_back_to_prompt_md() {
        let dir = temp_dir("oversized");
        let worktree = dir.0.join("wt");
        fs::create_dir_all(&worktree).unwrap();
        let base = canned_server(vec![
            (200, FOR_ISSUE_OK.to_string()),
            (200, TOKEN_OK.to_string()),
            (200, START_OK.to_string()),
        ]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree: worktree.clone(),
            seen: Default::default(),
        });
        let mut deps = make_deps(&base, &dir.0, worktrees);
        deps.issue_seed = Arc::new(|_| {
            Some(IssueSeed {
                title: "Huge".to_string(),
                description: Some("x".repeat(PROMPT_ARGV_MAX_BYTES + 1)),
            })
        });

        let prepared = match prepare(&PrepareRequest::Issue(request("EXP-42")), &deps).unwrap() {
            Prepared::Ready(prepared) => prepared,
            other => panic!("expected Ready, got {other:?}"),
        };
        assert_eq!(prepared.spawn.args.last().map(String::as_str), Some(SEED_LINE));
        let prompt = fs::read_to_string(worktree.join(PROMPT_FILE)).unwrap();
        assert!(prompt.contains("**EXP-42: Huge**"));
    }

    // ---- the batch happy path through the SAME prepare ----

    #[test]
    fn prepare_batch_full_sequence() {
        let dir = temp_dir("batch-happy");
        let worktree = dir.0.join("wt");
        fs::create_dir_all(&worktree).unwrap();
        let base = canned_server(vec![
            (200, TOKEN_OK.to_string()),
            (200, START_BATCH_OK.to_string()),
        ]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree: worktree.clone(),
            seen: Default::default(),
        });
        let deps = make_deps(&base, &dir.0, worktrees.clone());

        let prepared = match prepare(&PrepareRequest::Batch(batch_request()), &deps).unwrap() {
            Prepared::Ready(prepared) => prepared,
            Prepared::Disabled(reason) => panic!("unexpectedly disabled: {reason:?}"),
        };

        assert_eq!(prepared.session_id, "sess-b");
        assert_eq!(prepared.branch, "exp/batch-a1b2c3d4");
        assert_eq!(prepared.tab_title, "claude · EXP-42 +1");
        assert_eq!(prepared.tab_title_prefix, "EXP-42 +1");
        assert_eq!(prepared.issue_identifier, "batch-a1b2c3d4");
        // P9 refresher inputs ride along (repo id from the request's group,
        // clone path under the repos root).
        assert_eq!(prepared.repository_id.as_deref(), Some("repo-1"));
        assert_eq!(prepared.clone, dir.0.join("repos").join("acme").join("web"));

        // Git prepared the BATCH branch from the dialog-resolved repo (no
        // repositories.forIssue call — the canned server held only token +
        // start).
        let seen = worktrees.seen.lock().unwrap();
        assert_eq!(
            seen.as_slice(),
            &[(
                "acme/web".to_string(),
                "main".to_string(),
                "exp/batch-a1b2c3d4".to_string(),
                Some("2026-07-03T12:55:00.000Z".to_string())
            )]
        );

        // .exp-mcp.json (any subagents Claude spawns inherit it).
        let mcp = fs::read_to_string(worktree.join(".exp-mcp.json")).unwrap();
        assert!(mcp.contains("Bearer expu_seeded"));

        // The spawn args: ultracode = `--effort ultracode` (model untouched),
        // NO --agents (batch runs pre-define no subagents), the
        // explicit+strict MCP config (EXP-83), plan_mode:false ⇒ the skip
        // flag, and the FULL rendered prompt positional-last — a small batch
        // prompt rides argv directly, so NO PROMPT.md lands on disk.
        assert_eq!(prepared.spawn.program, "git");
        assert_eq!(
            prepared.spawn.args[..4],
            [
                "--model".to_string(),
                "opus".to_string(),
                "--effort".to_string(),
                "ultracode".to_string(),
            ]
        );
        assert!(!prepared.spawn.args.iter().any(|arg| arg == "--agents"));
        assert_eq!(
            prepared.spawn.args[4..7],
            [
                "--mcp-config".to_string(),
                ".exp-mcp.json".to_string(),
                "--strict-mcp-config".to_string(),
            ]
        );
        assert_eq!(prepared.spawn.args[7], "--dangerously-skip-permissions");
        let positional = prepared.spawn.args.last().unwrap();
        assert!(positional.contains("implement ALL 2 issues"));
        assert!(positional.contains("### EXP-42: Fix login flicker"));
        assert!(positional.contains("### EXP-43: Add badge"));
        assert!(positional.contains("exp/batch-a1b2c3d4"));
        assert!(positional.contains("issueIds: [\"issue-1\", \"issue-2\"]"));
        assert!(!worktree.join(PROMPT_FILE).exists());
        assert_eq!(prepared.spawn.cwd.as_deref(), Some(worktree.as_path()));
    }

    /// An oversized batch prompt takes the same PROMPT.md fallback as the
    /// issue path — the size gate applies to both shapes.
    #[test]
    fn oversized_batch_prompt_falls_back_to_prompt_md() {
        let dir = temp_dir("batch-oversized");
        let worktree = dir.0.join("wt");
        fs::create_dir_all(&worktree).unwrap();
        let base = canned_server(vec![
            (200, TOKEN_OK.to_string()),
            (200, START_BATCH_OK.to_string()),
        ]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree: worktree.clone(),
            seen: Default::default(),
        });
        let deps = make_deps(&base, &dir.0, worktrees);
        let mut req = batch_request();
        req.issues[0].description = Some("x".repeat(PROMPT_ARGV_MAX_BYTES + 1));

        let prepared = match prepare(&PrepareRequest::Batch(req), &deps).unwrap() {
            Prepared::Ready(prepared) => prepared,
            other => panic!("expected Ready, got {other:?}"),
        };
        assert_eq!(prepared.spawn.args.last().map(String::as_str), Some(SEED_LINE));
        let prompt = fs::read_to_string(worktree.join(PROMPT_FILE)).unwrap();
        assert!(prompt.contains("### EXP-42: Fix login flicker"));
    }

    #[test]
    fn batch_session_limit_and_token_denied_map_like_the_issue_path() {
        let dir = temp_dir("batch-limit");
        let worktree = dir.0.join("wt");
        fs::create_dir_all(&worktree).unwrap();
        let base = canned_server(vec![
            (200, TOKEN_OK.to_string()),
            (412, r#"{"error":{"message":"Concurrent coding session limit reached — upgrade to run more.","code":-32012,"data":{"code":"PRECONDITION_FAILED","httpStatus":412}}}"#.to_string()),
        ]);
        let worktrees = Arc::new(FakeWorktrees { worktree, seen: Default::default() });
        let deps = make_deps(&base, &dir.0, worktrees);
        match prepare(&PrepareRequest::Batch(batch_request()), &deps).unwrap() {
            Prepared::Disabled(DisabledReason::SessionLimit { message }) => {
                assert!(message.contains("upgrade"));
            }
            other => panic!("expected SessionLimit, got {other:?}"),
        }

        let dir = temp_dir("batch-denied");
        let worktree = dir.0.join("wt");
        fs::create_dir_all(&worktree).unwrap();
        let base = canned_server(vec![(
            403,
            r#"{"error":{"message":"You are not a member of this team","code":-32003,"data":{"code":"FORBIDDEN","httpStatus":403}}}"#.to_string(),
        )]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree,
            seen: Default::default(),
        });
        let deps = make_deps(&base, &dir.0, worktrees);
        match prepare(&PrepareRequest::Batch(batch_request()), &deps).unwrap() {
            Prepared::Disabled(DisabledReason::TokenDenied { message }) => {
                assert!(message.contains("not a member"));
            }
            other => panic!("expected TokenDenied, got {other:?}"),
        }
    }

    // ---- The hidden key auto-mints on the FIRST coding session ----

    /// §7.2 runtime path: an EMPTY token store at launch time silently mints
    /// via `users.mintPersonalApiKey` (request 3 — the key race lands between
    /// `installationToken` and `codingSessions.start`), stores the raw key +
    /// row id, and `.exp-mcp.json` carries the fresh key. No manual key UI exists
    /// anywhere; this is the only way the key ever comes to be.
    #[test]
    fn first_session_auto_mints_the_hidden_personal_key() {
        let dir = temp_dir("auto-mint");
        let worktree = dir.0.join("wt");
        fs::create_dir_all(&worktree).unwrap();
        let base = canned_server(vec![
            (200, FOR_ISSUE_OK.to_string()),
            (200, TOKEN_OK.to_string()),
            (200, MINT_OK.to_string()),
            (200, START_OK.to_string()),
        ]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree: worktree.clone(),
            seen: Default::default(),
        });
        let deps = make_deps(&base, &dir.0, worktrees);
        // The launcher finds NO stored key — the runtime auto-mint must fire.
        deps.token_store.delete("acct", SecretKind::PersonalApiKey);
        assert_eq!(deps.token_store.get("acct", SecretKind::PersonalApiKey), None);

        match prepare(&PrepareRequest::Issue(request("EXP-42")), &deps).unwrap() {
            Prepared::Ready(prepared) => assert_eq!(prepared.session_id, "sess-1"),
            other => panic!("expected Ready, got {other:?}"),
        }

        // Minted key + row id kept for later sessions / Regenerate (§7.2).
        assert_eq!(
            deps.token_store
                .get("acct", SecretKind::PersonalApiKey)
                .as_deref(),
            Some("expu_minted_runtime")
        );
        assert_eq!(
            deps.token_store
                .get("acct", SecretKind::PersonalApiKeyId)
                .as_deref(),
            Some("key-9")
        );
        // .exp-mcp.json is the ONLY on-disk consumer of the raw key (§7.1 step 4).
        let mcp = fs::read_to_string(worktree.join(".exp-mcp.json")).unwrap();
        assert!(mcp.contains("Bearer expu_minted_runtime"), "mcp: {mcp}");
    }

    // ---- §7.6: N issues = N worktrees/branches/sessions (client never
    //      self-throttles; per-launch state never bleeds across issues) ----

    #[test]
    fn two_issues_prepare_into_isolated_worktrees_and_sessions() {
        let dir = temp_dir("concurrent-prep");
        let wt_a = dir.0.join("wt-a");
        let wt_b = dir.0.join("wt-b");
        fs::create_dir_all(&wt_a).unwrap();
        fs::create_dir_all(&wt_b).unwrap();

        let launch = |identifier: &str, issue_id: &str, session: &str, worktree: &PathBuf| {
            let base = canned_server(vec![
                (200, FOR_ISSUE_OK.to_string()),
                (200, TOKEN_OK.to_string()),
                (
                    200,
                    format!(
                        r#"{{"result":{{"data":{{"session":{{"id":"{session}","issueId":"{issue_id}","status":"running"}}}}}}}}"#
                    ),
                ),
            ]);
            let worktrees = Arc::new(FakeWorktrees {
                worktree: worktree.clone(),
                seen: Default::default(),
            });
            let deps = make_deps(&base, &dir.0, worktrees);
            let mut req = request(identifier);
            req.issue_id = issue_id.to_string();
            match prepare(&PrepareRequest::Issue(req), &deps).unwrap() {
                Prepared::Ready(prepared) => prepared,
                other => panic!("expected Ready, got {other:?}"),
            }
        };

        let a = launch("EXP-1", "issue-a", "sess-a", &wt_a);
        let b = launch("EXP-2", "issue-b", "sess-b", &wt_b);

        // Distinct branches, worktrees, session ids, tab titles — the §7.6
        // "no collision" invariant at the prepare layer (the manager-side
        // PTY/tab isolation is tests/concurrent.rs).
        assert_eq!(a.branch, "exp/EXP-1");
        assert_eq!(b.branch, "exp/EXP-2");
        assert_ne!(a.worktree, b.worktree);
        assert_ne!(a.session_id, b.session_id);
        assert_eq!(a.tab_title, "claude · EXP-1");
        assert_eq!(b.tab_title, "claude · EXP-2");
        // Both spawn specs are cwd-bound to their OWN worktree.
        assert_eq!(a.spawn.cwd.as_deref(), Some(wt_a.as_path()));
        assert_eq!(b.spawn.cwd.as_deref(), Some(wt_b.as_path()));
    }

    #[test]
    fn default_device_label_is_never_empty() {
        assert!(!default_device_label().trim().is_empty());
    }

    #[test]
    fn prompt_falls_back_to_identifier_when_sync_store_misses() {
        let dir = temp_dir("fallback");
        let worktree = dir.0.join("wt");
        fs::create_dir_all(&worktree).unwrap();
        let base = canned_server(vec![
            (200, FOR_ISSUE_OK.to_string()),
            (200, TOKEN_OK.to_string()),
            (200, START_OK.to_string()),
        ]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree: worktree.clone(),
            seen: Default::default(),
        });
        let mut deps = make_deps(&base, &dir.0, worktrees);
        deps.issue_seed = Arc::new(|_| None);

        let prepared = match prepare(&PrepareRequest::Issue(request("EXP-7")), &deps).unwrap() {
            Prepared::Ready(prepared) => prepared,
            other => panic!("expected Ready, got {other:?}"),
        };
        let positional = prepared.spawn.args.last().unwrap();
        assert!(positional.contains("**EXP-7: EXP-7**"));
        assert!(positional.contains("(no description)"));
    }
}
