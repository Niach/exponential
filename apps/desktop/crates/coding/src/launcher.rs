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
//! takes `LaunchOutcome::Spawned { session_id, .. }` and hands the session id
//! to the steer publisher (§08; EXP-249 removed the PTY tee with the binary
//! mirror).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use api::error::ApiError;
use api::token_store::TokenStore;
use api::trpc::TrpcClient;
use api::{coding_sessions, issues, repositories, users};
#[cfg(feature = "gpui")]
use gpui::App;
use terminal::pty::SpawnSpec;
use terminal::tab::{TabId, TabKind};
#[cfg(feature = "gpui")]
use terminal::TerminalManager;

use crate::agent::CodingAgent;
use crate::argv::{
    session_args, AgentMcp, LaunchOptions, SessionIdentity, SessionTail, HOOK_CONFIG_ENV,
    HOOK_PORT_ENV, MCP_SESSION_ID_ENV, MCP_TOKEN_ENV, MCP_URL_ENV, OBSERVER_TOKEN_ENV,
    OBSERVER_URL_ENV,
};
use crate::action_prompt::{
    chat_prompt, render_action_prompt_full, render_run_resume_prompt, ActionInputValue,
    TriggerNote, WorkspaceNote,
};
use crate::action_prompt::{create_action_prompt, fix_pr_conflicts_prompt};
use crate::batch_launcher::{
    action_run_branch, batch_branch_name, chat_run_branch, BatchLaunchRequest, RepoGroup,
};
use crate::run_cleanup::RunCleanup;
use crate::run_registry::{RunFix, RunInput, RunIssue, RunKind, RunRecord};
use domain::IssueStatus;
use crate::batch_prompt::{render_batch_prompt, BatchPromptArgs};
use crate::doctor::{run_doctor, ToolCheck};
use crate::pi_bridge::{write_pi_bridge, write_pi_observer, write_pi_plan};
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
pub const SESSION_HEARTBEAT_INTERVAL: std::time::Duration =
    std::time::Duration::from_secs(30 * 60);

/// EXP-511: the per-worktree scratch dir the steer publisher downloads a
/// steered message's image attachments into, so the agent reads a FILE instead
/// of an auth-gated URL. `coding` never talks to the relay (§3.1) — it only
/// owns the name and the git exclusion; the hosts (`ui`/`cli`) hand the path to
/// the publisher.
pub const STEER_IMAGES_DIR: &str = ".exp-steer-images";

/// The launcher's never-committed seed files + scratch dirs, appended to the
/// clone's shared `.git/info/exclude`. Best-effort coverage only: the
/// secret-carrying `.exp-mcp.json` has its own HARD, verified guard at the
/// write site ([`wire_agent_mcp`] via [`crate::git_worktree::ensure_ignored`],
/// EXP-474), and the PROMPT.md exclude rides
/// [`crate::prompt::deliver_prompt_file`].
const LOCAL_EXCLUDES: &[&str] = &[
    crate::mcp_json::MCP_JSON_FILE,
    crate::pi_bridge::PI_BRIDGE_FILE,
    crate::pi_bridge::PI_OBSERVER_FILE,
    crate::pi_bridge::PI_PLAN_FILE,
    crate::worktree_agents::AGENTS_FILE,
    STEER_IMAGES_DIR,
];

/// Where the launch came from (§7.1). Both origins run the SAME sequence —
/// the variant exists for the session's audit surface, not for branching.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaunchOrigin {
    /// The Start-coding button on the issue-detail header.
    Local,
    /// A relay `start_session` frame (§08's control channel). `started_by`
    /// (EXP-432) is the requesting teammate when the start targeted this
    /// machine as a SHARED server device — echoed into
    /// `codingSessions.start`/`heartbeat` so the session row is
    /// requester-owned; `None` for the owner's own remote starts.
    Relay {
        device_id: String,
        claimant: String,
        started_by: Option<String>,
    },
}

/// The (started_by, device_id) attribution pair every `codingSessions.start`
/// and heartbeat carries. `started_by_id` (EXP-432) is set only by a relay
/// start that named a requesting teammate — it makes the session row
/// requester-owned. `device_id` is the relay origin's device when there is
/// one, otherwise THIS machine's steer deviceId ([`CodingDeps::device_id`],
/// EXP-549) so the server can stamp `coding_sessions.device_id` and snapshot
/// the machine's current label. `(None, None)` — the byte-identical legacy
/// wire — when neither is known (tests).
fn attribution<'a>(
    origin: &'a LaunchOrigin,
    deps: &'a CodingDeps,
) -> coding_sessions::Attribution<'a> {
    let (started_by_id, relay_device_id) = match origin {
        LaunchOrigin::Relay {
            device_id,
            started_by,
            ..
        } => (started_by.as_deref(), Some(device_id.as_str())),
        _ => (None, None),
    };
    coding_sessions::Attribution {
        // Only a `started_by` relay start re-owns the row (EXP-432).
        started_by_id,
        device_id: relay_device_id.or(deps.device_id.as_deref()),
    }
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
    if let Ok(output) = terminal::process::background_command("hostname").output() {
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
    /// EXP-662: seed the RESUME prompt instead of the issue's first-launch
    /// one — "a previous session already worked on this branch, inspect what
    /// it left behind, continue" — and clamp `options.plan_mode` off (the
    /// plan already happened).
    ///
    /// This is the DEGRADED half of resume, not resume itself: an exact
    /// relaunch of the recorded conversation is
    /// [`PrepareRequest::ResumeRun`], and callers resolve that first
    /// ([`crate::run_registry::latest_for_issue`]). A record-less issue
    /// (coded before EXP-662, on another machine, or with its transcript
    /// gone) lands here — a fresh session in the reused worktree, told to
    /// pick the branch work back up.
    pub resume_prompt: bool,
}

/// Which program an action run executes (EXP-257/EXP-259). `Team` is a
/// user-authored action (fresh body fetched via `actions.get` right before
/// the run — EXP-268 removed the per-device trust gate); the other two are
/// the server-defined virtual BUILTINS whose prompts are composed from
/// shipped constants (`body` stays empty).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionRunKind {
    /// A team action row: preamble [+ inputs] + the fresh body.
    Team,
    /// The "Create action" creator (EXP-257): scratch cwd, prompt generated
    /// from the `description`/`repo` input values.
    CreateAction,
    /// The "Fix merge conflicts" run (EXP-259): spawned in the WORKTREE of
    /// the selected PR's branch (the caller resolved the `pr` input to the
    /// representative issue), rebases onto the PR's LIVE base (resolved via
    /// `issues.prepareConflictFix` at launch — EXP-324; a stacked PR rebases
    /// onto its parent's branch, a stale base is server-retargeted to the
    /// default first), resolves, force-pushes, and merges via
    /// `exponential_pr_merge`.
    FixConflicts {
        /// The PR's head branch (e.g. `exp/EXP-42` / `exp/batch-<id8>`).
        branch: String,
        /// The repo's server-reported default branch — the rebase-target
        /// FALLBACK for old servers without `issues.prepareConflictFix`
        /// (EXP-324); the live resolution in [`prepare`] wins otherwise.
        default_branch: String,
        /// The representative issue's identifier (prompt context + the
        /// `exponential_pr_merge` argument).
        identifier: String,
        /// The representative issue's UUID — the `issues.prepareConflictFix`
        /// argument (EXP-324).
        issue_id: String,
    },
    /// The hidden "Chat" builtin (EXP-615): a free-prompt session on the
    /// picked repository's TRUNK CLONE at its default branch — no worktree,
    /// no branch, no PR contract. The `prompt` input rides to the agent
    /// VERBATIM (no action preamble, no inputs section): this is the "open a
    /// terminal tab on the repo" shape, so anything we wrapped around it
    /// would be words the user did not write.
    Chat,
}

impl ActionRunKind {
    /// Whether this run is a server-defined virtual builtin (its session row
    /// is keyed by team — `codingSessions.start` can't resolve a team from
    /// the builtin literal).
    pub fn is_builtin(&self) -> bool {
        !matches!(self, Self::Team)
    }
}

/// An action run's launch input (EXP-253): no PR contract, no status flips —
/// an interactive agent session on the repo's trunk clone (autopulled), a
/// PR branch's worktree (the fix-conflicts builtin), or, for a repo-less
/// action, a scratch dir holding only the MCP config.
#[derive(Clone, Debug)]
pub struct ActionLaunchRequest {
    pub action_id: String,
    /// EXP-637: the client-minted run id ([`crate::batch_launcher::new_run_id`])
    /// — names this run's worktree branch (`exp/<slug>-<id8>` /
    /// `exp/chat-<id8>`) and keys its `runs.json` record. Minted by the
    /// caller (desktop `action_run`, CLI `launch`) exactly like a batch id.
    pub run_id: String,
    /// Display snapshot (tab title + heartbeat scope).
    pub action_name: String,
    /// The action's team (EXP-257): the builtin creator prompt targets it,
    /// and builtin session rows must send it (`codingSessions.start` can't
    /// resolve a team from the builtin literal).
    pub team_id: String,
    /// The FRESH body — the caller fetched it via `actions.get` right before
    /// the run (synced rows carry no body). Empty for the builtins (their
    /// prompts are generated).
    pub body: String,
    /// `Some` = run in this repo's trunk clone on the default branch (or,
    /// for [`ActionRunKind::FixConflicts`], the PR branch's worktree —
    /// REQUIRED there); `None` = repo-less (scratch dir). Local starts
    /// resolve it from the window resolver; relay starts carry it in the
    /// frame (batch precedent — the desktop syncs no repositories). Ignored
    /// for the creator builtin (its repo INPUT only pins the authored
    /// action's `repositoryId`).
    pub repo: Option<RepoGroup>,
    /// The resolved run-time input values (EXP-257), definition-ordered:
    /// real actions inject them as the prompt's `## Inputs` section; the
    /// creator builtin reads its `description`/`repo` inputs to build the
    /// creator prompt.
    pub inputs: Vec<ActionInputValue>,
    /// Which program this run executes (team action or a virtual builtin).
    pub kind: ActionRunKind,
    /// `Some` when an AUTOMATION started this run (EXP-530): renders the
    /// prompt's `## Trigger` section and stamps `startedReason` on the
    /// session row. Always `None` for user starts and the builtins (their
    /// generated prompts ignore it).
    pub trigger: Option<TriggerNote>,
    /// EXP-583: the `automations` row that fired it, stamped on the session
    /// row beside `startedReason`. `None` on every user start — the server
    /// refuses one without the other.
    pub automation_id: Option<String>,
    pub device_label: String,
    pub origin: LaunchOrigin,
    /// The FULL option set (EXP-257 — same per-agent vocabulary as issue
    /// runs; the old Claude-only clamp is gone).
    pub options: LaunchOptions,
}

/// EXP-637: RESUME an ended action/chat run — same workspace, same agent,
/// a NEW `coding_sessions` row pointing back at the old one
/// (`resumedFromId`). Everything the resume needs comes from the recorded
/// [`RunRecord`]; the caller only supplies the launch context.
#[derive(Clone, Debug)]
pub struct ResumeRunRequest {
    pub record: RunRecord,
    pub device_label: String,
    pub origin: LaunchOrigin,
    /// Optional per-resume overrides; `None` keeps the recorded values (a
    /// resumed run keeps its agent and options by contract — only the model
    /// and effort may be nudged).
    pub model: Option<String>,
    pub effort: Option<String>,
}

/// The four launch shapes ONE [`prepare`] serves.
#[derive(Clone, Debug)]
pub enum PrepareRequest {
    Issue(LaunchRequest),
    Batch(BatchLaunchRequest),
    Action(ActionLaunchRequest),
    ResumeRun(ResumeRunRequest),
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
        // Best-effort exclude coverage for the launcher's local-only files
        // ([`LOCAL_EXCLUDES`]), via the shared, never-committed
        // `.git/info/exclude`.
        let _ = crate::git_worktree::ensure_local_excludes(&clone, LOCAL_EXCLUDES);
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
    /// Where codex records its session rollouts, for the exact-session
    /// recovery a resume needs ([`prepare_resume_run`] — the only reader
    /// since EXP-662). `None` (production) = auto-detect
    /// [`crate::codex_sessions::default_codex_sessions_root`]; tests inject
    /// a fixture tree.
    pub codex_sessions_root: Option<PathBuf>,
    /// The app data dir — repo-less action runs execute in
    /// `<data_dir>/actions/<action id>/` (EXP-253).
    pub data_dir: PathBuf,
    /// EXP-637: where Claude Code writes per-cwd transcripts
    /// (`~/.claude/projects`). `None` (production) = auto-detect from `HOME`;
    /// tests inject a fixture tree. Read only by the resume probe — this
    /// crate cannot depend on `steer`, which owns the tailing side (§3.1).
    pub claude_projects_root: Option<PathBuf>,
    /// This machine's steer deviceId (desktop: [`api::device_identity`] via
    /// `steer::persistent_device_id`; CLI: `cli_device_id`). Rides
    /// `codingSessions.start`/`heartbeat` so the server stamps the session
    /// row's `device_id` and snapshots the machine's CURRENT label
    /// (EXP-549). `None` only in tests.
    pub device_id: Option<String>,
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
    /// batch branch `exp/batch-<id8>`. EXP-637: an action/chat run carries
    /// its own run branch here too (it used to be empty), so
    /// `LocalSessions::holds_branch` protects run worktrees and the
    /// inventory's `busy` follows.
    pub branch: String,
    /// EXP-637: what [`branch`](Self::branch) was cut from — the run
    /// cleanup's "did anything land here" compare. `None` for issue/batch
    /// sessions (the prune owns those) and repo-less runs.
    pub base_branch: Option<String>,
    /// EXP-637: the run worktree to auto-remove when the run ends, iff it is
    /// clean and carries no commits ([`crate::run_cleanup`]). `None` for
    /// issue/batch sessions (their worktrees survive by design) and for
    /// runs with no worktree of their own.
    pub run_cleanup: Option<RunCleanup>,
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
    /// EXP-275: the spawn runs with permissions bypassed
    /// (`--dangerously-skip-permissions` / codex bypass — mirrors
    /// `permission_args`: plan mode wins the starting mode, so it clears
    /// this). The activity emitter uses it to keep permission-flavored
    /// notifications from becoming "blocked on approval" cards.
    pub bypass_permissions: bool,
    /// EXP-529: the spawn launched into plan mode (claude `--permission-mode
    /// plan` / pi's plan extension) — mutually exclusive with
    /// `bypass_permissions` by the derivation above. The activity emitter
    /// stamps it into the launch narration so remote viewers can tell the
    /// run's effective permission posture.
    pub plan_mode: bool,
    /// EXP-383: which agent CLI the spawn runs. The steer wiring picks the
    /// matching activity emitter (claude transcript tail / codex rollout
    /// tail / pi observer) — every steer-room launch path flows through
    /// here, so resume needs no separate plumbing.
    pub agent: CodingAgent,
    /// EXP-443: claude's own session id, minted here and passed via
    /// `--session-id` so the transcript pin (and the hook router's `bound`
    /// set) exist before the first hook fires. `None` on native resume (the
    /// conversation keeps its original id — the SessionStart hook seeds the
    /// pin instead) and on non-claude agents.
    pub claude_session_id: Option<String>,
    /// EXP-443: the originator stamped into codex's rollout metas via
    /// [`crate::argv::CODEX_ORIGINATOR_ENV`] — the codex emitter's discovery
    /// discriminator. `None` on non-codex agents.
    pub codex_originator: Option<String>,
    /// EXP-443: the exact rollout session id a codex NATIVE RESUME reopens —
    /// the strongest discovery pin (an id match beats any originator/mtime
    /// heuristic). `None` on fresh spawns and non-codex agents.
    pub codex_resume_id: Option<String>,
    /// EXP-478: the clone's launch gate, held since BEFORE the worktree was
    /// created — the auto-prune's policy is a pre-fetch snapshot that cannot
    /// protect a worktree born after it, so the gate parks the prune for the
    /// launch's whole flight. The UI `.take()`s it out (the
    /// [`spawn_prepared_with`] destructure would drop it pre-spawn) and drops
    /// it only after `LocalSessions` registers the session; failure paths
    /// release via RAII. `None` for runs that never touch a session worktree
    /// (trunk-clone and scratch-dir action runs — the prune skips the clone
    /// root itself).
    pub launch_hold: Option<crate::launch_gate::LaunchHold>,
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

/// The claude hooks sidecar's per-session wiring (EXP-249), handed in by the
/// app/ui layer: it owns the `steer::hooks::HookServer` (this crate cannot
/// depend on `steer` — §3.1) and keeps it alive for the session's lifetime,
/// while the launcher writes the settings file and puts the two env vars on
/// the spawn. `port`/`token` address the loopback server; `settings_json` is
/// the ready-made file content (`steer::hooks::hook_settings_json`).
///
/// Absent (or a non-claude agent) = no sidecar: the session runs exactly as
/// it did before, on grid-only detection.
#[derive(Clone, Debug)]
pub struct HookSetup {
    pub port: u16,
    pub token: String,
    pub settings_json: String,
}

/// The pi observer sidecar's per-session wiring (EXP-383), handed in by the
/// app/ui layer exactly like [`HookSetup`]: it owns the
/// `steer::pi_observer::ObserverServer` (this crate cannot depend on `steer`
/// — §3.1) and the launcher puts the two env vars on a pi spawn so the
/// `.exp-pi-observer.ts` extension can reach it.
///
/// Absent (or a non-pi agent) = no observer env: the extension file is still
/// written but stays inert.
#[derive(Clone, Debug)]
pub struct ObserverSetup {
    pub port: u16,
    pub token: String,
}

/// Where the per-session `--settings` files live: under the app data dir,
/// NEVER in the worktree. A `.claude/settings.json` inside the tree would be
/// committable by the agent AND would land in claude's project-approval scan
/// (the EXP-98 trap that made `.exp-mcp.json` non-discoverable). Files sit
/// one level down in a per-process dir (`claude-hooks/<pid>/`) so the path
/// in the agent's argv encodes WHICH exponential process (desktop or CLI
/// daemon — they share the data dir) spawned the session; the reaper's quit
/// sweep skips sessions a live sibling still owns (REV-20).
const HOOK_SETTINGS_DIR: &str = "claude-hooks";

/// One settings file per session accumulates; drop the ancient ones.
const HOOK_SETTINGS_TTL: std::time::Duration = std::time::Duration::from_secs(7 * 24 * 60 * 60);

/// The instance's `/api/mcp` endpoint from the tRPC base URL.
fn mcp_url(base_url: &str) -> String {
    format!("{}/api/mcp", base_url.trim_end_matches('/'))
}

/// A single filesystem path segment from an untrusted server id — a crafted
/// id can never escape the directory it is joined onto.
fn path_segment(id: &str) -> Option<String> {
    let segment: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    (!segment.is_empty()).then_some(segment)
}

/// The two per-session hook files [`write_hook_settings`] lands next to each
/// other: the `--settings` content and the 0600 curl config carrying the
/// bearer token (REV-51 — the hook command references it via
/// `$EXP_HOOK_CONFIG` so the token never rides curl's world-readable argv).
struct HookFiles {
    settings: PathBuf,
    curl_config: PathBuf,
}

/// The curl config's content — mirrors `steer::hooks::hook_curl_config` (the
/// two crates cannot depend on each other, §3.1).
fn hook_curl_config(token: &str) -> String {
    format!("header = \"Authorization: Bearer {token}\"\n")
}

/// Write the curl config owner-only: it holds the session credential.
fn write_hook_curl_config(path: &Path, token: &str) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write as _;
        use std::os::unix::fs::OpenOptionsExt as _;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(hook_curl_config(token).as_bytes())
    }
    #[cfg(not(unix))]
    std::fs::write(path, hook_curl_config(token))
}

/// Write the session's `--settings` file and its curl config, or `None` when
/// there is no sidecar to wire (no [`HookSetup`], a non-claude agent, or an
/// unwritable data dir — all of which just mean grid-only detection, never a
/// failed launch). Both files share the settings TTL prune.
fn write_hook_settings(
    data_dir: &Path,
    session_id: &str,
    agent: CodingAgent,
    hooks: Option<&HookSetup>,
) -> Option<HookFiles> {
    let hooks = hooks.filter(|_| agent == CodingAgent::Claude)?;
    let root = data_dir.join(HOOK_SETTINGS_DIR);
    prune_hook_settings(&root);
    let dir = root.join(std::process::id().to_string());
    std::fs::create_dir_all(&dir).ok()?;
    let segment = path_segment(session_id)?;
    let settings = dir.join(format!("{segment}.settings.json"));
    std::fs::write(&settings, &hooks.settings_json).ok()?;
    let curl_config = dir.join(format!("{segment}.curl.cfg"));
    write_hook_curl_config(&curl_config, &hooks.token).ok()?;
    Some(HookFiles { settings, curl_config })
}

