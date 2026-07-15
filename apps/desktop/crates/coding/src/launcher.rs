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
use api::{coding_sessions, repositories, users};
use gpui::App;
use terminal::pty::SpawnSpec;
use terminal::tab::{TabId, TabKind};
use terminal::TerminalManager;

use crate::argv::{session_args, LaunchOptions};
use crate::batch_launcher::{batch_branch_name, BatchLaunchRequest};
use crate::batch_prompt::{render_batch_prompt, BatchPromptArgs};
use crate::doctor::{run_doctor, ToolCheck};
use crate::git_credentials;
use crate::git_worktree::{
    branch_name, clone_path, create_worktree, ensure_clone, fetch_base,
    shared_cargo_target_dir, GitError, TokenUrl,
};
use crate::mcp_json::write_mcp_json;
use crate::prompt::{deliver_prompt, render_prompt};
use crate::settings::Settings;

/// Cadence of the `codingSessions.heartbeat` liveness ping while the claude
/// child is alive. Must stay far inside the server's staleness window
/// (`CODING_SESSION_STALE_HOURS` = 24h in `@exp/db-schema/domain`, measured
/// from the row's `updated_at`) so that dozens of pings would have to fail
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
    /// Hostname; also `coding_sessions.device_label`.
    pub device_label: String,
    pub origin: LaunchOrigin,
    /// The Start-coding dialog's model/effort/mode choices (settings
    /// defaults for relay starts — [`LaunchOptions::issue_defaults`]).
    pub options: LaunchOptions,
}