/// Drop settings files past the TTL wherever they sit in the tree — per-pid
/// dirs and pre-REV-20 flat files alike — and clear pid dirs that end up
/// empty (`remove_dir` refuses non-empty ones, so a live dir is never lost).
fn prune_hook_settings(root: &Path) {
    let stale = |entry: &std::fs::DirEntry| {
        entry
            .metadata()
            .and_then(|meta| meta.modified())
            .map(|modified| modified.elapsed().is_ok_and(|age| age > HOOK_SETTINGS_TTL))
            .unwrap_or(false)
    };
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Ok(files) = std::fs::read_dir(&path) {
                for file in files.flatten() {
                    if stale(&file) {
                        let _ = std::fs::remove_file(file.path());
                    }
                }
            }
            let _ = std::fs::remove_dir(&path);
        } else if stale(&entry) {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// The spawn-env half of the sidecar wiring — applied only when the settings
/// file actually landed, so claude never advertises a port its hooks can't
/// reach.
fn apply_hook_env(
    spawn: SpawnSpec,
    hooks: Option<&HookSetup>,
    files: Option<&HookFiles>,
) -> SpawnSpec {
    match (hooks, files) {
        (Some(hooks), Some(files)) => spawn
            .env(HOOK_PORT_ENV, hooks.port.to_string())
            .env(
                HOOK_CONFIG_ENV,
                files.curl_config.to_string_lossy().into_owned(),
            ),
        _ => spawn,
    }
}

/// Step 4's per-agent MCP wiring, shared by the issue/batch skeleton and the
/// action sequence (EXP-257 — action runs stopped being Claude-only). It
/// authenticates the spawned agent as the real user against `/api/mcp`:
///
/// - claude: the cwd `.exp-mcp.json` (any subagents it spawns inherit the
///   session's MCP servers; NOT named .mcp.json — EXP-98, see
///   [`crate::mcp_json`]) — the ONLY on-disk consumer of the raw key.
///   EXP-474 invariant: the launch HARD-FAILS unless that file is verifiably
///   git-ignored at the cwd ([`crate::git_worktree::ensure_ignored`], run
///   BEFORE the write so an unguardable repo never gets the key on disk).
/// - codex: `-c mcp_servers.*` argv overrides; the raw key rides ONLY the
///   spawn env (EXP_MCP_TOKEN) — never disk, never argv.
/// - pi: the launcher-written `.exp-pi-mcp.ts` bridge extension (pi has no
///   native MCP); url + key ride the spawn env like codex.
/// EXP-637: the GUARD half of [`wire_agent_mcp`], split out because it must
/// still run BEFORE the session row exists — a repo whose ignore rules can't
/// be verified must fail the launch before anything server-side is created,
/// while the WRITES now have to wait for the row id (the
/// `X-Exp-Session-Id` header is part of the config).
fn guard_agent_mcp(agent: CodingAgent, cwd: &Path) -> Result<(), CodingError> {
    if agent == CodingAgent::Claude {
        // EXP-474: the key never lands in a repo we cannot prove ignores it.
        crate::git_worktree::ensure_ignored(cwd, &[crate::mcp_json::MCP_JSON_FILE])?;
    }
    Ok(())
}

/// EXP-637: `session_id` is the `coding_sessions` row this launch just
/// created — it rides every MCP call as `X-Exp-Session-Id` so the server can
/// resolve `exponential_sessions_end` (and spare a self-merged session).
/// `None` = no session (agent shells), which keeps the pre-EXP-637 wiring
/// byte-identical.
fn wire_agent_mcp(
    agent: CodingAgent,
    cwd: &Path,
    base_url: &str,
    personal_key: &str,
    session_id: Option<&str>,
) -> Result<AgentMcp, CodingError> {
    match agent {
        CodingAgent::Claude => {
            // Re-run the guard: cheap, and it keeps this function safe to
            // call on its own.
            crate::git_worktree::ensure_ignored(cwd, &[crate::mcp_json::MCP_JSON_FILE])?;
            write_mcp_json(cwd, base_url, personal_key, session_id)
                .map_err(|e| CodingError::Io(format!("write .exp-mcp.json: {e}")))?;
            Ok(AgentMcp::ClaudeFile)
        }
        CodingAgent::Codex => Ok(AgentMcp::CodexOverrides {
            url: mcp_url(base_url),
            session_id: session_id.map(str::to_string),
        }),
        CodingAgent::Pi => {
            write_pi_bridge(cwd)
                .map_err(|e| CodingError::Io(format!("write .exp-pi-mcp.ts: {e}")))?;
            // The observer extension rides along unconditionally (EXP-383):
            // static file, inert without the EXP_OBSERVER_* env — but its
            // absence with the env set would fail the `-e` load, so a write
            // failure is only logged when no observer is wired anyway.
            write_pi_observer(cwd)
                .map_err(|e| CodingError::Io(format!("write .exp-pi-observer.ts: {e}")))?;
            // Same posture for the plan-mode extension (EXP-441): always on
            // the argv, inert without EXP_PI_PLAN_MODE.
            write_pi_plan(cwd)
                .map_err(|e| CodingError::Io(format!("write .exp-pi-plan.ts: {e}")))?;
            Ok(AgentMcp::PiExtension)
        }
    }
}

/// The spawn-env gate of the pi plan-mode extension (EXP-441): a pi launch
/// with plan mode on sets [`crate::argv::PI_PLAN_MODE_ENV`]; without it the
/// always-written `.exp-pi-plan.ts` returns immediately.
fn apply_pi_plan_env(spawn: SpawnSpec, agent: CodingAgent, plan_mode: bool) -> SpawnSpec {
    if agent == CodingAgent::Pi && plan_mode {
        spawn.env(crate::argv::PI_PLAN_MODE_ENV, "1")
    } else {
        spawn
    }
}

/// The spawn-env half of the pi observer wiring (EXP-383): url + token for
/// the `.exp-pi-observer.ts` extension. Only a pi spawn with a live sidecar
/// gets them — without the env the extension returns immediately.
fn apply_observer_env(
    spawn: SpawnSpec,
    agent: CodingAgent,
    observer: Option<&ObserverSetup>,
) -> SpawnSpec {
    match (agent, observer) {
        (CodingAgent::Pi, Some(observer)) => spawn
            .env(
                OBSERVER_URL_ENV,
                format!("http://127.0.0.1:{}", observer.port),
            )
            .env(OBSERVER_TOKEN_ENV, &observer.token),
        _ => spawn,
    }
}

/// The spawn-env half of [`wire_agent_mcp`]: the MCP credential for codex/pi
/// rides the ENV (claude's rides `.exp-mcp.json`) — codex reads it through
/// `bearer_token_env_var`, the pi bridge reads url + token directly.
fn apply_mcp_env(
    spawn: SpawnSpec,
    agent: CodingAgent,
    base_url: &str,
    personal_key: &str,
    session_id: Option<&str>,
) -> SpawnSpec {
    let spawn = match agent {
        CodingAgent::Claude => spawn,
        CodingAgent::Codex => spawn.env(MCP_TOKEN_ENV, personal_key),
        CodingAgent::Pi => spawn
            .env(MCP_URL_ENV, mcp_url(base_url))
            .env(MCP_TOKEN_ENV, personal_key)
            // Embedded sessions must not block on pi's startup
            // update/network checks.
            .env("PI_SKIP_VERSION_CHECK", "1"),
    };
    // EXP-637: the pi bridge has no native MCP headers — it reads the run's
    // session id from the env and sets `x-exp-session-id` itself. Harmless
    // (and unread) on claude/codex, which carry it in their own config.
    match session_id {
        Some(id) => spawn.env(MCP_SESSION_ID_ENV, id),
        None => spawn,
    }
}

/// EXP-637: everything between `codingSessions.start` and the spawn can
/// still fail (an unwritable MCP config, a bad argv). Ending the row keeps a
/// failed launch from leaving a "coding now" badge nobody can clear.
fn end_on_error<T>(
    trpc: &TrpcClient,
    session_id: &str,
    result: Result<T, CodingError>,
) -> Result<T, CodingError> {
    if result.is_err() {
        let _ = coding_sessions::end(trpc, session_id);
    }
    result
}

/// EXP-637: where pi records a run's transcript
/// (`<data_dir>/pi-sessions/<row id>.jsonl`). pi opens a FRESH session when
/// the file does not exist and RESUMES it when it does, so the same path
/// serves both. `None` when the id has no usable path segment.
fn pi_session_file(data_dir: &Path, session_id: &str) -> Option<PathBuf> {
    let segment = path_segment(session_id)?;
    let dir = data_dir.join("pi-sessions");
    let _ = std::fs::create_dir_all(&dir);
    Some(dir.join(format!("{segment}.jsonl")))
}

/// The shared 412/401/403 mapping for `repositories.installationToken`.
fn token_error_reason(err: ApiError, full_name: &str) -> Result<DisabledReason, CodingError> {
    match err {
        ApiError::Http { status: 412, message } => Ok(DisabledReason::GithubAppMissing {
            full_name: full_name.to_string(),
            message,
        }),
        ApiError::Http { status: status @ (401 | 403), message } => {
            Ok(DisabledReason::TokenDenied {
                message: format!("{message} (HTTP {status})"),
            })
        }
        err => Err(err.into()),
    }
}

fn map_token_error(err: ApiError, full_name: &str) -> Result<Prepared, CodingError> {
    token_error_reason(err, full_name).map(Prepared::Disabled)
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
    prepare_with_hooks(req, deps, None, None)
}

/// [`prepare`] with the EXP-249 claude hooks sidecar wired in. The caller
/// (app/ui, which owns both crates) starts a `steer::hooks::HookServer`,
/// passes its port/token/settings JSON as a [`HookSetup`], holds the server
/// for the session's lifetime, and hands its event receiver to the activity
/// emitter. `coding` never sees the server itself (§3.1: no `steer`
/// dependency), only the three values the spawn needs.
pub fn prepare_with_hooks(
    req: &PrepareRequest,
    deps: &CodingDeps,
    hooks: Option<&HookSetup>,
    observer: Option<&ObserverSetup>,
) -> Result<Prepared, CodingError> {
    // Action runs share none of the worktree/branch/PR skeleton below —
    // they get their own sequence (EXP-253).
    if let PrepareRequest::Action(action_req) = req {
        return prepare_action(action_req, deps, hooks, observer);
    }
    if let PrepareRequest::ResumeRun(resume_req) = req {
        return prepare_resume_run(resume_req, deps, hooks, observer);
    }
    let resume_prompt =
        matches!(req, PrepareRequest::Issue(issue_req) if issue_req.resume_prompt);
    let mut options = match req {
        PrepareRequest::Issue(issue_req) => issue_req.options.clone(),
        PrepareRequest::Batch(batch_req) => batch_req.options.clone(),
        PrepareRequest::Action(_) | PrepareRequest::ResumeRun(_) => {
            unreachable!("dispatched above")
        }
    };
    // A resume NEVER re-enters plan mode (EXP-202): the plan already
    // happened in the work being picked back up. The dialog clamps this too,
    // but the invariant belongs here so every caller (remote resume included)
    // inherits it.
    options.plan_mode &= !resume_prompt;
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
        PrepareRequest::Action(_) | PrepareRequest::ResumeRun(_) => {
            unreachable!("dispatched above")
        }
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
        PrepareRequest::Action(_) | PrepareRequest::ResumeRun(_) => {
            unreachable!("dispatched above")
        }
    };
    let url = minted.url.clone();
    let repos_root = deps.settings.repos_root_path();
    // The clone path the worktree hangs off — the P9 token refresher's target.
    let clone = clone_path(&repos_root, url.full_name());
    // EXP-478: gate BEFORE the worktree exists — from here until the UI
    // registers the session, the auto-prune must not run on this clone.
    let launch_hold = crate::launch_gate::hold(&clone);
    let worktree = deps.worktrees.prepare(
        &repos_root,
        url.full_name(),
        &minted.default_branch,
        &branch,
        &url,
        minted.expires_at.as_deref(),
    )?;

    // Step 4a — the EXP-474 ignore GUARD, still before anything server-side
    // exists. The WRITES moved after step 6 (EXP-637: the config carries the
    // session row's `X-Exp-Session-Id`).
    let personal_key = key_handle
        .join()
        .map_err(|_| CodingError::Io("personal-key thread panicked".to_string()))??;
    guard_agent_mcp(agent, &worktree)?;

    // Step 5 — the seed prompt (both shapes: direct argv delivery when
    // small, PROMPT.md + seed line otherwise). EXP-662: this path ALWAYS
    // seeds a prompt — an exact relaunch of a recorded conversation is
    // [`prepare_resume_run`], and a `resume_prompt` request is precisely the
    // case where no record could be resolved, so it gets a fresh session in
    // the reused worktree told to pick the branch work back up.
    let rendered = match req {
        PrepareRequest::Issue(issue_req) if issue_req.resume_prompt => {
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
        PrepareRequest::Action(_) | PrepareRequest::ResumeRun(_) => {
            unreachable!("dispatched above")
        }
    };
    let delivery = deliver_prompt(&worktree, &clone, &rendered)
        .map_err(|e| CodingError::Io(format!("deliver prompt: {e}")))?;

    // Step 6 — the session row, BEFORE spawn (the id keys everything).
    let session = match req {
        PrepareRequest::Issue(issue_req) => coding_sessions::start(
            &deps.trpc,
            &issue_req.issue_id,
            Some(&issue_req.device_label),
            attribution(&issue_req.origin, deps),
            // A resume is `prepare_resume_run`'s business (EXP-662); this
            // path always starts a run of its own.
            None,
        ),
        PrepareRequest::Batch(batch_req) => coding_sessions::start_batch(
            &deps.trpc,
            &batch_req.team_id,
            Some(&batch_req.device_label),
            attribution(&batch_req.origin, deps),
            None,
        ),
        PrepareRequest::Action(_) | PrepareRequest::ResumeRun(_) => {
            unreachable!("dispatched above")
        }
    };
    let session = match session {
        Ok(session) => session,
        Err(ApiError::Http { status: 412, message }) => {
            return Ok(Prepared::Disabled(DisabledReason::SessionLimit { message }))
        }
        Err(err) => return Err(err.into()),
    };

    // Step 4b (EXP-637) — the per-agent MCP wiring, now that the row id
    // exists to pin into it. Anything that fails from here on ends the row
    // so a failed launch leaves no ghost badge.
    let agent_mcp = end_on_error(
        &deps.trpc,
        &session.id,
        wire_agent_mcp(
            agent,
            &worktree,
            deps.trpc.base_url(),
            &personal_key,
            Some(&session.id),
        ),
    )?;

    // Step 6.5 (EXP-194) — the LAUNCHER parks backlog/todo issues in
    // `in_progress`. Under plan mode the agent's MCP status call would only
    // land after plan approval, so without this the issue lingers in backlog
    // while visibly "coding now". After the session row so a Disabled
    // outcome never flips anything; best-effort — a failed write never
    // blocks the launch. Only backlog/todo flip: never downgrade
    // in_progress/in_review/done/cancelled/duplicate (client-side snapshot,
    // same guard the dialog's state hints use).
    //
    // EXP-314: this stays an ENUM-only write (an anchor). The gate is
    // therefore correct for custom statuses too — an issue in a custom
    // `started` status carries the `in_progress` anchor and is not flipped.
    // An issue in a custom BACKLOG/UNSTARTED status IS flipped, and the
    // server's trigger derives `status_id` from the enum, so it lands in the
    // team's BUILTIN In Progress row (leaving its custom status). Accepted
    // for v1: parking is a coding-flow convenience, not a status editor.
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
        PrepareRequest::Action(_) | PrepareRequest::ResumeRun(_) => {
            unreachable!("dispatched above")
        }
    };
    for issue_id in flip_ids {
        let mut input = issues::IssuesUpdateInput::new(issue_id);
        input.status = Some(IssueStatus::InProgress);
        let _ = issues::issues_update(&deps.trpc, &input);
    }

    // EXP-210: stamp THIS agent into the worktree's recorded-agent marker
    // (after every can-still-fail step, so a Disabled outcome records
    // nothing) — a later resume reads it to decide whether the agent's
    // native reopen can work here at all. Best-effort: a failed write only
    // costs a future resume offer.
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
        PrepareRequest::Action(_) | PrepareRequest::ResumeRun(_) => {
            unreachable!("dispatched above")
        }
    };
    let tail = SessionTail::Prompt(delivery.positional());
    // EXP-389: pre-trust the clone in codex's own config — a remotely
    // started session would otherwise park forever on the TUI's
    // directory-trust screen (codex resolves a linked worktree's trust
    // subject to the main clone, so the clone is the entry that matters).
    if agent == CodingAgent::Codex {
        crate::codex_trust::ensure_trusted(&clone);
    }
    // EXP-414: same pre-accept for claude, keyed by the spawn CWD — claude's
    // trust dialog is per-directory and every session gets a fresh worktree.
    if agent == CodingAgent::Claude {
        crate::claude_trust::ensure_onboarded(
            &worktree,
            options.skip_permissions && !options.plan_mode,
        );
    }
    let hook_settings = write_hook_settings(&deps.data_dir, &session.id, agent, hooks);
    // EXP-443: identity minted BEFORE spawn, so the transcript pin and the
    // hook router's bound set exist from tick zero — a foreign agent sharing
    // the cwd can never be tailed into this session's feed. Always fresh
    // here (EXP-662: this path never reopens a conversation); codex gets a
    // per-session originator stamped into its rollout metas.
    let claude_session_id =
        (agent == CodingAgent::Claude).then(|| uuid::Uuid::new_v4().to_string());
    let codex_originator = (agent == CodingAgent::Codex)
        .then(|| crate::argv::codex_session_originator(&session.id));
    let pi_session = (agent == CodingAgent::Pi)
        .then(|| pi_session_file(&deps.data_dir, &session.id))
        .flatten();

    // EXP-662: record the SESSION exactly like `prepare_action` records a
    // run — same file, same fields, kind Issue/Batch — so a later Resume
    // (dialog checkbox, remote frame, ended strip) relaunches THIS
    // conversation instead of guessing at the worktree's latest one.
    // Best-effort: a failed write only costs the Resume offer.
    let (kind, record_issue_id, record_identifier, batch_id, issues, team_id) = match req {
        PrepareRequest::Issue(issue_req) => (
            RunKind::Issue,
            Some(issue_req.issue_id.clone()),
            Some(issue_req.issue_identifier.clone()),
            None,
            Vec::new(),
            // The row's team, as the server resolved it from the issue —
            // the request carries none.
            session.team_id.clone().unwrap_or_default(),
        ),
        PrepareRequest::Batch(batch_req) => (
            RunKind::Batch,
            None,
            None,
            Some(batch_req.batch_id.clone()),
            batch_req
                .issues
                .iter()
                .map(|issue| RunIssue {
                    issue_id: issue.issue_id.clone(),
                    identifier: issue.issue_identifier.clone(),
                })
                .collect(),
            batch_req.team_id.clone(),
        ),
        PrepareRequest::Action(_) | PrepareRequest::ResumeRun(_) => {
            unreachable!("dispatched above")
        }
    };
    crate::run_registry::record(
        &deps.data_dir,
        RunRecord {
            session_id: session.id.clone(),
            account_id: deps.account_id.clone(),
            agent,
            kind,
            // Sessions have no action.
            action_id: String::new(),
            action_name: String::new(),
            team_id,
            issue_id: record_issue_id,
            issue_identifier: record_identifier,
            batch_id,
            issues,
            cwd: worktree.clone(),
            clone: Some(clone.clone()),
            repo: Some(full_name.clone()),
            repository_id: Some(repository_id.clone()),
            branch: Some(branch.clone()),
            base_branch: Some(minted.default_branch.clone()),
            claude_session_id: claude_session_id.clone(),
            pi_session_file: pi_session.clone(),
            codex_originator: codex_originator.clone(),
            inputs: Vec::new(),
            model: options.model.clone(),
            effort: options.effort.clone(),
            ultracode: options.ultracode,
            skip_permissions: options.skip_permissions,
            fix: None,
            // Only action runs automate (EXP-530).
            started_reason: None,
            resumed_from_id: None,
            recorded_at: crate::run_registry::now_secs(),
            extra: BTreeMap::new(),
        },
    );

    let args = session_args(
        options,
        &agent_mcp,
        hook_settings.as_ref().map(|files| files.settings.as_path()),
        SessionIdentity {
            claude_session_id: claude_session_id.as_deref(),
            pi_session_file: pi_session.as_deref(),
        },
        tail,
    );
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
    // The MCP credential env half of the wiring ([`apply_mcp_env`]).
    spawn = apply_mcp_env(
        spawn,
        agent,
        deps.trpc.base_url(),
        &personal_key,
        Some(&session.id),
    );
    spawn = apply_hook_env(spawn, hooks, hook_settings.as_ref());
    spawn = apply_observer_env(spawn, agent, observer);
    spawn = apply_pi_plan_env(spawn, agent, options.plan_mode);
    if let Some(originator) = &codex_originator {
        spawn = spawn.env(crate::argv::CODEX_ORIGINATOR_ENV, originator);
    }

    let heartbeat_scope = match req {
        PrepareRequest::Issue(issue_req) => {
            let attribution = attribution(&issue_req.origin, deps);
            coding_sessions::HeartbeatScope {
                issue_id: Some(issue_req.issue_id.clone()),
                team_id: None,
                action_id: None,
                action_name: None,
                device_label: Some(issue_req.device_label.clone()),
                started_by_id: attribution.started_by_id.map(str::to_string),
                device_id: attribution.device_id.map(str::to_string),
                // Only action runs automate (EXP-530).
                started_reason: None,
                automation_id: None,
                // The server refuses a branch beside an issueId.
                branch: None,
            }
        }
        PrepareRequest::Batch(batch_req) => {
            let attribution = attribution(&batch_req.origin, deps);
            coding_sessions::HeartbeatScope {
                issue_id: None,
                team_id: Some(batch_req.team_id.clone()),
                action_id: None,
                action_name: None,
                device_label: Some(batch_req.device_label.clone()),
                started_by_id: attribution.started_by_id.map(str::to_string),
                device_id: attribution.device_id.map(str::to_string),
                // Only action runs automate (EXP-530).
                started_reason: None,
                automation_id: None,
                // The batch branch is minted client-side and already
                // recorded by `start_batch`; nothing to re-assert.
                branch: None,
            }
        }
        PrepareRequest::Action(_) | PrepareRequest::ResumeRun(_) => {
            unreachable!("dispatched above")
        }
    };

    Ok(Prepared::Ready(PreparedLaunch {
        session_id: session.id,
        issue_identifier,
        worktree,
        clone,
        repository_id: Some(repository_id),
        branch,
        // Issue/batch worktrees are the prune's business, not the run
        // cleanup's — they survive their session by design.
        base_branch: None,
        run_cleanup: None,
        spawn,
        tab_title,
        tab_title_prefix,
        heartbeat_scope,
        tab_kind: TabKind::Claude,
        bypass_permissions: options.skip_permissions && !options.plan_mode,
        plan_mode: options.plan_mode,
        agent,
        claude_session_id,
        codex_originator,
        // EXP-662: this path never reopens a rollout — a codex resume is
        // `prepare_resume_run`'s.
        codex_resume_id: None,
        launch_hold: Some(launch_hold),
    }))
}