/// The two launch shapes ONE [`prepare`] serves.
#[derive(Clone, Debug)]
pub enum PrepareRequest {
    Issue(LaunchRequest),
    Batch(BatchLaunchRequest),
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
        // .exp-mcp.json carries the raw expu_ key and claude is told to
        // commit + push — keep it out of `git add -A` via the shared,
        // never-committed `.git/info/exclude` (best-effort by design; the
        // PROMPT.md exclude rides [`crate::prompt::deliver_prompt_file`]).
        let _ = crate::git_worktree::ensure_local_excludes(
            &clone,
            &[crate::mcp_json::MCP_JSON_FILE],
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
                "Link a repository to this project in workspace settings.".to_string()
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
    /// The workspace `repositories` row id — re-mints the installation token
    /// mid-session (EXP-56 P9).
    pub repository_id: String,
    /// The real git branch (keeps its `/`), e.g. `exp/EXP-42` — or the
    /// batch branch `exp/batch-<id8>`.
    pub branch: String,
    /// The claude invocation in the worktree (§7.1 step 7).
    pub spawn: SpawnSpec,
    /// Tab strip default title (`claude · EXP-42` / `claude · EXP-42 +2`).
    pub tab_title: String,
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
    // Step 0 — the doctor gate. Cheap relative to clone/mint and structural:
    // the relay origin has no button whose disabled state could have gated
    // this.
    let report = run_doctor(&deps.settings);
    if let Some(failed) = report.first_failure() {
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

    // Step 4 — .exp-mcp.json (authenticates the spawned claude as the real
    // user; any subagents it spawns inherit the session's MCP servers; NOT
    // named .mcp.json — EXP-98, see `crate::mcp_json`).
    let personal_key = key_handle
        .join()
        .map_err(|_| CodingError::Io("personal-key thread panicked".to_string()))??;
    write_mcp_json(&worktree, deps.trpc.base_url(), &personal_key)
        .map_err(|e| CodingError::Io(format!("write .exp-mcp.json: {e}")))?;

    // Step 5 — the seed prompt (both shapes: direct argv delivery when
    // small, PROMPT.md + seed line otherwise).
    let rendered = match req {
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
    };
    let delivery = deliver_prompt(&worktree, &clone, &rendered)
        .map_err(|e| CodingError::Io(format!("deliver prompt: {e}")))?;

    // Step 6 — the session row, BEFORE spawn (the id keys everything).
    let session = match req {
        PrepareRequest::Issue(issue_req) => coding_sessions::start(
            &deps.trpc,
            &issue_req.issue_id,
            Some(&issue_req.device_label),
        ),
        PrepareRequest::Batch(batch_req) => coding_sessions::start_batch(
            &deps.trpc,
            &batch_req.workspace_id,
            Some(&batch_req.device_label),
        ),
    };
    let session = match session {
        Ok(session) => session,
        Err(ApiError::Http { status: 412, message }) => {
            return Ok(Prepared::Disabled(DisabledReason::SessionLimit { message }))
        }
        Err(err) => return Err(err.into()),
    };

    // Step 7's spawn spec — argv from [`crate::argv`]: explicit `--model`,
    // the native permission posture, and the prompt positional-last (bytes
    // typed into the PTY before the TUI enters raw mode get swallowed during
    // startup, so the prompt must never be delivered via stdin).
    let (args, issue_identifier, tab_title) = match req {
        PrepareRequest::Issue(issue_req) => (
            session_args(&issue_req.options, delivery.positional()),
            issue_req.issue_identifier.clone(),
            format!("claude · {}", issue_req.issue_identifier),
        ),
        PrepareRequest::Batch(batch_req) => {
            let first = batch_req
                .issues
                .first()
                .map(|issue| issue.issue_identifier.as_str())
                .unwrap_or("batch");
            let extra = batch_req.issues.len().saturating_sub(1);
            (
                session_args(&batch_req.options, delivery.positional()),
                format!("batch-{}", batch_req.batch_id),
                format!("claude · {first} +{extra}"),
            )
        }
    };
    let spawn = SpawnSpec::new(&deps.settings.resolved_claude_path())
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

    Ok(Prepared::Ready(PreparedLaunch {
        session_id: session.id,
        issue_identifier,
        worktree,
        clone,
        repository_id,
        branch,
        spawn,
        tab_title,
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
    let PreparedLaunch { session_id, worktree, branch, spawn, tab_title, .. } = prepared;

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
                    let _ = coding_sessions::heartbeat(&trpc, &session_id);
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
                .open_tab(TabKind::Claude, tab_title, &spawn, Some(on_exit), cx)
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
        canned_server, make_deps, temp_dir, FakeWorktrees, FOR_ISSUE_OK, MINT_OK, START_BATCH_OK,
        START_OK, TOKEN_OK,
    };
    use api::token_store::SecretKind;
    use std::fs;

    fn request(identifier: &str) -> LaunchRequest {
        LaunchRequest {
            issue_id: "issue-1".to_string(),
            issue_identifier: identifier.to_string(),
            device_label: "testbox".to_string(),
            origin: LaunchOrigin::Local,
            // The dialog defaults: fable, no effort, no ultracode, plan mode ON.
            options: LaunchOptions {
                model: "fable".to_string(),
                effort: "".to_string(),
                ultracode: false,
                plan_mode: true,
            },
        }
    }

    fn batch_options() -> LaunchOptions {
        LaunchOptions {
            model: "opus".to_string(),
            effort: "high".to_string(),
            ultracode: true,
            plan_mode: false,
        }
    }

    fn batch_request() -> BatchLaunchRequest {
        BatchLaunchRequest {
            batch_id: "a1b2c3d4".to_string(),
            workspace_id: "ws-1".to_string(),
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
                },
                BatchIssueSpec {
                    issue_id: "issue-2".to_string(),
                    issue_identifier: "EXP-43".to_string(),
                    title: "Add badge".to_string(),
                    description: None,
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
                    "Link a repository to this project in workspace settings."
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
            (412, r#"{"error":{"message":"The Exponential GitHub App is not installed on acme/web. Reconnect it in workspace settings.","code":-32012,"data":{"code":"PRECONDITION_FAILED","httpStatus":412}}}"#.to_string()),
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
            (403, r#"{"error":{"message":"You are not a member of this workspace","code":-32003,"data":{"code":"FORBIDDEN","httpStatus":403}}}"#.to_string()),
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
        // P9 refresher inputs: the server-confirmed repo id + the clone path
        // under the repos root (independent of the fake worktree location).
        assert_eq!(prepared.repository_id, "repo-1");
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
        assert_eq!(prepared.issue_identifier, "batch-a1b2c3d4");
        // P9 refresher inputs ride along (repo id from the request's group,
        // clone path under the repos root).
        assert_eq!(prepared.repository_id, "repo-1");
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
            r#"{"error":{"message":"You are not a member of this workspace","code":-32003,"data":{"code":"FORBIDDEN","httpStatus":403}}}"#.to_string(),
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

        // Minted key + row id stashed for later sessions / Regenerate (§7.2).
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