/// The action-run sequence (EXP-253; blocking, background executor) — the
/// deliberately SHORT sibling of the issue/batch skeleton above: no PR
/// contract, no status flips.
///
/// 0. doctor — the SELECTED agent always (EXP-257: action runs take the
///    full option set, no Claude clamp); `git` only when repo-backed (a
///    repo-less action needs no git at all);
/// 1. cwd — repo-backed team action: mint the JIT token (cache-seeded like a
///    session), ensure the trunk clone + ambient auth, then a BEST-EFFORT
///    autopull (`clone_manager::auto_sync` — a dirty/diverged trunk still
///    launches; the trunk-sync engine surfaces that state); the
///    fix-conflicts builtin (EXP-259) instead fetches the PR branch and
///    creates/reuses ITS worktree; repo-less:
///    `<data_dir>/actions/<action id>/`, created on demand;
/// 2. per-agent MCP wiring in the cwd ([`wire_agent_mcp`] — shared with the
///    issue path; repo-backed also git-excludes the seed files);
/// 3. prompt — [`render_action_prompt`] preamble [+ `## Inputs`] + the fresh
///    body; the BUILTINS instead render [`create_action_prompt`] from the
///    `description`/`repo` input values (EXP-257) or
///    [`fix_pr_conflicts_prompt`] from the resolved PR target (EXP-259);
/// 4. `codingSessions.start({actionId[, teamId]})` — BEFORE spawn; its id
///    keys the tab + steer room like any session (teamId rides only for the
///    builtin literals);
/// 5. spawn spec — the selected agent's interactive session argv
///    (model/effort/toggles + its MCP posture).
fn prepare_action(
    req: &ActionLaunchRequest,
    deps: &CodingDeps,
    hooks: Option<&HookSetup>,
    observer: Option<&ObserverSetup>,
) -> Result<Prepared, CodingError> {
    // EXP-257: options apply AS-IS — same per-agent vocabulary as an issue
    // run (the server validates remote starts identically).
    let options = req.options.clone();
    let agent = options.agent;
    // The creator builtin always runs in its scratch dir — a repo INPUT only
    // pins the authored action's repositoryId, never this run's cwd. The
    // fix-conflicts builtin REQUIRES its repo (checked below).
    let repo = if matches!(req.kind, ActionRunKind::CreateAction) {
        &None
    } else {
        &req.repo
    };
    if matches!(req.kind, ActionRunKind::FixConflicts { .. }) && repo.is_none() {
        return Err(CodingError::Io(
            "the fix-conflicts run needs the pull request's repository".to_string(),
        ));
    }
    // EXP-615: a chat run is repo-BOUND (its `repo` input is required — a
    // scratch-dir chat would be a shell with no code in it) and prompt-BOUND
    // (its `prompt` input IS the program). Both are validated here, before
    // any doctor/git/network work, so a malformed start costs nothing.
    let chat_user_prompt = match &req.kind {
        ActionRunKind::Chat => {
            if repo.is_none() {
                return Err(CodingError::Io(
                    "the chat run needs a repository".to_string(),
                ));
            }
            let Some(prompt) = req
                .inputs
                .iter()
                .find(|input| input.key == "prompt")
                .map(|input| input.value.trim())
                .filter(|value| !value.is_empty())
            else {
                return Err(CodingError::Io(
                    "the chat run is missing its prompt".to_string(),
                ));
            };
            Some(prompt.to_string())
        }
        _ => None,
    };

    // Step 0 — doctor: the selected agent always; git only when a clone is
    // involved.
    let report = run_doctor(&deps.settings);
    let agent_check = report.check_for(agent);
    if !agent_check.ok {
        return Ok(Prepared::Disabled(DisabledReason::DoctorFailed(
            agent_check.clone(),
        )));
    }
    if repo.is_some() {
        if let Some(failed) = report.first_failure_for(agent) {
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

    // Step 1 — resolve the cwd. `trunk_clone` stays the CLONE root for
    // repo-backed runs even when the cwd is a worktree (fix-conflicts) —
    // the shared cargo cache and the session registry key off it.
    let mut trunk_clone: Option<PathBuf> = None;
    // EXP-324: the fix-conflicts rebase target, resolved live inside the
    // repo-backed arm below (None for every other kind); consumed by the
    // prompt render in step 3.
    let mut fix_rebase_onto: Option<String> = None;
    // EXP-478/EXP-637: every repo-backed run now works in its OWN worktree
    // (fix-conflicts on the PR branch, Team/Chat on a fresh run branch), so
    // it gates the clone for the launch's whole flight like an issue/batch
    // launch. Only repo-less scratch runs skip it.
    let mut launch_hold: Option<crate::launch_gate::LaunchHold> = None;
    // EXP-637: the run branch + what it was cut from, recorded for the
    // cleanup and the resume path. `None` for repo-less runs.
    let mut run_branch: Option<String> = None;
    let mut base_branch: Option<String> = None;
    let (cwd, repository_id) = match repo {
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
            // EXP-324: resolve the PR's LIVE rebase target before any git
            // work. A stacked PR's base is its parent's branch, not the repo
            // default, and a stale base (parent squash-merged, branch left
            // behind) is healed — retargeted onto the default — by this call
            // server-side. Guessing the base instead is exactly the EXP-320
            // bug, and this run ends in a force-push + auto-merge, so any
            // failure other than "old server without the procedure" (404 →
            // legacy default-branch behavior) is a hard, retryable error.
            fix_rebase_onto = match &req.kind {
                ActionRunKind::FixConflicts {
                    issue_id,
                    default_branch,
                    ..
                } => match issues::prepare_conflict_fix(&deps.trpc, issue_id) {
                    Ok(resolved) => Some(resolved.rebase_onto),
                    Err(ApiError::Http { status: 404, .. }) => Some(default_branch.clone()),
                    Err(err) => {
                        return Err(CodingError::Io(format!(
                            "could not resolve the pull request's base branch: {err}"
                        )))
                    }
                },
                _ => None,
            };
            let repos_root = deps.settings.repos_root_path();
            let clone = crate::git_worktree::ensure_clone(&repos_root, url.full_name(), &url)?;
            git_credentials::ensure(&clone, &url, minted.expires_at.as_deref())?;
            // EXP-637: no `clone_manager::auto_sync` here any more — the
            // trunk clone stopped being a run cwd, so a run neither needs it
            // pulled nor may park the trunk-sync engine on its own dirt.
            // Same best-effort [`LOCAL_EXCLUDES`] coverage as a session
            // worktree — the action's agent may be codex/pi since EXP-257.
            let _ = crate::git_worktree::ensure_local_excludes(&clone, LOCAL_EXCLUDES);
            let cwd = match &req.kind {
                // EXP-259: the fix-conflicts run works on the PR branch, not
                // the trunk — fetch the branch (it may only exist on origin;
                // the PR may have been coded on another device) and
                // create/reuse its worktree, cutting a missing local branch
                // from origin/<branch>.
                ActionRunKind::FixConflicts { branch, .. } => {
                    crate::git_worktree::validate_branch_arg(branch, "fix conflicts")?;
                    launch_hold = Some(crate::launch_gate::hold(&clone));
                    crate::git_worktree::fetch_base(&clone, branch, &url)?;
                    // EXP-324: the rebase target must exist as a local
                    // remote-tracking ref too — auto_sync above is
                    // best-effort. Server data is untrusted (same stance as
                    // the branch arg), so the ref is validated before it can
                    // reach git argv.
                    if let Some(rebase_onto) = fix_rebase_onto.as_deref() {
                        crate::git_worktree::validate_branch_arg(
                            rebase_onto,
                            "fix conflicts base",
                        )?;
                        crate::git_worktree::fetch_base(&clone, rebase_onto, &url)?;
                    }
                    let worktree = crate::git_worktree::create_worktree(
                        &clone,
                        branch,
                        &format!("origin/{branch}"),
                        &url,
                    )?;
                    // `create_worktree` reuses a stale local branch as-is
                    // (right for ordinary sessions — their unpushed work must
                    // survive a relaunch), but THIS run ends in a force-push
                    // whose lease is the remote ref fetched just above: from
                    // a stale branch it would push away remote-only commits.
                    // Fast-forward to origin (or refuse on local-only
                    // commits) before spawning.
                    crate::git_worktree::ensure_branch_at_origin(
                        &clone, &worktree, branch, &url,
                    )?;
                    run_branch = Some(branch.clone());
                    base_branch = fix_rebase_onto.clone();
                    worktree
                }
                // EXP-637 (decision 1): a Team action or a Chat run gets its
                // OWN worktree + branch cut from the repo's default, instead
                // of writing into the trunk clone. Whatever the agent
                // changes is then either committed onto a branch that can
                // become a PR, or discarded with the worktree — the trunk
                // stays clean and autopull keeps running.
                ActionRunKind::Team | ActionRunKind::Chat => {
                    let branch = match &req.kind {
                        ActionRunKind::Chat => chat_run_branch(&req.run_id),
                        _ => action_run_branch(&req.action_name, &req.run_id),
                    };
                    crate::git_worktree::validate_branch_arg(&branch, "action run")?;
                    // The gate must be held BEFORE the worktree exists: the
                    // auto-prune's policy is a pre-fetch snapshot that cannot
                    // protect a worktree born after it (EXP-478).
                    launch_hold = Some(crate::launch_gate::hold(&clone));
                    let worktree = deps.worktrees.prepare(
                        &repos_root,
                        url.full_name(),
                        &minted.default_branch,
                        &branch,
                        &url,
                        minted.expires_at.as_deref(),
                    )?;
                    run_branch = Some(branch);
                    base_branch = Some(minted.default_branch.clone());
                    worktree
                }
                // The creator builtin never reaches this arm (its repo input
                // only pins the authored action's repositoryId).
                _ => clone.clone(),
            };
            trunk_clone = Some(clone);
            (cwd, Some(repo.repository_id.clone()))
        }
        // Repo-less: a scratch dir holding only the MCP config (+ PROMPT.md
        // when the body is large). No git, no token. The id is a server
        // UUID, but server data is untrusted here by design — sanitize the
        // path segment so a crafted id can never escape `<data_dir>/actions/`.
        None => {
            let Some(segment) = path_segment(&req.action_id) else {
                return Err(CodingError::Io("empty action id".to_string()));
            };
            // EXP-637: PER RUN, not per action — two concurrent runs of the
            // same repo-less action would otherwise share one `.exp-mcp.json`
            // and overwrite each other's session header. codex's trust entry
            // still keys on the action dir (its parent), so the pre-trust
            // below keeps working.
            let Some(run_segment) = path_segment(&req.run_id) else {
                return Err(CodingError::Io("empty run id".to_string()));
            };
            let scratch = deps
                .data_dir
                .join("actions")
                .join(segment)
                .join(run_segment);
            std::fs::create_dir_all(&scratch)
                .map_err(|e| CodingError::Io(format!("create action scratch dir: {e}")))?;
            (scratch, None)
        }
    };

    // Step 2 — the EXP-474 ignore GUARD (the WRITES moved after step 4:
    // EXP-637 pins the session row's `X-Exp-Session-Id` into the config).
    let personal_key = key_handle
        .join()
        .map_err(|_| CodingError::Io("personal-key thread panicked".to_string()))??;
    guard_agent_mcp(agent, &cwd)?;

    // Step 3 — the prompt (size-gated like a session's; the PROMPT.md
    // exclude write no-ops without a `.git`). The builtins render their
    // generated prompts; real actions get the preamble [+ inputs section] +
    // the fresh body.
    // EXP-637: the run's own workspace, rendered into the prompt so the
    // agent knows where it is and how work leaves the worktree (an
    // issue-LESS PR via `repositoryId` + `head` — EXP-626).
    let workspace = match (&run_branch, &base_branch, &repository_id) {
        (Some(branch), Some(default_branch), Some(repository_id)) => Some(WorkspaceNote {
            branch: branch.clone(),
            default_branch: default_branch.clone(),
            repository_id: repository_id.clone(),
        }),
        _ => None,
    };
    let rendered = match &req.kind {
        ActionRunKind::CreateAction => {
            let Some(description) = req
                .inputs
                .iter()
                .find(|input| input.key == "description")
                .map(|input| input.value.trim())
                .filter(|value| !value.is_empty())
            else {
                return Err(CodingError::Io(
                    "the builtin Create-action run is missing its description input".to_string(),
                ));
            };
            let repo_input = req
                .inputs
                .iter()
                .find(|input| input.key == "repo" && !input.value.trim().is_empty())
                .map(|input| {
                    (
                        input.value.as_str(),
                        input.display.as_deref().unwrap_or(input.value.as_str()),
                    )
                });
            // EXP-273: the icon the author picked in the run form, passed
            // through so the created action lands with its glyph already set.
            let icon_input = req
                .inputs
                .iter()
                .find(|input| input.key == "icon" && !input.value.trim().is_empty())
                .map(|input| input.value.trim());
            // EXP-615: the optional name the author typed — blank leaves the
            // naming to the agent (the pre-EXP-615 behavior).
            let name_input = req
                .inputs
                .iter()
                .find(|input| input.key == "name" && !input.value.trim().is_empty())
                .map(|input| input.value.trim());
            create_action_prompt(&req.team_id, description, repo_input, icon_input, name_input)
        }
        // EXP-615: the user's own words, verbatim — no preamble, no inputs
        // section (validated non-empty at the top of this function).
        // Everything else (trunk clone cwd, MCP wiring, session row,
        // steering) is the ordinary action-run path.
        ActionRunKind::Chat => {
            chat_prompt(&chat_user_prompt.clone().unwrap_or_default(), workspace.as_ref())
        }
        ActionRunKind::FixConflicts {
            branch,
            default_branch,
            identifier,
            ..
        } => fix_pr_conflicts_prompt(
            identifier,
            branch,
            // The live base resolved above; the repo default only when the
            // server predates issues.prepareConflictFix (EXP-324).
            fix_rebase_onto.as_deref().unwrap_or(default_branch),
        ),
        ActionRunKind::Team => render_action_prompt_full(
            &req.action_name,
            &req.body,
            &req.inputs,
            req.trigger.as_ref(),
            workspace.as_ref(),
        ),
    };
    // EXP-637: the PROMPT.md exclude belongs in the CLONE's shared
    // `.git/info/exclude` — a run worktree has no `.git` dir of its own.
    let delivery = deliver_prompt(&cwd, trunk_clone.as_deref().unwrap_or(&cwd), &rendered)
        .map_err(|e| CodingError::Io(format!("deliver prompt: {e}")))?;

    // Step 4 — the session row, BEFORE spawn. Only the builtin literals
    // carry teamId (the server forbids it on real action ids).
    let session = match coding_sessions::start_action(
        &deps.trpc,
        coding_sessions::ActionStart {
            action_id: &req.action_id,
            team_id: req.kind.is_builtin().then_some(req.team_id.as_str()),
            started_reason: req.trigger.as_ref().map(|note| note.started_reason()),
            automation_id: req.automation_id.as_deref(),
            device_label: Some(&req.device_label),
            branch: run_branch.as_deref(),
            resumed_from_id: None,
            attribution: attribution(&req.origin, deps),
        },
    ) {
        Ok(session) => session,
        Err(ApiError::Http { status: 412, message }) => {
            return Ok(Prepared::Disabled(DisabledReason::SessionLimit { message }))
        }
        Err(err) => return Err(err.into()),
    };

    // Step 4b (EXP-637) — the per-agent MCP wiring, now that the row id
    // exists to pin into it. Everything after this ends the row on failure
    // so a half-launched run leaves no ghost badge.
    let agent_mcp = end_on_error(
        &deps.trpc,
        &session.id,
        wire_agent_mcp(
            agent,
            &cwd,
            deps.trpc.base_url(),
            &personal_key,
            Some(&session.id),
        ),
    )?;

    // EXP-210: stamp THIS agent into the run worktree's recorded-agent
    // marker, exactly like the issue path — a later resume reads it to
    // decide whether a native `--resume`/`--continue` can work here.
    let _ = crate::worktree_agents::record_worktree_agent(&cwd, agent);

    // Step 5 — the spawn spec: the selected agent, session argv. EXP-389:
    // pre-trust the run's directory first (the trunk clone for repo-backed
    // runs — a linked worktree resolves to it — or the scratch dir's PARENT,
    // which is stable per action across runs) so codex never parks on its
    // directory-trust screen.
    if agent == CodingAgent::Codex {
        let trust_root = trunk_clone
            .as_deref()
            .or_else(|| cwd.parent())
            .unwrap_or(&cwd);
        crate::codex_trust::ensure_trusted(trust_root);
    }
    // EXP-414: claude keys trust by the spawn cwd itself (worktree/scratch).
    if agent == CodingAgent::Claude {
        crate::claude_trust::ensure_onboarded(
            &cwd,
            options.skip_permissions && !options.plan_mode,
        );
    }
    let hook_settings = write_hook_settings(&deps.data_dir, &session.id, agent, hooks);
    // EXP-443: action runs mint identities like a session — they share the
    // trunk-clone cwd with each other and with agent shells, exactly the
    // collision the pin/originator disambiguate. Always fresh (no resume).
    let claude_session_id =
        (agent == CodingAgent::Claude).then(|| uuid::Uuid::new_v4().to_string());
    let codex_originator = (agent == CodingAgent::Codex)
        .then(|| crate::argv::codex_session_originator(&session.id));
    let pi_session = (agent == CodingAgent::Pi)
        .then(|| pi_session_file(&deps.data_dir, &session.id))
        .flatten();
    let args = session_args(
        &options,
        &agent_mcp,
        hook_settings.as_ref().map(|files| files.settings.as_path()),
        SessionIdentity {
            claude_session_id: claude_session_id.as_deref(),
            pi_session_file: pi_session.as_deref(),
        },
        SessionTail::Prompt(delivery.positional()),
    );
    // EXP-615: a chat tab is named after the REPO it opened on — every chat
    // carries the same action name ("Chat"), so `action · Chat` would make a
    // strip of them unreadable.
    let tab_prefix = match &req.kind {
        ActionRunKind::Chat => repo
            .as_ref()
            .map(|repo| repo_short_name(&repo.full_name).to_string())
            .unwrap_or_else(|| req.action_name.clone()),
        _ => req.action_name.clone(),
    };
    let tab_title = match &req.kind {
        ActionRunKind::Chat => format!("chat · {tab_prefix}"),
        _ => format!("action · {tab_prefix}"),
    };
    let mut spawn = SpawnSpec::new(&deps.settings.resolved_path_for(agent))
        .args(args)
        .cwd(&cwd);
    spawn = apply_mcp_env(
        spawn,
        agent,
        deps.trpc.base_url(),
        &personal_key,
        Some(&session.id),
    );
    spawn = apply_hook_env(spawn, hooks, hook_settings.as_ref());
    spawn = apply_observer_env(spawn, agent, observer);
    spawn = apply_pi_plan_env(spawn, agent, options.plan_mode);
    if let Some(originator) = &codex_originator {
        spawn = spawn.env(crate::argv::CODEX_ORIGINATOR_ENV, originator);
    }
    if let Some(clone) = &trunk_clone {
        // Same EXP-76 shared-cache posture as a session — keyed off the
        // CLONE (the fix-conflicts cwd is a worktree); inert repo-less.
        spawn = spawn
            .env(
                "CARGO_TARGET_DIR",
                shared_cargo_target_dir(clone).to_string_lossy().into_owned(),
            )
            .env("CARGO_INCREMENTAL", "0");
    }

    // EXP-637: every repo-backed run carries its branch now — the EXP-102
    // live-branch guard then blocks the prune on the run worktree exactly as
    // it does on a session worktree. Repo-less runs keep the empty string
    // (branch-keyed registry lookups stay a miss).
    let branch = run_branch.clone().unwrap_or_default();
    // EXP-637: a Team/Chat run owns its worktree, so it also owns cleaning
    // it up — but NEVER the fix-conflicts worktree (that is the PR's branch,
    // shared with the issue session that opened it).
    let run_cleanup = match (&req.kind, &trunk_clone, &run_branch, &base_branch) {
        (
            ActionRunKind::Team | ActionRunKind::Chat,
            Some(clone),
            Some(branch),
            Some(base_branch),
        ) => Some(RunCleanup {
            clone: clone.clone(),
            worktree: cwd.clone(),
            branch: branch.clone(),
            base_branch: base_branch.clone(),
        }),
        _ => None,
    };

    // EXP-637: everything a RESUME of this run needs — recorded once the row
    // and the argv identity are both known. Best-effort: a failed write only
    // costs the Resume offer.
    crate::run_registry::record(
        &deps.data_dir,
        RunRecord {
            session_id: session.id.clone(),
            account_id: deps.account_id.clone(),
            agent,
            kind: match &req.kind {
                ActionRunKind::Team => RunKind::Team,
                ActionRunKind::Chat => RunKind::Chat,
                ActionRunKind::CreateAction => RunKind::CreateAction,
                ActionRunKind::FixConflicts { .. } => RunKind::FixConflicts,
            },
            action_id: req.action_id.clone(),
            action_name: req.action_name.clone(),
            team_id: req.team_id.clone(),
            // EXP-662's session fields: an action run has no issue subject.
            issue_id: None,
            issue_identifier: None,
            batch_id: None,
            issues: Vec::new(),
            cwd: cwd.clone(),
            clone: trunk_clone.clone(),
            repo: repo.as_ref().map(|repo| repo.full_name.clone()),
            repository_id: repository_id.clone(),
            branch: run_branch.clone(),
            base_branch: base_branch.clone(),
            claude_session_id: claude_session_id.clone(),
            pi_session_file: pi_session.clone(),
            codex_originator: codex_originator.clone(),
            inputs: req
                .inputs
                .iter()
                .map(|input| RunInput {
                    key: input.key.clone(),
                    value: input.value.clone(),
                    display: input.display.clone(),
                })
                .collect(),
            model: options.model.clone(),
            effort: options.effort.clone(),
            ultracode: options.ultracode,
            skip_permissions: options.skip_permissions,
            fix: match &req.kind {
                ActionRunKind::FixConflicts {
                    branch,
                    default_branch,
                    identifier,
                    issue_id,
                } => Some(RunFix {
                    branch: branch.clone(),
                    default_branch: default_branch.clone(),
                    identifier: identifier.clone(),
                    issue_id: issue_id.clone(),
                }),
                _ => None,
            },
            started_reason: req
                .trigger
                .as_ref()
                .map(|note| note.started_reason().to_string()),
            resumed_from_id: None,
            recorded_at: crate::run_registry::now_secs(),
            extra: BTreeMap::new(),
        },
    );

    Ok(Prepared::Ready(PreparedLaunch {
        session_id: session.id,
        issue_identifier: req.action_name.clone(),
        worktree: cwd.clone(),
        clone: trunk_clone.unwrap_or(cwd),
        repository_id,
        branch,
        base_branch: base_branch.clone(),
        run_cleanup,
        spawn,
        tab_title,
        tab_title_prefix: tab_prefix,
        heartbeat_scope: coding_sessions::HeartbeatScope {
            issue_id: None,
            team_id: session.team_id.clone(),
            action_id: Some(req.action_id.clone()),
            action_name: Some(req.action_name.clone()),
            device_label: Some(req.device_label.clone()),
            started_by_id: attribution(&req.origin, deps)
                .started_by_id
                .map(str::to_string),
            device_id: attribution(&req.origin, deps).device_id.map(str::to_string),
            // EXP-530: the SAME reason the start stamped, echoed on every
            // ping so a swept row resurrects still badged Automated (both
            // hosts arrive here — the GUI automation host and the CLI
            // daemon's worker both set `req.trigger`).
            started_reason: req
                .trigger
                .as_ref()
                .map(|note| note.started_reason().to_string()),
            automation_id: req.automation_id.clone(),
            // EXP-637: the run branch, so a resurrected row still points at
            // the worktree the agent is working in.
            branch: run_branch.clone(),
        },
        tab_kind: TabKind::Action(req.action_id.clone()),
        bypass_permissions: options.skip_permissions && !options.plan_mode,
        plan_mode: options.plan_mode,
        agent,
        claude_session_id,
        codex_originator,
        codex_resume_id: None,
        launch_hold,
    }))
}

/// Claude Code's per-cwd transcript dir name: every non-alphanumeric
/// character becomes `-` (mirror of `steer::activity::munge_claude_project_dir`
/// — the two crates cannot depend on each other, §3.1). "projects" is CLAUDE
/// CODE's own directory name, never our renamed product entity (EXP-191).
fn munge_claude_project_dir(path: &Path) -> String {
    path.to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// `~/.claude/projects` unless the caller injected a fixture root.
fn claude_projects_root(deps: &CodingDeps) -> Option<PathBuf> {
    if let Some(root) = &deps.claude_projects_root {
        return Some(root.clone());
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(PathBuf::from(home).join(".claude").join("projects"))
}

/// Does claude still hold the recorded conversation for this cwd?
///
/// The munged-cwd directory is the fast path; claude caps very long project
/// directory names (~200 chars) and appends a short hash instead, so a miss
/// falls back to scanning every project directory for `<session_id>.jsonl`
/// — the uuid is unique, so the first hit IS the transcript. Never depend on
/// claude's naming rule beyond the fast path.
fn claude_transcript_exists(deps: &CodingDeps, cwd: &Path, session_id: &str) -> bool {
    let Some(root) = claude_projects_root(deps) else {
        return false;
    };
    let file_name = format!("{session_id}.jsonl");
    if root
        .join(munge_claude_project_dir(cwd))
        .join(&file_name)
        .is_file()
    {
        return true;
    }
    let Ok(entries) = std::fs::read_dir(&root) else {
        return false;
    };
    entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .any(|entry| entry.path().join(&file_name).is_file())
}

/// EXP-637/EXP-662 — RESUME an ended run or SESSION (blocking, background
/// executor): action, chat, issue and batch all come through here, told apart
/// only by [`RunRecord::kind`].
///
/// The recorded [`RunRecord`] already answers every question the original
/// launch had to resolve (agent, cwd, branch, options), so this sequence is
/// the SHORT one: doctor → workspace still there → repo-backed re-auth and
/// idempotent worktree → native transcript probe (else a fresh session
/// seeded with the matching resume prompt) → a NEW session row carrying
/// `resumedFromId` → MCP wiring with the new id → argv → re-record.
///
/// A resume never enters plan mode (whatever plan there was already ran) and
/// never flips an issue's status: the launch that opened the work did that,
/// and resuming a `done` issue must not reopen it.
fn prepare_resume_run(
    req: &ResumeRunRequest,
    deps: &CodingDeps,
    hooks: Option<&HookSetup>,
    observer: Option<&ObserverSetup>,
) -> Result<Prepared, CodingError> {
    let record = &req.record;
    let agent = record.agent;
    let options = LaunchOptions {
        agent,
        model: req.model.clone().unwrap_or_else(|| record.model.clone()),
        effort: req.effort.clone().unwrap_or_else(|| record.effort.clone()),
        ultracode: record.ultracode,
        // The plan already happened in the run being continued.
        plan_mode: false,
        skip_permissions: record.skip_permissions,
    };

    // Step 0 — doctor: the RECORDED agent (a resume never switches agents),
    // plus git when the run lives in a clone.
    let report = run_doctor(&deps.settings);
    let failure = if record.clone.is_some() {
        report.first_failure_for(agent)
    } else {
        let check = report.check_for(agent);
        (!check.ok).then_some(check)
    };
    if let Some(failed) = failure {
        return Ok(Prepared::Disabled(DisabledReason::DoctorFailed(
            failed.clone(),
        )));
    }

    // Step 1 — the workspace. A scratch dir the user cleared, or a worktree
    // the prune reclaimed, is a hard stop: there is nothing to resume INTO
    // (the callers filter on `resumable()` first; this is the backstop).
    let cwd = record.cwd.clone();
    if !cwd.is_dir() {
        return Err(CodingError::Io(format!(
            "this run's workspace is gone ({})",
            cwd.display()
        )));
    }

    // §7.2 — the personal key, raced like every other launch.
    let key_handle = {
        let trpc = Arc::clone(&deps.trpc);
        let store = Arc::clone(&deps.token_store);
        let account_id = deps.account_id.clone();
        std::thread::spawn(move || users::ensure_personal_key(&trpc, &store, &account_id))
    };

    // Step 2 — repo-backed: a fresh JIT token, ambient auth re-installed
    // (the recorded one expired long ago), and an IDEMPOTENT worktree
    // re-create (it normally already exists; this heals a worktree whose
    // admin files git pruned under us). EXP-662: through the injected
    // [`WorktreeProvider`], which runs the SAME ensure_clone → credentials →
    // fetch_base → create_worktree sequence a fresh launch takes.
    let mut launch_hold: Option<crate::launch_gate::LaunchHold> = None;
    // The base a fallback prompt compares against (`origin/<base>..HEAD`).
    let mut default_branch = record.base_branch.clone().unwrap_or_default();
    if let (Some(clone), Some(repository_id)) = (&record.clone, &record.repository_id) {
        let minted = match crate::token_cache::token_cache().get_or_mint_with_margin(
            &deps.trpc,
            repository_id,
            crate::token_refresh::REFRESH_LEAD,
        ) {
            Ok(minted) => minted,
            Err(err) => {
                return map_token_error(err, record.repo.as_deref().unwrap_or("repository"))
            }
        };
        let url = minted.url.clone();
        if default_branch.is_empty() {
            default_branch = minted.default_branch.clone();
        }
        launch_hold = Some(crate::launch_gate::hold(clone));
        match &record.branch {
            Some(branch) => {
                crate::git_worktree::validate_branch_arg(branch, "resume run")?;
                deps.worktrees.prepare(
                    &deps.settings.repos_root_path(),
                    url.full_name(),
                    &default_branch,
                    branch,
                    &url,
                    minted.expires_at.as_deref(),
                )?;
            }
            // A branch-less repo-backed record (a pre-EXP-637 trunk-clone
            // run) still needs its ambient auth reinstalled.
            None => git_credentials::ensure(clone, &url, minted.expires_at.as_deref())?,
        }
    }

    // Step 3 — can the agent reopen its own conversation? Each agent answers
    // differently, and a `no` is never fatal: the fallback is a fresh
    // session seeded with the resume prompt.
    let marker_allows_resume = crate::worktree_agents::worktree_agents(&cwd)
        .is_none_or(|recorded| recorded.contains(&agent));
    let claude_resume_id = record
        .claude_session_id
        .clone()
        .filter(|_| marker_allows_resume && agent == CodingAgent::Claude)
        .filter(|id| claude_transcript_exists(deps, &cwd, id));
    let codex_resume_id = (marker_allows_resume && agent == CodingAgent::Codex)
        .then(|| {
            deps.codex_sessions_root
                .clone()
                .or_else(crate::codex_sessions::default_codex_sessions_root)
                .and_then(|root| {
                    crate::codex_sessions::find_codex_session_id(
                        &root,
                        &cwd,
                        record.codex_originator.as_deref(),
                    )
                })
        })
        .flatten();
    // pi resumes by FILE: the recorded transcript path, when it still exists.
    let pi_resume_file = record
        .pi_session_file
        .clone()
        .filter(|_| marker_allows_resume && agent == CodingAgent::Pi)
        .filter(|path| path.is_file());
    let native_resume = claude_resume_id.is_some()
        || codex_resume_id.is_some()
        || pi_resume_file.is_some();

    // Step 4 — the seed prompt, only when nothing native survived. An ISSUE
    // session gets the issue-shaped resume prompt (PR contract, comment
    // thread, `origin/<default>` compare) — the same one a record-less
    // resume takes through [`prepare`]; everything else gets the run-shaped
    // one, named by [`RunRecord::display_name`].
    let delivery = if native_resume {
        let _ = std::fs::remove_file(cwd.join(PROMPT_FILE));
        None
    } else {
        let rendered = match record.kind {
            RunKind::Issue => {
                let identifier = record.issue_identifier.as_deref().unwrap_or_default();
                let seed = record
                    .issue_id
                    .as_deref()
                    .and_then(|issue_id| (deps.issue_seed)(issue_id));
                let title = seed
                    .as_ref()
                    .map(|seed| seed.title.as_str())
                    .unwrap_or(identifier);
                render_resume_prompt(identifier, title, &default_branch)
            }
            _ => render_run_resume_prompt(record),
        };
        Some(
            deliver_prompt(&cwd, record.clone.as_deref().unwrap_or(&cwd), &rendered)
                .map_err(|e| CodingError::Io(format!("deliver prompt: {e}")))?,
        )
    };

    let personal_key = key_handle
        .join()
        .map_err(|_| CodingError::Io("personal-key thread panicked".to_string()))??;
    guard_agent_mcp(agent, &cwd)?;

    // Step 5 — a NEW session row in the recorded SUBJECT's shape, pointing
    // back at the run it continues.
    let session = match record.kind {
        RunKind::Issue => coding_sessions::start(
            &deps.trpc,
            record.issue_id.as_deref().unwrap_or_default(),
            Some(&req.device_label),
            attribution(&req.origin, deps),
            Some(&record.session_id),
        ),
        RunKind::Batch => coding_sessions::start_batch(
            &deps.trpc,
            &record.team_id,
            Some(&req.device_label),
            attribution(&req.origin, deps),
            Some(&record.session_id),
        ),
        _ => coding_sessions::start_action(
            &deps.trpc,
            coding_sessions::ActionStart {
                action_id: &record.action_id,
                team_id: (record.kind != RunKind::Team).then_some(record.team_id.as_str()),
                // A resume is always a PERSON's doing, never an automation.
                started_reason: None,
                automation_id: None,
                device_label: Some(&req.device_label),
                branch: record.branch.as_deref(),
                resumed_from_id: Some(&record.session_id),
                attribution: attribution(&req.origin, deps),
            },
        ),
    };
    let session = match session {
        Ok(session) => session,
        Err(ApiError::Http { status: 412, message }) => {
            return Ok(Prepared::Disabled(DisabledReason::SessionLimit { message }))
        }
        Err(err) => return Err(err.into()),
    };

    let agent_mcp = end_on_error(
        &deps.trpc,
        &session.id,
        wire_agent_mcp(
            agent,
            &cwd,
            deps.trpc.base_url(),
            &personal_key,
            Some(&session.id),
        ),
    )?;
    let _ = crate::worktree_agents::record_worktree_agent(&cwd, agent);

    // Step 6 — the spawn spec, mirroring the fresh action path.
    if agent == CodingAgent::Codex {
        let trust_root = record
            .clone
            .as_deref()
            .or_else(|| cwd.parent())
            .unwrap_or(&cwd);
        crate::codex_trust::ensure_trusted(trust_root);
    }
    if agent == CodingAgent::Claude {
        crate::claude_trust::ensure_onboarded(&cwd, options.skip_permissions);
    }
    let hook_settings = write_hook_settings(&deps.data_dir, &session.id, agent, hooks);
    // A native claude resume keeps the recorded conversation's id; anything
    // else mints a fresh pin.
    let claude_session_id = (agent == CodingAgent::Claude && claude_resume_id.is_none())
        .then(|| uuid::Uuid::new_v4().to_string());
    let codex_originator =
        (agent == CodingAgent::Codex).then(|| crate::argv::codex_session_originator(&session.id));
    let pi_session = (agent == CodingAgent::Pi)
        .then(|| {
            pi_resume_file
                .clone()
                .or_else(|| pi_session_file(&deps.data_dir, &session.id))
        })
        .flatten();
    let tail = match (&delivery, &claude_resume_id, &codex_resume_id) {
        (Some(delivery), _, _) => SessionTail::Prompt(delivery.positional()),
        (None, Some(id), _) => SessionTail::ClaudeResume(id),
        (None, None, Some(id)) => SessionTail::CodexResume(id),
        // pi resumes purely through `--session <recorded file>`.
        (None, None, None) => SessionTail::None,
    };
    let args = session_args(
        &options,
        &agent_mcp,
        hook_settings.as_ref().map(|files| files.settings.as_path()),
        SessionIdentity {
            claude_session_id: claude_session_id.as_deref(),
            pi_session_file: pi_session.as_deref(),
        },
        tail,
    );
    // EXP-662: a resumed SESSION is titled like a fresh one (`claude ·
    // EXP-42` / `claude · EXP-42 +1`) — the strip must not tell a resume
    // apart from the launch it continues.
    let tab_prefix = record.display_name();
    let tab_title = match record.kind {
        RunKind::Issue | RunKind::Batch => format!("{} · {tab_prefix}", agent.id()),
        RunKind::Chat => format!("chat · {tab_prefix}"),
        _ => format!("action · {tab_prefix}"),
    };
    let mut spawn = SpawnSpec::new(&deps.settings.resolved_path_for(agent))
        .args(args)
        .cwd(&cwd);
    spawn = apply_mcp_env(
        spawn,
        agent,
        deps.trpc.base_url(),
        &personal_key,
        Some(&session.id),
    );
    spawn = apply_hook_env(spawn, hooks, hook_settings.as_ref());
    spawn = apply_observer_env(spawn, agent, observer);
    if let Some(originator) = &codex_originator {
        spawn = spawn.env(crate::argv::CODEX_ORIGINATOR_ENV, originator);
    }
    if let Some(clone) = &record.clone {
        spawn = spawn
            .env(
                "CARGO_TARGET_DIR",
                shared_cargo_target_dir(clone).to_string_lossy().into_owned(),
            )
            .env("CARGO_INCREMENTAL", "0");
    }

    // Only a run that OWNS its worktree may have it reclaimed — never the
    // fix-conflicts run's shared PR worktree, never an issue/batch one.
    let run_cleanup = match (&record.clone, &record.branch, &record.base_branch) {
        (Some(clone), Some(branch), Some(base_branch)) if record.kind.owns_run_worktree() => {
            Some(RunCleanup {
                clone: clone.clone(),
                worktree: cwd.clone(),
                branch: branch.clone(),
                base_branch: base_branch.clone(),
            })
        }
        _ => None,
    };

    // The resumed run gets its OWN record — a resume of a resume chains.
    crate::run_registry::record(
        &deps.data_dir,
        RunRecord {
            session_id: session.id.clone(),
            claude_session_id: claude_session_id.clone().or_else(|| claude_resume_id.clone()),
            pi_session_file: pi_session.clone(),
            codex_originator: codex_originator.clone(),
            model: options.model.clone(),
            effort: options.effort.clone(),
            started_reason: None,
            resumed_from_id: Some(record.session_id.clone()),
            recorded_at: crate::run_registry::now_secs(),
            ..record.clone()
        },
    );

    // The re-created row's start scope, in the recorded subject's shape —
    // an issue scope refuses a branch server-side, and a batch one carries
    // the team the record already pinned.
    let scope_attribution = attribution(&req.origin, deps);
    let heartbeat_scope = match record.kind {
        RunKind::Issue => coding_sessions::HeartbeatScope {
            issue_id: record.issue_id.clone(),
            team_id: None,
            action_id: None,
            action_name: None,
            device_label: Some(req.device_label.clone()),
            started_by_id: scope_attribution.started_by_id.map(str::to_string),
            device_id: scope_attribution.device_id.map(str::to_string),
            started_reason: None,
            automation_id: None,
            branch: None,
        },
        RunKind::Batch => coding_sessions::HeartbeatScope {
            issue_id: None,
            team_id: Some(record.team_id.clone()),
            action_id: None,
            action_name: None,
            device_label: Some(req.device_label.clone()),
            started_by_id: scope_attribution.started_by_id.map(str::to_string),
            device_id: scope_attribution.device_id.map(str::to_string),
            started_reason: None,
            automation_id: None,
            branch: None,
        },
        _ => coding_sessions::HeartbeatScope {
            issue_id: None,
            team_id: session.team_id.clone(),
            action_id: Some(record.action_id.clone()),
            action_name: Some(record.action_name.clone()),
            device_label: Some(req.device_label.clone()),
            started_by_id: scope_attribution.started_by_id.map(str::to_string),
            device_id: scope_attribution.device_id.map(str::to_string),
            started_reason: None,
            automation_id: None,
            branch: record.branch.clone(),
        },
    };
    let issue_identifier = match record.kind {
        RunKind::Issue => record.issue_identifier.clone().unwrap_or_default(),
        RunKind::Batch => format!("batch-{}", record.batch_id.clone().unwrap_or_default()),
        _ => record.action_name.clone(),
    };
    let tab_kind = match record.kind {
        RunKind::Issue | RunKind::Batch => TabKind::Claude,
        _ => TabKind::Action(record.action_id.clone()),
    };

    Ok(Prepared::Ready(PreparedLaunch {
        session_id: session.id,
        issue_identifier,
        worktree: cwd.clone(),
        clone: record.clone.clone().unwrap_or(cwd),
        repository_id: record.repository_id.clone(),
        branch: record.branch.clone().unwrap_or_default(),
        base_branch: record.base_branch.clone(),
        run_cleanup,
        spawn,
        tab_title,
        tab_title_prefix: tab_prefix,
        heartbeat_scope,
        tab_kind,
        bypass_permissions: options.skip_permissions,
        plan_mode: false,
        agent,
        claude_session_id,
        codex_originator,
        codex_resume_id,
        launch_hold,
    }))
}

/// An EXP-325 promptless agent-shell launch input: the terminal dock's "+"
/// menu picked an installed agent and a repo; the agent spawns as a FRESH
/// interactive session on the repo's trunk clone with the picked agent's
/// settings defaults and the usual MCP wiring — but NO issue/batch/action
/// subject. Deliberately no `codingSessions` row / heartbeat / exit hook /
/// steer room: session subjects are issue XOR batch XOR action and this is
/// none of them, so teammates never see a badge for an empty local session.
#[derive(Clone, Debug)]
pub struct AgentShellRequest {
    /// The picked agent's settings defaults
    /// ([`LaunchOptions::defaults_for`]).
    pub options: LaunchOptions,
    /// The trunk repo to run in, resolved by the caller (the desktop syncs
    /// no repositories). Just the two ids the token/clone path needs — the
    /// default branch resolves live from the minted token, never here.
    pub repository_id: String,
    /// `owner/name` — the clone-root key.
    pub full_name: String,
    /// EXP-369: run in THIS directory instead of the trunk clone root (the
    /// settings pane's per-worktree terminal). Must be a path of the same
    /// clone — the ambient git auth and the shared cargo cache are
    /// clone-scoped, and a linked worktree shares both. A cwd outside the
    /// clone can no longer leak the MCP key either way: the EXP-474 guard
    /// ([`crate::git_worktree::ensure_ignored`]) resolves the governing repo
    /// from the cwd itself.
    pub cwd_override: Option<PathBuf>,
}

/// [`prepare_agent_shell`] done: everything the foreground needs to open the
/// tab ([`TabKind::AgentShell`], no exit hook, no heartbeat).
#[derive(Debug)]
pub struct AgentShellLaunch {
    /// The agent invocation on the trunk clone (also the cwd).
    pub spawn: SpawnSpec,
    /// The trunk clone — the P9 token refresher's ambient-auth target.
    pub clone: PathBuf,
    /// The team `repositories` row id — feeds the token refresher hold.
    pub repository_id: String,
    /// Tab strip default title (`claude · exponential`).
    pub tab_title: String,
    /// The agent id, re-attached to live OSC titles (EXP-145 decoration).
    pub tab_title_prefix: String,
}

/// [`prepare_agent_shell`]'s outcome: ready to spawn, or disabled-with-reason.
#[derive(Debug)]
pub enum PreparedAgentShell {
    Ready(AgentShellLaunch),
    Disabled(DisabledReason),
}

/// The EXP-325 promptless prepare — the repo-backed arm of [`prepare_action`]
/// minus the prompt and the session row (blocking network/git I/O, gpui-free
/// — run on the background executor): doctor → JIT token → clone/ambient
/// auth → best-effort autopull → per-agent MCP wiring →
/// [`SessionTail::None`] argv.
pub fn prepare_agent_shell(
    req: &AgentShellRequest,
    deps: &CodingDeps,
) -> Result<PreparedAgentShell, CodingError> {
    let options = &req.options;
    let agent = options.agent;

    // Doctor — the picked agent AND git (a trunk clone is always involved).
    let report = run_doctor(&deps.settings);
    if let Some(failed) = report.first_failure_for(agent) {
        return Ok(PreparedAgentShell::Disabled(DisabledReason::DoctorFailed(
            failed.clone(),
        )));
    }

    // §7.2 — the personal key (the MCP credential), raced like a session's.
    let key_handle = {
        let trpc = Arc::clone(&deps.trpc);
        let store = Arc::clone(&deps.token_store);
        let account_id = deps.account_id.clone();
        std::thread::spawn(move || users::ensure_personal_key(&trpc, &store, &account_id))
    };

    // JIT token → clone → ambient auth → best-effort autopull (the action
    // path's repo-backed arm — the session works on whatever the trunk holds
    // and the trunk-sync engine keeps surfacing its state).
    let minted = match crate::token_cache::token_cache().get_or_mint_with_margin(
        &deps.trpc,
        &req.repository_id,
        crate::token_refresh::REFRESH_LEAD,
    ) {
        Ok(minted) => minted,
        Err(err) => {
            return token_error_reason(err, &req.full_name).map(PreparedAgentShell::Disabled)
        }
    };
    let url = minted.url.clone();
    let repos_root = deps.settings.repos_root_path();
    let clone = ensure_clone(&repos_root, url.full_name(), &url)?;
    git_credentials::ensure(&clone, &url, minted.expires_at.as_deref())?;
    let _ = crate::clone_manager::auto_sync(&clone, &url);
    // Best-effort [`LOCAL_EXCLUDES`] coverage; `.exp-mcp.json` gets its hard
    // EXP-474 guard in [`wire_agent_mcp`], resolved from the actual cwd
    // (which may be an EXP-369 worktree).
    let _ = crate::git_worktree::ensure_local_excludes(&clone, LOCAL_EXCLUDES);

    // EXP-369: the agent runs in the pinned worktree when the caller gave
    // one — the MCP config file has to land in the SAME dir (claude/pi read
    // it relative to their cwd).
    let cwd = agent_shell_cwd(req, &clone);

    // The per-agent MCP wiring (the agent authenticates as the real user).
    let personal_key = key_handle
        .join()
        .map_err(|_| CodingError::Io("personal-key thread panicked".to_string()))??;
    let agent_mcp = wire_agent_mcp(agent, &cwd, deps.trpc.base_url(), &personal_key, None)?;

    // The spawn spec: no prompt, no hooks sidecar (hooks are per-session —
    // there is no session row to scope one to). EXP-389: same codex
    // directory pre-trust as a session (an EXP-369 worktree cwd resolves to
    // the clone anyway).
    if agent == CodingAgent::Codex {
        crate::codex_trust::ensure_trusted(&clone);
    }
    // EXP-414: claude keys trust by the spawn cwd itself.
    if agent == CodingAgent::Claude {
        crate::claude_trust::ensure_onboarded(
            &cwd,
            options.skip_permissions && !options.plan_mode,
        );
    }
    // EXP-443: no claude session id — shells stay hookless/unpinned, and the
    // whole point of the pin is that real sessions stop listening to them.
    let args = session_args(
        options,
        &agent_mcp,
        None,
        SessionIdentity::default(),
        SessionTail::None,
    );
    let tab_title = agent_shell_tab_title(agent, req, &cwd);
    let mut spawn = SpawnSpec::new(&deps.settings.resolved_path_for(agent))
        .args(args)
        .cwd(&cwd);
    spawn = apply_mcp_env(spawn, agent, deps.trpc.base_url(), &personal_key, None);
    if agent == CodingAgent::Codex {
        // EXP-443: shells share the trunk cwd with action runs — a distinct
        // originator keeps their rollouts out of every session's strict pass.
        spawn = spawn.env(
            crate::argv::CODEX_ORIGINATOR_ENV,
            crate::argv::CODEX_SHELL_ORIGINATOR,
        );
    }
    // Same EXP-76 shared-cache posture as a session (keyed off the clone).
    spawn = spawn
        .env(
            "CARGO_TARGET_DIR",
            shared_cargo_target_dir(&clone).to_string_lossy().into_owned(),
        )
        .env("CARGO_INCREMENTAL", "0");

    Ok(PreparedAgentShell::Ready(AgentShellLaunch {
        spawn,
        clone,
        repository_id: req.repository_id.clone(),
        tab_title,
        tab_title_prefix: agent.id().to_string(),
    }))
}

/// Where an agent shell runs: the caller's pinned worktree (EXP-369) or the
/// trunk clone root. The clone stays the ambient-auth / cargo-cache anchor
/// either way — only the cwd (and with it the MCP config file's home) moves.
fn agent_shell_cwd(req: &AgentShellRequest, clone: &Path) -> PathBuf {
    req.cwd_override
        .clone()
        .unwrap_or_else(|| clone.to_path_buf())
}

/// The segment after the owner in a `owner/repo` full name — the tab label
/// the agent-shell and the EXP-615 chat tabs share (`acme/web` → `web`).
fn repo_short_name(full_name: &str) -> &str {
    full_name.rsplit('/').next().unwrap_or(full_name)
}

/// The agent-shell tab title: `claude · <repo>` on the trunk clone, and
/// `claude · <worktree dir>` for an EXP-369 pinned worktree (the repo name is
/// the same for every one of them — the worktree dir is what distinguishes
/// the tabs). Pure, so the cwd/title pairing is unit-testable without a
/// network round-trip.
fn agent_shell_tab_title(agent: CodingAgent, req: &AgentShellRequest, cwd: &Path) -> String {
    let label = req
        .cwd_override
        .as_ref()
        .and_then(|_| cwd.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| {
            req.full_name
                .rsplit('/')
                .next()
                .unwrap_or(req.full_name.as_str())
                .to_string()
        });
    format!("{} · {label}", agent.id())
}

/// Foreground follow-up to the child-exit edge (§7.5): the ui layer passes
/// one of these into [`spawn_prepared_with`] to flip its play↔stop state /
/// clear its local-session registry / detach the steer publisher when the
/// agent child dies. Receives the captured [`terminal::pty::ChildExit`] so
/// the steer `bye` can carry the spec'd `exit:<code>` outcome. Runs on the
/// gpui foreground AFTER the idempotent `codingSessions.end` fire-and-forget
/// thread is spawned. The `coding` crate itself never needs it —
/// [`spawn_prepared`] passes `None`.
#[cfg(feature = "gpui")]
pub type ExitNotify = Box<dyn FnOnce(&terminal::pty::ChildExit, &mut App) + 'static>;

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
#[cfg(feature = "gpui")]
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
#[cfg(feature = "gpui")]
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
    let on_exit: terminal::ExitHook = Box::new(move |_tab, exit, cx| {
        // Disconnect the heartbeat thread — the child is gone, so the row is
        // about to be ended and must stop being kept alive.
        drop(heartbeat_stop);
        // Blocking HTTP off the foreground; best-effort — the server also
        // reconciles (idempotent end), and a dead network here must never
        // take the exit-strip rendering down with it.
        let trpc = Arc::clone(&exit_trpc);
        let end_session_id = end_session_id.clone();
        std::thread::spawn(move || {
            end_session_best_effort(&trpc, &end_session_id);
        });
        if let Some(notify) = exit_notify {
            notify(exit, cx);
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
                end_session_best_effort(&trpc, &session_id);
            });
        })?;

    Ok(LaunchOutcome::Spawned {
        session_id,
        terminal_tab: tab_id,
        worktree,
        branch,
    })
}

/// EXP-640: observer for the outcome of EVERY `codingSessions.end` this
/// crate (and, via [`end_session`], the host) issues. The desktop's
/// crash-recovery registry hangs off it: an entry is dropped only once the
/// end actually RESOLVED. Before this, the host dropped the entry as soon as
/// the local session went away — so an end rejected by the server's 426
/// min-version gate (every call from a just-superseded build, while the app
/// sat on the blocking "Update required" screen) left the row `running` with
/// nothing left to reconcile it, until the server's 2h sweep.
pub type SessionEndObserver =
    Arc<dyn Fn(&str, &Result<coding_sessions::CodingSession, ApiError>) + Send + Sync>;

static SESSION_END_OBSERVER: OnceLock<SessionEndObserver> = OnceLock::new();

/// Install the process-wide [`SessionEndObserver`]. First caller wins; a
/// later call is a no-op (the host installs it once at bootstrap).
pub fn set_session_end_observer(observer: SessionEndObserver) {
    let _ = SESSION_END_OBSERVER.set(observer);
}

/// `codingSessions.end` with the outcome reported to the
/// [`SessionEndObserver`]. Every end this crate issues goes through here so
/// the host's registry sees each outcome exactly where it happens.
pub fn end_session(
    trpc: &TrpcClient,
    session_id: &str,
) -> Result<coding_sessions::CodingSession, ApiError> {
    let result = coding_sessions::end(trpc, session_id);
    if let Some(observer) = SESSION_END_OBSERVER.get() {
        observer(session_id, &result);
    }
    result
}

/// Best-effort session end for teardown paths that bypass the exit hook
/// (app quit with a live child, relay kill). Idempotent server-side; the
/// outcome still reaches the [`SessionEndObserver`].
pub fn end_session_best_effort(trpc: &TrpcClient, session_id: &str) {
    let _ = end_session(trpc, session_id);
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
            resume_prompt: false,
        }
    }

    /// EXP-474: the write-site guard — a repo whose committed `.gitignore`
    /// re-includes `.exp-mcp.json` (a `!` pattern outranks
    /// `.git/info/exclude`) must refuse the Claude wiring BEFORE the key
    /// ever lands on disk.
    #[test]
    fn wire_agent_mcp_refuses_claude_when_the_repo_reincludes_the_key_file() {
        let dir = temp_dir("mcp-reinclude");
        let repo = dir.0.join("repo");
        fs::create_dir_all(&repo).unwrap();
        let git = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .args(args)
                .current_dir(&repo)
                .env("GIT_AUTHOR_NAME", "t")
                .env("GIT_AUTHOR_EMAIL", "t@example.com")
                .env("GIT_COMMITTER_NAME", "t")
                .env("GIT_COMMITTER_EMAIL", "t@example.com")
                .output()
                .unwrap();
            assert!(out.status.success(), "git {args:?} failed");
        };
        git(&["init", "--quiet", "-b", "main"]);
        fs::write(repo.join(".gitignore"), "!.exp-mcp.json\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "--quiet", "-m", "reinclude"]);

        let err = wire_agent_mcp(CodingAgent::Claude, &repo, "http://localhost:1", "expu_x", None)
            .unwrap_err();
        assert!(matches!(err, CodingError::Git(_)), "wrong error: {err:?}");
        assert!(!repo.join(crate::mcp_json::MCP_JSON_FILE).exists(), "key landed on disk");
    }

    /// The repo-less action-scratch flow keeps working: no governing work
    /// tree means nothing can stage the file, so the guard passes and the
    /// config is written.
    #[test]
    fn wire_agent_mcp_writes_the_key_file_in_a_repo_less_scratch_dir() {
        let dir = temp_dir("mcp-scratch");
        let wired = wire_agent_mcp(CodingAgent::Claude, &dir.0, "http://localhost:1", "expu_x", None)
            .unwrap();
        assert_eq!(wired, AgentMcp::ClaudeFile);
        assert!(dir.0.join(crate::mcp_json::MCP_JSON_FILE).exists());
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
                    Some("claude not found on PATH. Set an absolute path.")
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

    /// A base repo-less action request — tests override the fields they
    /// exercise (EXP-257: team_id/inputs/kind ride every request).
    fn action_request() -> ActionLaunchRequest {
        ActionLaunchRequest {
            action_id: "act-1".to_string(),
            run_id: "1a2b3c4d".to_string(),
            action_name: "Code review".to_string(),
            team_id: "ws-1".to_string(),
            body: "# Review\nScan the backlog.".to_string(),
            repo: None,
            inputs: Vec::new(),
            kind: ActionRunKind::Team,
            trigger: None,
            automation_id: None,
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

        let req = action_request();
        let prepared = match prepare(&PrepareRequest::Action(req), &deps).unwrap() {
            Prepared::Ready(prepared) => prepared,
            Prepared::Disabled(reason) => panic!("unexpectedly disabled: {reason:?}"),
        };

        // EXP-637: the scratch dir is PER RUN (`actions/<action>/<run>`) —
        // concurrent runs of the same repo-less action must not share one
        // `.exp-mcp.json` and overwrite each other's session header.
        let scratch = dir.0.join("actions").join("act-1").join("1a2b3c4d");
        assert_eq!(prepared.worktree, scratch);
        assert!(scratch.join(crate::mcp_json::MCP_JSON_FILE).exists());
        // No repo: nothing for the token refresher, no branch to track.
        assert_eq!(prepared.repository_id, None);
        assert_eq!(prepared.branch, "");
        assert_eq!(prepared.session_id, "sess-a");
        assert_eq!(prepared.tab_title, "action · Code review");
        assert_eq!(prepared.tab_kind, TabKind::Action("act-1".to_string()));

        // Claude session argv: explicit model + strict MCP config; plan mode
        // off (the request's choice — EXP-257 honors the options as-is); the
        // prompt is the preamble + body positional.
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

        // EXP-443: action runs mint identities like a session (claude here —
        // a fresh --session-id, no codex originator).
        let stripped = strip_session_id(&prepared);
        assert!(!stripped.iter().any(|arg| arg == "--session-id"));
        assert_eq!(prepared.codex_originator, None);
        assert_eq!(prepared.codex_resume_id, None);
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
        let mut req = action_request();
        req.action_id = "../../escape".to_string();
        req.action_name = "Evil".to_string();
        req.body = "x".to_string();
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
        let mut req = action_request();
        req.action_name = "Groom".to_string();
        req.body = "do it".to_string();
        match prepare(&PrepareRequest::Action(req), &deps).unwrap() {
            Prepared::Disabled(DisabledReason::SessionLimit { message }) => {
                assert!(message.contains("limit"));
            }
            other => panic!("expected SessionLimit, got {other:?}"),
        }
    }

    /// EXP-530: an automation-started run stamps startedReason on the start
    /// mutation and renders the `## Trigger` prompt section — one flag, two
    /// seams, locked together.
    #[test]
    fn prepare_action_trigger_rides_the_start_and_the_prompt() {
        let dir = temp_dir("action-trigger");
        let (base, captured) = canned_server_recording(vec![(200, START_ACTION_OK.to_string())]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree: dir.0.join("unused"),
            seen: Default::default(),
        });
        let deps = make_deps(&base, &dir.0, worktrees);

        let mut req = action_request();
        req.trigger = Some(TriggerNote {
            kind: crate::action_prompt::TriggerNoteKind::Schedule {
                phrase: "daily at 07:00".to_string(),
            },
        });
        req.automation_id = Some("auto-1".to_string());
        let prepared = match prepare(&PrepareRequest::Action(req), &deps).unwrap() {
            Prepared::Ready(prepared) => prepared,
            Prepared::Disabled(reason) => panic!("unexpectedly disabled: {reason:?}"),
        };

        let prompt = prepared.spawn.args.last().unwrap();
        assert!(prompt.contains("## Trigger"));
        assert!(prompt.contains("started automatically by the action's schedule \
(daily at 07:00, device time)"));

        // The heartbeat carries it too — a row the staleness sweep reaped
        // must resurrect still badged Automated, not hand-started.
        assert_eq!(
            prepared.heartbeat_scope.started_reason.as_deref(),
            Some("schedule")
        );
        // EXP-583: and which automation fired it.
        assert_eq!(
            prepared.heartbeat_scope.automation_id.as_deref(),
            Some("auto-1")
        );

        let requests = captured.lock().unwrap();
        assert_eq!(requests.len(), 1, "{requests:?}");
        assert!(requests[0].contains(r#""startedReason":"schedule""#));
        assert!(requests[0].contains(r#""automationId":"auto-1""#));
    }

    /// EXP-257: action runs honor the full claude option set — plan mode ON
    /// composes the plan permission args exactly like an issue session (the
    /// old Claude-only clamp forced it off).
    #[test]
    fn prepare_action_honors_claude_plan_mode() {
        let dir = temp_dir("action-plan");
        let (base, _captured) = canned_server_recording(vec![(200, START_ACTION_OK.to_string())]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree: dir.0.join("unused"),
            seen: Default::default(),
        });
        let deps = make_deps(&base, &dir.0, worktrees);
        let mut req = action_request();
        req.options.plan_mode = true;
        req.options.effort = "high".to_string();

        let prepared = match prepare(&PrepareRequest::Action(req), &deps).unwrap() {
            Prepared::Ready(prepared) => prepared,
            other => panic!("expected Ready, got {other:?}"),
        };
        let args = &prepared.spawn.args;
        assert!(args
            .windows(2)
            .any(|w| w == ["--permission-mode", "plan"]));
        assert!(args.windows(2).any(|w| w == ["--effort", "high"]));
    }

    /// EXP-257: a CODEX action run — codex argv + env-token MCP posture, no
    /// on-disk `.exp-mcp.json`, tab titled `action · …` with the codex
    /// program.
    #[test]
    fn prepare_action_codex_uses_overrides_and_env_token() {
        let dir = temp_dir("action-codex");
        let (base, _captured) = canned_server_recording(vec![(200, START_ACTION_OK.to_string())]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree: dir.0.join("unused"),
            seen: Default::default(),
        });
        let mut deps = make_deps(&base, &dir.0, worktrees);
        deps.settings.codex_path = "git".to_string(); // runnable stub
        let mut req = action_request();
        req.options = LaunchOptions {
            agent: CodingAgent::Codex,
            model: "gpt-5.6-sol".to_string(),
            effort: "high".to_string(),
            ultracode: false,
            plan_mode: false,
            skip_permissions: false,
        };

        let prepared = match prepare(&PrepareRequest::Action(req), &deps).unwrap() {
            Prepared::Ready(prepared) => prepared,
            other => panic!("expected Ready, got {other:?}"),
        };
        // NO on-disk MCP config for codex — the key must not land in the
        // scratch dir.
        let scratch = dir.0.join("actions").join("act-1");
        assert!(!scratch.join(".exp-mcp.json").exists());
        assert!(!scratch.join(".exp-pi-mcp.ts").exists());
        let args = &prepared.spawn.args;
        // EXP-389: the update-prompt suppression leads every codex argv.
        assert_eq!(
            args[..4],
            [
                "-c".to_string(),
                "check_for_update_on_startup=false".to_string(),
                "-m".to_string(),
                "gpt-5.6-sol".to_string()
            ]
        );
        assert!(args.contains(&format!("mcp_servers.exponential.url=\"{base}/api/mcp\"")));
        assert!(args
            .contains(&"mcp_servers.exponential.bearer_token_env_var=\"EXP_MCP_TOKEN\"".to_string()));
        assert!(!args.iter().any(|arg| arg.contains("expu_")));
        // Auto preset (skip OFF) + the key in the spawn env only.
        assert!(args.contains(&"workspace-write".to_string()));
        assert!(prepared
            .spawn
            .env
            .contains(&("EXP_MCP_TOKEN".to_string(), "expu_seeded".to_string())));
        // The prompt still rides positional-last.
        assert!(args.last().unwrap().contains("Scan the backlog."));
    }

    /// EXP-257: a PI action run — the bridge extension lands in the scratch
    /// dir, `-e` loads it, url/token/skip-version ride the env.
    #[test]
    fn prepare_action_pi_writes_the_bridge() {
        let dir = temp_dir("action-pi");
        let (base, _captured) = canned_server_recording(vec![(200, START_ACTION_OK.to_string())]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree: dir.0.join("unused"),
            seen: Default::default(),
        });
        let mut deps = make_deps(&base, &dir.0, worktrees);
        deps.settings.pi_path = "git".to_string(); // runnable stub
        // EXP-409: the pi auth gate checks credential presence (auth.json or
        // a provider env key) — CI runners have neither, so satisfy the real
        // probe the way a real pi setup would.
        std::env::set_var("ANTHROPIC_API_KEY", "test-pi-credential");
        let mut req = action_request();
        req.options = LaunchOptions {
            agent: CodingAgent::Pi,
            model: "grok-4.5".to_string(),
            effort: String::new(),
            ultracode: false,
            plan_mode: false,
            skip_permissions: false,
        };

        let prepared = match prepare(&PrepareRequest::Action(req), &deps).unwrap() {
            Prepared::Ready(prepared) => prepared,
            other => panic!("expected Ready, got {other:?}"),
        };
        let scratch = dir.0.join("actions").join("act-1").join("1a2b3c4d");
        let bridge = fs::read_to_string(scratch.join(".exp-pi-mcp.ts")).unwrap();
        assert!(!bridge.contains("expu_"));
        assert!(!scratch.join(".exp-mcp.json").exists());
        let args = &prepared.spawn.args;
        assert!(args.windows(2).any(|w| w == ["-e", "./.exp-pi-mcp.ts"]));
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

    fn hook_setup() -> HookSetup {
        HookSetup {
            port: 45321,
            token: "hook-token-1".to_string(),
            settings_json: r#"{"hooks":{"Stop":[]}}"#.to_string(),
        }
    }

    /// EXP-249: a claude session gets its hooks sidecar wired — the settings
    /// file lands under the DATA DIR (never in the worktree, where the agent
    /// could commit it and claude's project scan would see it), rides
    /// `--settings`, and the port + curl-config-path env vars its hook
    /// command expands ride the spawn (REV-51: the token itself only ever
    /// sits in the 0600 config file).
    #[test]
    fn prepare_wires_the_claude_hooks_sidecar_outside_the_worktree() {
        let dir = temp_dir("hooks-claude");
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
        let hooks = hook_setup();

        let prepared = match prepare_with_hooks(
            &PrepareRequest::Issue(request("EXP-42")),
            &deps,
            Some(&hooks),
            None,
        )
        .unwrap()
        {
            Prepared::Ready(prepared) => prepared,
            other => panic!("expected Ready, got {other:?}"),
        };

        let args = &prepared.spawn.args;
        let at = args.iter().position(|arg| arg == "--settings").expect("--settings");
        let settings = PathBuf::from(&args[at + 1]);
        assert_eq!(
            settings,
            dir.0
                .join(HOOK_SETTINGS_DIR)
                .join(std::process::id().to_string())
                .join("sess-1.settings.json")
        );
        assert!(!settings.starts_with(&worktree), "settings file inside the worktree");
        assert_eq!(fs::read_to_string(&settings).unwrap(), hooks.settings_json);
        assert!(!worktree.join(".claude").exists(), "never a worktree .claude dir");
        // Between the model/effort pair and the MCP flags — the prompt keeps
        // the tail. EXP-443: the minted --session-id rides directly after.
        assert!(at > 0 && args[at - 1] == "fable");
        assert_eq!(args[at + 2], "--session-id");
        assert_eq!(args[at + 4], "--mcp-config");
        assert!(
            prepared
                .spawn
                .env
                .contains(&(HOOK_PORT_ENV.to_string(), "45321".to_string())),
            "missing hook port env: {:?}",
            prepared.spawn.env
        );
        // REV-51: the token rides a 0600 curl config referenced by path —
        // never the spawn-expanded hook command line.
        let config_path = prepared
            .spawn
            .env
            .iter()
            .find(|(key, _)| key == HOOK_CONFIG_ENV)
            .map(|(_, value)| PathBuf::from(value))
            .expect("hook curl config env");
        assert_eq!(
            config_path,
            settings.with_file_name("sess-1.curl.cfg"),
            "curl config lands next to the settings file"
        );
        assert_eq!(
            fs::read_to_string(&config_path).unwrap(),
            "header = \"Authorization: Bearer hook-token-1\"\n"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = fs::metadata(&config_path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "curl config must be owner-only");
        }
    }

    /// No sidecar handed in, or a non-claude agent: nothing is written and
    /// the argv/env are exactly what they were before EXP-249.
    #[test]
    fn hooks_are_absent_without_a_setup_and_for_non_claude_agents() {
        let dir = temp_dir("hooks-absent");
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

        let prepared = match prepare(&PrepareRequest::Issue(request("EXP-42")), &deps).unwrap() {
            Prepared::Ready(prepared) => prepared,
            other => panic!("expected Ready, got {other:?}"),
        };
        assert!(!prepared.spawn.args.iter().any(|arg| arg == "--settings"));
        assert!(!prepared
            .spawn
            .env
            .iter()
            .any(|(key, _)| key == HOOK_PORT_ENV || key == HOOK_CONFIG_ENV));
        assert!(!dir.0.join(HOOK_SETTINGS_DIR).exists());

        // Codex has no hooks system: the setup is ignored entirely.
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
            model: String::new(),
            effort: String::new(),
            ultracode: false,
            plan_mode: false,
            skip_permissions: false,
        };
        let hooks = hook_setup();
        let prepared =
            match prepare_with_hooks(&PrepareRequest::Issue(req), &deps, Some(&hooks), None)
                .unwrap()
            {
                Prepared::Ready(prepared) => prepared,
                other => panic!("expected Ready, got {other:?}"),
            };
        assert!(!prepared.spawn.args.iter().any(|arg| arg == "--settings"));
        assert!(!prepared
            .spawn
            .env
            .iter()
            .any(|(key, _)| key == HOOK_PORT_ENV || key == HOOK_CONFIG_ENV));
        assert!(!dir.0.join(HOOK_SETTINGS_DIR).exists());
    }

    /// EXP-478: `prepare` gates the clone from before the worktree exists,
    /// and the hold rides the `PreparedLaunch` until the caller drops it —
    /// the auto-prune's `try_exclusive` must refuse for that whole span.
    #[test]
    fn prepare_takes_the_launch_gate_hold_until_dropped() {
        let dir = temp_dir("launch-gate");
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

        let prepared = match prepare(&PrepareRequest::Issue(request("EXP-42")), &deps).unwrap() {
            Prepared::Ready(prepared) => prepared,
            other => panic!("expected Ready, got {other:?}"),
        };
        let clone = prepared.clone.clone();
        assert!(prepared.launch_hold.is_some());
        assert!(
            crate::launch_gate::try_exclusive(&clone, || ()).is_none(),
            "the prune must be refused while the launch is in flight"
        );
        drop(prepared);
        assert!(
            crate::launch_gate::try_exclusive(&clone, || ()).is_some(),
            "dropping the prepared launch releases the gate"
        );
    }

    /// An action run is a claude session too — same sidecar, keyed by ITS
    /// session id.
    #[test]
    fn prepare_action_wires_the_hooks_sidecar() {
        let dir = temp_dir("hooks-action");
        let (base, _captured) = canned_server_recording(vec![(200, START_ACTION_OK.to_string())]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree: dir.0.join("unused"),
            seen: Default::default(),
        });
        let deps = make_deps(&base, &dir.0, worktrees);
        let hooks = hook_setup();

        let prepared = match prepare_with_hooks(
            &PrepareRequest::Action(action_request()),
            &deps,
            Some(&hooks),
            None,
        )
        .unwrap()
        {
            Prepared::Ready(prepared) => prepared,
            other => panic!("expected Ready, got {other:?}"),
        };
        let args = &prepared.spawn.args;
        let at = args.iter().position(|arg| arg == "--settings").expect("--settings");
        assert_eq!(
            PathBuf::from(&args[at + 1]),
            dir.0
                .join(HOOK_SETTINGS_DIR)
                .join(std::process::id().to_string())
                .join("sess-a.settings.json")
        );
        let config_path = prepared
            .spawn
            .env
            .iter()
            .find(|(key, _)| key == HOOK_CONFIG_ENV)
            .map(|(_, value)| PathBuf::from(value))
            .expect("hook curl config env");
        assert!(
            fs::read_to_string(&config_path)
                .unwrap()
                .contains("Bearer hook-token-1"),
            "curl config carries the session token"
        );
    }

    /// EXP-257: a missing selected agent blocks an action run with ITS copy
    /// (no Claude clamp — the codex doctor row gates a codex action).
    #[test]
    fn prepare_action_missing_selected_agent_blocks() {
        let dir = temp_dir("action-agent-missing");
        let worktrees = Arc::new(FakeWorktrees {
            worktree: dir.0.join("unused"),
            seen: Default::default(),
        });
        // Unroutable base: any network call would error — the doctor gate
        // must fire first.
        let mut deps = make_deps("http://127.0.0.1:1", &dir.0, worktrees);
        deps.settings.codex_path = "definitely-not-a-real-binary-exp".to_string();
        let mut req = action_request();
        req.options.agent = CodingAgent::Codex;

        match prepare(&PrepareRequest::Action(req), &deps).unwrap() {
            Prepared::Disabled(DisabledReason::DoctorFailed(check)) => {
                assert_eq!(check.tool, crate::doctor::Tool::Codex);
            }
            other => panic!("expected DoctorFailed, got {other:?}"),
        }
    }

    /// EXP-257: resolved input values land in the delivered prompt as the
    /// `## Inputs` section, between the preamble and the body.
    #[test]
    fn prepare_action_inputs_land_in_the_prompt() {
        let dir = temp_dir("action-inputs");
        let (base, _captured) = canned_server_recording(vec![(200, START_ACTION_OK.to_string())]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree: dir.0.join("unused"),
            seen: Default::default(),
        });
        let deps = make_deps(&base, &dir.0, worktrees);
        let mut req = action_request();
        req.inputs = vec![
            ActionInputValue {
                key: "scope".to_string(),
                label: "Scope".to_string(),
                input_type: "text".to_string(),
                value: "only urgent issues".to_string(),
                display: None,
            },
            ActionInputValue {
                key: "board".to_string(),
                label: "Board".to_string(),
                input_type: "board".to_string(),
                value: "board-uuid-1".to_string(),
                display: Some("Web".to_string()),
            },
        ];

        let prepared = match prepare(&PrepareRequest::Action(req), &deps).unwrap() {
            Prepared::Ready(prepared) => prepared,
            other => panic!("expected Ready, got {other:?}"),
        };
        let prompt = prepared.spawn.args.last().unwrap();
        assert!(prompt.contains("## Inputs"));
        assert!(prompt.contains("- Scope (text): only urgent issues"));
        assert!(prompt.contains("- Board (board): Web (`board-uuid-1`)"));
        // Body still verbatim after the divider.
        assert!(prompt.ends_with("---\n\n# Review\nScan the backlog."));
    }

    /// EXP-257: the BUILTIN "Create action" run — creator prompt from the
    /// description/repo input values, sanitized scratch cwd, repo IGNORED
    /// (never a clone/token call), and the session start carries teamId.
    #[test]
    fn prepare_action_builtin_renders_the_creator_prompt_in_scratch() {
        let dir = temp_dir("action-builtin");
        let (base, captured) = canned_server_recording(vec![(
            200,
            r#"{"result":{"data":{"session":{"id":"sess-c","issueId":null,"teamId":"ws-1","actionId":null,"actionName":"Create action","status":"running"}}}}"#
                .to_string(),
        )]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree: dir.0.join("unused"),
            seen: Default::default(),
        });
        let deps = make_deps(&base, &dir.0, worktrees.clone());
        let mut req = action_request();
        req.action_id = "builtin:create-action".to_string();
        req.action_name = "Create action".to_string();
        req.body = String::new();
        req.kind = ActionRunKind::CreateAction;
        // A repo group riding in must be IGNORED — the builtin always runs
        // in its scratch dir (the repo INPUT below is what Claude binds).
        req.repo = Some(RepoGroup {
            repository_id: "repo-1".to_string(),
            full_name: "acme/web".to_string(),
            default_branch: "main".to_string(),
        });
        req.inputs = vec![
            ActionInputValue {
                key: "description".to_string(),
                label: "Description".to_string(),
                input_type: "text".to_string(),
                value: "triage new widget feedback weekly".to_string(),
                display: None,
            },
            ActionInputValue {
                key: "repo".to_string(),
                label: "Repository".to_string(),
                input_type: "repo".to_string(),
                value: "repo-1".to_string(),
                display: Some("acme/web".to_string()),
            },
        ];

        let prepared = match prepare(&PrepareRequest::Action(req), &deps).unwrap() {
            Prepared::Ready(prepared) => prepared,
            other => panic!("expected Ready, got {other:?}"),
        };
        // Scratch cwd under actions/, colon sanitized; repo fully ignored
        // (no git call, no token, no repository to refresh).
        assert!(prepared.worktree.starts_with(dir.0.join("actions")));
        assert!(!prepared.worktree.to_string_lossy().contains(':'));
        assert_eq!(prepared.repository_id, None);
        assert!(worktrees.seen.lock().unwrap().is_empty(), "no git for the builtin");
        // The creator prompt, seeded from the input VALUES.
        let prompt = prepared.spawn.args.last().unwrap();
        assert!(prompt.contains("create ONE new action"));
        assert!(prompt.contains("ws-1"));
        assert!(prompt.contains("triage new widget feedback weekly"));
        assert!(prompt.contains("Set `repositoryId` to `repo-1` (acme/web)"));
        // Exactly one request — the session start with the builtin literal +
        // teamId (the server inserts actionId NULL + the constant name).
        let requests = captured.lock().unwrap();
        assert_eq!(requests.len(), 1, "{requests:?}");
        assert!(requests[0].contains(r#""actionId":"builtin:create-action""#));
        assert!(requests[0].contains(r#""teamId":"ws-1""#));
        assert_eq!(prepared.session_id, "sess-c");
    }

    /// EXP-257: a builtin run without its required description input is a
    /// hard error (the dialog/relay validated upstream — this is the guard).
    #[test]
    fn prepare_action_builtin_requires_the_description_input() {
        let dir = temp_dir("action-builtin-missing");
        let worktrees = Arc::new(FakeWorktrees {
            worktree: dir.0.join("unused"),
            seen: Default::default(),
        });
        let deps = make_deps("http://127.0.0.1:1", &dir.0, worktrees);
        let mut req = action_request();
        req.action_id = "builtin:create-action".to_string();
        req.kind = ActionRunKind::CreateAction;
        req.inputs = Vec::new();

        match prepare(&PrepareRequest::Action(req), &deps) {
            Err(CodingError::Io(message)) => assert!(message.contains("description")),
            other => panic!("expected the missing-description error, got {other:?}"),
        }
    }

    /// EXP-259: the fix-conflicts builtin is repo-backed by definition — a
    /// request without the PR's repo group is a hard error BEFORE any
    /// doctor/network work (the caller resolves it upstream).
    #[test]
    fn prepare_action_fix_conflicts_requires_the_repo() {
        let dir = temp_dir("action-fix-conflicts-repoless");
        let worktrees = Arc::new(FakeWorktrees {
            worktree: dir.0.join("unused"),
            seen: Default::default(),
        });
        let deps = make_deps("http://127.0.0.1:1", &dir.0, worktrees);
        let mut req = action_request();
        req.action_id = "builtin:fix-conflicts".to_string();
        req.action_name = "Fix merge conflicts".to_string();
        req.body = String::new();
        req.kind = ActionRunKind::FixConflicts {
            branch: "exp/EXP-42".to_string(),
            default_branch: "main".to_string(),
            identifier: "EXP-42".to_string(),
            issue_id: "issue-1".to_string(),
        };
        req.repo = None;

        match prepare(&PrepareRequest::Action(req), &deps) {
            Err(CodingError::Io(message)) => {
                assert!(message.contains("repository"), "{message}");
            }
            other => panic!("expected the missing-repo error, got {other:?}"),
        }
    }

    /// A chat request (EXP-615) — the hidden builtin's shape: no body, the
    /// two inputs the dialog fills, and the picked repository's group.
    fn chat_request(repository_id: &str) -> ActionLaunchRequest {
        let mut req = action_request();
        req.action_id = "builtin:chat".to_string();
        req.action_name = "Chat".to_string();
        req.body = String::new();
        req.kind = ActionRunKind::Chat;
        req.repo = Some(RepoGroup {
            repository_id: repository_id.to_string(),
            full_name: "acme/web".to_string(),
            default_branch: "main".to_string(),
        });
        req.inputs = vec![
            ActionInputValue {
                key: "prompt".to_string(),
                label: "Prompt".to_string(),
                input_type: "textarea".to_string(),
                value: "  where does the widget rate limit live?  ".to_string(),
                display: None,
            },
            ActionInputValue {
                key: "repo".to_string(),
                label: "Repository".to_string(),
                input_type: "repo".to_string(),
                value: repository_id.to_string(),
                display: Some("acme/web".to_string()),
            },
        ];
        req
    }

    /// EXP-615: chat is repo-BOUND — its `repo` input is required, and a
    /// scratch-dir chat would be a shell with no code in it. Refused before
    /// any doctor/network work.
    #[test]
    fn prepare_action_chat_requires_the_repo() {
        let dir = temp_dir("action-chat-repoless");
        let worktrees = Arc::new(FakeWorktrees {
            worktree: dir.0.join("unused"),
            seen: Default::default(),
        });
        let deps = make_deps("http://127.0.0.1:1", &dir.0, worktrees);
        let mut req = chat_request("repo-chat-none");
        req.repo = None;

        match prepare(&PrepareRequest::Action(req), &deps) {
            Err(CodingError::Io(message)) => {
                assert_eq!(message, "the chat run needs a repository");
            }
            other => panic!("expected the missing-repo error, got {other:?}"),
        }
    }

    /// EXP-615: the `prompt` input IS the chat's program — an empty one is a
    /// hard error, never a session spawned with nothing to say.
    #[test]
    fn prepare_action_chat_requires_the_prompt() {
        let dir = temp_dir("action-chat-promptless");
        let worktrees = Arc::new(FakeWorktrees {
            worktree: dir.0.join("unused"),
            seen: Default::default(),
        });
        let deps = make_deps("http://127.0.0.1:1", &dir.0, worktrees);
        let mut req = chat_request("repo-chat-empty");
        req.inputs.retain(|input| input.key != "prompt");

        match prepare(&PrepareRequest::Action(req), &deps) {
            Err(CodingError::Io(message)) => {
                assert_eq!(message, "the chat run is missing its prompt");
            }
            other => panic!("expected the missing-prompt error, got {other:?}"),
        }
    }

    /// EXP-615/EXP-637: the happy path — the run lands in its OWN worktree
    /// on `exp/chat-<run id>` (never the trunk clone), the user's words ride
    /// VERBATIM and LAST (no action preamble, no `## Inputs` section), and
    /// the tab is named after the repository, not after the
    /// always-identical action name.
    #[test]
    fn prepare_action_chat_runs_in_its_own_worktree() {
        let dir = temp_dir("action-chat-ok");
        let (base, captured) = canned_server_recording(vec![
            (200, TOKEN_OK.to_string()),
            (
                200,
                r#"{"result":{"data":{"session":{"id":"sess-chat","issueId":null,"teamId":"ws-1","actionId":null,"actionName":"Chat","status":"running"}}}}"#
                    .to_string(),
            ),
        ]);
        // Pre-seed the clone at the §7.1 layout path so `ensure_clone` reuses
        // it instead of cloning over the network (the autopull and the
        // excludes write are best-effort and no-op on it).
        let clone = dir.0.join("repos").join("acme").join("web");
        fs::create_dir_all(&clone).unwrap();
        for args in [
            &["init", "--quiet"][..],
            // `git_credentials::ensure` re-points origin at the bare URL —
            // the seeded clone needs the remote to exist for that.
            &["remote", "add", "origin", "https://github.com/acme/web.git"][..],
        ] {
            let status = std::process::Command::new("git")
                .args(args)
                .current_dir(&clone)
                .status()
                .unwrap();
            assert!(status.success(), "git {args:?}");
        }
        let run_worktree = dir.0.join("chat-worktree");
        fs::create_dir_all(&run_worktree).unwrap();
        let worktrees = Arc::new(FakeWorktrees {
            worktree: run_worktree.clone(),
            seen: Default::default(),
        });
        let deps = make_deps(&base, &dir.0, worktrees.clone());

        let mut req = chat_request("repo-chat-ok");
        req.run_id = "1a2b3c4d".to_string();
        let prepared = match prepare(&PrepareRequest::Action(req), &deps).unwrap() {
            Prepared::Ready(prepared) => prepared,
            other => panic!("expected Ready, got {other:?}"),
        };
        // EXP-637: its OWN worktree on `exp/chat-<id8>`, cut from the repo's
        // default branch — the trunk clone stays untouched (and autopull
        // keeps running on it).
        assert_eq!(prepared.worktree, run_worktree);
        assert_eq!(prepared.clone, clone);
        assert_eq!(prepared.branch, "exp/chat-1a2b3c4d");
        assert_eq!(prepared.base_branch.as_deref(), Some("main"));
        let seen = worktrees.seen.lock().unwrap().clone();
        assert_eq!(seen.len(), 1, "{seen:?}");
        assert_eq!(seen[0].0, "acme/web");
        assert_eq!(seen[0].2, "exp/chat-1a2b3c4d");
        // The clone's prune gate is held for the launch's whole flight.
        assert!(prepared.launch_hold.is_some());
        // A clean chat worktree cleans itself up when the run ends.
        let cleanup = prepared.run_cleanup.as_ref().expect("run cleanup");
        assert_eq!(cleanup.worktree, run_worktree);
        assert_eq!(cleanup.branch, "exp/chat-1a2b3c4d");
        assert_eq!(cleanup.base_branch, "main");
        assert_eq!(prepared.repository_id.as_deref(), Some("repo-chat-ok"));
        // The prompt is the user's own words, LAST and verbatim — only the
        // two-line workspace/close-out preamble precedes them.
        let prompt = prepared.spawn.args.last().unwrap();
        assert!(
            prompt.ends_with("---\n\nwhere does the widget rate limit live?"),
            "{prompt}"
        );
        assert!(prompt.starts_with("You work on branch `exp/chat-1a2b3c4d`"));
        assert!(prompt.contains("`exponential_sessions_end`"));
        // Named after the repo (`owner/repo` → `repo`).
        assert_eq!(prepared.tab_title, "chat · web");
        assert_eq!(prepared.tab_title_prefix, "web");
        // The session row is a BUILTIN row: the literal id + teamId, and the
        // heartbeat keeps re-sending the action scope.
        let requests = captured.lock().unwrap();
        assert!(requests
            .iter()
            .any(|request| request.contains(r#""actionId":"builtin:chat""#)
                && request.contains(r#""teamId":"ws-1""#)));
        assert_eq!(prepared.session_id, "sess-chat");
        assert_eq!(
            prepared.heartbeat_scope.action_id.as_deref(),
            Some("builtin:chat")
        );
        assert_eq!(prepared.heartbeat_scope.action_name.as_deref(), Some("Chat"));
    }

    /// EXP-637 (decision 1): a repo-backed TEAM action gets its own worktree
    /// on `exp/<slug>-<id8>` cut from the repo default — it never writes into
    /// the trunk clone any more — and the prompt tells it how work leaves
    /// that worktree (an issue-LESS PR via `repositoryId` + `head`).
    #[test]
    fn prepare_action_team_repo_backed_cuts_a_run_worktree() {
        let dir = temp_dir("action-run-worktree");
        let (base, _captured) = canned_server_recording(vec![
            (200, TOKEN_OK.to_string()),
            (200, START_ACTION_OK.to_string()),
        ]);
        let clone = dir.0.join("repos").join("acme").join("web");
        fs::create_dir_all(&clone).unwrap();
        for args in [
            &["init", "--quiet"][..],
            &["remote", "add", "origin", "https://github.com/acme/web.git"][..],
        ] {
            let status = std::process::Command::new("git")
                .args(args)
                .current_dir(&clone)
                .status()
                .unwrap();
            assert!(status.success(), "git {args:?}");
        }
        let run_worktree = dir.0.join("run-worktree");
        fs::create_dir_all(&run_worktree).unwrap();
        let worktrees = Arc::new(FakeWorktrees {
            worktree: run_worktree.clone(),
            seen: Default::default(),
        });
        let deps = make_deps(&base, &dir.0, worktrees.clone());

        let mut req = action_request();
        req.run_id = "1a2b3c4d".to_string();
        req.repo = Some(RepoGroup {
            repository_id: "repo-run-wt".to_string(),
            full_name: "acme/web".to_string(),
            default_branch: "main".to_string(),
        });
        let prepared = match prepare(&PrepareRequest::Action(req), &deps).unwrap() {
            Prepared::Ready(prepared) => prepared,
            other => panic!("expected Ready, got {other:?}"),
        };

        assert_eq!(prepared.worktree, run_worktree);
        assert_eq!(prepared.clone, clone);
        // The slug comes from the action NAME, lowercased and hyphenated.
        assert_eq!(prepared.branch, "exp/code-review-1a2b3c4d");
        assert_eq!(prepared.base_branch.as_deref(), Some("main"));
        let seen = worktrees.seen.lock().unwrap().clone();
        assert_eq!(seen.len(), 1, "{seen:?}");
        assert_eq!(seen[0].2, "exp/code-review-1a2b3c4d");
        assert!(prepared.launch_hold.is_some());
        assert!(prepared.run_cleanup.is_some());
        // The run branch rides the heartbeat scope so a swept row resurrects
        // still pointing at this worktree.
        assert_eq!(
            prepared.heartbeat_scope.branch.as_deref(),
            Some("exp/code-review-1a2b3c4d")
        );
        // The prompt's `## Workspace` section names the branch and the
        // issue-less pr_open shape (EXP-626).
        let prompt = prepared.spawn.args.last().unwrap();
        assert!(prompt.contains("## Workspace"), "{prompt}");
        assert!(prompt.contains("branch `exp/code-review-1a2b3c4d`"));
        assert!(prompt.contains("`repositoryId: \"repo-run-wt\"`"));
        assert!(prompt.contains("`exponential_sessions_end`"));
        // The run is recorded so it can be resumed after it ends.
        let record = crate::run_registry::get(&dir.0, "sess-a").expect("run record");
        assert_eq!(record.branch.as_deref(), Some("exp/code-review-1a2b3c4d"));
        assert_eq!(record.base_branch.as_deref(), Some("main"));
        assert_eq!(record.cwd, run_worktree);
        assert_eq!(record.clone.as_deref(), Some(clone.as_path()));
        assert_eq!(record.agent, CodingAgent::Claude);
        assert_eq!(record.kind, crate::run_registry::RunKind::Team);
    }

    // ---- EXP-637: resume ----

    /// A repo-LESS recorded run — the shortest resume path (no token, no
    /// git), so the tests below exercise identity + wire, not plumbing.
    fn resume_record(dir: &Path, session_id: &str) -> crate::run_registry::RunRecord {
        let cwd = dir.join("scratch");
        fs::create_dir_all(&cwd).unwrap();
        crate::run_registry::RunRecord {
            session_id: session_id.to_string(),
            account_id: "acct".to_string(),
            agent: CodingAgent::Claude,
            kind: crate::run_registry::RunKind::Team,
            action_id: "act-1".to_string(),
            action_name: "Code review".to_string(),
            team_id: "ws-1".to_string(),
            issue_id: None,
            issue_identifier: None,
            batch_id: None,
            issues: Vec::new(),
            cwd,
            clone: None,
            repo: None,
            repository_id: None,
            branch: None,
            base_branch: None,
            claude_session_id: Some("claude-1".to_string()),
            pi_session_file: None,
            codex_originator: None,
            inputs: Vec::new(),
            model: "fable".to_string(),
            effort: String::new(),
            ultracode: false,
            skip_permissions: false,
            fix: None,
            started_reason: Some("schedule".to_string()),
            resumed_from_id: None,
            recorded_at: crate::run_registry::now_secs(),
            extra: BTreeMap::new(),
        }
    }

    fn resume_request(record: crate::run_registry::RunRecord) -> ResumeRunRequest {
        ResumeRunRequest {
            record,
            device_label: "box".to_string(),
            origin: LaunchOrigin::Local,
            model: None,
            effort: None,
        }
    }

    /// The claude native path: the recorded transcript still exists under
    /// `~/.claude/projects/<munged cwd>/<id>.jsonl`, so the resume reopens
    /// THAT conversation (`--resume <id>`, no seed prompt, no `--session-id`).
    #[test]
    fn prepare_resume_run_reuses_the_recorded_claude_transcript() {
        let dir = temp_dir("resume-claude");
        let (base, captured) = canned_server_recording(vec![(
            200,
            r#"{"result":{"data":{"session":{"id":"sess-new","issueId":null,"teamId":"ws-1","actionId":"act-1","actionName":"Code review","status":"running"}}}}"#
                .to_string(),
        )]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree: dir.0.join("unused"),
            seen: Default::default(),
        });
        let mut deps = make_deps(&base, &dir.0, worktrees);
        let record = resume_record(&dir.0, "sess-old");
        // Seed the transcript claude would have written for this cwd.
        let projects = dir.0.join("claude-projects");
        let project_dir = projects.join(munge_claude_project_dir(&record.cwd));
        fs::create_dir_all(&project_dir).unwrap();
        fs::write(project_dir.join("claude-1.jsonl"), "{}\n").unwrap();
        deps.claude_projects_root = Some(projects);

        let prepared =
            match prepare(&PrepareRequest::ResumeRun(resume_request(record)), &deps).unwrap() {
                Prepared::Ready(prepared) => prepared,
                other => panic!("expected Ready, got {other:?}"),
            };
        let args = &prepared.spawn.args;
        let at = args.iter().position(|a| a == "--resume").expect("--resume");
        assert_eq!(args[at + 1], "claude-1");
        assert!(!args.iter().any(|a| a == "--session-id"), "{args:?}");
        assert_eq!(prepared.session_id, "sess-new");
        assert_eq!(prepared.tab_kind, TabKind::Action("act-1".to_string()));
        // The NEW row points back at the run it continues, and a resume is
        // never an automation.
        let requests = captured.lock().unwrap();
        assert!(
            requests
                .iter()
                .any(|r| r.contains(r#""resumedFromId":"sess-old""#)),
            "{requests:?}"
        );
        assert!(!requests.iter().any(|r| r.contains("startedReason")));
        drop(requests);
        // ... and the resumed run is recorded in its own right.
        let fresh = crate::run_registry::get(&dir.0, "sess-new").expect("record");
        assert_eq!(fresh.resumed_from_id.as_deref(), Some("sess-old"));
        assert_eq!(fresh.claude_session_id.as_deref(), Some("claude-1"));
        assert_eq!(fresh.started_reason, None);
    }

    /// Claude caps long project directory names and appends a hash, so the
    /// munged-cwd fast path misses; the probe then finds the transcript by
    /// its uuid anywhere under the projects root.
    #[test]
    fn prepare_resume_run_finds_the_transcript_under_a_hashed_project_dir() {
        let dir = temp_dir("resume-claude-hashed");
        let (base, _captured) = canned_server_recording(vec![(
            200,
            r#"{"result":{"data":{"session":{"id":"sess-new","issueId":null,"teamId":"ws-1","actionId":"act-1","actionName":"Code review","status":"running"}}}}"#
                .to_string(),
        )]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree: dir.0.join("unused"),
            seen: Default::default(),
        });
        let mut deps = make_deps(&base, &dir.0, worktrees);
        let record = resume_record(&dir.0, "sess-old");
        let projects = dir.0.join("claude-projects");
        let project_dir = projects.join("-some-truncated-prefix-nvsi4o");
        fs::create_dir_all(&project_dir).unwrap();
        fs::write(project_dir.join("claude-1.jsonl"), "{}\n").unwrap();
        // A stray FILE at the root and an unrelated dir must not confuse it.
        fs::write(projects.join("notes.txt"), "").unwrap();
        fs::create_dir_all(projects.join("-other-project")).unwrap();
        deps.claude_projects_root = Some(projects);

        let prepared =
            match prepare(&PrepareRequest::ResumeRun(resume_request(record)), &deps).unwrap() {
                Prepared::Ready(prepared) => prepared,
                other => panic!("expected Ready, got {other:?}"),
            };
        let args = &prepared.spawn.args;
        let at = args.iter().position(|a| a == "--resume").expect("--resume");
        assert_eq!(args[at + 1], "claude-1");
        let fresh = crate::run_registry::get(&dir.0, "sess-new").expect("record");
        assert_eq!(fresh.claude_session_id.as_deref(), Some("claude-1"));
    }

    /// No recoverable transcript: a FRESH session in the same workspace,
    /// seeded with the resume prompt (and a fresh `--session-id` pin).
    #[test]
    fn prepare_resume_run_seeds_the_resume_prompt_without_a_transcript() {
        let dir = temp_dir("resume-fallback");
        let (base, _captured) = canned_server_recording(vec![(
            200,
            r#"{"result":{"data":{"session":{"id":"sess-new2","issueId":null,"teamId":"ws-1","actionId":"act-1","actionName":"Code review","status":"running"}}}}"#
                .to_string(),
        )]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree: dir.0.join("unused"),
            seen: Default::default(),
        });
        let mut deps = make_deps(&base, &dir.0, worktrees);
        deps.claude_projects_root = Some(dir.0.join("empty-projects"));

        let prepared = match prepare(
            &PrepareRequest::ResumeRun(resume_request(resume_record(&dir.0, "sess-old2"))),
            &deps,
        )
        .unwrap()
        {
            Prepared::Ready(prepared) => prepared,
            other => panic!("expected Ready, got {other:?}"),
        };
        let args = &prepared.spawn.args;
        assert!(!args.iter().any(|a| a == "--resume"), "{args:?}");
        assert!(args.iter().any(|a| a == "--session-id"), "{args:?}");
        let prompt = args.last().unwrap();
        assert!(prompt.contains("RESUMING the run \"Code review\""), "{prompt}");
        assert!(prompt.contains("`exponential_sessions_end`"));
    }

    /// A workspace the prune (or the user) reclaimed has nothing to resume
    /// INTO — the launch must refuse, not spawn in a fabricated directory.
    #[test]
    fn prepare_resume_run_refuses_a_missing_workspace() {
        let dir = temp_dir("resume-gone");
        let (base, _captured) = canned_server_recording(vec![]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree: dir.0.join("unused"),
            seen: Default::default(),
        });
        let deps = make_deps(&base, &dir.0, worktrees);
        let mut record = resume_record(&dir.0, "sess-old3");
        record.cwd = dir.0.join("vanished");

        match prepare(&PrepareRequest::ResumeRun(resume_request(record)), &deps) {
            Err(CodingError::Io(message)) => {
                assert!(message.contains("workspace is gone"), "{message}");
            }
            other => panic!("expected the missing-workspace error, got {other:?}"),
        }
    }

    /// A fix-conflicts action request with the PR's repo group attached
    /// (EXP-324 tests). Unique repository ids per test — the token cache is
    /// process-global, and a shared id would swallow the mint request and
    /// misalign the canned-server sequence.
    fn fix_conflicts_request(repository_id: &str) -> ActionLaunchRequest {
        let mut req = action_request();
        req.action_id = "builtin:fix-conflicts".to_string();
        req.action_name = "Fix merge conflicts".to_string();
        req.body = String::new();
        req.kind = ActionRunKind::FixConflicts {
            branch: "exp/EXP-42".to_string(),
            default_branch: "main".to_string(),
            identifier: "EXP-42".to_string(),
            issue_id: "issue-fix-1".to_string(),
        };
        req.repo = Some(RepoGroup {
            repository_id: repository_id.to_string(),
            full_name: "acme/web".to_string(),
            default_branch: "main".to_string(),
        });
        req
    }

    /// EXP-324: the run rebases onto the PR's LIVE base and ends in a
    /// force-push + auto-merge — proceeding on a guessed base is the EXP-320
    /// bug, so a failed `issues.prepareConflictFix` (other than 404) is a
    /// hard, retryable error BEFORE any git work.
    #[test]
    fn prepare_action_fix_conflicts_hard_errors_when_base_resolution_fails() {
        let dir = temp_dir("action-fix-conflicts-base-502");
        let (base, captured) = canned_server_recording(vec![
            (200, TOKEN_OK.to_string()),
            (
                502,
                r#"{"error":{"message":"GitHub returned 500 for acme/web#241","code":-32603,"data":{"code":"BAD_GATEWAY","httpStatus":502}}}"#.to_string(),
            ),
        ]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree: dir.0.join("unused"),
            seen: Default::default(),
        });
        let deps = make_deps(&base, &dir.0, worktrees);

        match prepare(
            &PrepareRequest::Action(fix_conflicts_request("repo-fix-502")),
            &deps,
        ) {
            Err(CodingError::Io(message)) => {
                assert!(
                    message.contains("could not resolve the pull request's base branch"),
                    "{message}"
                );
            }
            other => panic!("expected the base-resolution error, got {other:?}"),
        }
        let requests = captured.lock().unwrap();
        assert!(requests[0].starts_with("POST /api/trpc/repositories.installationToken"));
        assert!(requests[1].starts_with("POST /api/trpc/issues.prepareConflictFix"));
        assert!(requests[1].ends_with(r#"{"issueId":"issue-fix-1"}"#));
        // Hard-stopped before any git work.
        assert!(!dir.0.join("repos").join("acme").exists());
    }

    /// EXP-324: an OLD server without `issues.prepareConflictFix` answers
    /// 404 — the launch degrades to the legacy default-branch rebase instead
    /// of failing. The pre-seeded clone (no `origin` remote) makes the flow
    /// die at the branch fetch — a GIT error, past the base resolution —
    /// without any network.
    #[test]
    fn prepare_action_fix_conflicts_falls_back_to_default_branch_on_404() {
        let dir = temp_dir("action-fix-conflicts-base-404");
        let (base, captured) = canned_server_recording(vec![
            (200, TOKEN_OK.to_string()),
            (
                404,
                r#"{"error":{"message":"No procedure found on path \"issues.prepareConflictFix\"","code":-32004,"data":{"code":"NOT_FOUND","httpStatus":404}}}"#.to_string(),
            ),
        ]);
        // Pre-seed the clone at the §7.1 layout path so ensure_clone reuses
        // it instead of cloning over the network.
        let clone = dir.0.join("repos").join("acme").join("web");
        fs::create_dir_all(&clone).unwrap();
        let git = |args: &[&str]| {
            let status = std::process::Command::new("git")
                .args(args)
                .current_dir(&clone)
                .status()
                .unwrap();
            assert!(status.success(), "git {args:?}");
        };
        git(&["init", "--quiet"]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree: dir.0.join("unused"),
            seen: Default::default(),
        });
        let deps = make_deps(&base, &dir.0, worktrees);

        let result = prepare(
            &PrepareRequest::Action(fix_conflicts_request("repo-fix-404")),
            &deps,
        );
        match result {
            // Died on git plumbing (the seeded clone has no `origin`
            // remote) — i.e. PAST the 404'd base resolution, on the legacy
            // path. The exact op doesn't matter; NOT being the
            // base-resolution Io error is the assertion.
            Err(CodingError::Git(_)) => {}
            other => panic!("expected a git error past the base resolution, got {other:?}"),
        }
        let requests = captured.lock().unwrap();
        assert!(requests[1].starts_with("POST /api/trpc/issues.prepareConflictFix"));
    }

    /// EXP-443: every fresh claude launch mints `--session-id <uuid>`; the
    /// exact-argv tests strip the (random) pair after asserting it is a
    /// valid UUID that matches the prepared field.
    fn strip_session_id(prepared: &PreparedLaunch) -> Vec<String> {
        let mut args = prepared.spawn.args.clone();
        let at = args
            .iter()
            .position(|arg| arg == "--session-id")
            .expect("--session-id present on a fresh claude launch");
        args.remove(at);
        let id = args.remove(at);
        uuid::Uuid::parse_str(&id).expect("a valid uuid");
        assert_eq!(prepared.claude_session_id.as_deref(), Some(id.as_str()));
        args
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
            strip_session_id(&prepared),
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
        // EXP-275: plan mode wins the starting mode, so no bypass posture.
        assert!(!prepared.bypass_permissions);

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

    /// EXP-662: `resume_prompt` is the DEGRADED half of resume — no record
    /// could be resolved for the issue, so a FRESH session runs in the reused
    /// worktree, seeded with the resume prompt (inspect the branch, continue,
    /// update the PR) instead of the first-launch one.
    #[test]
    fn prepare_issue_resume_prompt_seeds_the_resume_prompt() {
        let dir = temp_dir("issue-resume-prompt");
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
        req.resume_prompt = true;

        let prepared = match prepare(&PrepareRequest::Issue(req), &deps).unwrap() {
            Prepared::Ready(prepared) => prepared,
            other => panic!("expected Ready, got {other:?}"),
        };

        assert_eq!(
            prepared.spawn.args.last().unwrap(),
            &render_resume_prompt("EXP-42", "Fix login flicker", "main")
        );
        // The plan already happened in the work being picked back up (the
        // fixture request carries plan_mode: true).
        assert!(!prepared.spawn.args.iter().any(|arg| arg == "plan"));
        // EXP-662: no cwd-scoped native tail survives anywhere in this path.
        assert!(!prepared
            .spawn
            .args
            .iter()
            .any(|arg| arg == "--continue" || arg == "--resume"));
        // A fresh conversation, so a fresh pin — and nothing codex-shaped.
        assert!(prepared.spawn.args.iter().any(|arg| arg == "--session-id"));
        assert_eq!(prepared.codex_resume_id, None);
        // Stale-seed hygiene still holds (direct delivery removes it).
        assert!(!worktree.join(PROMPT_FILE).exists());
    }

    /// EXP-662: an issue session is recorded in `runs.json` exactly like an
    /// action run, so a later Resume relaunches THIS conversation.
    #[test]
    fn prepare_issue_records_a_run() {
        let dir = temp_dir("issue-record");
        let worktree = dir.0.join("wt");
        fs::create_dir_all(&worktree).unwrap();
        // A worktree git can't resolve still satisfies `resumable()`'s
        // `.git` check — the launcher's ignore guard degrades to a no-op.
        fs::write(worktree.join(".git"), "gitdir: /elsewhere").unwrap();
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

        let prepared = match prepare(&PrepareRequest::Issue(request("EXP-42")), &deps).unwrap() {
            Prepared::Ready(prepared) => prepared,
            other => panic!("expected Ready, got {other:?}"),
        };

        let record = crate::run_registry::get(&dir.0, "sess-1").expect("run record");
        assert_eq!(record.kind, RunKind::Issue);
        assert_eq!(record.issue_id.as_deref(), Some("issue-1"));
        assert_eq!(record.issue_identifier.as_deref(), Some("EXP-42"));
        assert_eq!(record.display_name(), "EXP-42");
        assert_eq!(record.batch_id, None);
        assert!(record.issues.is_empty());
        // A session has no action; the workspace pins are the resume's.
        assert_eq!(record.action_id, "");
        assert_eq!(record.action_name, "");
        assert_eq!(record.cwd, worktree);
        assert_eq!(
            record.clone.as_deref(),
            Some(dir.0.join("repos").join("acme").join("web").as_path())
        );
        assert_eq!(record.repo.as_deref(), Some("acme/web"));
        assert_eq!(record.repository_id.as_deref(), Some("repo-1"));
        assert_eq!(record.branch.as_deref(), Some("exp/EXP-42"));
        assert_eq!(record.base_branch.as_deref(), Some("main"));
        assert_eq!(record.claude_session_id, prepared.claude_session_id);
        assert_eq!(record.resumed_from_id, None);
        assert_eq!(record.started_reason, None);
        // ... and it is what a Resume on this issue resolves to.
        assert_eq!(
            crate::run_registry::latest_for_issue(&dir.0, "acct", "issue-1")
                .map(|record| record.session_id)
                .as_deref(),
            Some("sess-1")
        );
    }

    /// EXP-662: the batch shape records its whole issue list, so its resume
    /// can name itself (`EXP-42 +1`) without a sync store.
    #[test]
    fn prepare_batch_records_a_run() {
        let dir = temp_dir("batch-record");
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

        match prepare(&PrepareRequest::Batch(batch_request()), &deps).unwrap() {
            Prepared::Ready(prepared) => prepared,
            other => panic!("expected Ready, got {other:?}"),
        };

        let record = crate::run_registry::get(&dir.0, "sess-b").expect("run record");
        assert_eq!(record.kind, RunKind::Batch);
        assert_eq!(record.batch_id.as_deref(), Some("a1b2c3d4"));
        assert_eq!(record.issue_id, None);
        assert_eq!(
            record.issues,
            vec![
                RunIssue {
                    issue_id: "issue-1".to_string(),
                    identifier: "EXP-42".to_string(),
                },
                RunIssue {
                    issue_id: "issue-2".to_string(),
                    identifier: "EXP-43".to_string(),
                },
            ]
        );
        assert_eq!(record.display_name(), "EXP-42 +1");
        assert_eq!(record.team_id, "ws-1");
        assert_eq!(record.branch.as_deref(), Some("exp/batch-a1b2c3d4"));
        // A batch is never an issue-keyed resume candidate.
        assert_eq!(
            crate::run_registry::latest_for_issue(&dir.0, "acct", "issue-1"),
            None
        );
    }

    /// A repo-backed ISSUE record — unique repository ids per test (the
    /// token cache is process-global; a shared id could swallow the mint and
    /// misalign the canned-server sequence).
    fn issue_resume_record(
        dir: &Path,
        session_id: &str,
        repository_id: &str,
    ) -> crate::run_registry::RunRecord {
        let cwd = dir.join(format!("wt-{session_id}"));
        fs::create_dir_all(&cwd).unwrap();
        fs::write(cwd.join(".git"), "gitdir: /elsewhere").unwrap();
        crate::run_registry::RunRecord {
            kind: RunKind::Issue,
            action_id: String::new(),
            action_name: String::new(),
            issue_id: Some("issue-1".to_string()),
            issue_identifier: Some("EXP-42".to_string()),
            cwd,
            clone: Some(dir.join("repos").join("acme").join("web")),
            repo: Some("acme/web".to_string()),
            repository_id: Some(repository_id.to_string()),
            branch: Some("exp/EXP-42".to_string()),
            base_branch: Some("main".to_string()),
            started_reason: None,
            ..resume_record(dir, session_id)
        }
    }

    /// EXP-662: Resume on an ended ISSUE session relaunches the recorded
    /// conversation — a new issue-scoped row linked by `resumedFromId`, the
    /// exact `--resume <uuid>`, and NONE of the first-launch side effects
    /// (no status flip, no run cleanup on the session worktree).
    #[test]
    fn prepare_resume_run_relaunches_an_issue_record() {
        let dir = temp_dir("resume-issue");
        let (base, captured) = canned_server_recording(vec![
            (200, TOKEN_OK.to_string()),
            (
                200,
                r#"{"result":{"data":{"session":{"id":"sess-new","issueId":"issue-1","teamId":"ws-1","status":"running"}}}}"#
                    .to_string(),
            ),
        ]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree: dir.0.join("unused"),
            seen: Default::default(),
        });
        let mut deps = make_deps(&base, &dir.0, worktrees.clone());
        let record = issue_resume_record(&dir.0, "sess-old", "repo-resume-issue");
        // The transcript claude recorded for this worktree.
        let projects = dir.0.join("claude-projects");
        let project_dir = projects.join(munge_claude_project_dir(&record.cwd));
        fs::create_dir_all(&project_dir).unwrap();
        fs::write(project_dir.join("claude-1.jsonl"), "{}\n").unwrap();
        deps.claude_projects_root = Some(projects);
        let cwd = record.cwd.clone();

        let prepared = match prepare(
            &PrepareRequest::ResumeRun(resume_request(record)),
            &deps,
        )
        .unwrap()
        {
            Prepared::Ready(prepared) => prepared,
            other => panic!("expected Ready, got {other:?}"),
        };

        let args = &prepared.spawn.args;
        let at = args.iter().position(|a| a == "--resume").expect("--resume");
        assert_eq!(args[at + 1], "claude-1");
        assert!(!args.iter().any(|a| a == "--session-id"), "{args:?}");
        assert_eq!(prepared.spawn.cwd.as_deref(), Some(cwd.as_path()));
        // An issue-shaped session, indistinguishable from a fresh launch.
        assert_eq!(prepared.session_id, "sess-new");
        assert_eq!(prepared.issue_identifier, "EXP-42");
        assert_eq!(prepared.tab_title, "claude · EXP-42");
        assert_eq!(prepared.tab_kind, TabKind::Claude);
        assert_eq!(prepared.branch, "exp/EXP-42");
        assert_eq!(
            prepared.heartbeat_scope.issue_id.as_deref(),
            Some("issue-1")
        );
        assert_eq!(prepared.heartbeat_scope.team_id, None);
        assert_eq!(prepared.heartbeat_scope.action_id, None);
        // The server refuses a branch beside an issueId.
        assert_eq!(prepared.heartbeat_scope.branch, None);
        // A session worktree is the prune's, never the run cleanup's.
        assert!(prepared.run_cleanup.is_none());
        assert!(prepared.launch_hold.is_some());
        // Step 2 went through the injected provider (D6), on the recorded
        // branch and base.
        let seen = worktrees.seen.lock().unwrap().clone();
        assert_eq!(
            seen.as_slice(),
            &[(
                "acme/web".to_string(),
                "main".to_string(),
                "exp/EXP-42".to_string(),
                Some("2026-07-03T12:55:00.000Z".to_string())
            )]
        );

        let requests = captured.lock().unwrap();
        assert!(
            requests.iter().any(|r| r.contains(r#""issueId":"issue-1""#)
                && r.contains(r#""resumedFromId":"sess-old""#)),
            "{requests:?}"
        );
        // EXP-662 D3: resuming a done issue must not reopen it.
        assert!(
            !requests
                .iter()
                .any(|r| r.starts_with("POST /api/trpc/issues.update")),
            "{requests:?}"
        );
        drop(requests);

        // The resumed session chains onto the one it continues.
        let fresh = crate::run_registry::get(&dir.0, "sess-new").expect("record");
        assert_eq!(fresh.kind, RunKind::Issue);
        assert_eq!(fresh.issue_id.as_deref(), Some("issue-1"));
        assert_eq!(fresh.resumed_from_id.as_deref(), Some("sess-old"));
        assert_eq!(fresh.claude_session_id.as_deref(), Some("claude-1"));
    }

    /// The issue fallback: the recorded transcript is gone, so the resume
    /// seeds the ISSUE-shaped resume prompt (PR contract + comment thread),
    /// not the run-shaped one.
    #[test]
    fn prepare_resume_run_issue_seeds_the_issue_resume_prompt() {
        let dir = temp_dir("resume-issue-fallback");
        let (base, _captured) = canned_server_recording(vec![
            (200, TOKEN_OK.to_string()),
            (
                200,
                r#"{"result":{"data":{"session":{"id":"sess-new","issueId":"issue-1","teamId":"ws-1","status":"running"}}}}"#
                    .to_string(),
            ),
        ]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree: dir.0.join("unused"),
            seen: Default::default(),
        });
        let mut deps = make_deps(&base, &dir.0, worktrees);
        deps.claude_projects_root = Some(dir.0.join("empty-projects"));

        let prepared = match prepare(
            &PrepareRequest::ResumeRun(resume_request(issue_resume_record(
                &dir.0,
                "sess-old",
                "repo-resume-issue-fallback",
            ))),
            &deps,
        )
        .unwrap()
        {
            Prepared::Ready(prepared) => prepared,
            other => panic!("expected Ready, got {other:?}"),
        };

        let args = &prepared.spawn.args;
        assert!(!args.iter().any(|a| a == "--resume"), "{args:?}");
        assert!(args.iter().any(|a| a == "--session-id"), "{args:?}");
        assert_eq!(
            args.last().unwrap(),
            &render_resume_prompt("EXP-42", "Fix login flicker", "main")
        );
    }

    /// EXP-662: the batch shape resumes the same way — one team-scoped row
    /// on the recorded batch branch, named after its issue list.
    #[test]
    fn prepare_resume_run_relaunches_a_batch_record() {
        let dir = temp_dir("resume-batch");
        let (base, captured) = canned_server_recording(vec![
            (200, TOKEN_OK.to_string()),
            (200, START_BATCH_OK.to_string()),
        ]);
        let worktrees = Arc::new(FakeWorktrees {
            worktree: dir.0.join("unused"),
            seen: Default::default(),
        });
        let mut deps = make_deps(&base, &dir.0, worktrees.clone());
        let mut record = issue_resume_record(&dir.0, "sess-old-batch", "repo-resume-batch");
        record.kind = RunKind::Batch;
        record.issue_id = None;
        record.issue_identifier = None;
        record.batch_id = Some("a1b2c3d4".to_string());
        record.issues = vec![
            RunIssue {
                issue_id: "issue-1".to_string(),
                identifier: "EXP-42".to_string(),
            },
            RunIssue {
                issue_id: "issue-2".to_string(),
                identifier: "EXP-43".to_string(),
            },
        ];
        record.branch = Some("exp/batch-a1b2c3d4".to_string());
        let projects = dir.0.join("claude-projects");
        let project_dir = projects.join(munge_claude_project_dir(&record.cwd));
        fs::create_dir_all(&project_dir).unwrap();
        fs::write(project_dir.join("claude-1.jsonl"), "{}\n").unwrap();
        deps.claude_projects_root = Some(projects);

        let prepared = match prepare(
            &PrepareRequest::ResumeRun(resume_request(record)),
            &deps,
        )
        .unwrap()
        {
            Prepared::Ready(prepared) => prepared,
            other => panic!("expected Ready, got {other:?}"),
        };

        assert_eq!(prepared.session_id, "sess-b");
        assert_eq!(prepared.issue_identifier, "batch-a1b2c3d4");
        assert_eq!(prepared.tab_title, "claude · EXP-42 +1");
        assert_eq!(prepared.tab_kind, TabKind::Claude);
        assert_eq!(prepared.branch, "exp/batch-a1b2c3d4");
        assert_eq!(prepared.heartbeat_scope.issue_id, None);
        assert_eq!(prepared.heartbeat_scope.team_id.as_deref(), Some("ws-1"));
        assert!(prepared.run_cleanup.is_none());
        let seen = worktrees.seen.lock().unwrap().clone();
        assert_eq!(seen.len(), 1, "{seen:?}");
        assert_eq!(seen[0].2, "exp/batch-a1b2c3d4");

        let requests = captured.lock().unwrap();
        assert!(
            requests.iter().any(|r| r.contains(r#""teamId":"ws-1""#)
                && r.contains(r#""resumedFromId":"sess-old-batch""#)),
            "{requests:?}"
        );
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
                    strip_session_id(&prepared)[..8],
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
        // Prompt positional-last, model/effort flags present, the EXP-389
        // update-prompt suppression leading.
        assert_eq!(
            args[..4],
            [
                "-c".to_string(),
                "check_for_update_on_startup=false".to_string(),
                "-m".to_string(),
                "gpt-5.6-sol".to_string()
            ]
        );
        assert!(args.contains(&"model_reasoning_effort=\"high\"".to_string()));
        assert!(args.last().unwrap().contains("EXP-42"));
        // EXP-443: no claude session id, but the per-session originator is
        // minted and stamped into the spawn env for rollout discovery.
        assert_eq!(prepared.claude_session_id, None);
        assert!(!args.iter().any(|arg| arg == "--session-id"));
        let originator = crate::argv::codex_session_originator("sess-1");
        assert_eq!(prepared.codex_originator.as_deref(), Some(originator.as_str()));
        assert_eq!(prepared.codex_resume_id, None);
        assert!(prepared
            .spawn
            .env
            .contains(&(crate::argv::CODEX_ORIGINATOR_ENV.to_string(), originator)));
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
        // EXP-409: the pi auth gate checks credential presence (auth.json or
        // a provider env key) — CI runners have neither, so satisfy the real
        // probe the way a real pi setup would.
        std::env::set_var("ANTHROPIC_API_KEY", "test-pi-credential");
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
            args[..4],
            [
                "--model".to_string(),
                "grok-4.5".to_string(),
                "--thinking".to_string(),
                "high".to_string(),
            ]
        );
        // EXP-637: every pi run records into its own transcript file, so a
        // later resume can name it exactly — before the `-e` extension loads.
        let session_at = args.iter().position(|a| a == "--session").expect("--session");
        assert!(args[session_at + 1].ends_with("pi-sessions/sess-1.jsonl"), "{args:?}");
        let bridge_at = args
            .windows(2)
            .position(|w| w == ["-e", "./.exp-pi-mcp.ts"])
            .expect("bridge");
        assert!(session_at < bridge_at, "{args:?}");
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

    /// EXP-383: a pi launch with an observer sidecar wired gets the
    /// EXP_OBSERVER_* env, the second `-e` extension, and the observer file
    /// on disk (git-excluded like the bridge). Without a sidecar the file
    /// still lands but the env stays absent (inert extension).
    #[test]
    fn pi_launch_wires_the_observer_sidecar() {
        let dir = temp_dir("pi-observer");
        let base = canned_server(vec![
            (200, FOR_ISSUE_OK.to_string()),
            (200, TOKEN_OK.to_string()),
            (200, START_OK.to_string()),
            // The second (no-sidecar) prepare replays the same sequence.
            (200, FOR_ISSUE_OK.to_string()),
            (200, TOKEN_OK.to_string()),
            (200, START_OK.to_string()),
        ]);
        let worktree = dir.0.join("wt");
        fs::create_dir_all(&worktree).unwrap();
        let worktrees = Arc::new(FakeWorktrees {
            worktree: worktree.clone(),
            seen: Default::default(),
        });
        let mut deps = make_deps(&base, &dir.0, worktrees);
        deps.settings.pi_path = "git".to_string(); // runnable stub
        // EXP-409: the pi auth gate checks credential presence (auth.json or
        // a provider env key) — CI runners have neither, so satisfy the real
        // probe the way a real pi setup would.
        std::env::set_var("ANTHROPIC_API_KEY", "test-pi-credential");
        let mut req = request("EXP-42");
        req.options.agent = CodingAgent::Pi;

        let observer = ObserverSetup {
            port: 45678,
            token: "obs-token".to_string(),
        };
        let prepared = match prepare_with_hooks(
            &PrepareRequest::Issue(req),
            &deps,
            None,
            Some(&observer),
        )
        .unwrap()
        {
            Prepared::Ready(prepared) => prepared,
            other => panic!("expected Ready, got {other:?}"),
        };

        // All three extensions ride argv; the observer + plan files are on
        // disk and secret-free.
        let args = &prepared.spawn.args;
        let extensions: Vec<&str> = args
            .iter()
            .enumerate()
            .filter(|(_, arg)| *arg == "-e")
            .filter_map(|(i, _)| args.get(i + 1).map(String::as_str))
            .collect();
        assert_eq!(
            extensions,
            ["./.exp-pi-mcp.ts", "./.exp-pi-observer.ts", "./.exp-pi-plan.ts"]
        );
        let observer_source = fs::read_to_string(worktree.join(".exp-pi-observer.ts")).unwrap();
        assert!(!observer_source.contains("expu_"));
        let plan_source = fs::read_to_string(worktree.join(".exp-pi-plan.ts")).unwrap();
        assert!(!plan_source.contains("expu_"));
        for (key, value) in [
            ("EXP_OBSERVER_URL", "http://127.0.0.1:45678".to_string()),
            ("EXP_OBSERVER_TOKEN", "obs-token".to_string()),
            // The request fixture launches with plan mode ON — the pi plan
            // gate env must be set (EXP-441).
            ("EXP_PI_PLAN_MODE", "1".to_string()),
        ] {
            assert!(
                prepared.spawn.env.contains(&(key.to_string(), value.clone())),
                "missing env {key}={value}: {:?}",
                prepared.spawn.env
            );
        }

        // No sidecar ⇒ no env (the extension returns immediately); plan mode
        // OFF ⇒ no EXP_PI_PLAN_MODE either (the plan extension stays inert).
        let mut req = request("EXP-43");
        req.options.agent = CodingAgent::Pi;
        req.options.plan_mode = false;
        let prepared = match prepare(&PrepareRequest::Issue(req), &deps).unwrap() {
            Prepared::Ready(prepared) => prepared,
            other => panic!("expected Ready, got {other:?}"),
        };
        assert!(
            !prepared
                .spawn
                .env
                .iter()
                .any(|(key, _)| key.starts_with("EXP_OBSERVER_") || key == "EXP_PI_PLAN_MODE"),
            "{:?}",
            prepared.spawn.env
        );
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
                    Some("codex not found on PATH. Set an absolute path.")
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
        let args = strip_session_id(&prepared);
        assert_eq!(
            args[..4],
            [
                "--model".to_string(),
                "opus".to_string(),
                "--effort".to_string(),
                "ultracode".to_string(),
            ]
        );
        assert!(!args.iter().any(|arg| arg == "--agents"));
        assert_eq!(
            args[4..7],
            [
                "--mcp-config".to_string(),
                ".exp-mcp.json".to_string(),
                "--strict-mcp-config".to_string(),
            ]
        );
        assert_eq!(args[7], "--dangerously-skip-permissions");
        // EXP-275: the emitter's permission posture mirrors the argv.
        assert!(prepared.bypass_permissions);
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

    fn agent_shell_request(cwd_override: Option<PathBuf>) -> AgentShellRequest {
        AgentShellRequest {
            options: LaunchOptions {
                agent: CodingAgent::Claude,
                model: "fable".to_string(),
                effort: String::new(),
                ultracode: false,
                plan_mode: false,
                skip_permissions: false,
            },
            repository_id: "repo-1".to_string(),
            full_name: "acme/web".to_string(),
            cwd_override,
        }
    }

    /// EXP-369: a pinned worktree becomes the agent's cwd (and with it the
    /// `.exp-mcp.json` / pi-bridge target); without one the trunk clone is.
    #[test]
    fn agent_shell_runs_in_the_pinned_worktree_when_given_one() {
        let clone = PathBuf::from("/repos/acme/web");
        let worktree = PathBuf::from("/repos/acme/web.worktrees/exp-EXP-42");

        assert_eq!(agent_shell_cwd(&agent_shell_request(None), &clone), clone);
        assert_eq!(
            agent_shell_cwd(&agent_shell_request(Some(worktree.clone())), &clone),
            worktree
        );
    }

    /// The tab title names the worktree for a pinned run (every worktree of a
    /// repo would otherwise render the same chip) and the repo otherwise.
    #[test]
    fn agent_shell_tab_title_names_the_worktree_then_the_repo() {
        let clone = PathBuf::from("/repos/acme/web");
        let worktree = PathBuf::from("/repos/acme/web.worktrees/exp-EXP-42");
        assert_eq!(
            agent_shell_tab_title(CodingAgent::Claude, &agent_shell_request(None), &clone),
            "claude · web"
        );
        assert_eq!(
            agent_shell_tab_title(
                CodingAgent::Codex,
                &agent_shell_request(Some(worktree.clone())),
                &worktree
            ),
            "codex · exp-EXP-42"
        );
    }
}
