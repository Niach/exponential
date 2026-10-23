//! The Start-coding launcher (masterplan-v3 §7.1, DC-1) — ONE prepare
//! sequence for every session ([`PrepareRequest::Session`]): a thread
//! session, a team agent's channel, a child an agent started. A local start
//! and a remote one are the same thing now (EXP-1292, Threads v2): a
//! `queued` `sessions` row naming this device, picked up off the shape by
//! the host ([`crate::pickup`]), which CLAIMS it (`sessions.claim`) and
//! fetches its `sessions.brief` BEFORE calling [`prepare`]. The launcher
//! never creates a server row — the row exists before the device sees it.
//!
//! Split to match gpui's threading model while keeping one code path:
//!
//! 1. [`prepare`] — doctor → workspace (worktree on the session's branch, or
//!    a scratch dir for a repo-less session) → `sessions.report{branch}` →
//!    MCP wiring → the brief as the seed prompt. **Blocking network and git
//!    I/O, gpui-free** — run it on the background executor. Returns either a
//!    [`PreparedLaunch`] or a [`DisabledReason`] (never falsely block,
//!    always explain — none of these are errors/panics).
//! 2. The host hands the [`PreparedLaunch`] to the ACP engine
//!    (`engine::start`), which owns the session from there: its turns, its
//!    `sessions.report` state and its end.
//!
//! The launcher never touches PTYs and never talks to the steer relay (§3.1:
//! `coding` does not depend on `steer`).

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use api::error::ApiError;
use api::session_device::{self, ReportStatus, SessionBrief, SessionReport};
use api::token_store::TokenStore;
use api::trpc::TrpcClient;
use api::users;
use terminal::pty::SpawnSpec;
use terminal::tab::TabId;

use crate::agent::CodingAgent;
use crate::argv::{
    shell_args, AgentMcp, LaunchOptions, McpServerWire,
    CLAUDE_MCP_TOOL_TIMEOUT_ENV, CLAUDE_MCP_TOOL_TIMEOUT_MS,
    MCP_SESSION_ID_ENV, MCP_TOKEN_ENV,
};
use crate::context_layout::{self, CarriedBase, ContextLayers};
use crate::doctor::{run_doctor, ToolCheck};
use crate::git_credentials;
use crate::git_worktree::{
    branch_name, carried_branch, clone_path, create_worktree_with, ensure_clone, fetch_base,
    seed_empty_remote, shared_cargo_target_dir, GitError, Parked, TokenUrl,
};
use crate::mcp_json::write_mcp_json;
use crate::mcp_servers::ResolvedMcp;
use crate::run_cleanup::RunCleanup;
use crate::run_registry::{launch_extra, RunKind, RunRecord};
use crate::settings::Settings;
use domain::rows::SessionRow;

/// EXP-511: the per-worktree scratch dir the steer publisher downloads a
/// steered message's image attachments into, so the agent reads a FILE instead
/// of an auth-gated URL. Wave D: a steered FILE (any type) lands here too, as
/// `<id>/<filename>`; the name stays because existing worktrees' git excludes
/// name it. `coding` never talks to the relay (§3.1) — it only
/// owns the name and the git exclusion; the hosts (`ui`/`cli`) hand the path to
/// the publisher.
pub const STEER_IMAGES_DIR: &str = ".exp-steer-images";

/// The launcher's never-committed seed files + scratch dirs, appended to the
/// clone's shared `.git/info/exclude`. Best-effort coverage only: the
/// secret-carrying `.exp-mcp.json` has its own HARD, verified guard at the
/// write site ([`wire_agent_mcp`] via [`crate::git_worktree::ensure_ignored`],
/// EXP-474).
const LOCAL_EXCLUDES: &[&str] = &[
    crate::mcp_json::MCP_JSON_FILE,
    crate::worktree_agents::AGENTS_FILE,
    STEER_IMAGES_DIR,
    // FEED-22: Claude Code's `isolation: "worktree"` nests agent worktrees
    // inside the checkout — hidden clone-wide so they never dirty the trunk.
    crate::git_worktree::CLAUDE_WORKTREES_DIR,
];

/// EXP-1005 — the start-time account pick for one launch: rewrite
/// `options.account` to the profile with the most headroom when the device
/// rotates accounts. Off the usage cache only (never a probe: a launch must
/// not wait on a fetch), so a machine that has never read its logins' usage
/// keeps the launch's own account.
fn apply_start_pick(
    options: &mut LaunchOptions,
    deps: &CodingDeps,
    check: &ToolCheck,
) -> Option<crate::account_rotation::StartPick> {
    // Settle the profile first, so the rotation below judges a login that
    // exists (an unlisted launch account is never moved).
    resolve_launch_account(options, deps, check);
    let now_ms = chrono::Utc::now().timestamp_millis();
    // FEED-61: the runs already live on each login count against it, so
    // concurrent starts spread instead of sharing one 5h window.
    let profiles = crate::account_rotation::weigh_live_runs(
        crate::agent_usage::profile_usage_snapshot(options.agent, &deps.data_dir),
        &crate::run_registry::live_runs_per_account(&deps.data_dir, options.agent),
        now_ms,
    );
    let model = Some(options.model.as_str()).filter(|model| !model.is_empty());
    // EXP-1107 (owner decision): the last used login is a pin too, so every
    // resolved account rides as named.
    crate::account_rotation::apply_start_pick(
        &mut options.account,
        true,
        &profiles,
        deps.settings.auto_rotate_accounts,
        options.agent,
        model,
        now_ms,
    )
}

/// EXP-1138 / EXP-1158 — settle `options.account` on the login the launch
/// runs on ([`crate::agent_profiles::resolve_launch_account`]) and say
/// whether the caller NAMED it. An unnamed launch's last used profile is
/// judged by the doctor's account gate here, so a signed-out one is skipped
/// rather than refused; `None` = no profile can take it (the gate refuses).
fn resolve_launch_account(options: &mut LaunchOptions, deps: &CodingDeps, check: &ToolCheck) -> bool {
    let agent = options.agent;
    let resolved = crate::agent_profiles::resolve_launch_account(
        &deps.data_dir,
        agent,
        options.account.take(),
        |profile| profile_signed_out(deps, agent, Some(check), profile),
    );
    options.account = resolved.account;
    resolved.named
}

/// Whether the named `profile` of `agent` is PROVABLY signed out — the
/// probe [`crate::doctor::DoctorReport::account_failure`] refuses on (an
/// unreadable answer fails open, like the gate). `check` = the doctor's,
/// when at hand: the profile it found signed in needs no second probe.
fn profile_signed_out(
    deps: &CodingDeps,
    agent: CodingAgent,
    check: Option<&ToolCheck>,
    profile: &str,
) -> bool {
    if check.and_then(|check| check.signed_in_profile.as_deref()) == Some(profile) {
        return false;
    }
    let Some(dir) = crate::agent_profiles::profile_dir(&deps.data_dir, agent, profile) else {
        return false;
    };
    crate::doctor::probe_profile_signed_in(
        agent,
        &deps.settings.resolved_path_for(agent),
        &terminal::pty::login_path(),
        &dir,
    ) == Some(false)
}

fn note_start_pick(pick: Option<&crate::account_rotation::StartPick>) {
    if let Some(pick) = pick {
        log::info!(
            "coding: account rotation at start — {} ({} → {})",
            pick.message,
            pick.from,
            pick.to
        );
    }
}

/// EXP-1005 — the pick's line, prefixed to the run's seed prompt: the hop
/// is visible in the transcript, and the agent reads that nothing about the
/// task changed.
fn with_start_note(prompt: String, pick: Option<&crate::account_rotation::StartPick>) -> String {
    match pick {
        Some(pick) => format!("{}\n\n{prompt}", pick.run_note()),
        None => prompt,
    }
}

/// The machine's hostname — §7.1's `device_label` (also the server-side
/// device label). Env vars first (cheap), then the
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

/// EXP-1292 — the branch of a session with no single linked issue:
/// `exp/session-<first 8 of its id>`, lowercased so the server's
/// issue-identifier parse (an UPPERCASE `[A-Z0-9]+-\d+` tail) can never link
/// it to an issue.
pub const SESSION_BRANCH_PREFIX: &str = "exp/session-";

/// EXP-1292 — the branch rule: the row's own `branch` when it already has
/// one (a resume, a reclaimed row); else the session that OWNS the root's
/// issue work ([`owns_issue_work`]) with exactly one linked issue takes the
/// issue's (`<prefix><IDENTIFIER>`, the PR-linking convention); else
/// `exp/session-<id8>` ([`session_own_branch`]). `parent_branch` = the
/// brief's (a child of a coding parent works under that parent's branch, so
/// the issue branch is the parent's, never its own).
pub fn session_branch(
    row: &SessionRow,
    issue_identifiers: &[String],
    branch_prefix: &str,
    parent_branch: Option<&str>,
) -> String {
    if let Some(branch) = row.branch.as_deref().map(str::trim).filter(|b| !b.is_empty()) {
        return branch.to_string();
    }
    if let ([identifier], true) = (issue_identifiers, owns_issue_work(row, parent_branch)) {
        return branch_name(branch_prefix, identifier);
    }
    session_own_branch(row)
}

/// Whether this session carries its ROOT's issue work (its issue branch,
/// its run record's issues): every root (NEW-1: a root works itself), or a
/// child whose brief names no `parent_branch`. A child of a coding parent
/// never does (that parent holds the issue branch).
pub fn owns_issue_work(row: &SessionRow, parent_branch: Option<&str>) -> bool {
    !is_child_session(row) || parent_branch.map(str::trim).is_none_or(str::is_empty)
}

/// Whether the session runs the agents part of the playbook
/// ([`crate::skill::Extras::dispatch`]): exactly an agent's conversation
/// session (the persistent kind). NEW-1: the row's legacy `dispatch` stamp
/// is a read shim and never decides anything.
pub fn dispatches(row: &SessionRow) -> bool {
    row.is_persistent()
}

/// A trimmed, non-blank text field.
fn named_text(text: &Option<String>) -> Option<String> {
    text.as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

/// EXP-1292 — whether the row is a sub-session (it has a parent).
pub fn is_child_session(row: &SessionRow) -> bool {
    row.parent_id.as_deref().is_some_and(|parent| !parent.trim().is_empty())
}

/// EXP-1292 — the session's OWN branch, `exp/session-<id8>`
/// ([`SESSION_BRANCH_PREFIX`]): nobody else's, so the session owns (and may
/// reclaim) its worktree.
pub fn session_own_branch(row: &SessionRow) -> String {
    format!("{SESSION_BRANCH_PREFIX}{}", crate::run_cleanup::session_id8(&row.id))
}

/// EXP-1292/1341 — what a session is CALLED on this device (tab title, the
/// computer-use cursor badge, logs): THE session title
/// (`domain::session_title`) over what the launch knows — the row's stored
/// title, its issues (`issue_identifiers`, titled by the brief's
/// `## Issues` lines: a child's brief lists only its ASSIGNED issues), the
/// brief's `## Request` first line — else `Untitled`. Never a
/// generic `Session`.
pub fn session_label(row: &SessionRow, issue_identifiers: &[String], brief: &str) -> String {
    use domain::session_title::{title_from_parts, TitleIssue};
    let titled = crate::pickup::brief_issues(brief);
    let issues: Vec<TitleIssue<'_>> = issue_identifiers
        .iter()
        .map(|identifier| TitleIssue {
            identifier,
            title: titled
                .iter()
                .find(|(id, _)| id == identifier)
                .map_or("", |(_, title)| title.as_str()),
        })
        .collect();
    let label = title_from_parts(row.title.as_deref(), &issues, brief_request(brief));
    // An issue the brief did not title reads `#EXP-42` / `#EXP-42 + 2`.
    if issues.first().is_some_and(|issue| issue.title.trim().is_empty()) && label.starts_with('#') {
        return label.split_whitespace().collect::<Vec<_>>().join(" ");
    }
    label
}

/// The brief's `## Request` body (always its FIRST section), `None` when the
/// brief opens with another one.
fn brief_request(brief: &str) -> Option<&str> {
    brief.strip_prefix("## Request\n\n")
}

/// EXP-1292 — the session's runner pickers layered OVER the host's launch
/// defaults (`base`): the row's `cli` picks the agent (a different agent than
/// the defaults' starts from that agent's own settings defaults), then its
/// model/effort (normalized against the agent's closed sets; blank keeps the
/// default), plan mode, ultracode, computer use and account. Capabilities
/// mask everything (codex never plans, only claude runs ultracode).
///
/// Every launch leaves here with an explicit model.
pub fn session_launch_options(row: &SessionRow, base: &LaunchOptions, settings: &Settings) -> LaunchOptions {
    let mut options = runner_options(row, base, settings);
    let agent = options.agent;
    options.model = explicit_model(agent, &options.model, settings);
    if agent.supports_subagent_model() {
        options.subagent_model = crate::settings::normalize_choice(
            &options.subagent_model,
            agent.model_values(),
            settings.subagent_model_for(agent),
        );
    }
    options
}

/// `model` inside `agent`'s closed set, else this machine's default model
/// for it: no launch omits the model flag.
fn explicit_model(agent: CodingAgent, model: &str, settings: &Settings) -> String {
    let fallback = Some(settings.model_for(agent))
        .filter(|model| agent.model_values().contains(model))
        .unwrap_or(agent.default_model());
    crate::settings::normalize_choice(model, agent.model_values(), fallback)
}

fn runner_options(row: &SessionRow, base: &LaunchOptions, settings: &Settings) -> LaunchOptions {
    use crate::settings::normalize_choice;
    let agent = row
        .cli
        .as_deref()
        .and_then(CodingAgent::parse)
        .unwrap_or(base.agent);
    let mut options = if agent == base.agent {
        base.clone()
    } else {
        LaunchOptions::defaults_for(settings, agent)
    };
    let picked = |value: &Option<String>| {
        value
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    if let Some(model) = picked(&row.model) {
        let normalized = normalize_choice(&model, agent.model_values(), "");
        if !normalized.is_empty() {
            options.model = normalized;
        }
    }
    if let Some(effort) = picked(&row.effort) {
        let normalized = normalize_choice(&effort, agent.effort_values(), "");
        if !normalized.is_empty() {
            options.effort = normalized;
        }
    }
    options.plan_mode = row.plan_mode;
    options.ultracode = row.ultracode && agent.supports_ultracode();
    options.computer_use = Some(row.computer_use);
    if let Some(account) = picked(&row.account) {
        options.account = Some(account);
    }
    options
}

/// EXP-1292 — ONE launch input for every session: the claimed row, its
/// `sessions.brief`, the root's linked issues and, on a resume, the run
/// record that re-spawns its agent CLI natively.
#[derive(Clone, Debug)]
pub struct SessionLaunchRequest {
    /// The claimed row (runner pickers: cli, account, model, effort,
    /// plan_mode, computer_use, ultracode, repository_id, branch, kind).
    pub session: SessionRow,
    /// Seed prompt = `brief.brief`; system append layers = runbook +
    /// `team_prompt` + `agent_instructions`; a child's worktree bases on
    /// `parent_branch`.
    pub brief: SessionBrief,
    /// The root's linked issues (session_issues ⋈ issues), link order; may
    /// be empty. One of them names the branch; they label the run.
    pub issue_identifiers: Vec<String>,
    /// `Some` = re-spawn THIS row's agent CLI natively from its run record
    /// (a message woke a session with no engine on this host).
    pub resume: Option<ResumeRun>,
    /// Hostname, for the log and the tab.
    pub device_label: String,
    /// The host's launch defaults, layered UNDER the row's pickers
    /// ([`session_launch_options`]).
    pub options: LaunchOptions,
}

/// EXP-1292 — a native resume of a session: the SAME row (no new row, no
/// `resumed_from_id`), its recorded run, and the first pending person
/// message as the turn that wakes it.
#[derive(Clone, Debug)]
pub struct ResumeRun {
    pub record: RunRecord,
    /// The first pending message's body.
    pub prompt: String,
    /// That message's `messages.id`, carried onto
    /// [`PreparedLaunch::seed_message_id`] so the engine consumes it once the
    /// seed turn is sent. `None` = nothing to consume (a resume the device
    /// started itself, e.g. an account hop).
    pub message_id: Option<String>,
    /// EXP-1334: `Some` = a RECONFIGURE (a person's `sessions.configure`
    /// moved the row's options past the run's): the row's pickers are
    /// authoritative — its effort (blank = this machine's default, never the
    /// record's), ultracode and computer use — and the run re-enters the plan
    /// mode it was in. `None` = an ordinary wake (never enters plan mode).
    pub reconfigure: Option<ResumeReconfigure>,
    /// EXP-1280: `true` = the waking message is a plan approved "in a fresh
    /// context" (`card.fresh`): the run starts a NEW conversation seeded
    /// with `prompt` instead of reopening the recorded one — what a live
    /// engine's `/clear` does, for a session whose engine is gone (a daemon
    /// restart while the card waited).
    pub fresh: bool,
}

/// EXP-1334: what a reconfigure resume carries beyond the row.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResumeReconfigure {
    /// Re-enter plan mode (the live run's mode, else the row's pick).
    pub plan_mode: bool,
}

/// The launch shapes ONE [`prepare`] serves. EXP-1292: ONE — the issue,
/// batch, action and resume-row shapes all collapsed into a session.
#[derive(Clone, Debug)]
pub enum PrepareRequest {
    Session(SessionLaunchRequest),
}

/// §7.1 step 3, injectable for tests: turn (repos_root, repo, branch, token)
/// into a ready worktree. The real impl is [`GitWorktrees`] (argv git).
/// `expires_at` is the token's real ISO-8601 expiry from the mint — the
/// ambient-auth install's no-downgrade stamp (EXP-73).
/// `parked` = what to do with a layout dir that is not on `branch`: a launch
/// reclaims it, a resume keeps it where the agent left it (FEED-129).
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
        parked: Parked,
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
        parked: Parked,
    ) -> Result<PathBuf, GitError> {
        let clone = ensure_clone(repos_root, full_name, url)?;
        git_credentials::ensure(&clone, url, expires_at)?;
        // An EMPTY repository gets its first commit here, so the run has a
        // base to cut from and its PR a base to target.
        let seeded = seed_empty_remote(&clone, default_branch, url)?;
        // Best-effort: a stale-but-present origin/<default> still yields a
        // valid worktree; only a truly missing base ref fails below.
        if !seeded {
            let _ = fetch_base(&clone, default_branch, url);
        }
        let worktree =
            create_worktree_with(&clone, branch, &format!("origin/{default_branch}"), url, parked)?;
        // Best-effort exclude coverage for the launcher's local-only files
        // ([`LOCAL_EXCLUDES`]), via the shared, never-committed
        // `.git/info/exclude`.
        let _ = crate::git_worktree::ensure_local_excludes(&clone, LOCAL_EXCLUDES);
        Ok(worktree)
    }
}

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
    /// Git ops ([`GitWorktrees`] in production).
    pub worktrees: Arc<dyn WorktreeProvider>,
    /// Where codex records its session rollouts, for the exact-session
    /// recovery a resume needs ([`prepare_resume`] — the only reader). `None` (production) = auto-detect
    /// [`crate::codex_sessions::default_codex_sessions_root`]; tests inject
    /// a fixture tree.
    pub codex_sessions_root: Option<PathBuf>,
    /// The app data dir — repo-less sessions run in
    /// `<data_dir>/scratch/<root id>/<session id>/` (EXP-1292).
    pub data_dir: PathBuf,
    /// EXP-637: where Claude Code writes per-cwd transcripts
    /// (`~/.claude/projects`). `None` (production) = auto-detect from `HOME`;
    /// tests inject a fixture tree. Read only by the resume probe — this
    /// crate cannot depend on `steer`, which owns the tailing side (§3.1).
    pub claude_projects_root: Option<PathBuf>,
    /// This machine's steer deviceId (desktop: [`api::device_identity`] via
    /// `steer::persistent_device_id`; CLI: `cli_device_id`) — the id a
    /// `queued` sessions row names to land here (EXP-1292). `None` only in
    /// tests.
    pub device_id: Option<String>,
    /// EXP-746: whether THIS host can run the ACP engine at all — a HOST
    /// fact, not a machine one: the engine drives its connection on the
    /// steer runtime, so a host that failed to start one (or has no engine
    /// linked in) has nowhere to run a session. EXP-773: the engine is the
    /// ONLY coding transport, so `false` disables coding on this host
    /// outright ([`DisabledReason::AcpUnavailable`]).
    pub acp_available: bool,
}

/// EXP-746: how an ACP run reopens the conversation it continues.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResumeSeed {
    /// The recorded ACP `sessionId` — `session/load` takes it.
    Acp(String),
    /// The agent's OWN id (claude's session uuid, codex's rollout id) —
    /// EXP-758: the engine records it alongside the ACP one, so this is what
    /// reopens the conversation when no ACP session id survived.
    Native(String),
}

/// EXP-746: everything the ACP engine needs that the PTY path expresses as
/// argv — the prompt as TEXT (the engine sends it as a `session/prompt`
/// message; PROMPT.md delivery is a TUI affordance and never happens here),
/// the normalized options, the per-agent MCP posture already wired by
/// [`wire_agent_mcp`], the `sessions` row id and the resume seed.
#[derive(Clone, Debug)]
pub struct AcpLaunch {
    pub prompt: Option<String>,
    pub options: LaunchOptions,
    pub mcp: AgentMcp,
    pub session_id: String,
    pub resume: Option<ResumeSeed>,
    /// The claude `--settings <path>` file, carrying [`CLAUDE_FLAG_SETTINGS`]
    /// (EXP-1124): NOT a hooks sidecar (the ACP path has none) but the
    /// REAPER's only process selection anchor — `reaper::select` matches the
    /// `claude-hooks/<pid>/` segment in the process command line, so an ACP
    /// claude that dropped the flag would be invisible to the quit sweep
    /// (EXP-300 all over again).
    /// `None` for every other agent, which the reaper never anchored either.
    pub reaper_settings_path: Option<PathBuf>,
    /// EXP-1025: the text on the agent's additive system-prompt channel
    /// (claude `--append-system-prompt`, codex `developer_instructions`):
    /// the run playbook, the team prompt and (EXP-1292) a team agent's
    /// instructions, composed by [`crate::skill::system_append_layers`] off
    /// the session's `sessions.brief` at prepare time — start AND resume, so
    /// an edit reaches the next resume of every existing session.
    pub system_append: String,
    /// EXP-1051: the RAW BYTE COUNTS of everything this launch put into the
    /// agent's context before its first turn — playbook, team prompt, seed
    /// prompt, the project memory files the agent loads itself, and the
    /// Exponential MCP surface. The engine turns them into token estimates
    /// (`domain::contract::CONTEXT_LAYOUT_CHARS_PER_TOKEN`) and publishes the
    /// `context_layout` frame; nothing is estimated here. Built by
    /// [`context_layers_for`], best-effort throughout.
    pub context_layers: ContextLayers,
    /// EXP-792: the launch's team MCP servers, resolved (`exponential` is
    /// NOT among them — it stays [`Self::mcp`]). The adapters render them
    /// into their own config (claude inline, codex `thread/start`). Empty for
    /// an agent shell and for every pick-less run.
    pub servers: Vec<McpServerWire>,
    /// EXP-792: every credential value the launcher put in the spawn env
    /// for those servers — the engine's redactor masks them out of the
    /// activity channel like the `expu_` key. Never read for anything else.
    pub mcp_secrets: McpSecrets,
    /// N-5 / NEW-5: `Some(cwd)` = a repo-less run, confined to its scratch
    /// dir ([`crate::scratch::is_scratch_dir`]); `None` = a worktree run.
    pub confine_to: Option<PathBuf>,
}

/// EXP-792: the resolved servers' secret values, for the steer redactor.
/// A newtype so a `{:?}` of the launch never prints them.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct McpSecrets(Vec<String>);

impl McpSecrets {
    pub fn new(values: Vec<String>) -> Self {
        Self(values)
    }

    pub fn into_vec(self) -> Vec<String> {
        self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl std::fmt::Debug for McpSecrets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "McpSecrets(<{} redacted>)", self.0.len())
    }
}

/// §7.1's non-fatal "why this session can't run here" set — each renders as
/// a small inline error with a remediation link (never falsely block,
/// always explain). None of these panic and none are transport errors.
#[derive(Clone, Debug)]
pub enum DisabledReason {
    /// The GitHub App is not installed on the repo (server 412) — install is
    /// web-only (§7.9); link out.
    GithubAppMissing { full_name: String, message: String },
    /// The agent CLI or `git` failed the doctor (§7.7) — names which tool.
    DoctorFailed(ToolCheck),
    /// The server refused to mint the installation token (401/403).
    TokenDenied { message: String },
    /// EXP-773: the picked agent cannot run on the ACP engine — the doctor's
    /// `acp` check said no (a too-old CLI) or this HOST
    /// has no engine at all. The engine is the ONLY coding transport, so
    /// there is nothing to fall back to; `note` carries the doctor's own
    /// explanation when it has one.
    AcpUnavailable { label: String, note: Option<String> },
    /// EXP-849: this resume asked to continue on a DIFFERENT account and the
    /// run cannot be handed over — codex (one login per session, its
    /// conversation lives in the login's own rollout store) or a missing
    /// target profile.
    AccountSwitchRefused { message: String },
}

impl DisabledReason {
    /// User-facing copy (§7.1: inline error / disabled-button helper text,
    /// and the agent message a refused pickup posts into the conversation).
    pub fn message(&self) -> String {
        match self {
            DisabledReason::GithubAppMissing { message, .. } => message.clone(),
            DisabledReason::DoctorFailed(check) => {
                check.error.clone().unwrap_or_else(|| format!("{} is not available", check.tool))
            }
            DisabledReason::TokenDenied { message } => message.clone(),
            DisabledReason::AccountSwitchRefused { message } => message.clone(),
            DisabledReason::AcpUnavailable { label, note } => match note {
                Some(note) => format!("{label} cannot run a session here: {note}"),
                None => format!("{label} cannot run a session on this machine."),
            },
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
    /// The `sessions` row id — keys the session tab (§06), the steer session
    /// room (§08) and the run record.
    pub session_id: String,
    /// What the session is called on this device ([`session_label`]):
    /// `EXP-42`, `EXP-42 +2`, its title, an agent's name. Feeds the
    /// log/registry surfaces.
    pub issue_identifier: String,
    pub worktree: PathBuf,
    /// The shared clone the worktree hangs off (`<repos_root>/<owner>/<name>`)
    /// — the token refresher's ambient-auth target (EXP-73: the credential
    /// file + helper config live in the clone's shared `.git`, so refreshing
    /// the clone covers every worktree).
    pub clone: PathBuf,
    /// The team `repositories` row id — re-mints the installation token
    /// mid-session (EXP-56 P9). `None` for a repo-less session (nothing to
    /// refresh: no clone, no token).
    pub repository_id: Option<String>,
    /// The real git branch (keeps its `/`), e.g. `exp/EXP-42` or
    /// `exp/session-<id8>` ([`session_branch`]), so
    /// `LocalSessions::holds_branch` protects the worktree and the
    /// inventory's `busy` follows. Empty for a repo-less session.
    pub branch: String,
    /// EXP-637: what [`branch`](Self::branch) was cut from — the run
    /// cleanup's "did anything land here" compare. `None` for an issue's
    /// session (the prune owns its worktree) and repo-less sessions.
    pub base_branch: Option<String>,
    /// EXP-688: the git ref "Latest changes" is measured from —
    /// `origin/<base>` for every session that has a repo (a child's base is
    /// its parent's branch). Unlike [`base_branch`](Self::base_branch) this
    /// is set for issue sessions too: it is the DIFF base, not the cleanup
    /// compare. `None` for a repo-less session — nothing to diff against.
    pub base_ref: Option<String>,
    /// EXP-637: the session worktree to auto-remove when the session ends,
    /// iff it is clean and carries no commits ([`crate::run_cleanup`]).
    /// `None` for an issue's session (its worktree survives by design) and
    /// for sessions with no worktree of their own.
    pub run_cleanup: Option<RunCleanup>,
    /// EXP-792: every picked team MCP server this run starts WITHOUT (not
    /// connected, refresh failed, removed, the resolve call failed) plus the
    /// server's own launch notes, as [`ResolvedMcp::warnings`] left them.
    /// Never a secret and never a blocker: the hosts show them (the desktop
    /// as one toast, the CLI as log lines) so a run missing its tools is not
    /// a silent one. Empty for every pick-less launch.
    pub mcp_warnings: Vec<String>,
    /// The agent invocation in the worktree (§7.1 step 7): program, cwd and
    /// env. `args` is always EMPTY — the ACP adapter composes the argv.
    pub spawn: SpawnSpec,
    /// EXP-746: the ACP engine's half of the spawn.
    pub acp: AcpLaunch,
    /// Tab strip default title (`claude · EXP-42` / `claude · EXP-42 +2`).
    pub tab_title: String,
    /// Issue identity re-attached to live OSC titles (EXP-145): `EXP-42` /
    /// `EXP-42 +2` — claude's OSC titles replace the whole tab title, so the
    /// strip shows `EXP-42 · <claude's title>`.
    pub tab_title_prefix: String,
    /// EXP-1292: the `messages.id` the seed turn delivers (a resume's waking
    /// message), consumed by the engine once that turn is sent. `None` for a
    /// fresh session (its brief is no message) and a device-started resume.
    pub seed_message_id: Option<String>,
    /// EXP-275/EXP-690: the spawn runs with permissions bypassed
    /// (`--dangerously-skip-permissions` / codex bypass — mirrors
    /// `permission_args`: every claude/codex run bypasses, and plan mode
    /// wins the starting mode, so it clears this). The activity emitter uses
    /// it to keep permission-flavored
    /// notifications from becoming "blocked on approval" cards.
    pub bypass_permissions: bool,
    /// EXP-529: the spawn launched into plan mode. EXP-1280: Exponential's
    /// plan mode (the engine's flag), never a CLI permission mode, so it
    /// rides NEXT TO `bypass_permissions` (always on). The activity emitter
    /// stamps it into the launch narration.
    pub plan_mode: bool,
    /// EXP-383: which agent CLI the spawn runs. The steer wiring picks the
    /// matching activity emitter (claude transcript tail / codex rollout
    /// tail) — every steer-room launch path flows through
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
    /// launch's whole flight. The host `.take()`s it out and drops it only
    /// after it registered the session; failure paths release via RAII.
    /// `None` for a repo-less session (no worktree to protect).
    pub launch_hold: Option<crate::launch_gate::LaunchHold>,
    /// EXP-1005: the start-time account pick that moved this launch off the
    /// account its options named (`None` = it runs on that account). The
    /// seed prompt carries the note; hosts log it.
    pub account_pick: Option<crate::account_rotation::StartPick>,
    /// EXP-1334: the team MCP pick this launch resolved
    /// ([`ResolvedMcp::pick`]): the baseline a live run's between-turn check
    /// compares `sessions.mcpPicks` against. `None` = unknown.
    pub mcp_pick: Option<Vec<String>>,
}

/// [`prepare`]'s outcome: ready to spawn, or disabled-with-reason.
#[derive(Debug)]
pub enum Prepared {
    Ready(PreparedLaunch),
    Disabled(DisabledReason),
}

/// §7.1's `LaunchOutcome`, produced by the host's spawn (or directly by the
/// caller when [`prepare`] returned [`Prepared::Disabled`]).
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

/// Where the per-session `--settings` files live: under the app data dir,
/// NEVER in the worktree. A `.claude/settings.json` inside the tree would be
/// committable by the agent AND would land in claude's project-approval scan
/// (the EXP-98 trap that made `.exp-mcp.json` non-discoverable). Files sit
/// one level down in a per-process dir (`claude-hooks/<pid>/`) so the path
/// in the agent's argv encodes WHICH exponential process (desktop or CLI
/// daemon — they share the data dir) spawned the session; the reaper's quit
/// sweep skips sessions a live sibling still owns (REV-20).
const HOOK_SETTINGS_DIR: &str = "claude-hooks";

/// EXP-773: the ONE `runs.json` `transport` value. The field survives the
/// PTY's deletion so an older reader still parses a record we write, and so
/// [`crate::run_registry::RunRecord::is_acp`] can refuse to resume a `"pty"`
/// record this build can no longer re-enter.
pub const ACP_TRANSPORT: &str = "acp";

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
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    (!segment.is_empty()).then_some(segment)
}

/// EXP-1124: the flag-settings layer every ACP claude spawn carries. Flag
/// settings outrank the user's, and a user `defaultMode: auto` otherwise
/// routes tool calls through the server-side classifier before
/// `can_use_tool` ever reaches the adapter, so auto is disabled outright.
/// EXP-1280: the CLI only ever runs `bypassPermissions` (plan mode is
/// Exponential's), so `useAutoModeDuringPlan` is gone.
pub const CLAUDE_FLAG_SETTINGS: &str = r#"{"permissions":{"disableAutoMode":"disable"}}"#;

/// EXP-746: the claude `--settings` file ([`CLAUDE_FLAG_SETTINGS`], EXP-1124)
/// at a per-pid path. It wires no hooks (the engine reads `session/update`
/// notifications), and claude's `--settings <path>` argv is the REAPER's only process-selection
/// anchor (`reaper::select` matches the `claude-hooks/<pid>/` segment in the
/// process COMMAND LINE, reaper.rs:111), so an ACP claude that dropped the
/// flag would be invisible to the quit sweep and EXP-300's escaped processes
/// would return. `None` for every other agent — the reaper never anchored
/// those either (their children die with the engine's kill-on-drop guard).
fn write_acp_reaper_anchor(
    data_dir: &Path,
    session_id: &str,
    agent: CodingAgent,
) -> Option<PathBuf> {
    if agent != CodingAgent::Claude {
        return None;
    }
    let root = data_dir.join(HOOK_SETTINGS_DIR);
    prune_hook_settings(&root);
    let dir = root.join(std::process::id().to_string());
    let segment = path_segment(session_id)?;
    let settings = dir.join(format!("{segment}.settings.json"));
    // A lost anchor launches claude WITHOUT the flag layer (the EXP-1124
    // classifier wall), so retry once and say so rather than fail silently.
    let write = || {
        std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&settings, CLAUDE_FLAG_SETTINGS))
    };
    if let Err(err) = write().or_else(|_| write()) {
        log::warn!("coding: could not write the claude flag settings {}: {err}", settings.display());
        return None;
    }
    Some(settings)
}

/// EXP-746: can this launch actually run on the ACP engine? Both halves must
/// hold — the AGENT speaks the protocol ([`ToolCheck::acp`], non-fatal and
/// separate from the launch gate) and the HOST can drive one
/// ([`CodingDeps::acp_available`]).
///
/// EXP-752: plan mode is NOT an exception — a plan-mode launch resolves its
/// transport exactly like any other.
fn acp_ready(
    report: &crate::doctor::DoctorReport,
    agent: CodingAgent,
    deps: &CodingDeps,
) -> bool {
    deps.acp_available && report.check_for(agent).acp == Some(true)
}

/// EXP-758: the doctor's own explanation for [`acp_ready`] answering `false`,
/// when it has one — the claude version-floor copy, codex's version floor.
/// `None` for a host with no engine, where the reason is this BUILD rather
/// than anything the CLI could tell the user.
fn acp_note<'a>(report: &'a crate::doctor::DoctorReport, agent: CodingAgent) -> Option<&'a str> {
    report
        .check_for(agent)
        .acp_note
        .as_deref()
        .filter(|note| !note.trim().is_empty())
}

/// EXP-773: the ACP gate. The engine is the ONE coding transport, so an
/// agent that fails the doctor's `acp` check (or a host with no engine at
/// all) cannot start a run — it says WHY instead of silently opening a
/// terminal tab nobody asked for.
fn acp_gate(
    report: &crate::doctor::DoctorReport,
    agent: CodingAgent,
    deps: &CodingDeps,
) -> Option<DisabledReason> {
    if acp_ready(report, agent, deps) {
        return None;
    }
    Some(DisabledReason::AcpUnavailable {
        label: agent.label().to_string(),
        note: acp_note(report, agent).map(str::to_string),
    })
}

/// EXP-758: the step-0 doctor gate for the launch's agent — the agent's own
/// CLI check, plus git when the launch clones (`needs_git`). EXP-1138: then
/// the LOGIN the launch spends (`account`, [`DoctorReport::account_failure`])
/// — no profile, or a signed-out one, refuses.
fn doctor_gate(
    report: &crate::doctor::DoctorReport,
    deps: &CodingDeps,
    agent: CodingAgent,
    account: Option<&str>,
    needs_git: bool,
) -> Option<ToolCheck> {
    let failed = if needs_git {
        report.first_failure_for(agent)
    } else {
        let check = report.check_for(agent);
        (!check.ok).then_some(check)
    };
    if let Some(failed) = failed {
        return Some(failed.clone());
    }
    report.account_failure(&deps.settings, &deps.data_dir, agent, account)
}

/// Drop settings files past the TTL wherever they sit in the tree — per-pid
/// dirs and pre-REV-20 flat files alike — and clear pid dirs that end up
/// empty (`remove_dir` refuses non-empty ones, so a live dir is never lost).
/// THIS process's dir is skipped whole: its anchors belong to live runs, and
/// removing it could race a concurrent launch's `create_dir_all` + write.
fn prune_hook_settings(root: &Path) {
    let own = std::process::id().to_string();
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
            if entry.file_name() == own.as_str() {
                continue;
            }
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

/// The per-agent MCP wiring, shared by every session launch, resume and the
/// agent shell. It
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
/// EXP-637: the GUARD half of [`wire_agent_mcp`], split out so a repo whose
/// ignore rules can't be verified fails the launch before any key is
/// written.
///
/// EXP-773: only the agent SHELL still writes a key file. A coding run is
/// ACP-only, where claude gets `AgentMcp::ClaudeInline` — nothing lands in
/// the cwd and the key rides the child env — so there is nothing to guard.
fn guard_agent_mcp(agent: CodingAgent, cwd: &Path, shell: bool) -> Result<(), CodingError> {
    if agent == CodingAgent::Claude && shell {
        // EXP-474: the key never lands in a repo we cannot prove ignores it.
        crate::git_worktree::ensure_ignored(cwd, &[crate::mcp_json::MCP_JSON_FILE])?;
    }
    Ok(())
}

/// EXP-637: `session_id` is the `sessions` row this launch runs — it rides
/// every MCP call as `X-Exp-Session-Id` so the server can resolve
/// `exponential_sessions_end` (and spare a self-merged session).
/// `None` = no session (agent shells), which keeps the pre-EXP-637 wiring
/// byte-identical.
///
/// EXP-773: `shell` marks the `+` menu's agent SHELL — an interactive TUI,
/// the only remaining consumer of claude's cwd `.exp-mcp.json`. Every coding
/// run is the ACP engine, where claude takes the inline config instead.
fn wire_agent_mcp(
    agent: CodingAgent,
    cwd: &Path,
    base_url: &str,
    personal_key: &str,
    session_id: Option<&str>,
    shell: bool,
) -> Result<AgentMcp, CodingError> {
    match agent {
        CodingAgent::Claude if !shell => {
            // EXP-746: no key file on the ACP arm — the engine passes inline
            // `--mcp-config` JSON with a `${EXP_MCP_TOKEN}` header and the key
            // rides the child env ([`apply_mcp_env`]).
            Ok(AgentMcp::ClaudeInline {
                url: mcp_url(base_url),
                session_id: session_id.map(str::to_string),
            })
        }
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
    }
}

/// The spawn-env half of [`wire_agent_mcp`]: the MCP credential for codex
/// rides the ENV (claude's rides `.exp-mcp.json`) — codex reads it through
/// `bearer_token_env_var`.
fn apply_mcp_env(
    spawn: SpawnSpec,
    agent: CodingAgent,
    personal_key: &str,
    session_id: Option<&str>,
    shell: bool,
) -> SpawnSpec {
    let spawn = match agent {
        // EXP-746: the ACP arm's inline `--mcp-config` reads the key from the
        // env (`${EXP_MCP_TOKEN}`); the agent shell keeps it in `.exp-mcp.json`.
        CodingAgent::Claude if !shell => with_claude_mcp_timeout(
            spawn.env(MCP_TOKEN_ENV, personal_key),
            std::env::var_os(CLAUDE_MCP_TOOL_TIMEOUT_ENV),
        ),
        CodingAgent::Claude => {
            with_claude_mcp_timeout(spawn, std::env::var_os(CLAUDE_MCP_TOOL_TIMEOUT_ENV))
        }
        CodingAgent::Codex => spawn.env(MCP_TOKEN_ENV, personal_key),
    };
    // EXP-637: harmless (and unread) on claude/codex, which carry the session
    // header in their own config.
    match session_id {
        Some(id) => spawn.env(MCP_SESSION_ID_ENV, id),
        None => spawn,
    }
}

/// EXP-792 (EXP-747 B2): the account PROFILE half of the spawn env — ONLY
/// the agent's config-dir variable (`CLAUDE_CONFIG_DIR` / `CODEX_HOME`)
/// pointed at the profile's dir under `{data_dir}/agents/<agent>/<id>/`.
/// Nothing at all for an unpinned slot and for an id no profile answers to —
/// never reached by a launch, which resolves a profile first and is refused
/// without one ([`DoctorReport::account_failure`]).
pub fn apply_account_env(
    spawn: SpawnSpec,
    agent: CodingAgent,
    data_dir: &Path,
    account: Option<&str>,
) -> SpawnSpec {
    match crate::agent_profiles::config_env(data_dir, agent, account) {
        Some((key, value)) => spawn.env(key, value),
        None => spawn,
    }
}

/// EXP-981: the env var that pins the model claude's SUBAGENTS run on. Claude
/// reads it at spawn; there is no flag for it.
pub const CLAUDE_SUBAGENT_MODEL_ENV: &str = "CLAUDE_CODE_SUBAGENT_MODEL";

/// The subagent-model half of the spawn env, applied at every site
/// [`apply_account_env`] is (sessions, resumes and agent shells alike). Nothing at all for a blank pick (the CLI's own
/// default) or for any agent but claude.
///
/// The value is the same one the `--model` flag gets: claude takes its model
/// ALIASES verbatim, so [`crate::argv::claude_model_arg`] is the one
/// normalization both go through and the subagent can never end up on a
/// spelling the main agent would reject.
pub fn apply_subagent_model_env(
    spawn: SpawnSpec,
    agent: CodingAgent,
    subagent_model: &str,
) -> SpawnSpec {
    if !agent.supports_subagent_model() {
        return spawn;
    }
    match crate::argv::claude_model_arg(subagent_model) {
        Some(model) => spawn.env(CLAUDE_SUBAGENT_MODEL_ENV, model),
        None => spawn,
    }
}

/// FEED-25: bound every claude MCP call (both arms — the agent shell and the
/// ACP child run with the same [`SpawnSpec`] env). `inherited` is the
/// host's own `MCP_TOOL_TIMEOUT`: a user who tuned it keeps their value, the
/// launcher only fills the CLI's no-timeout default.
fn with_claude_mcp_timeout(spawn: SpawnSpec, inherited: Option<std::ffi::OsString>) -> SpawnSpec {
    if inherited.is_some_and(|value| !value.is_empty()) {
        return spawn;
    }
    spawn.env(CLAUDE_MCP_TOOL_TIMEOUT_ENV, CLAUDE_MCP_TOOL_TIMEOUT_MS.to_string())
}

/// EXP-792: the launch's team MCP server pick, resolved server-side
/// (`mcpServers.resolveForLaunch`: the member's own credentials, refreshed
/// there). A pick NEVER blocks a launch: a server the member has not
/// connected, one whose refresh failed, one the team removed, or a failed
/// resolve call altogether is skipped with a logged warning and the run
/// starts without those tools. An empty pick resolves to nothing without
/// touching the network (every pick-less launch and every agent shell).
///
/// EXP-1140: resolved FOR `session_id` — the server hands out only that
/// row's persisted pick, to its owner or host, and never for the agent's own
/// key. EXP-1292: the pick lives on the `sessions` row (set by
/// `sessions.send`), so no ids are named here — the row's whole pick plus
/// the members' shared connections and the roster for the note (FEED-73).
fn resolve_mcp_servers(deps: &CodingDeps, session_id: &str) -> ResolvedMcp {
    let resolved = crate::mcp_servers::resolve(&deps.trpc, None, session_id);
    for warning in &resolved.warnings {
        log::warn!("{warning}");
    }
    resolved
}

/// EXP-1196: give the run this device's computer-use server when its
/// Computer use switch is on — EXP-1249: or when the RUN asked for it
/// (`options`' per-run `computer_use`, the composer "+" toggle; `None` =
/// the switch decides, [`LaunchOptions::computer_use_on`]). The grant names the run by `session_id`, so it
/// is minted after the row exists like the team servers; the entry and its
/// token then ride [`ResolvedMcp`] through the same config and env path.
/// Returns whether the run got it. BEST-EFFORT: a machine that cannot do
/// computer use (a headless session, a desktop without portals) launches without, with a
/// warning in the log and no section in the prompt. `label` = what the agent
/// cursor's badge shows while this run drives ([`session_label`]).
fn attach_computer_use(
    deps: &CodingDeps,
    options: &LaunchOptions,
    team_mcp: &mut ResolvedMcp,
    session_id: &str,
    label: &str,
) -> bool {
    if !options.computer_use_on(deps.settings.computer_use) {
        return false;
    }
    match computer::grant(session_id, label) {
        Ok(grant) => {
            crate::mcp_servers::attach_computer(team_mcp, &grant);
            true
        }
        Err(reason) => {
            log::warn!("coding: computer use is on but unavailable, launching without it: {reason}");
            false
        }
    }
}

/// EXP-1236: wire the device's code-mode server for this run, AFTER
/// [`attach_computer_use`] so the computer entry is among the upstreams its
/// scripts may call. Returns whether the run got it. BEST-EFFORT like
/// computer use: a host that cannot bind a loopback port launches without,
/// with a warning in the log and no section in the prompt.
fn attach_code_mode(
    team_mcp: &mut ResolvedMcp,
    agent_mcp: &AgentMcp,
    personal_key: &str,
    session_id: &str,
    label: &str,
) -> bool {
    let Some((upstreams, direct_only)) =
        crate::mcp_servers::codemode_upstreams(agent_mcp, personal_key, session_id, team_mcp)
    else {
        return false;
    };
    match codemode::grant(session_id, label, upstreams, direct_only) {
        Ok(grant) => {
            crate::mcp_servers::attach_codemode(team_mcp, &grant);
            true
        }
        Err(reason) => {
            log::warn!("coding: code mode unavailable, launching without it: {reason}");
            false
        }
    }
}

/// EXP-1196/EXP-1236: what this launch carries beyond the playbook, for
/// [`crate::skill::system_append_layers`]: the computer-use section names the device's
/// subagent model.
fn run_extras(
    deps: &CodingDeps,
    computer_use: bool,
    code_mode: bool,
    dispatch: bool,
) -> crate::skill::Extras {
    crate::skill::Extras {
        computer_use: computer_use.then(|| deps.settings.computer_use_model.clone()),
        code_mode,
        dispatch,
    }
}

/// EXP-792: the spawn-env half of the team servers, beside [`apply_mcp_env`]:
/// every resolved secret under the launcher-minted name (`EXP_MCP_TOKEN_<n>`,
/// `EXP_MCP_ENV_<n>_<NAME>`, a stdio server's own `<NAME>`) — the values the
/// agents' `${VAR}` references and codex's env-named fields resolve to. The
/// server LIST itself never rides the env: claude and codex both carry it in
/// their own config.
fn apply_mcp_server_env(spawn: SpawnSpec, resolved: &ResolvedMcp) -> SpawnSpec {
    let mut spawn = spawn;
    for (name, value) in &resolved.env {
        spawn = spawn.env(name, value);
    }
    spawn
}

/// The shared 412/401/403 mapping for `repositories.installationToken`.
fn token_error_reason(err: ApiError, full_name: &str) -> Result<DisabledReason, CodingError> {
    match err {
        ApiError::Http { status: 412, message } => {
            Ok(DisabledReason::GithubAppMissing { full_name: full_name.to_string(), message })
        }
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

/// EXP-1158 — what a launch moves "last used" to, decided from the REQUEST
/// before `launch_account`/the start-time rotation rewrite its account.
/// Last used = per agent, the login a PERSON last started or switched a run
/// on, on that device (`agent_accounts[agent].profiles[].active`); the last
/// used agent = `launch_defaults.defaultAgent`. Agent-started sessions
/// (children, a team agent's channel), resumes and auto-rotation never move
/// it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct LastUsedStamp {
    agent: CodingAgent,
    /// The account the person NAMED; `None` = nothing named, so only the
    /// agent is stamped.
    account: Option<String>,
}

/// EXP-1292: a person starts a thread ROOT (`sessions.send` from a
/// composer); a child is an agent's doing and an agent root's channel wakes
/// on its triggers, so neither stamps. A resume keeps its recorded account.
fn last_used_stamp(req: &PrepareRequest, options: &LaunchOptions) -> Option<LastUsedStamp> {
    let PrepareRequest::Session(session_req) = req;
    let row = &session_req.session;
    let person = session_req.resume.is_none() && row.parent_id.is_none() && !row.is_persistent();
    person.then(|| LastUsedStamp {
        agent: options.agent,
        account: options
            .account
            .as_deref()
            .map(str::trim)
            .filter(|account| !account.is_empty())
            .map(str::to_string),
    })
}

fn apply_last_used_stamp(stamp: &LastUsedStamp, data_dir: &Path) {
    record_last_used(data_dir, stamp.agent, stamp.account.as_deref());
}

/// EXP-1158 — record that a PERSON started or switched a run on `agent` /
/// `account` (`None` = nothing named, the agent alone; the retired `system`
/// clears the login pointer): the login (`profiles.json`'s `active`) and `settings.json`'s
/// `defaultAgent`, each only on a change. [`prepare`] calls it for every
/// person-started launch; the desktop's LOCAL mid-run switch calls it itself
/// (a local resume is also what a rotation hop is, so `prepare` cannot tell
/// the two apart). Best effort — a failed write costs the memory, never the
/// launch.
pub fn record_last_used(data_dir: &Path, agent: CodingAgent, account: Option<&str>) {
    if let Some(account) = account.map(str::trim).filter(|account| !account.is_empty()) {
        if let Err(err) = crate::agent_profiles::note_last_used(data_dir, agent, account) {
            log::warn!("coding: last used login not recorded ({account}): {err}");
        }
    }
    let path = Settings::default_path(data_dir);
    let mut settings = Settings::load(&path);
    if settings.default_agent != agent {
        settings.default_agent = agent;
        if let Err(err) = settings.save(&path) {
            log::warn!("coding: last used agent not recorded: {err}");
        }
    }
}

/// [`prepare_session`], then (EXP-1158) the last used stamp of a launch a
/// PERSON started, once it is [`Prepared::Ready`]: a refused launch moves
/// nothing.
pub fn prepare(req: &PrepareRequest, deps: &CodingDeps) -> Result<Prepared, CodingError> {
    let PrepareRequest::Session(session_req) = req;
    let options = session_launch_options(&session_req.session, &session_req.options, &deps.settings);
    let stamp = last_used_stamp(req, &options);
    let prepared = match &session_req.resume {
        None => prepare_session(session_req, options, deps)?,
        Some(resume) => prepare_resume(session_req, resume, deps)?,
    };
    if let (Some(stamp), Prepared::Ready(_)) = (&stamp, &prepared) {
        apply_last_used_stamp(stamp, &deps.data_dir);
    }
    Ok(prepared)
}

/// §7.2 — the personal key (the agent's MCP credential): read, or minted on
/// first use, on a side thread so it races the git prep.
fn spawn_personal_key(deps: &CodingDeps) -> std::thread::JoinHandle<Result<String, ApiError>> {
    let trpc = Arc::clone(&deps.trpc);
    let store = Arc::clone(&deps.token_store);
    let account_id = deps.account_id.clone();
    std::thread::spawn(move || users::ensure_personal_key(&trpc, &store, &account_id))
}

fn join_personal_key(
    handle: std::thread::JoinHandle<Result<String, ApiError>>,
) -> Result<String, CodingError> {
    Ok(handle
        .join()
        .map_err(|_| CodingError::Io("personal-key thread panicked".to_string()))??)
}

/// EXP-1292 — a repo-less session's scratch dir:
/// `<data_dir>/scratch/<root id>/<session id>/` ([`crate::scratch`]), path
/// segments sanitized (server ids are untrusted here by design, so a crafted
/// id can never escape the scratch root). Created on demand.
fn scratch_cwd(deps: &CodingDeps, row: &SessionRow) -> Result<PathBuf, CodingError> {
    let (Some(root), Some(session)) = (path_segment(row.root_id_or_self()), path_segment(&row.id)) else {
        return Err(CodingError::Io("empty session id".to_string()));
    };
    let dir = crate::scratch::scratch_root(&deps.data_dir).join(root).join(session);
    std::fs::create_dir_all(&dir)
        .map_err(|e| CodingError::Io(format!("create session scratch dir: {e}")))?;
    Ok(dir)
}

/// EXP-1292 — tell the server the branch the launcher put the session on,
/// once its worktree exists (the host claims with no branch). Best-effort:
/// a lost write costs the row its branch label until the agent's PR lands,
/// never the launch.
fn report_branch(trpc: &TrpcClient, session_id: &str, branch: &str) {
    let report = SessionReport {
        branch: Some(branch.to_string()),
        ..SessionReport::default()
    };
    if let Err(err) = session_device::report(trpc, session_id, &report) {
        log::warn!("coding: session {session_id}'s branch {branch} not reported: {err}");
    }
}

/// The steps of a FRESH session launch (blocking; run on the background
/// executor). The host already claimed the row and fetched its brief.
///
/// 0. doctor — the SELECTED agent (EXP-201: a missing codex never blocks a
///    claude launch), plus `git` when the session has a repository, then the
///    login it spends (EXP-1138) and the ACP gate (EXP-773);
/// 1. workspace — `repository_id` set: mint the JIT installation token
///    (session-gated, cache-seeded), then the worktree on the session's
///    branch ([`session_branch`]) cut from `origin/<brief.parent_branch>`
///    for a child, else the repo's resolved default; no repository: the
///    scratch dir ([`scratch_cwd`]);
/// 2. `sessions.report{branch}` once the worktree exists;
/// 3. the per-agent MCP wiring pinned to the row (`X-Exp-Session-Id`), the
///    team servers resolved FOR it, computer use and code mode;
/// 4. the run record (`runs.json`) a later resume re-enters;
/// 5. the seed prompt = the brief, verbatim; the system append = runbook +
///    team prompt + agent instructions ([`crate::skill::system_append_layers`]).
fn prepare_session(
    req: &SessionLaunchRequest,
    mut options: LaunchOptions,
    deps: &CodingDeps,
) -> Result<Prepared, CodingError> {
    let row = &req.session;
    let session_id = row.id.clone();
    // EXP-1005: every fresh launch on this device starts on the signed-in
    // profile of its agent with the MOST headroom, read off the usage cache
    // (`Settings.auto_rotate_accounts`, default on, turns it off; codex
    // never moves). The launch's own account stands on a tie.
    // The doctor runs FIRST (EXP-1138): the pick lands an unnamed launch on
    // a signed-in profile; the gate below judges the result.
    let report = run_doctor(&deps.settings, &deps.data_dir);
    let agent = options.agent;
    let account_pick = apply_start_pick(&mut options, deps, report.check_for(agent));
    note_start_pick(account_pick.as_ref());
    let options = &options;
    let parent_branch = req
        .brief
        .parent_branch
        .as_deref()
        .map(str::trim)
        .filter(|parent| !parent.is_empty());
    // EXP-1292: a child is handed its ROOT's issues (its label); one under a
    // coding parent works on a branch of its own, so it owns its worktree
    // like an issue-less one ([`owns_issue_work`]).
    let own_issues: &[String] = if owns_issue_work(row, parent_branch) {
        &req.issue_identifiers
    } else {
        &[]
    };
    let repository_id = row
        .repository_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string);

    // Step 0 — the doctor gate, PER-AGENT; git only when a clone is involved.
    if let Some(failed) =
        doctor_gate(&report, deps, agent, options.account.as_deref(), repository_id.is_some())
    {
        return Ok(Prepared::Disabled(DisabledReason::DoctorFailed(failed)));
    }
    // EXP-773: the ACP engine is the ONE coding transport — an agent that
    // cannot speak it says so here instead of falling back to a terminal tab.
    if let Some(reason) = acp_gate(&report, agent, deps) {
        return Ok(Prepared::Disabled(reason));
    }

    // Step 1 — the workspace.
    let mut key_handle = None;
    let mut launch_hold: Option<crate::launch_gate::LaunchHold> = None;
    let mut workspace: Option<RepoWorkspace> = None;
    let cwd = match &repository_id {
        Some(repository_id) => {
            // Step 1a — the JIT installation token (≤1 h real TTL, never
            // persisted/logged server-side — TokenUrl + scrubbed git errors
            // enforce that). Through the process-wide cache, deliberately:
            // this seeds it, so the refresher's first pass and the git-bar's
            // next sync are cache hits (EXP-73). The margin is the
            // refresher's LEAD: the ambient token must be born with enough
            // life to reach the refresher's first scheduled pass.
            let minted = match crate::token_cache::token_cache().get_or_mint_with_margin(
                &deps.trpc,
                repository_id,
                // EXP-1292: sessions belong to a repository, not a board; the
                // mint resolves the repo's own branch (team pin → GitHub).
                None,
                crate::token_refresh::REFRESH_LEAD,
            ) {
                Ok(minted) => minted,
                Err(err) => return map_token_error(err, repository_id),
            };
            // §7.2 — the personal-key read/mint races the git prep on a side
            // thread; only the MCP wiring needs the result.
            key_handle = Some(spawn_personal_key(deps));
            let url = minted.url.clone();
            let repos_root = deps.settings.repos_root_path();
            let clone = clone_path(&repos_root, url.full_name());
            launch_hold = Some(crate::launch_gate::hold(&clone));
            {
                let mut branch = session_branch(row, own_issues, &deps.settings.branch_prefix, parent_branch);
                // EXP-1292: an issue sits in any number of threads; a second one
                // starting while another LIVE session on this machine holds the
                // issue's branch takes its own instead of sharing the checkout.
                if row.branch.as_deref().is_none_or(|own| own.trim().is_empty())
                    && branch != session_own_branch(row)
                    && crate::run_registry::live_branch_holder(
                        &deps.data_dir,
                        repository_id,
                        &branch,
                        &session_id,
                    )
                    .is_some()
                {
                    log::info!(
                        "session {session_id}: {branch} is held by a live session here — taking its own branch"
                    );
                    branch = session_own_branch(row);
                }
                crate::git_worktree::validate_branch_arg(&branch, "session")?;
                // EXP-1292: a child works ON its parent's branch line — its
                // worktree is cut from `origin/<parent branch>` (and its PR then
                // stacks on the parent's); a root (no parent branch) cuts
                // from the repo default. Server data is untrusted,
                // so the ref is validated before it can reach git argv.
                let base_branch = match parent_branch {
                    Some(parent) => {
                        crate::git_worktree::validate_branch_arg(parent, "parent branch")?;
                        parent.to_string()
                    }
                    None => minted.default_branch.clone(),
                };
                // EXP-478: the gate above is taken BEFORE the worktree exists:
                // from there until the host registers the session, the
                // auto-prune must not run on this clone (the clone path is the
                // P9 token refresher's target too).
                let worktree = deps.worktrees.prepare(
                    &repos_root,
                    url.full_name(),
                    &base_branch,
                    &branch,
                    &url,
                    minted.expires_at.as_deref(),
                    Parked::Reclaim,
                )?;
                workspace = Some(RepoWorkspace {
                    clone,
                    full_name: url.full_name().to_string(),
                    branch,
                    base_branch,
                });
                worktree
            }
        }
        // Repo-less: a scratch dir meant to hold only the launcher's
        // sidecars — purged whole with the session (EXP-764). No git, no
        // token, no launch hold.
        None => scratch_cwd(deps, row)?,
    };

    let personal_key = join_personal_key(key_handle.unwrap_or_else(|| spawn_personal_key(deps)))?;
    // Step 2 — the branch the row runs on, now that its worktree exists.
    if let Some(workspace) = &workspace {
        report_branch(&deps.trpc, &session_id, &workspace.branch);
    }

    // Step 3 — the EXP-474 ignore guard, then the per-agent MCP wiring
    // pinned to the row.
    guard_agent_mcp(agent, &cwd, false)?;
    let agent_mcp = wire_agent_mcp(
        agent,
        &cwd,
        deps.trpc.base_url(),
        &personal_key,
        Some(&session_id),
        false,
    )?;
    // EXP-792/1140: the row's team MCP server pick, resolved FOR it
    // server-side (unconnected or unpicked ones are skipped, never a
    // blocker). The pick lives on the row, so no ids are named here.
    let mut team_mcp = resolve_mcp_servers(deps, &session_id);
    let label = session_label(row, &req.issue_identifiers, &req.brief.brief);
    let computer_use = attach_computer_use(deps, options, &mut team_mcp, &session_id, &label);
    let code_mode = attach_code_mode(&mut team_mcp, &agent_mcp, &personal_key, &session_id, &label);

    // EXP-210: stamp THIS agent into the workspace's recorded-agent marker
    // (after every can-still-fail step) — a later resume reads it to decide
    // whether the agent's native reopen can work here at all.
    let _ = crate::worktree_agents::record_worktree_agent_id(&cwd, agent.id());

    // EXP-389/EXP-414: pre-trust the workspace in the run's account PROFILE
    // config (EXP-792) — a remotely started session would otherwise park
    // forever on a directory-trust screen. codex resolves a linked
    // worktree's trust subject to the main clone (a scratch dir: its root's
    // dir, stable across the tree); claude keys trust by the spawn cwd.
    let profile_dir = crate::agent_profiles::account_dir(
        &deps.data_dir,
        Some(agent),
        options.account.as_deref(),
    );
    if agent == CodingAgent::Codex {
        let trust_root = workspace
            .as_ref()
            .map(|workspace| workspace.clone.as_path())
            .or_else(|| cwd.parent())
            .unwrap_or(&cwd);
        crate::codex_trust::ensure_trusted(trust_root, profile_dir.as_deref());
    }
    if agent == CodingAgent::Claude {
        // EXP-690: always seed `bypassPermissionsModeAccepted` — every run
        // bypasses, and even a plan-mode run is one Shift+Tab from it.
        crate::claude_trust::ensure_onboarded(&cwd, true, profile_dir.as_deref());
    }
    // EXP-746: the reaper's claude anchor (the EXP-1124 flag settings).
    let reaper_anchor = write_acp_reaper_anchor(&deps.data_dir, &session_id, agent);
    // EXP-443: the codex originator is stamped into its rollout metas before
    // the spawn. The claude session id is the ACP adapter's own to mint.
    let codex_originator =
        (agent == CodingAgent::Codex).then(|| crate::argv::codex_session_originator(&session_id));

    // Step 4 — the run record a later resume (a message waking this row on a
    // host with no engine) re-enters. Best-effort: a failed write only costs
    // that native resume.
    crate::run_registry::record(
        &deps.data_dir,
        RunRecord {
            session_id: session_id.clone(),
            account_id: deps.account_id.clone(),
            agent,
            kind: RunKind::of(row),
            team_id: row.team_id.clone().unwrap_or_default(),
            issues: own_issues.to_vec(),
            // EXP-1134/1292/1341: only a REAL name — the row's stored title
            // (`domain::session_title`'s first rung). Blank stays `None`,
            // which is what asks the agent CLI to name a thread
            // (`engine::session::record_wants_name`): children and issue
            // threads included; NEW-8: an agent's conversation never asks.
            title: named_text(&row.title),
            cwd: cwd.clone(),
            clone: workspace.as_ref().map(|workspace| workspace.clone.clone()),
            repo: workspace.as_ref().map(|workspace| workspace.full_name.clone()),
            repository_id: repository_id.clone(),
            branch: workspace.as_ref().map(|workspace| workspace.branch.clone()),
            base_branch: workspace.as_ref().map(|workspace| workspace.base_branch.clone()),
            claude_session_id: None,
            codex_originator: codex_originator.clone(),
            model: options.model.clone(),
            effort: options.effort.clone(),
            ultracode: options.ultracode,
            // EXP-773: the engine is the ONE transport; the string stays on
            // the record so an older reader still parses it. The ACP ids land
            // later: the engine upserts this record once `session/new` answers.
            transport: Some(ACP_TRANSPORT.to_string()),
            acp_session_id: None,
            agent_native_session_id: None,
            acp_child_pid: None,
            host_pid: None,
            recorded_at: crate::run_registry::now_secs(),
            // EXP-792: the account profile, for a resume to re-enter;
            // EXP-849: plus the row's account this launch honored.
            extra: {
                let mut extra = launch_extra(&[], options.account.as_deref());
                extra.insert(
                    crate::run_registry::ROW_ACCOUNT_KEY.to_string(),
                    serde_json::Value::String(crate::agent_profiles::profile_id(
                        row.account.as_deref(),
                    )),
                );
                extra
            },
        },
    );

    let spawn = session_spawn(
        deps,
        agent,
        &cwd,
        workspace.as_ref().map(|workspace| workspace.clone.as_path()),
        options,
        &personal_key,
        &session_id,
        &team_mcp,
        codex_originator.as_deref(),
    );

    // Step 5 — the seed prompt is the brief, verbatim (EXP-1005: the account
    // hop said first), and the layered system append.
    let seed = Some(req.brief.brief.trim_end())
        .filter(|brief| !brief.trim().is_empty())
        .map(|brief| with_start_note(brief.to_string(), account_pick.as_ref()));
    let system_append = crate::skill::system_append_layers(
        req.brief.team_prompt.as_deref(),
        req.brief.agent_instructions.as_deref(),
        &run_extras(deps, computer_use, code_mode, dispatches(row)),
    );
    // EXP-1051: a fresh session carries no base from anywhere — every layer
    // is one this launch just built.
    let context_layers = context_layers_for(
        agent,
        &cwd,
        profile_dir.as_deref(),
        &system_append,
        seed.as_deref(),
        None,
    );
    // FEED-73: the team-MCP delegation note rides AFTER the measured
    // layers (the team layer stays the server layers' bytes).
    let system_append = with_delegation_note(system_append, &team_mcp);
    // EXP-637: a session with no issue of its own owns its worktree (its
    // `exp/session-<id8>` branch is nobody else's), so it also owns cleaning
    // it up when it ends clean and empty. An issue's worktree is the prune's
    // business and survives its session by design.
    let run_cleanup = workspace
        .as_ref()
        .filter(|_| own_issues.is_empty())
        .map(|workspace| RunCleanup {
            clone: workspace.clone.clone(),
            worktree: cwd.clone(),
            branch: workspace.branch.clone(),
            base_branch: workspace.base_branch.clone(),
        });
    Ok(Prepared::Ready(PreparedLaunch {
        session_id: session_id.clone(),
        account_pick: account_pick.clone(),
        issue_identifier: label.clone(),
        worktree: cwd.clone(),
        clone: workspace
            .as_ref()
            .map(|workspace| workspace.clone.clone())
            .unwrap_or_else(|| cwd.clone()),
        repository_id,
        branch: workspace
            .as_ref()
            .map(|workspace| workspace.branch.clone())
            .unwrap_or_default(),
        base_branch: run_cleanup.as_ref().map(|cleanup| cleanup.base_branch.clone()),
        base_ref: workspace
            .as_ref()
            .map(|workspace| format!("origin/{}", workspace.base_branch)),
        run_cleanup,
        mcp_warnings: team_mcp.warnings.clone(),
        spawn,
        tab_title: format!("{} · {label}", agent.id()),
        tab_title_prefix: label,
        acp: AcpLaunch {
            prompt: seed,
            options: options.clone(),
            mcp: agent_mcp,
            session_id,
            // A fresh session has nothing to reopen.
            resume: None,
            reaper_settings_path: reaper_anchor,
            system_append,
            context_layers,
            servers: team_mcp.servers.clone(),
            mcp_secrets: McpSecrets::new(team_mcp.secret_values()),
            confine_to: crate::scratch::is_scratch_dir(&deps.data_dir, &cwd).then(|| cwd.clone()),
        },
        seed_message_id: None,
        bypass_permissions: true,
        plan_mode: options.plan_mode,
        agent,
        claude_session_id: None,
        codex_originator,
        codex_resume_id: None,
        launch_hold,
        mcp_pick: team_mcp.pick.clone(),
    }))
}

/// A repo-backed session's workspace, as step 1 left it.
struct RepoWorkspace {
    clone: PathBuf,
    full_name: String,
    branch: String,
    /// What the branch was cut from (the parent's branch for a child).
    base_branch: String,
}

/// The agent invocation in the workspace: program, cwd and env (the ACP
/// adapter composes its own argv from [`AcpLaunch`]). Shared by a fresh
/// launch and a resume so the two can never spawn differently.
#[allow(clippy::too_many_arguments)]
fn session_spawn(
    deps: &CodingDeps,
    agent: CodingAgent,
    cwd: &Path,
    clone: Option<&Path>,
    options: &LaunchOptions,
    personal_key: &str,
    session_id: &str,
    team_mcp: &ResolvedMcp,
    codex_originator: Option<&str>,
) -> SpawnSpec {
    let mut spawn = SpawnSpec::new(&deps.settings.resolved_path_for(agent)).cwd(cwd);
    // The MCP credential env half of the wiring ([`apply_mcp_env`]).
    spawn = apply_mcp_env(spawn, agent, personal_key, Some(session_id), false);
    spawn = apply_mcp_server_env(spawn, team_mcp);
    spawn = apply_account_env(spawn, agent, &deps.data_dir, options.account.as_deref());
    spawn = apply_subagent_model_env(spawn, agent, &options.subagent_model);
    if let Some(originator) = codex_originator {
        spawn = spawn.env(crate::argv::CODEX_ORIGINATOR_ENV, originator);
    }
    if let Some(clone) = clone {
        // EXP-76 disk hygiene, both inherited by every cargo the session
        // runs: one shared build cache for ALL of this repo's session
        // worktrees, and no incremental caches (session builds are
        // few-shot). Inert for non-Rust repos and repo-less sessions.
        spawn = spawn
            .env(
                "CARGO_TARGET_DIR",
                shared_cargo_target_dir(clone).to_string_lossy().into_owned(),
            )
            .env("CARGO_INCREMENTAL", "0");
    }
    spawn
}

/// EXP-1292 — RESUME a session natively (blocking, background executor): a
/// person's message reached a `running`/`idle` row this host runs no engine
/// for (the host restarted, an agent's process exited idle), so the SAME
/// row's agent CLI is re-spawned from its run record and the message opens
/// its first turn. No new row, no `resumed_from_id`: the row id, the steer
/// room and the record key all stay.
///
/// The recorded [`RunRecord`] already answers every question the original
/// launch had to resolve (agent, cwd, branch, options), so this sequence is
/// the SHORT one: doctor → workspace still there (a reclaimed worktree is
/// re-created, a purged scratch dir re-made) → repo-backed re-auth and
/// idempotent worktree → native transcript probe → MCP wiring → re-record.
/// A resume never enters plan mode (whatever plan there was already ran) —
/// except a reconfigure ([`ResumeRun::reconfigure`], EXP-1334).
///
/// `req.options.account` names a SWITCH (EXP-849: the EXP-1005 rotation hop,
/// a person's account change); `None` keeps the recorded login.
fn prepare_resume(
    req: &SessionLaunchRequest,
    resume: &ResumeRun,
    deps: &CodingDeps,
) -> Result<Prepared, CodingError> {
    let row = &req.session;
    let record = &resume.record;
    let session_id = row.id.clone();
    let agent = record.agent;
    // EXP-849: what account this resume runs on — the one asked for (a
    // switch), else the recorded one. A record that names none (written by
    // a build that ran the retired ambient login) resumes on the device's
    // resolved profile, its claude transcript handed over below.
    let requested_account = req
        .options
        .account
        .as_deref()
        .filter(|id| !crate::agent_profiles::is_unpinned(Some(id)))
        .map(|id| id.trim().to_string());
    let recorded_account = record.account();
    let legacy_ambient = requested_account.is_none() && recorded_account.is_none();
    let resume_account = requested_account.clone().or_else(|| recorded_account.clone()).or_else(|| {
        crate::agent_profiles::resolve_launch_account(&deps.data_dir, agent, None, |profile| {
            profile_signed_out(deps, agent, None, profile)
        })
        .account
    });
    // A switch is a request that names a DIFFERENT login than the run used;
    // "resume on the account it already ran on" is an ordinary resume,
    // however the caller spelled it.
    let switching = requested_account.is_some() && resume_account != recorded_account;
    let picked = |value: &Option<String>| {
        value
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    // EXP-1334: a reconfigure takes the row's pickers as they are now — a
    // cleared effort is this machine's default for the agent, never the one
    // the record last ran on.
    let reconfigure = resume.reconfigure;
    let default_effort = || {
        if req.options.agent == agent {
            req.options.effort.clone()
        } else {
            LaunchOptions::defaults_for(&deps.settings, agent).effort
        }
    };
    let options = LaunchOptions {
        agent,
        // The row's pick wins (a person may change the model on a live
        // session); the record keeps what the run last ran on. Explicit
        // always: a blank falls to this machine's default.
        model: explicit_model(
            agent,
            &picked(&row.model).unwrap_or_else(|| record.model.clone()),
            &deps.settings,
        ),
        effort: match reconfigure {
            Some(_) => picked(&row.effort).unwrap_or_else(default_effort),
            None => picked(&row.effort).unwrap_or_else(|| record.effort.clone()),
        },
        ultracode: match reconfigure {
            Some(_) => row.ultracode && agent.supports_ultracode(),
            None => record.ultracode,
        },
        // EXP-1280: plan mode is the engine's flag, so a resume re-enters it
        // as the row says (still planning = not approved yet) — a
        // reconfigure carries the mode the live run was in (EXP-1334).
        plan_mode: match reconfigure {
            Some(reconfigure) => reconfigure.plan_mode,
            None => row.plan_mode,
        },
        // EXP-981: the run registry keeps no subagent pin, so a resume runs
        // claude's subagents on this machine's subagent model.
        subagent_model: deps.settings.subagent_model_for(agent).to_string(),
        // EXP-1292: the pick lives on the row, resolved server-side.
        mcp_server_ids: Vec::new(),
        // EXP-792: the SAME config dir (credentials, trust, codex rollouts)
        // the run used — unless this resume switches (gated below).
        account: resume_account.clone(),
        computer_use: Some(row.computer_use),
    };
    let profile_dir = crate::agent_profiles::account_dir(
        &deps.data_dir,
        Some(agent),
        options.account.as_deref(),
    );
    // EXP-849 — refuse a switch the machine cannot honor, BEFORE anything is
    // touched: a named target that does not exist here, and codex, whose
    // conversation lives inside the login's own rollout store.
    if switching {
        if agent != CodingAgent::Claude {
            return Ok(Prepared::Disabled(DisabledReason::AccountSwitchRefused {
                message: format!(
                    "{} cannot continue a session on another account. Start a new session on it instead.",
                    agent.label()
                ),
            }));
        }
        if let (Some(target), None) = (&resume_account, &profile_dir) {
            return Ok(Prepared::Disabled(DisabledReason::AccountSwitchRefused {
                message: format!("That account ({target}) is not set up on this machine."),
            }));
        }
    }
    // Step 0 — doctor: the RECORDED agent (a resume never switches agents),
    // plus git when the session lives in a clone ([`doctor_gate`]), judged
    // on the login the CONTINUATION spends (EXP-1138).
    let report = run_doctor(&deps.settings, &deps.data_dir);
    if let Some(failed) = doctor_gate(
        &report,
        deps,
        agent,
        options.account.as_deref(),
        record.clone.is_some(),
    ) {
        return Ok(Prepared::Disabled(DisabledReason::DoctorFailed(failed)));
    }
    // EXP-773: same ACP gate as a fresh launch.
    if let Some(reason) = acp_gate(&report, agent, deps) {
        return Ok(Prepared::Disabled(reason));
    }
    // Step 1 — the workspace. A repo-backed session whose worktree the run
    // cleanup or the prune reclaimed (its PR landed) is re-created in step 2
    // on its recorded branch, cut fresh from `origin/<base>` when the branch
    // went with it; the agent hears about it in its first turn. EXP-1292: a
    // repo-less session's scratch dir is re-made where it was (a team agent
    // never ends, so its channel must survive a purged dir); a branch-less
    // repo-backed record with its dir gone is a hard stop.
    let mut cwd = record.cwd.clone();
    let workspace_reclaimed = record.workspace_reclaimed();
    if record.clone.is_none() {
        std::fs::create_dir_all(&cwd)
            .map_err(|e| CodingError::Io(format!("re-create session scratch dir: {e}")))?;
        crate::scratch::touch(&cwd);
    } else if !cwd.is_dir() && !workspace_reclaimed {
        return Err(CodingError::Io(format!(
            "this session's workspace is gone ({})",
            cwd.display()
        )));
    }

    // Step 2 — repo-backed: a fresh JIT token, ambient auth re-installed
    // (the recorded one expired long ago), and an IDEMPOTENT worktree
    // re-create through the injected [`WorktreeProvider`] (the SAME sequence
    // a fresh launch takes; it heals a worktree git pruned under us).
    let mut launch_hold: Option<crate::launch_gate::LaunchHold> = None;
    let mut key_handle = None;
    // The base the reclaimed note compares against (`origin/<base>..HEAD`).
    let mut default_branch = record.base_branch.clone().unwrap_or_default();
    if let (Some(clone), Some(repository_id)) = (&record.clone, &record.repository_id) {
        let minted = match crate::token_cache::token_cache().get_or_mint_with_margin(
            &deps.trpc,
            repository_id,
            None,
            crate::token_refresh::REFRESH_LEAD,
        ) {
            Ok(minted) => minted,
            Err(err) => {
                return map_token_error(err, record.repo.as_deref().unwrap_or("repository"))
            }
        };
        // §7.2 — the personal key, raced against the git work like a fresh
        // launch's.
        key_handle = Some(spawn_personal_key(deps));
        let url = minted.url.clone();
        if default_branch.is_empty() {
            default_branch = minted.default_branch.clone();
        }
        launch_hold = Some(crate::launch_gate::hold(clone));
        // A reclaimed worktree is cut from the CURRENT base: the parent's
        // branch while the brief still names one, else the repo default (a
        // recorded parent branch may be long merged and gone).
        if workspace_reclaimed {
            let current = req
                .brief
                .parent_branch
                .as_deref()
                .map(str::trim)
                .filter(|parent| !parent.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| minted.default_branch.clone());
            crate::git_worktree::validate_branch_arg(&current, "resume base")?;
            if current != default_branch {
                log::info!(
                    "resume {}: re-cutting the reclaimed worktree from {current} (recorded base {default_branch})",
                    record.session_id,
                );
                default_branch = current;
            }
        }
        match &record.branch {
            Some(branch) => {
                crate::git_worktree::validate_branch_arg(branch, "resume run")?;
                // FEED-129: a standing worktree is KEPT as the agent left it
                // (`Parked::Keep`) — a checkout back to the recorded branch
                // refused every resume of a run that had moved to another
                // branch and edited files, and its messages sat unconsumed.
                let worktree = deps.worktrees.prepare(
                    &deps.settings.repos_root_path(),
                    url.full_name(),
                    &default_branch,
                    branch,
                    &url,
                    minted.expires_at.as_deref(),
                    Parked::Keep,
                )?;
                // A re-created worktree is the cwd wherever git put it (the
                // layout path, normally — the same one the record names, so
                // the agent's cwd-keyed transcript still resolves).
                if workspace_reclaimed {
                    if worktree != cwd {
                        log::info!(
                            "resume {}: worktree re-created at {} (recorded {})",
                            record.session_id,
                            worktree.display(),
                            cwd.display()
                        );
                    }
                    cwd = worktree;
                } else if let Some(carried) = carried_branch(&cwd, branch) {
                    // The run stands on another branch: the row follows, so
                    // the thread's PR opens from where the commits are.
                    log::info!(
                        "resume {}: worktree stays on {carried} (recorded branch {branch}); the row follows",
                        record.session_id
                    );
                    report_branch(&deps.trpc, &session_id, &carried);
                }
            }
            // A branch-less repo-backed record still needs its ambient auth
            // reinstalled.
            None => git_credentials::ensure(clone, &url, minted.expires_at.as_deref())?,
        }
    }

    // Step 3 — can the agent reopen its own conversation? Each agent answers
    // differently, and a `no` is never fatal: the fallback is a fresh
    // conversation seeded with the brief and the message.
    let marker_allows_resume = crate::worktree_agents::worktree_agents(&cwd)
        .is_none_or(|recorded| recorded.contains(&agent));
    // EXP-758: a run also carries the agent's OWN handle — the engine wrote
    // `agent_native_session_id` (claude's session uuid, codex's thread id) —
    // which reopens the conversation when no ACP session id survived. Each
    // agent's handles are tried in order (the recorded pin first, the native
    // id second) and the FIRST one that still names a live conversation wins.
    // EXP-849: the transcript is looked for in the tree the resume will RUN
    // against — the TARGET profile's, which on a switch is not the one that
    // holds it yet. So hand it over first (transcript only, never a
    // credential): a switch whose copy fails degrades like a pruned one.
    let acp_native = record.agent_native_session_id.clone();
    let source_profile_dir = crate::agent_profiles::account_dir(
        &deps.data_dir,
        Some(agent),
        recorded_account.as_deref(),
    );
    // EXP-1280: a fresh-context approval reopens nothing — the transcript
    // stays where it is, the new conversation starts from the brief and the
    // plan (the live path's `/clear`).
    let fresh = resume.fresh;
    let claude_resume_id = (marker_allows_resume && agent == CodingAgent::Claude && !fresh)
        .then(|| {
            record
                .claude_session_id
                .clone()
                .into_iter()
                .chain(acp_native.clone())
                .find(|id| {
                    if switching || legacy_ambient {
                        copy_claude_transcript(
                            deps,
                            source_profile_dir.as_deref(),
                            profile_dir.as_deref(),
                            id,
                        );
                    }
                    claude_transcript_exists_in(deps, profile_dir.as_deref(), id)
                })
        })
        .flatten();
    let codex_resume_id = (marker_allows_resume && agent == CodingAgent::Codex && !fresh)
        .then(|| {
            acp_native.clone().or_else(|| {
                deps.codex_sessions_root
                    .clone()
                    .or_else(|| crate::codex_sessions::codex_sessions_root(profile_dir.as_deref()))
                    .and_then(|root| {
                        crate::codex_sessions::find_codex_session_id(
                            &root,
                            &cwd,
                            record.codex_originator.as_deref(),
                        )
                    })
            })
        })
        .flatten();
    // EXP-746 (D8): an ACP run's surviving conversation is its recorded ACP
    // session id — `session/load` replays the WHOLE thread, so it is as
    // native a resume as the handles above.
    let acp_resume_id = record.acp_session_id.clone().filter(|_| record.is_acp() && !fresh);
    let native_resume =
        acp_resume_id.is_some() || claude_resume_id.is_some() || codex_resume_id.is_some();

    // Step 4 — the first turn. A native resume sends the message verbatim;
    // a conversation that did not survive starts over from the brief with
    // the message after it. A re-created worktree is news the transcript
    // cannot know, so its note opens the turn; an account switch (EXP-935)
    // tells the agent to carry on, LAST.
    let message = resume.prompt.trim();
    let reclaimed_note = workspace_reclaimed
        .then(|| {
            record.branch.as_deref().map(|branch| {
                // Which story is true depends on what the re-created worktree
                // actually holds: a reclaimer only deletes a MERGED branch
                // (fresh cut, nothing ahead), while a worktree a person
                // removed leaves the branch and its commits standing.
                let ahead = crate::run_cleanup::count_commits(
                    &cwd,
                    &format!("origin/{default_branch}..{branch}"),
                )
                .unwrap_or(0);
                crate::prompt::reclaimed_workspace_note(
                    branch,
                    &default_branch,
                    ahead,
                    native_resume && message.is_empty() && !switching,
                )
            })
        })
        .flatten();
    let body = if native_resume {
        Some(message.to_string()).filter(|text| !text.is_empty())
    } else {
        let brief = req.brief.brief.trim_end();
        match (brief.trim().is_empty(), message.is_empty()) {
            (true, true) => None,
            (true, false) => Some(message.to_string()),
            (false, true) => Some(brief.to_string()),
            (false, false) => Some(format!("{brief}\n\n{message}")),
        }
    };
    let rendered = match (&reclaimed_note, body) {
        (Some(note), Some(body)) => Some(format!("{note}\n\n{body}")),
        (Some(note), None) => Some(note.clone()),
        (None, body) => body,
    };
    // An account-only switch of an IDLE session seeds no turn of its own
    // (it resumes idle on the new login); one picked on the usage WALL
    // continues the walled work.
    let walled = row.blocked.as_ref().is_some_and(|wall| !wall.is_null());
    let rendered = match (switching, rendered) {
        (true, Some(body)) => {
            Some(format!("{body}\n\n{}", crate::prompt::ACCOUNT_SWITCH_CONTINUE_PROMPT))
        }
        (true, None) if walled => Some(crate::prompt::ACCOUNT_SWITCH_CONTINUE_PROMPT.to_string()),
        (_, body) => body,
    };
    let personal_key = join_personal_key(key_handle.unwrap_or_else(|| spawn_personal_key(deps)))?;
    guard_agent_mcp(agent, &cwd, false)?;
    let agent_mcp = wire_agent_mcp(
        agent,
        &cwd,
        deps.trpc.base_url(),
        &personal_key,
        Some(&session_id),
        false,
    )?;
    let _ = crate::worktree_agents::record_worktree_agent_id(&cwd, agent.id());
    // EXP-792/1140: the row's team MCP server pick, re-resolved now (fresh
    // tokens); unconnected or unpicked ones are skipped, never a blocker.
    // EXP-1330: a `/btw` aside gets NO MCP servers at all — not the team
    // pick (never even resolved), not code mode, not computer use: a side
    // question answers with the read-only built-in tools alone (the engine
    // drops `exponential` from its config too, `engine::aside`).
    let aside = record.is_aside();
    let mut team_mcp = if aside {
        ResolvedMcp::default()
    } else {
        resolve_mcp_servers(deps, &session_id)
    };
    let label = session_label(row, &req.issue_identifiers, &req.brief.brief);
    let computer_use =
        !aside && attach_computer_use(deps, &options, &mut team_mcp, &session_id, &label);
    let code_mode =
        !aside && attach_code_mode(&mut team_mcp, &agent_mcp, &personal_key, &session_id, &label);

    // Step 5 — the spawn spec, exactly like a fresh launch's.
    if agent == CodingAgent::Codex {
        let trust_root = record
            .clone
            .as_deref()
            .or_else(|| cwd.parent())
            .unwrap_or(&cwd);
        crate::codex_trust::ensure_trusted(trust_root, profile_dir.as_deref());
    }
    if agent == CodingAgent::Claude {
        crate::claude_trust::ensure_onboarded(&cwd, true, profile_dir.as_deref());
    }
    // EXP-746: the reaper's claude anchor (the EXP-1124 flag settings).
    let reaper_anchor = write_acp_reaper_anchor(&deps.data_dir, &session_id, agent);
    let codex_originator =
        (agent == CodingAgent::Codex).then(|| crate::argv::codex_session_originator(&session_id));
    let spawn = session_spawn(
        deps,
        agent,
        &cwd,
        record.clone.as_deref(),
        &options,
        &personal_key,
        &session_id,
        &team_mcp,
        codex_originator.as_deref(),
    );

    // Only a session that OWNS its worktree may have it reclaimed — never an
    // issue's.
    let run_cleanup = match (&record.clone, &record.branch, &record.base_branch) {
        (Some(clone), Some(branch), Some(base_branch)) if record.owns_run_worktree() => {
            Some(RunCleanup {
                clone: clone.clone(),
                worktree: cwd.clone(),
                branch: branch.clone(),
                base_branch: base_branch.clone(),
            })
        }
        _ => None,
    };

    // The SAME record, re-stamped (EXP-1292: same row, same key). The ACP
    // ids stay until the engine upserts the reopened ones.
    let mut resumed_record = RunRecord {
        cwd: cwd.clone(),
        claude_session_id: claude_resume_id.clone().or_else(|| record.claude_session_id.clone()),
        codex_originator: codex_originator.clone(),
        model: options.model.clone(),
        effort: options.effort.clone(),
        // EXP-1334: a reconfigure may have moved it.
        ultracode: options.ultracode,
        transport: Some(ACP_TRANSPORT.to_string()),
        acp_child_pid: None,
        host_pid: None,
        recorded_at: crate::run_registry::now_secs(),
        ..record.clone()
    };
    // EXP-849: the ACCOUNT this run actually landed on, which on a switch is
    // not the recorded one — the next resume must re-enter the login that
    // now holds the transcript.
    resumed_record.set_account(options.account.as_deref());
    // EXP-849: the row's account this resume honors — a later person's
    // switch is a row that moves past it.
    resumed_record.set_honored_row_account(row.account.as_deref());
    // EXP-1292: the row runs again — an END its last exit owed is moot.
    resumed_record.set_unreported_end(false);
    crate::run_registry::record(&deps.data_dir, resumed_record);

    // EXP-746 (D8): how the ACP engine reopens the conversation — the
    // recorded ACP session id first, else the agent's OWN handle (EXP-758).
    let acp_resume = acp_resume_id.clone().map(ResumeSeed::Acp).or_else(|| {
        claude_resume_id
            .clone()
            .or_else(|| codex_resume_id.clone())
            .map(ResumeSeed::Native)
    });
    let system_append = crate::skill::system_append_layers(
        req.brief.team_prompt.as_deref(),
        req.brief.agent_instructions.as_deref(),
        &run_extras(deps, computer_use, code_mode, record.kind.is_persistent() || dispatches(row)),
    );
    // EXP-1051: a NATIVE resume re-enters a transcript whose base context
    // this launch never built, so the recorded measured number is the only
    // honest one to carry. A resume that fell back to a fresh conversation
    // builds its own base and carries nothing.
    let context_layers = context_layers_for(
        agent,
        &cwd,
        profile_dir.as_deref(),
        &system_append,
        rendered.as_deref(),
        acp_resume.is_some().then(|| record.context_base()).flatten(),
    );
    // FEED-73: the team-MCP delegation note rides AFTER the measured layers.
    let system_append = with_delegation_note(system_append, &team_mcp);
    Ok(Prepared::Ready(PreparedLaunch {
        session_id: session_id.clone(),
        account_pick: None,
        issue_identifier: label.clone(),
        worktree: cwd.clone(),
        clone: record.clone.clone().unwrap_or_else(|| cwd.clone()),
        repository_id: record.repository_id.clone(),
        branch: record.branch.clone().unwrap_or_default(),
        base_branch: run_cleanup.as_ref().map(|cleanup| cleanup.base_branch.clone()),
        base_ref: (!default_branch.is_empty()).then(|| format!("origin/{default_branch}")),
        run_cleanup,
        mcp_warnings: team_mcp.warnings.clone(),
        spawn,
        tab_title: format!("{} · {label}", agent.id()),
        tab_title_prefix: label,
        acp: AcpLaunch {
            prompt: rendered,
            options: options.clone(),
            mcp: agent_mcp,
            session_id,
            resume: acp_resume,
            reaper_settings_path: reaper_anchor,
            system_append,
            context_layers,
            servers: team_mcp.servers.clone(),
            mcp_secrets: McpSecrets::new(team_mcp.secret_values()),
            confine_to: crate::scratch::is_scratch_dir(&deps.data_dir, &cwd).then(|| cwd.clone()),
        },
        seed_message_id: resume.message_id.clone(),
        bypass_permissions: true,
        plan_mode: options.plan_mode,
        agent,
        claude_session_id: None,
        codex_originator,
        codex_resume_id,
        launch_hold,
        mcp_pick: team_mcp.pick.clone(),
    }))
}

/// `~/.claude/projects` unless the caller injected a fixture root. EXP-746:
/// `pub` because the ACP engine replays a claude transcript from the same
/// tree (Past → Replay) and must resolve it exactly as the resume probe does.
pub fn claude_projects_root(deps: &CodingDeps) -> Option<PathBuf> {
    claude_projects_root_in(deps, None)
}

/// [`claude_projects_root`] for ONE account profile (EXP-849): a profile run
/// carries `CLAUDE_CONFIG_DIR=<profile dir>`, so its transcripts live under
/// `<profile dir>/projects`, not under `~/.claude`. `None` = the ambient
/// login, whose tree an injected fixture root stands in for.
pub fn claude_projects_root_in(deps: &CodingDeps, profile_dir: Option<&Path>) -> Option<PathBuf> {
    // A profile's tree is DETERMINED by its config dir — nothing stands in for
    // it, not even an injected fixture (which is the AMBIENT login's tree).
    if let Some(dir) = profile_dir {
        return Some(dir.join("projects"));
    }
    if let Some(root) = &deps.claude_projects_root {
        return Some(root.clone());
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(PathBuf::from(home).join(".claude").join("projects"))
}

/// EXP-849 — hand a claude transcript to ANOTHER account profile, so a resume
/// on that profile's `CLAUDE_CONFIG_DIR` can `--resume` into the conversation
/// the run had. Answers whether the target tree now holds it.
///
/// This copies a TRANSCRIPT and nothing else. A credential file is never
/// copied, snapshotted or restored — two CLIs sharing one credential store is
/// fine, two stores holding the same credential is a revoked login — and the
/// target profile's own `.credentials.json` is never touched.
///
/// Idempotent: a transcript already in the target tree (a switch back and
/// forth) is left exactly as it is, so the NEWER file always wins.
fn copy_claude_transcript(
    deps: &CodingDeps,
    from: Option<&Path>,
    to: Option<&Path>,
    session_id: &str,
) -> bool {
    let Some(target_root) = claude_projects_root_in(deps, to) else {
        return false;
    };
    if locate_claude_transcript(&target_root, session_id).is_some() {
        return true;
    }
    let Some(source) = claude_projects_root_in(deps, from)
        .and_then(|root| locate_claude_transcript(&root, session_id))
    else {
        return false;
    };
    // Keep claude's own per-cwd directory name: it munges the cwd (and
    // hash-suffixes long ones), and a name we invented would not be the one
    // the CLI looks under for THIS cwd.
    let Some(project) = source.parent().and_then(|dir| dir.file_name()) else {
        return false;
    };
    let dir = target_root.join(project);
    if let Err(err) = std::fs::create_dir_all(&dir) {
        log::warn!("coding: could not prepare {} for a resume: {err}", dir.display());
        return false;
    }
    match std::fs::copy(&source, dir.join(format!("{session_id}.jsonl"))) {
        Ok(_) => true,
        Err(err) => {
            log::warn!("coding: could not hand transcript {session_id} to the target account: {err}");
            false
        }
    }
}

/// EXP-761: the ONE way to find a claude transcript — `<root>/*/<session
/// id>.jsonl`, `root` being `~/.claude/projects` (or the child's
/// `CLAUDE_CONFIG_DIR/projects`). Claude names the per-cwd directory by
/// munging the cwd (every non-alphanumeric byte → `-`), but caps the name at
/// roughly 200 chars and appends a 6-char hash beyond that
/// (`…-actions-5ecb3c42-…--71h4ur`), so recomputing the name from the cwd
/// misses exactly the long worktree paths a daemon data dir produces. The
/// session uuid is unique across the tree, so the first hit IS the
/// transcript; nothing here depends on the naming rule. Steer's PTY emitter,
/// the resume probe and the engine's replay all resolve through this.
pub fn locate_claude_transcript(root: &Path, session_id: &str) -> Option<PathBuf> {
    let file_name = format!("{session_id}.jsonl");
    std::fs::read_dir(root)
        .ok()?
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.path().join(&file_name))
        .find(|path| path.is_file())
}

/// Does claude still hold the recorded conversation? (`cwd` is what the
/// record says; the transcript is found by session id, never by cwd —
/// [`locate_claude_transcript`].) EXP-746: `pub` for the engine's replay,
/// like [`claude_projects_root`] above.
pub fn claude_transcript_exists(deps: &CodingDeps, _cwd: &Path, session_id: &str) -> bool {
    claude_transcript_exists_in(deps, None, session_id)
}

/// [`claude_transcript_exists`] against ONE account profile's tree (EXP-849).
pub fn claude_transcript_exists_in(
    deps: &CodingDeps,
    profile_dir: Option<&Path>,
    session_id: &str,
) -> bool {
    claude_projects_root_in(deps, profile_dir)
        .is_some_and(|root| locate_claude_transcript(&root, session_id).is_some())
}

/// FEED-73 — `append` plus, when the run acts through team MCP servers with
/// a member roster, the [`crate::mcp_servers::delegation_note`], blank-line
/// separated. Unchanged otherwise.
fn with_delegation_note(append: String, team_mcp: &ResolvedMcp) -> String {
    match crate::mcp_servers::delegation_note(&team_mcp.members, &team_mcp.servers) {
        Some(note) => format!("{}\n\n{note}", append.trim_end()),
        None => append,
    }
}

/// EXP-1025 — the system-prompt append of an agent SHELL: the playbook plus
/// the team prompt. A shell has no session row, so no `sessions.brief`
/// either; the team prompt comes from `teams.getAgentPrompt` for the team
/// the caller is showing. BEST-EFFORT: no team, a team without a prompt or
/// a failed fetch degrades to the bare playbook and is logged, never a
/// refused shell.
fn shell_system_append(deps: &CodingDeps, team_id: Option<&str>) -> String {
    let extras = crate::skill::Extras::NONE;
    let Some(team_id) = team_id.map(str::trim).filter(|id| !id.is_empty()) else {
        return crate::skill::system_append_with(None, &extras);
    };
    match api::teams::teams_get_agent_prompt(&deps.trpc, team_id) {
        Ok(prompt) => crate::skill::system_append_with(Some(&prompt.agent_prompt), &extras),
        Err(err) => {
            log::warn!("coding: team prompt for {team_id} unavailable, launching without it: {err}");
            crate::skill::system_append_with(None, &extras)
        }
    }
}

/// EXP-1051 — the measured context layers for ONE launch ([`AcpLaunch::
/// context_layers`]). Every number is a BYTE COUNT of something this
/// launcher itself composed:
///
/// - the playbook, verbatim ([`crate::skill::playbook`]);
/// - the brief's team prompt and agent instructions, read back off the
///   composed `system_append` (which is what actually reaches the agent);
/// - the seed prompt, `None` when there is none;
/// - the project memory files the agent loads on its own from `cwd`;
/// - a resume's carried base from the previous run, when one was recorded.
///
/// EXP-1292: no tools segment — the `codingSessions.contextBudget` query it
/// read went with the old session router, and the v2 router has none.
pub fn context_layers_for(
    agent: CodingAgent,
    cwd: &Path,
    profile_dir: Option<&Path>,
    system_append: &str,
    prompt: Option<&str>,
    carried_base: Option<CarriedBase>,
) -> ContextLayers {
    ContextLayers {
        playbook_bytes: context_layout::playbook_bytes(system_append),
        team_bytes: context_layout::team_bytes(system_append),
        task_bytes: prompt.map(str::len),
        project: context_layout::project_memory(agent, cwd, profile_dir),
        tools_bytes: None,
        carried_base,
    }
}

/// An EXP-325 promptless agent-shell launch input: the terminal dock's "+"
/// menu picked an installed agent and a repo; the agent spawns as a FRESH
/// interactive session on the repo's trunk clone with the picked agent's
/// settings defaults and the usual MCP wiring — but NO session
/// subject. Deliberately no `sessions` row / exit hook / steer room: a
/// shell is a local terminal, so teammates never see a badge for it.
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
    /// EXP-1025: the active team, for its team prompt. A shell creates no
    /// session row (nothing server-side names a team for it), so the caller
    /// passes the team it is showing; `None` = the bare playbook.
    pub team_id: Option<String>,
}

/// [`prepare_agent_shell`] done: everything the foreground needs to open the
/// tab ([`TabKind::AgentShell`], no exit hook).
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

/// The EXP-325 promptless prepare — the repo-backed half of a session
/// launch minus the prompt and the session row (blocking network/git I/O, gpui-free
/// — run on the background executor): doctor → JIT token → clone/ambient
/// auth → best-effort autopull → per-agent MCP wiring → the promptless
/// [`shell_args`] argv.
pub fn prepare_agent_shell(
    req: &AgentShellRequest,
    deps: &CodingDeps,
) -> Result<PreparedAgentShell, CodingError> {
    let mut options = req.options.clone();
    let agent = options.agent;
    // EXP-1138: a shell on a removed ambient login opens on the device
    // default instead (the same rule a session launch takes).
    let report = run_doctor(&deps.settings, &deps.data_dir);
    resolve_launch_account(&mut options, deps, report.check_for(agent));
    let options = &options;

    // Doctor — the picked agent AND git (a trunk clone is always involved),
    // then the login the shell opens on.
    if let Some(failed) = doctor_gate(&report, deps, agent, options.account.as_deref(), true) {
        return Ok(PreparedAgentShell::Disabled(DisabledReason::DoctorFailed(failed)));
    }

    // §7.2 — the personal key (the MCP credential), raced like a session's.
    let key_handle = {
        let trpc = Arc::clone(&deps.trpc);
        let store = Arc::clone(&deps.token_store);
        let account_id = deps.account_id.clone();
        std::thread::spawn(move || users::ensure_personal_key(&trpc, &store, &account_id))
    };

    // JIT token → clone → ambient auth → best-effort autopull (the shell
    // works on whatever the trunk holds and the trunk-sync engine keeps
    // surfacing its state).
    let minted = match crate::token_cache::token_cache().get_or_mint_with_margin(
        &deps.trpc,
        &req.repository_id,
        // A promptless shell is trunk work, not a board's — repo-level branch.
        None,
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
    // one — the MCP config file has to land in the SAME dir (claude reads it
    // relative to its cwd).
    let cwd = agent_shell_cwd(req, &clone);

    // The per-agent MCP wiring (the agent authenticates as the real user).
    let personal_key = key_handle
        .join()
        .map_err(|_| CodingError::Io("personal-key thread panicked".to_string()))??;
    let agent_mcp = wire_agent_mcp(
        agent,
        &cwd,
        deps.trpc.base_url(),
        &personal_key,
        None,
        // EXP-773: an agent shell is a TERMINAL tab by definition (EXP-325)
        // — the ONE remaining consumer of claude's cwd `.exp-mcp.json`.
        true,
    )?;

    // The spawn spec: no prompt, no hooks sidecar (hooks are per-session —
    // there is no session row to scope one to). EXP-389: same codex
    // directory pre-trust as a session (an EXP-369 worktree cwd resolves to
    // the clone anyway).
    // EXP-792: a shell on a profile trusts + runs inside that profile's dir.
    let profile_dir =
        crate::agent_profiles::account_dir(&deps.data_dir, Some(agent), options.account.as_deref());
    if agent == CodingAgent::Codex {
        crate::codex_trust::ensure_trusted(&clone, profile_dir.as_deref());
    }
    // EXP-414: claude keys trust by the spawn cwd itself.
    if agent == CodingAgent::Claude {
        crate::claude_trust::ensure_onboarded(&cwd, true, profile_dir.as_deref());
    }
    let system_append = shell_system_append(deps, req.team_id.as_deref());
    let args = shell_args(options, &agent_mcp, &system_append);
    let tab_title = agent_shell_tab_title(agent, req, &cwd);
    let mut spawn = SpawnSpec::new(&deps.settings.resolved_path_for(agent))
        .args(args)
        .cwd(&cwd);
    // Agent shells are always interactive TUIs (never the ACP engine).
    spawn = apply_mcp_env(spawn, agent, &personal_key, None, true);
    spawn = apply_account_env(spawn, agent, &deps.data_dir, options.account.as_deref());
    spawn = apply_subagent_model_env(spawn, agent, &options.subagent_model);
    if agent == CodingAgent::Codex {
        // EXP-443: shells share the trunk cwd — a distinct originator keeps
        // their rollouts out of every session's strict pass.
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

/// EXP-640: observer for the outcome of EVERY end this crate (and, via
/// [`end_session`], the host) reports. The desktop's crash-recovery registry
/// hangs off it: an entry is dropped only once the end actually RESOLVED —
/// an end rejected by the server's 426 min-version gate must leave the
/// entry for the reconcile to retry.
pub type SessionEndObserver = Arc<dyn Fn(&str, &Result<(), ApiError>) + Send + Sync>;

static SESSION_END_OBSERVER: OnceLock<SessionEndObserver> = OnceLock::new();

/// Install the process-wide [`SessionEndObserver`]. First caller wins; a
/// later call is a no-op (the host installs it once at bootstrap).
pub fn set_session_end_observer(observer: SessionEndObserver) {
    let _ = SESSION_END_OBSERVER.set(observer);
}

/// EXP-1292 — a THREAD session's agent process is gone:
/// `sessions.report{status: ended}`, with the outcome reported to the
/// [`SessionEndObserver`]. Every end this crate issues goes through here so
/// the host's registry sees each outcome exactly where it happens. A team
/// agent never ends — its host calls [`idle_session`] instead.
pub fn end_session(trpc: &TrpcClient, session_id: &str) -> Result<(), ApiError> {
    let result = report_status(trpc, session_id, ReportStatus::Ended);
    if let Some(observer) = SESSION_END_OBSERVER.get() {
        observer(session_id, &result);
    }
    result
}

/// Best-effort session end for teardown paths that bypass the engine's own
/// end (app quit with a live child, a spawn that failed after prepare).
/// Idempotent server-side; the outcome still reaches the
/// [`SessionEndObserver`].
pub fn end_session_best_effort(trpc: &TrpcClient, session_id: &str) {
    let _ = end_session(trpc, session_id);
}

/// EXP-1292 — a team AGENT's process exited: `sessions.report{status:
/// idle}`. Agents never end; the next message or trigger firing wakes the
/// same row on this device.
///
/// #FEED-85: the idle report also clears `busy` — a session idled here (a
/// refused wake, a revive given up) is never left spinning on every client.
pub fn idle_session(trpc: &TrpcClient, session_id: &str) -> Result<(), ApiError> {
    session_device::report(trpc, session_id, &idle_report())
}

/// #FEED-85: [`idle_session`]'s write — idle and not busy.
pub fn idle_report() -> SessionReport {
    SessionReport {
        status: Some(ReportStatus::Idle),
        busy: Some(false),
        ..SessionReport::default()
    }
}

fn report_status(trpc: &TrpcClient, session_id: &str, status: ReportStatus) -> Result<(), ApiError> {
    let report = SessionReport {
        status: Some(status),
        ..SessionReport::default()
    };
    session_device::report(trpc, session_id, &report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        canned_server, canned_server_recording, make_deps, temp_dir, FakeWorktrees, MINT_OK,
        REPORT_OK, RESOLVE_NONE, TOKEN_OK,
    };
    use api::token_store::SecretKind;
    use std::fs;

    const SESSION_ID: &str = "5ECB3C42-1f2e-4c7a-9d1b-0a7b6c5d4e3f";

    /// A claimed THREAD row (EXP-1292) — the runner pickers the dialog
    /// defaults used to carry: claude, fable, plan mode ON.
    fn row(id: &str, repository_id: Option<&str>) -> SessionRow {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "team_id": "ws-1",
            "kind": "thread",
            "device_id": "dev-1",
            "user_id": "user-1",
            "cli": "claude",
            "model": "fable",
            "plan_mode": true,
            "repository_id": repository_id,
            "status": "running",
            "title": "Code review",
        }))
        .unwrap()
    }

    /// The host defaults the row's pickers layer over.
    fn base_options() -> LaunchOptions {
        LaunchOptions {
            agent: CodingAgent::Claude,
            model: "fable".to_string(),
            effort: String::new(),
            ultracode: false,
            plan_mode: false,
            subagent_model: String::new(),
            mcp_server_ids: Vec::new(),
            account: None,
            computer_use: None,
        }
    }

    const BRIEF: &str = "Work on EXP-42: Fix login flicker.\n\n## Request\n\nThe login page flickers.";

    fn session_request(row: SessionRow, identifiers: &[&str]) -> SessionLaunchRequest {
        SessionLaunchRequest {
            session: row,
            brief: SessionBrief {
                brief: BRIEF.to_string(),
                ..SessionBrief::default()
            },
            issue_identifiers: identifiers.iter().map(|id| id.to_string()).collect(),
            resume: None,
            device_label: "testbox".to_string(),
            options: base_options(),
        }
    }

    fn ready(prepared: Result<Prepared, CodingError>) -> PreparedLaunch {
        match prepared.unwrap() {
            Prepared::Ready(prepared) => prepared,
            Prepared::Disabled(reason) => panic!("unexpectedly disabled: {reason:?}"),
        }
    }

    fn launch(req: SessionLaunchRequest, deps: &CodingDeps) -> Result<Prepared, CodingError> {
        prepare(&PrepareRequest::Session(req), deps)
    }

    /// The seed prompt the ACP engine sends as the run's first message.
    fn seed_prompt(prepared: &PreparedLaunch) -> &str {
        prepared.acp.prompt.as_deref().expect("a seed prompt")
    }

    /// A repo-backed fresh launch's canned calls: the token mint, the branch
    /// report, the team-server resolve.
    fn repo_calls() -> Vec<(u16, String)> {
        vec![
            (200, TOKEN_OK.to_string()),
            (200, REPORT_OK.to_string()),
            (200, RESOLVE_NONE.to_string()),
        ]
    }

    fn fake_worktrees(worktree: &Path) -> Arc<FakeWorktrees> {
        Arc::new(FakeWorktrees {
            worktree: worktree.to_path_buf(),
            seen: Default::default(),
        })
    }


    /// EXP-752: nothing about plan mode gates a launch — a plan-mode run is
    /// as ACP-ready as any other.
    #[test]
    fn plan_mode_is_acp_ready_like_any_other_launch() {
        let dir = temp_dir("plan-acp");
        let base = canned_server(Vec::new());
        let worktrees = Arc::new(FakeWorktrees {
            worktree: dir.0.join("unused"),
            seen: Default::default(),
        });
        let deps = make_deps(&base, &dir.0, worktrees);
        let ready = |tool| ToolCheck {
            tool,
            ok: true,
            version: Some("1.0.0".to_string()),
            error: None,
            authed: None,
            account: None,
            usage_eligible: false,
            signed_in_profile: None,
            importable: None,
            profiles: Vec::new(),
            acp: Some(true),
            acp_note: None,
        };
        let report = crate::doctor::DoctorReport {
            claude: ready(crate::doctor::Tool::Claude),
            codex: ready(crate::doctor::Tool::Codex),
            git: ready(crate::doctor::Tool::Git),
        };
        let codex = CodingAgent::Codex;
        let claude = CodingAgent::Claude;
        assert!(acp_ready(&report, codex, &deps));
        assert!(acp_ready(&report, claude, &deps));
        // A host with no engine is never ready, whatever the agent says.
        let mut hostless = make_deps(&base, &dir.0, Arc::new(FakeWorktrees {
            worktree: dir.0.join("unused"),
            seen: Default::default(),
        }));
        hostless.acp_available = false;
        assert!(!acp_ready(&report, codex, &hostless));
        assert!(!acp_ready(&report, claude, &hostless));
        assert!(acp_gate(&report, claude, &deps).is_none());
        assert!(matches!(
            acp_gate(&report, claude, &hostless),
            Some(DisabledReason::AcpUnavailable { .. })
        ));
        // The mode is no part of the gate (claude's plan default is ON, so
        // this is the common launch).
        assert!(LaunchOptions::defaults_for(&deps.settings, CodingAgent::Claude).plan_mode);
    }

    /// A stub `claude` that answers `--version` with an ACP-ready version and
    /// nothing else — the doctor then reports `acp: Some(true)` without any
    /// real CLI on the machine.
    #[cfg(unix)]
    fn acp_ready_claude_stub(dir: &Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let stub = dir.join("bin").join("claude-acp-stub");
        fs::create_dir_all(stub.parent().unwrap()).unwrap();
        fs::write(
            &stub,
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo '9.9.9 (Claude Code stub)'; exit 0; fi\nexit 0\n",
        )
        .unwrap();
        fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).unwrap();
        // EXP-781: same ETXTBSY fork race as `test_support::acp_ready_stub` —
        // see the note there.
        crate::test_support::wait_until_executable(&stub);
        stub
    }

    /// EXP-1124: the anchor is also the flag-settings layer that keeps a
    /// user `defaultMode: auto` out of plan runs — auto disabled, and plan
    /// never taking auto-mode semantics.
    #[test]
    fn claude_flag_settings_disable_auto_mode() {
        let settings: serde_json::Value =
            serde_json::from_str(CLAUDE_FLAG_SETTINGS).expect("valid json");
        assert_eq!(settings["permissions"]["disableAutoMode"], "disable");
        assert!(settings.get("useAutoModeDuringPlan").is_none(), "EXP-1280: no CLI plan mode");
    }

    /// EXP-761: claude caps a long cwd's project dir name (~200 chars) and
    /// appends a hash; the transcript is found by session id under ANY
    /// project dir, and the recomputed full-length name is never consulted.
    #[test]
    fn locate_claude_transcript_finds_a_hash_suffixed_project_dir() {
        let dir = temp_dir("locate-transcript");
        let projects = dir.0.join("projects");
        let cwd = format!(
            "/private/tmp/{}/daemon-data/actions/5ecb3c42-1f2e-4c7a-9d1b-0a7b6c5d4e3f/wt",
            "x".repeat(160)
        );
        assert!(cwd.len() > 200, "{}", cwd.len());
        let munged: String = cwd
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        // What claude actually creates: the first ~200 chars plus `-<hash>`.
        let capped = format!("{}--71h4ur", &munged[..200]);
        let project_dir = projects.join(&capped);
        fs::create_dir_all(&project_dir).unwrap();
        fs::write(project_dir.join("sess-1.jsonl"), "{}\n").unwrap();
        // Noise: a stray file at the root, an empty sibling dir, and a
        // sibling holding a DIFFERENT session.
        fs::write(projects.join("notes.txt"), "").unwrap();
        fs::create_dir_all(projects.join("-other")).unwrap();
        fs::write(projects.join("-other").join("sess-2.jsonl"), "{}\n").unwrap();

        assert_eq!(
            locate_claude_transcript(&projects, "sess-1"),
            Some(project_dir.join("sess-1.jsonl"))
        );
        assert!(!projects.join(&munged).exists(), "the full name is never created");
        assert_eq!(locate_claude_transcript(&projects, "sess-3"), None);
        assert_eq!(locate_claude_transcript(&dir.0.join("missing"), "sess-1"), None);
        let mut deps = make_deps(
            &canned_server(vec![]),
            &dir.0,
            Arc::new(FakeWorktrees {
                worktree: dir.0.join("unused"),
                seen: Default::default(),
            }),
        );
        deps.claude_projects_root = Some(projects);
        assert!(claude_transcript_exists(&deps, Path::new(&cwd), "sess-1"));
        assert!(!claude_transcript_exists(&deps, Path::new(&cwd), "sess-2-missing"));
    }

    /// EXP-773: the EXP-474 ignore guard belongs to the FILE-writing arm — the
    /// agent SHELL. A repo whose committed `.gitignore` re-includes
    /// `.exp-mcp.json` still refuses a shell there, but a coding run writes no
    /// file at all (the key rides the child env), so it must launch.
    #[cfg(unix)]

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

        let err = wire_agent_mcp(CodingAgent::Claude, &repo, "http://localhost:1", "expu_x", None, true)
            .unwrap_err();
        assert!(matches!(err, CodingError::Git(_)), "wrong error: {err:?}");
        assert!(!repo.join(crate::mcp_json::MCP_JSON_FILE).exists(), "key landed on disk");
    }

    /// EXP-746: the ACP arm never writes the key file — the engine passes the
    /// MCP server inline and the key rides the child env.
    #[test]
    fn wire_agent_mcp_on_the_acp_arm_keeps_the_claude_key_off_disk() {
        let dir = temp_dir("mcp-acp-inline");
        let cwd = dir.0.join("wt");
        fs::create_dir_all(&cwd).unwrap();
        let mcp = wire_agent_mcp(
            CodingAgent::Claude,
            &cwd,
            "http://localhost:1/",
            "expu_x",
            Some("sid-1"),
            false,
        )
        .unwrap();
        assert!(
            matches!(&mcp, AgentMcp::ClaudeInline { url, session_id }
                if url == "http://localhost:1/api/mcp" && session_id.as_deref() == Some("sid-1")),
            "wrong wiring: {mcp:?}"
        );
        assert!(!cwd.join(crate::mcp_json::MCP_JSON_FILE).exists(), "key landed on disk");
    }

    #[test]
    fn apply_mcp_env_gives_claude_the_token_only_on_the_acp_arm() {
        let base = SpawnSpec::new("claude");
        let shell = apply_mcp_env(base.clone(), CodingAgent::Claude, "expu_k", Some("s"), true);
        assert!(!shell.env.iter().any(|(k, _)| k == MCP_TOKEN_ENV), "the agent shell keeps the key in the file");
        let acp = apply_mcp_env(base, CodingAgent::Claude, "expu_k", Some("s"), false);
        assert!(acp.env.iter().any(|(k, v)| k == MCP_TOKEN_ENV && v == "expu_k"));
        assert!(acp.env.iter().any(|(k, v)| k == MCP_SESSION_ID_ENV && v == "s"));
    }

    /// EXP-981: a claude subagent pick exports `CLAUDE_CODE_SUBAGENT_MODEL`
    /// with the SAME value `--model` would take; a blank pick (the CLI's own
    /// default) and every non-claude agent export nothing at all.
    #[test]
    fn apply_subagent_model_env_sets_the_claude_var_only_when_picked() {
        let base = SpawnSpec::new("agent").env("KEEP", "1");
        let picked = apply_subagent_model_env(base.clone(), CodingAgent::Claude, "sonnet");
        assert_eq!(picked.env.len(), 2, "{:?}", picked.env);
        assert_eq!(
            picked.env[1],
            ("CLAUDE_CODE_SUBAGENT_MODEL".to_string(), "sonnet".to_string())
        );
        // The value is the `--model` flag's, so the two can never drift.
        let flag = crate::argv::shell_args(
            &LaunchOptions {
                model: "sonnet".to_string(),
                ..LaunchOptions::defaults_for(&Settings::default(), CodingAgent::Claude)
            },
            &crate::argv::AgentMcp::ClaudeFile,
            crate::skill::RUN_SKILL,
        );
        let model_at = flag.iter().position(|arg| arg == "--model").expect("--model");
        assert_eq!(flag[model_at + 1], picked.env[1].1);

        // Blank = the CLI's own default: nothing exported.
        for blank in ["", "   "] {
            let spawn = apply_subagent_model_env(base.clone(), CodingAgent::Claude, blank);
            assert_eq!(spawn.env, base.env, "{blank:?}");
        }
        // Codex has no subagent model to pin.
        let codex = apply_subagent_model_env(base.clone(), CodingAgent::Codex, "sonnet");
        assert_eq!(codex.env, base.env);
    }

    /// EXP-792 (EXP-747 B2): a profile pick sets the agent's config-dir
    /// variable and NOTHING else; `None`/`system` and an unknown id set
    /// nothing.
    #[test]
    fn apply_account_env_sets_only_the_config_dir_var() {
        let data_dir = std::env::temp_dir().join(format!(
            "exp-account-env-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&data_dir).unwrap();
        let work = crate::agent_profiles::create(&data_dir, CodingAgent::Claude).unwrap();
        let codex_work = crate::agent_profiles::create(&data_dir, CodingAgent::Codex).unwrap();
        let base = SpawnSpec::new("agent").env("KEEP", "1");
        let claude = CodingAgent::Claude;

        let picked = apply_account_env(base.clone(), claude, &data_dir, Some(&work.id));
        assert_eq!(picked.env.len(), 2, "{:?}", picked.env);
        assert_eq!(
            picked.env[1],
            (
                "CLAUDE_CONFIG_DIR".to_string(),
                data_dir.join("agents").join("claude").join(&work.id).to_string_lossy().into_owned()
            )
        );
        let codex = apply_account_env(base.clone(), CodingAgent::Codex, &data_dir, Some(&codex_work.id));
        assert_eq!(codex.env[1].0, "CODEX_HOME");

        for none in [None, Some("system"), Some(""), Some("deadbeef")] {
            let spawn = apply_account_env(base.clone(), claude, &data_dir, none);
            assert_eq!(spawn.env, base.env, "{none:?}");
        }
        let _ = std::fs::remove_dir_all(&data_dir);
    }

    /// FEED-25: a claude child carries a finite per-call MCP timeout on BOTH
    /// arms — the agent shell and the ACP session (the CLI's own default is
    /// 27 hours); codex has its own bounds and never sees the
    /// variable.
    #[test]
    fn apply_mcp_env_bounds_claude_mcp_calls_on_both_arms() {
        let timeout = |spawn: &SpawnSpec| {
            spawn
                .env
                .iter()
                .find(|(k, _)| k == CLAUDE_MCP_TOOL_TIMEOUT_ENV)
                .map(|(_, v)| v.clone())
        };
        let base = SpawnSpec::new("agent");
        for shell in [true, false] {
            let claude = apply_mcp_env(base.clone(), CodingAgent::Claude, "expu_k", Some("s"), shell);
            // The host's own value wins when set; the test process may carry
            // one, so only the shape is pinned here — the pure helper below
            // pins the fill-the-gap rule.
            if std::env::var_os(CLAUDE_MCP_TOOL_TIMEOUT_ENV).is_none_or(|v| v.is_empty()) {
                assert_eq!(timeout(&claude), Some(CLAUDE_MCP_TOOL_TIMEOUT_MS.to_string()), "shell={shell}");
            }
            let codex = apply_mcp_env(base.clone(), CodingAgent::Codex, "expu_k", Some("s"), shell);
            assert_eq!(timeout(&codex), None, "shell={shell}");
        }
    }

    #[test]
    fn the_claude_mcp_timeout_only_fills_the_gap() {
        let base = SpawnSpec::new("claude");
        let filled = with_claude_mcp_timeout(base.clone(), None);
        assert_eq!(
            filled.env,
            vec![(CLAUDE_MCP_TOOL_TIMEOUT_ENV.to_string(), "120000".to_string())]
        );
        // An empty inherited value is no value.
        let empty = with_claude_mcp_timeout(base.clone(), Some(std::ffi::OsString::new()));
        assert_eq!(empty.env, filled.env);
        // A tuned host keeps its own number: nothing is pushed, so the child
        // inherits the parent's variable untouched.
        let tuned = with_claude_mcp_timeout(base, Some(std::ffi::OsString::from("30000")));
        assert!(tuned.env.is_empty(), "{:?}", tuned.env);
    }

    /// The repo-less action-scratch flow keeps working: no governing work
    /// tree means nothing can stage the file, so the guard passes and the
    /// config is written.
    #[test]
    fn wire_agent_mcp_writes_the_key_file_in_a_repo_less_scratch_dir() {
        let dir = temp_dir("mcp-scratch");
        let wired = wire_agent_mcp(CodingAgent::Claude, &dir.0, "http://localhost:1", "expu_x", None, true)
            .unwrap();
        assert_eq!(wired, AgentMcp::ClaudeFile);
        assert!(dir.0.join(crate::mcp_json::MCP_JSON_FILE).exists());
    }

    // ---- EXP-1292: the branch, the label, the layered options ----

    #[test]
    fn the_branch_rule_is_the_rows_then_one_issue_then_the_session_id() {
        let mut fresh = row(SESSION_ID, Some("repo-1"));
        let one = ["EXP-42".to_string()];
        assert_eq!(session_branch(&fresh, &one, "exp/", None), "exp/EXP-42");
        // Several issues (or none): the session's own, lowercased so the
        // server's identifier parse can never link it.
        let two = ["EXP-42".to_string(), "EXP-43".to_string()];
        assert_eq!(session_branch(&fresh, &two, "exp/", None), "exp/session-5ecb3c42");
        assert_eq!(session_branch(&fresh, &[], "exp/", None), "exp/session-5ecb3c42");
        assert!(SESSION_BRANCH_PREFIX.starts_with("exp/"));
        // A row that already runs on a branch keeps it (a reclaimed row).
        fresh.branch = Some("exp/EXP-7".to_string());
        assert_eq!(session_branch(&fresh, &two, "exp/", None), "exp/EXP-7");
        fresh.branch = Some("  ".to_string());
        assert_eq!(session_branch(&fresh, &[], "exp/", None), "exp/session-5ecb3c42");
        // EXP-1292: a child of a CODING parent is handed its ROOT's issue,
        // whose branch the parent runs on: the child takes its own.
        let mut child = row(SESSION_ID, Some("repo-1"));
        child.parent_id = Some("parent-1".to_string());
        assert_eq!(
            session_branch(&child, &one, "exp/", Some("exp/EXP-42")),
            "exp/session-5ecb3c42"
        );
        // A child whose brief names no parent branch takes the issue branch.
        assert_eq!(session_branch(&child, &one, "exp/", None), "exp/EXP-42");
        assert_eq!(session_branch(&child, &one, "exp/", Some("  ")), "exp/EXP-42");
        assert_eq!(session_branch(&child, &two, "exp/", None), "exp/session-5ecb3c42");
        // NEW-1: every root owns its issue work.
        assert!(owns_issue_work(&row(SESSION_ID, Some("repo-1")), None));
        assert!(owns_issue_work(&row(SESSION_ID, None), None));
        assert!(owns_issue_work(&child, None) && !owns_issue_work(&child, Some("exp/x")));
    }

    /// Who reads the agents part of the playbook: exactly an agent's
    /// conversation session (the persistent kind).
    #[test]
    fn only_agent_conversations_read_the_agents_playbook() {
        let thread = row("t-1", None);
        assert!(!dispatches(&thread));
        let mut agent = row("a-1", None);
        agent.agent_id = Some("ag-1".to_string());
        assert!(dispatches(&agent));
        let mut child = row("c-1", None);
        child.parent_id = Some("t-1".to_string());
        assert!(!dispatches(&child));
    }

    /// NEW-1: a root's own pickers decide its options; every launch carries
    /// an explicit model.
    #[test]
    fn a_roots_pickers_decide_the_options() {
        let dir = temp_dir("dispatch-options");
        let mut deps = make_deps("http://127.0.0.1:1", &dir.0, fake_worktrees(&dir.0.join("wt")));
        let mut root = row("d-1", None);
        root.model = Some("opus".to_string());
        root.effort = Some("max".to_string());
        root.ultracode = true;
        let host = base_options();
        let plain = session_launch_options(&root, &host, &deps.settings);
        assert_eq!(plain.model, "opus");
        assert_eq!(plain.effort, "max");
        assert!(plain.ultracode);
        // A blank model anywhere still launches explicit.
        let mut blank_host = base_options();
        blank_host.model = String::new();
        let mut plain = row("p-1", None);
        plain.model = None;
        assert_eq!(session_launch_options(&plain, &blank_host, &deps.settings).model, "fable");
        plain.cli = Some("codex".to_string());
        deps.settings.codex_model = String::new();
        assert_eq!(session_launch_options(&plain, &blank_host, &deps.settings).model, "gpt-5.6-sol");
    }

    /// EXP-1341: the label = THE session title over what the launch knows
    /// (title, the brief-titled issues, the request's first line), never a generic `Session`.
    #[test]
    fn the_label_is_the_session_title() {
        let brief = "## Request\n\n![shot](/api/attachments/x)\nSpeed it up\n\n## Issues\n\n- EXP-42 Cold start is slow\n- EXP-43 Lag";
        let mut thread = row("s-1", None);
        assert_eq!(session_label(&thread, &["EXP-42".to_string()], brief), "Code review");
        thread.title = None;
        assert_eq!(
            session_label(&thread, &["EXP-42".to_string()], brief),
            "#EXP-42 Cold start is slow"
        );
        assert_eq!(
            session_label(
                &thread,
                &["EXP-42".to_string(), "EXP-43".to_string(), "EXP-44".to_string()],
                brief
            ),
            "#EXP-42 Cold start is slow + 2"
        );
        // An issue the brief does not title reads by its identifier alone.
        assert_eq!(session_label(&thread, &["EXP-9".to_string(), "EXP-42".to_string()], ""), "#EXP-9 + 1");
        assert_eq!(session_label(&thread, &[], brief), "Speed it up");
        assert_eq!(session_label(&thread, &[], "## Issues\n\n- not-an-id"), "Untitled");
        let mut agent = row("a-1", None);
        agent.agent_id = Some("ag-1".to_string());
        agent.title = Some("Inbox watcher".to_string());
        assert_eq!(session_label(&agent, &[], ""), "Inbox watcher");
    }

    /// The row's pickers win over the host's defaults, normalized against the
    /// agent's closed sets and masked by its capabilities.
    #[test]
    fn the_rows_pickers_layer_over_the_host_defaults() {
        let dir = temp_dir("options-layering");
        let deps = make_deps("http://127.0.0.1:1", &dir.0, fake_worktrees(&dir.0.join("wt")));
        let mut picked = row("s-1", None);
        picked.effort = Some("HIGH".to_string());
        picked.ultracode = true;
        picked.computer_use = true;
        picked.account = Some(" 0a1b2c3d ".to_string());
        let options = session_launch_options(&picked, &base_options(), &deps.settings);
        assert_eq!(options.agent, CodingAgent::Claude);
        assert_eq!(options.model, "fable");
        assert_eq!(options.effort, "high");
        assert!(options.plan_mode && options.ultracode);
        assert_eq!(options.computer_use, Some(true));
        assert_eq!(options.account.as_deref(), Some("0a1b2c3d"));

        // A model outside the set keeps the default; blank pickers keep the
        // host's; no account on the row keeps the host's.
        let mut sparse = row("s-2", None);
        sparse.model = Some("not-a-model".to_string());
        sparse.plan_mode = false;
        let mut host = base_options();
        host.effort = "low".to_string();
        host.account = Some("host-profile".to_string());
        let options = session_launch_options(&sparse, &host, &deps.settings);
        assert_eq!(options.model, "fable");
        assert_eq!(options.effort, "low");
        assert!(!options.plan_mode);
        assert_eq!(options.computer_use, Some(false));
        assert_eq!(options.account.as_deref(), Some("host-profile"));

        // Another agent starts from ITS defaults; codex never carries
        // ultracode (EXP-1280: plan mode is every agent's, the row's pick).
        let mut codex = row("s-3", None);
        codex.cli = Some("codex".to_string());
        codex.model = Some("gpt-5.6-sol".to_string());
        codex.ultracode = true;
        let options = session_launch_options(&codex, &base_options(), &deps.settings);
        assert_eq!(options.agent, CodingAgent::Codex);
        assert_eq!(options.model, "gpt-5.6-sol");
        assert!(!options.ultracode);
        assert_eq!(options.plan_mode, codex.plan_mode);
    }

    // ---- the fresh session happy path ----

    /// EXP-1292: one claimed row with one issue — the brief is the seed
    /// prompt verbatim, the worktree sits on `exp/<IDENTIFIER>`, the branch
    /// is reported once the worktree exists, and no row is created.
    #[cfg(unix)]
    #[test]
    fn prepare_session_full_sequence() {
        let dir = temp_dir("session-happy");
        let worktree = dir.0.join("wt");
        fs::create_dir_all(&worktree).unwrap();
        let (base, captured) = canned_server_recording(repo_calls());
        let worktrees = fake_worktrees(&worktree);
        let mut deps = make_deps(&base, &dir.0, worktrees.clone());
        deps.settings.claude_path = acp_ready_claude_stub(&dir.0).to_string_lossy().into_owned();

        let prepared = ready(launch(session_request(row("sess-1", Some("repo-happy")), &["EXP-42"]), &deps));

        assert_eq!(prepared.session_id, "sess-1");
        assert_eq!(prepared.acp.session_id, "sess-1");
        assert_eq!(prepared.branch, "exp/EXP-42");
        assert_eq!(prepared.worktree, worktree);
        assert_eq!(prepared.tab_title, "claude · Code review");
        assert_eq!(prepared.tab_title_prefix, "Code review");
        assert_eq!(prepared.issue_identifier, "Code review");
        assert_eq!(prepared.repository_id.as_deref(), Some("repo-happy"));
        assert_eq!(prepared.clone, dir.0.join("repos").join("acme").join("web"));
        assert_eq!(prepared.base_ref.as_deref(), Some("origin/main"));
        // An issue's worktree is the prune's, never the run cleanup's.
        assert!(prepared.run_cleanup.is_none());
        assert_eq!(prepared.base_branch, None);
        assert_eq!(prepared.seed_message_id, None);

        // The spawn: configured program, worktree cwd, NO argv (the ACP
        // adapter composes it); the brief is the first message, verbatim.
        assert_eq!(prepared.spawn.program, deps.settings.claude_path);
        assert!(prepared.spawn.args.is_empty(), "{:?}", prepared.spawn.args);
        assert_eq!(prepared.spawn.cwd.as_deref(), Some(worktree.as_path()));
        assert_eq!(seed_prompt(&prepared), BRIEF);
        assert_eq!(prepared.acp.options.model, "fable");
        assert!(prepared.acp.options.plan_mode, "the row's pick");
        // EXP-1280: plan mode is Exponential's flag; the CLI always bypasses.
        assert!(prepared.bypass_permissions && prepared.plan_mode);
        assert!(prepared.acp.resume.is_none());

        let seen = worktrees.seen.lock().unwrap().clone();
        assert_eq!(
            seen,
            vec![(
                "acme/web".to_string(),
                "main".to_string(),
                "exp/EXP-42".to_string(),
                Some("2026-07-03T12:55:00.000Z".to_string())
            )]
        );

        // Claude's MCP server rides INLINE, keyed to the row; the key never
        // lands on disk.
        assert!(!worktree.join(crate::mcp_json::MCP_JSON_FILE).exists());
        match &prepared.acp.mcp {
            AgentMcp::ClaudeInline { url, session_id } => {
                assert_eq!(url, &mcp_url(&base));
                assert_eq!(session_id.as_deref(), Some("sess-1"));
            }
            other => panic!("expected the inline MCP posture, got {other:?}"),
        }
        assert!(prepared
            .spawn
            .env
            .contains(&(MCP_TOKEN_ENV.to_string(), "expu_seeded".to_string())));
        // The reaper's anchor carries the flag settings.
        let anchor = prepared.acp.reaper_settings_path.as_ref().expect("reaper anchor");
        assert_eq!(fs::read_to_string(anchor).unwrap(), CLAUDE_FLAG_SETTINGS);
        assert!(anchor.starts_with(dir.0.join("claude-hooks")), "{anchor:?}");

        // The wire: mint, branch report, resolve — and never a row creation.
        let requests = captured.lock().unwrap();
        assert_eq!(requests.len(), 3, "{requests:?}");
        assert!(requests[0].contains("repositories.installationToken"), "{}", requests[0]);
        assert!(requests[1].starts_with("POST /api/trpc/sessions.report"), "{}", requests[1]);
        assert!(
            requests[1].ends_with(r#"{"sessionId":"sess-1","branch":"exp/EXP-42"}"#),
            "{}",
            requests[1]
        );
        assert!(requests[2].contains("mcpServers.resolveForLaunch"), "{}", requests[2]);
        assert!(!requests[2].contains("serverIds"), "{}", requests[2]);
        assert!(requests[2].to_ascii_lowercase().contains("x-exp-session-id: sess-1"));
        assert!(!requests.iter().any(|r| r.contains("codingSessions")));
        drop(requests);

        // The record a later message's resume re-enters.
        let record = crate::run_registry::get(&dir.0, "sess-1").expect("run record");
        assert_eq!(record.kind, RunKind::Thread);
        assert_eq!(record.issues, vec!["EXP-42".to_string()]);
        assert_eq!(record.display_name(), "EXP-42");
        assert_eq!(record.team_id, "ws-1");
        assert_eq!(record.cwd, worktree);
        assert_eq!(record.repo.as_deref(), Some("acme/web"));
        assert_eq!(record.repository_id.as_deref(), Some("repo-happy"));
        assert_eq!(record.branch.as_deref(), Some("exp/EXP-42"));
        assert_eq!(record.base_branch.as_deref(), Some("main"));
        assert_eq!(record.account().as_deref(), Some(crate::test_support::TEST_PROFILE));
        assert!(record.is_acp());
        assert_eq!(record.acp_session_id, None, "the engine upserts it");
        assert!(!record.owns_run_worktree());
    }

    /// EXP-1292: a CHILD's worktree is cut from its parent's branch (the
    /// brief names it), and a session with no single issue runs on — and
    /// owns — `exp/session-<id8>`.
    #[test]
    fn a_child_session_cuts_its_worktree_from_the_parent_branch() {
        let dir = temp_dir("session-child");
        let worktree = dir.0.join("wt");
        fs::create_dir_all(&worktree).unwrap();
        let (base, captured) = canned_server_recording(repo_calls());
        let worktrees = fake_worktrees(&worktree);
        let deps = make_deps(&base, &dir.0, worktrees.clone());
        let mut child = row(SESSION_ID, Some("repo-child"));
        child.parent_id = Some("parent-1".to_string());
        child.root_id = Some("root-1".to_string());
        // Handed its ROOT's one issue: still its own branch and worktree.
        let mut req = session_request(child, &["EXP-41"]);
        req.brief.parent_branch = Some("exp/EXP-41".to_string());

        let prepared = ready(launch(req, &deps));
        assert_eq!(prepared.branch, "exp/session-5ecb3c42");
        let seen = worktrees.seen.lock().unwrap().clone();
        assert_eq!(seen[0].1, "exp/EXP-41", "based on the parent's branch");
        assert_eq!(seen[0].2, "exp/session-5ecb3c42");
        assert_eq!(prepared.base_ref.as_deref(), Some("origin/exp/EXP-41"));
        let cleanup = prepared.run_cleanup.as_ref().expect("its own worktree");
        assert_eq!(cleanup.branch, "exp/session-5ecb3c42");
        assert_eq!(cleanup.base_branch, "exp/EXP-41");
        assert_eq!(prepared.base_branch.as_deref(), Some("exp/EXP-41"));
        assert_eq!(prepared.tab_title, "claude · Code review");
        let requests = captured.lock().unwrap();
        assert!(
            requests[1].ends_with(&format!(
                r#"{{"sessionId":"{SESSION_ID}","branch":"exp/session-5ecb3c42"}}"#
            )),
            "{}",
            requests[1]
        );
        drop(requests);
        let record = crate::run_registry::get(&dir.0, SESSION_ID).expect("record");
        assert!(record.owns_run_worktree());
        assert_eq!(record.base_branch.as_deref(), Some("exp/EXP-41"));
    }

    /// NEW-1: a ROOT with a repository works itself — a worktree on its
    /// `exp/<IDENT>` branch cut from the default, exactly like a child, with
    /// the coding playbook;
    /// NEW-5: a worktree run is never confined, a repo-less one is confined
    /// to its scratch dir.
    #[test]
    fn a_root_works_in_its_own_worktree_and_only_a_repo_less_run_confines() {
        let dir = temp_dir("root-worktree");
        let worktree = dir.0.join("wt");
        fs::create_dir_all(&worktree).unwrap();
        let (base, captured) = canned_server_recording(repo_calls());
        let worktrees = fake_worktrees(&worktree);
        let deps = make_deps(&base, &dir.0, worktrees.clone());
        let root = row("root-1", Some("repo-r"));

        let prepared = ready(launch(session_request(root, &["EXP-42"]), &deps));
        assert_eq!(prepared.branch, "exp/EXP-42");
        assert_eq!(prepared.worktree, worktree);
        assert_eq!(prepared.acp.confine_to, None, "a worktree run is never confined");
        let seen = worktrees.seen.lock().unwrap().clone();
        assert_eq!((seen[0].1.as_str(), seen[0].2.as_str()), ("main", "exp/EXP-42"));
        let append = &prepared.acp.system_append;
        assert!(!append.contains(crate::skill::DISPATCH_HEADING));
        let requests = captured.lock().unwrap();
        assert!(requests.iter().any(|r| r.contains("sessions.report")), "{requests:?}");
        drop(requests);
        let record = crate::run_registry::get(&dir.0, "root-1").expect("record");
        assert_eq!(record.branch.as_deref(), Some("exp/EXP-42"));
        assert_eq!(record.issues, vec!["EXP-42".to_string()]);
        assert!(!record.extra.contains_key("dispatch"), "nothing writes the legacy flag");

        // Repo-less: the scratch dir, confined to it.
        let base = canned_server(vec![(200, RESOLVE_NONE.to_string())]);
        let deps = make_deps(&base, &dir.0, fake_worktrees(&dir.0.join("unused")));
        let prepared = ready(launch(session_request(row("scratch-1", None), &[]), &deps));
        assert!(crate::scratch::is_scratch_dir(&dir.0, &prepared.worktree));
        assert_eq!(prepared.acp.confine_to.as_deref(), Some(prepared.worktree.as_path()));
    }

    /// A child whose brief names no parent branch cuts from the repo default
    /// and takes the root's one issue branch, owning the issue work.
    #[test]
    fn a_child_without_a_parent_branch_takes_the_issue_branch_off_the_default() {
        let dir = temp_dir("dispatch-child");
        let worktree = dir.0.join("wt");
        fs::create_dir_all(&worktree).unwrap();
        let (base, captured) = canned_server_recording(repo_calls());
        let worktrees = fake_worktrees(&worktree);
        let deps = make_deps(&base, &dir.0, worktrees.clone());
        let mut child = row(SESSION_ID, Some("repo-dc"));
        child.parent_id = Some("disp-1".to_string());
        child.root_id = Some("disp-1".to_string());
        let prepared = ready(launch(session_request(child, &["EXP-42"]), &deps));
        assert_eq!(prepared.branch, "exp/EXP-42");
        let seen = worktrees.seen.lock().unwrap().clone();
        assert_eq!((seen[0].1.as_str(), seen[0].2.as_str()), ("main", "exp/EXP-42"));
        assert!(prepared.run_cleanup.is_none(), "an issue's worktree is the prune's");
        assert!(!prepared.acp.system_append.contains(crate::skill::DISPATCH_HEADING));
        let requests = captured.lock().unwrap();
        assert!(requests[1].ends_with(&format!(
            r#"{{"sessionId":"{SESSION_ID}","branch":"exp/EXP-42"}}"#
        )));
        drop(requests);
        let record = crate::run_registry::get(&dir.0, SESSION_ID).expect("record");
        assert_eq!(record.issues, vec!["EXP-42".to_string()]);
    }

    /// EXP-1292: an issue sits in many threads — a second one starting while
    /// a LIVE session here holds the issue's branch takes its own branch
    /// instead of sharing the checkout.
    #[test]
    fn a_second_thread_on_a_held_issue_branch_takes_its_own() {
        let dir = temp_dir("session-held-branch");
        let worktree = dir.0.join("wt");
        fs::create_dir_all(&worktree).unwrap();
        let mut holder: RunRecord = serde_json::from_value(serde_json::json!({
            "sessionId": "holder-1",
            "accountId": "acct",
            "agent": "claude",
            "kind": "thread",
            "teamId": "ws-1",
            "cwd": worktree.to_string_lossy(),
            "recordedAt": 1,
        }))
        .unwrap();
        holder.repository_id = Some("repo-held".to_string());
        holder.branch = Some("exp/EXP-41".to_string());
        holder.host_pid = Some(std::process::id());
        crate::run_registry::record(&dir.0, holder);
        let (base, _captured) = canned_server_recording(repo_calls());
        let deps = make_deps(&base, &dir.0, fake_worktrees(&worktree));
        let req = session_request(row(SESSION_ID, Some("repo-held")), &["EXP-41"]);

        let prepared = ready(launch(req, &deps));
        assert_eq!(prepared.branch, "exp/session-5ecb3c42");
    }

    /// Server data is untrusted: a parent branch that could smuggle a git
    /// option refuses the launch before any git work.
    #[test]
    fn a_hostile_parent_branch_is_refused() {
        let dir = temp_dir("session-hostile-parent");
        let (base, _captured) = canned_server_recording(repo_calls());
        let worktrees = fake_worktrees(&dir.0.join("wt"));
        let deps = make_deps(&base, &dir.0, worktrees.clone());
        let mut req = session_request(row("s-hostile", Some("repo-hostile")), &[]);
        req.brief.parent_branch = Some("--upload-pack=evil".to_string());
        assert!(launch(req, &deps).is_err());
        assert!(worktrees.seen.lock().unwrap().is_empty());
    }

    /// EXP-1292: a row with no repository runs in its scratch dir — no
    /// token, no git, no branch report; ONE call (the resolve).
    #[test]
    fn a_repo_less_session_runs_in_its_scratch_dir() {
        let dir = temp_dir("session-scratch");
        let (base, captured) = canned_server_recording(vec![(200, RESOLVE_NONE.to_string())]);
        let deps = make_deps(&base, &dir.0, fake_worktrees(&dir.0.join("unused")));
        let mut child = row("child-1", None);
        child.root_id = Some("root-1".to_string());
        child.parent_id = Some("root-1".to_string());

        let prepared = ready(launch(session_request(child, &[]), &deps));
        let scratch = dir.0.join("scratch").join("root-1").join("child-1");
        assert_eq!(prepared.worktree, scratch);
        assert!(scratch.is_dir());
        assert!(crate::scratch::is_scratch_dir(&dir.0, &scratch));
        assert!(!scratch.join(crate::mcp_json::MCP_JSON_FILE).exists());
        assert_eq!(prepared.repository_id, None);
        assert_eq!(prepared.branch, "");
        assert_eq!(prepared.base_ref, None);
        assert!(prepared.run_cleanup.is_none());
        assert!(prepared.launch_hold.is_none());
        assert_eq!(prepared.tab_title, "claude · Code review");
        assert_eq!(seed_prompt(&prepared), BRIEF);
        let requests = captured.lock().unwrap();
        assert_eq!(requests.len(), 1, "{requests:?}");
        assert!(requests[0].contains("mcpServers.resolveForLaunch"));
        drop(requests);
        let record = crate::run_registry::get(&dir.0, "child-1").expect("record");
        assert_eq!(record.clone, None);
        assert_eq!(record.cwd, scratch);
    }

    /// A crafted id must stay under `<data_dir>/scratch/` (defense in
    /// depth — real ids are UUIDs).
    #[test]
    fn a_hostile_session_id_stays_under_the_scratch_root() {
        let dir = temp_dir("session-traversal");
        let base = canned_server(vec![(200, RESOLVE_NONE.to_string())]);
        let deps = make_deps(&base, &dir.0, fake_worktrees(&dir.0.join("unused")));
        let prepared = ready(launch(session_request(row("../../escape", None), &[]), &deps));
        assert!(prepared.worktree.starts_with(dir.0.join("scratch")));
        assert!(!prepared.worktree.to_string_lossy().contains(".."));
    }

    /// EXP-1292: a team AGENT's channel records as `agent` (it never ends),
    /// and its instructions ride the system append after the team prompt.
    #[test]
    fn an_agent_root_carries_its_instructions_and_records_as_an_agent() {
        let dir = temp_dir("session-agent");
        let base = canned_server(vec![(200, RESOLVE_NONE.to_string())]);
        let deps = make_deps(&base, &dir.0, fake_worktrees(&dir.0.join("unused")));
        let mut agent = row("agent-1", None);
        agent.agent_id = Some("ag-1".to_string());
        agent.title = Some("Inbox watcher".to_string());
        agent.plan_mode = false;
        let mut req = session_request(agent, &[]);
        req.brief.team_prompt = Some("Always write tests.".to_string());
        req.brief.agent_instructions = Some("Triage new issues.".to_string());

        let prepared = ready(launch(req, &deps));
        assert_eq!(prepared.tab_title, "claude · Inbox watcher");
        let append = &prepared.acp.system_append;
        let team = append.find(crate::skill::TEAM_PROMPT_HEADING).expect("team block");
        let agent = append.find(crate::skill::AGENT_INSTRUCTIONS_HEADING).expect("agent block");
        // An agent's conversation reads the agents playbook.
        assert!(append.starts_with(crate::skill::playbook(true).trim_end()));
        assert!(team < agent, "{append}");
        assert!(append.ends_with("Triage new issues.\n"), "{append}");
        // Both server layers count as the team segment.
        assert_eq!(
            prepared.acp.context_layers.team_bytes,
            context_layout::team_bytes(append)
        );
        assert_eq!(prepared.acp.context_layers.task_bytes, Some(BRIEF.len()));
        assert_eq!(prepared.acp.context_layers.tools_bytes, None);
        let record = crate::run_registry::get(&dir.0, "agent-1").expect("record");
        assert_eq!(record.kind, RunKind::Agent);
        assert_eq!(record.display_name(), "Inbox watcher");
    }

    /// FEED-73: a session whose resolve carries a member roster gets the
    /// delegation note at the END of its `system_append`, after the layers
    /// were measured — `context_layers.team` stays the brief layers' bytes.
    #[test]
    fn a_session_with_a_roster_appends_the_delegation_note() {
        let dir = temp_dir("session-mcp-note");
        let resolve = serde_json::json!({"result":{"data":{
            "servers":[
                {"id":"s-lin","name":"linear","transport":"http","url":"https://l/mcp",
                 "headers":[{"name":"Authorization","value":"Bearer at-danny"}],
                 "actor":{"userId":"u1","name":"Danny","shared":false}},
                {"id":"s-lin","name":"linear-as-chris","transport":"http","url":"https://l/mcp",
                 "headers":[{"name":"Authorization","value":"Bearer at-chris"}],
                 "actor":{"userId":"u2","name":"Chris","shared":true}}
            ],
            "skipped":[],"warnings":[],
            "members":[
                {"serverId":"s-lin","serverName":"linear","userId":"u1","name":"Danny","state":"self"},
                {"serverId":"s-lin","serverName":"linear","userId":"u2","name":"Chris","state":"shared"},
                {"serverId":"s-lin","serverName":"linear","userId":"u3","name":"Max","state":"not_connected"}
            ]
        }}})
        .to_string();
        let (base, _captured) = canned_server_recording(vec![(200, resolve)]);
        let deps = make_deps(&base, &dir.0, fake_worktrees(&dir.0.join("unused")));
        let mut req = session_request(row("s-roster", None), &[]);
        req.brief.team_prompt = Some("Always write tests.".to_string());
        let prepared = ready(launch(req, &deps));
        let append = &prepared.acp.system_append;
        let note = "## Team MCP connections\n\
linear = your connection (Danny). linear_as_chris = acts as Chris (shared).\n\
Not shared: Max (not connected). Until a member shares their connection (Settings → MCP servers → Share with team), act through your own and say on whose behalf you write; you may ask them by @email in your reply.";
        assert!(append.ends_with(&format!("\n\n{note}")), "{append}");
        let before = append.strip_suffix(&format!("\n\n{note}")).unwrap();
        // EXP-1236: every launch carries the code-mode server, so its
        // section rides between the playbook and the team prompt.
        let extras = crate::skill::Extras { code_mode: true, ..crate::skill::Extras::NONE };
        let unnoted = crate::skill::system_append_layers(Some("Always write tests."), None, &extras);
        assert_eq!(before, unnoted.trim_end());
        assert_eq!(
            prepared.acp.context_layers.team_bytes,
            context_layout::team_bytes(&unnoted)
        );
        let names: Vec<&str> = prepared.acp.servers.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["linear", "linear_as_chris", "codemode"]);
    }

    // ---- the disabled surfaces (explain, never crash) ----

    #[test]
    fn doctor_failure_disables_without_any_network_call() {
        let dir = temp_dir("doctor");
        // Unroutable base: any network call would error the launch — proving
        // the doctor gate fires first.
        let mut deps = make_deps("http://127.0.0.1:1", &dir.0, fake_worktrees(&dir.0.join("wt")));
        deps.settings.claude_path = "definitely-not-a-real-binary-exp".to_string();
        match launch(session_request(row("s-doc", Some("repo-doc")), &["EXP-42"]), &deps).unwrap() {
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

    /// EXP-201 per-agent doctor gate: a missing codex blocks a CODEX session
    /// with the codex copy — while claude (the settings stub) stays fine.
    #[test]
    fn missing_selected_agent_blocks_with_its_own_copy() {
        let dir = temp_dir("codex-missing");
        let mut deps = make_deps("http://127.0.0.1:1", &dir.0, fake_worktrees(&dir.0.join("wt")));
        deps.settings.codex_path = "definitely-not-a-real-binary-exp".to_string();
        let mut codex = row("s-codex-missing", Some("repo-codex-missing"));
        codex.cli = Some("codex".to_string());
        match launch(session_request(codex, &["EXP-42"]), &deps).unwrap() {
            Prepared::Disabled(DisabledReason::DoctorFailed(check)) => {
                assert_eq!(check.tool, crate::doctor::Tool::Codex);
                assert_eq!(
                    check.error.as_deref(),
                    Some("codex did not start at the configured path. Remove codexPath from settings.json or set an absolute path.")
                );
            }
            other => panic!("expected DoctorFailed, got {other:?}"),
        }
    }

    #[test]
    fn app_missing_412_maps_to_github_app_missing() {
        let dir = temp_dir("app-missing");
        let base = canned_server(vec![
            (412, r#"{"error":{"message":"The Exponential GitHub App is not installed on acme/web. Reconnect it in team settings.","code":-32012,"data":{"code":"PRECONDITION_FAILED","httpStatus":412}}}"#.to_string()),
        ]);
        let worktrees = fake_worktrees(&dir.0.join("wt"));
        let deps = make_deps(&base, &dir.0, worktrees.clone());
        match launch(session_request(row("s-412", Some("repo-412")), &["EXP-42"]), &deps).unwrap() {
            Prepared::Disabled(reason @ DisabledReason::GithubAppMissing { .. }) => {
                assert!(reason.message().contains("not installed"));
            }
            other => panic!("expected GithubAppMissing, got {other:?}"),
        }
        assert!(worktrees.seen.lock().unwrap().is_empty(), "no git without a token");
    }

    #[test]
    fn token_403_maps_to_token_denied() {
        let dir = temp_dir("denied");
        let base = canned_server(vec![
            (403, r#"{"error":{"message":"You are not a member of this team","code":-32003,"data":{"code":"FORBIDDEN","httpStatus":403}}}"#.to_string()),
        ]);
        let deps = make_deps(&base, &dir.0, fake_worktrees(&dir.0.join("wt")));
        match launch(session_request(row("s-403", Some("repo-403")), &["EXP-42"]), &deps).unwrap() {
            Prepared::Disabled(DisabledReason::TokenDenied { message }) => {
                assert!(message.contains("not a member"));
            }
            other => panic!("expected TokenDenied, got {other:?}"),
        }
    }

    /// EXP-690: plan mode OFF rides the classic bypass flag — every run
    /// bypasses, there is nothing left to opt into.
    #[test]
    fn plan_mode_off_bypasses_permissions() {
        let dir = temp_dir("skip-flag");
        let worktree = dir.0.join("wt");
        fs::create_dir_all(&worktree).unwrap();
        let base = canned_server(repo_calls());
        let deps = make_deps(&base, &dir.0, fake_worktrees(&worktree));
        let mut picked = row("s-bypass", Some("repo-bypass"));
        picked.plan_mode = false;
        picked.effort = Some("xhigh".to_string());
        let prepared = ready(launch(session_request(picked, &["EXP-42"]), &deps));
        assert_eq!(prepared.acp.options.model, "fable");
        assert_eq!(prepared.acp.options.effort, "xhigh");
        assert!(!prepared.acp.options.plan_mode);
        assert!(prepared.bypass_permissions);
        assert!(!prepared.plan_mode);
    }

    /// EXP-201: a CODEX session writes NO `.exp-mcp.json` (the raw key rides
    /// only the spawn env as EXP_MCP_TOKEN), and titles the tab `codex · …`.
    #[test]
    fn prepare_codex_full_sequence() {
        let dir = temp_dir("codex-happy");
        let worktree = dir.0.join("wt");
        fs::create_dir_all(&worktree).unwrap();
        let base = canned_server(repo_calls());
        let deps = make_deps(&base, &dir.0, fake_worktrees(&worktree));
        let mut codex = row("sess-1", Some("repo-codex"));
        codex.cli = Some("codex".to_string());
        codex.model = Some("gpt-5.6-sol".to_string());
        codex.effort = Some("high".to_string());

        let prepared = ready(launch(session_request(codex, &["EXP-42"]), &deps));
        assert_eq!(prepared.tab_title, "codex · Code review");
        assert_eq!(prepared.spawn.program, deps.settings.codex_path);
        assert!(!worktree.join(".exp-mcp.json").exists());
        assert!(prepared.spawn.args.is_empty(), "{:?}", prepared.spawn.args);
        match &prepared.acp.mcp {
            AgentMcp::CodexOverrides { url, session_id } => {
                assert_eq!(url, &mcp_url(&base));
                assert_eq!(session_id.as_deref(), Some("sess-1"));
            }
            other => panic!("expected the codex MCP posture, got {other:?}"),
        }
        assert!(prepared
            .spawn
            .env
            .contains(&("EXP_MCP_TOKEN".to_string(), "expu_seeded".to_string())));
        assert!(prepared.bypass_permissions, "codex never plans");
        assert_eq!(prepared.acp.options.model, "gpt-5.6-sol");
        assert_eq!(prepared.acp.options.effort, "high");
        // EXP-443: the per-session originator is minted and stamped into the
        // spawn env for rollout discovery.
        assert_eq!(prepared.claude_session_id, None);
        let originator = crate::argv::codex_session_originator("sess-1");
        assert_eq!(prepared.codex_originator.as_deref(), Some(originator.as_str()));
        assert_eq!(prepared.codex_resume_id, None);
        assert!(prepared
            .spawn
            .env
            .contains(&(crate::argv::CODEX_ORIGINATOR_ENV.to_string(), originator)));
    }

    /// EXP-478: `prepare` gates the clone from before the worktree exists,
    /// and the hold rides the `PreparedLaunch` until the caller drops it.
    #[test]
    fn prepare_takes_the_launch_gate_hold_until_dropped() {
        let dir = temp_dir("launch-gate");
        let worktree = dir.0.join("wt");
        fs::create_dir_all(&worktree).unwrap();
        let base = canned_server(repo_calls());
        let deps = make_deps(&base, &dir.0, fake_worktrees(&worktree));
        let prepared = ready(launch(session_request(row("s-gate", Some("repo-gate")), &["EXP-42"]), &deps));
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

    /// §7.2: the first session ever on a machine mints the hidden personal
    /// key (after the token mint, before the branch report), stores it, and
    /// the spawn env carries the fresh key.
    #[test]
    fn first_session_auto_mints_the_hidden_personal_key() {
        let dir = temp_dir("auto-mint");
        let worktree = dir.0.join("wt");
        fs::create_dir_all(&worktree).unwrap();
        let base = canned_server(vec![
            (200, TOKEN_OK.to_string()),
            (200, MINT_OK.to_string()),
            (200, REPORT_OK.to_string()),
            (200, RESOLVE_NONE.to_string()),
        ]);
        let deps = make_deps(&base, &dir.0, fake_worktrees(&worktree));
        deps.token_store.delete("acct", SecretKind::PersonalApiKey);
        assert_eq!(deps.token_store.get("acct", SecretKind::PersonalApiKey), None);

        let prepared = ready(launch(session_request(row("sess-1", Some("repo-mint")), &["EXP-42"]), &deps));
        assert_eq!(prepared.session_id, "sess-1");
        assert_eq!(
            deps.token_store.get("acct", SecretKind::PersonalApiKey).as_deref(),
            Some("expu_minted_runtime")
        );
        assert_eq!(
            deps.token_store.get("acct", SecretKind::PersonalApiKeyId).as_deref(),
            Some("key-9")
        );
        assert!(!worktree.join(".exp-mcp.json").exists());
        assert!(prepared
            .spawn
            .env
            .contains(&(MCP_TOKEN_ENV.to_string(), "expu_minted_runtime".to_string())));
    }

    /// §7.6: N sessions = N worktrees/branches (per-launch state never
    /// bleeds across sessions).
    #[test]
    fn two_sessions_prepare_into_isolated_worktrees() {
        let dir = temp_dir("concurrent-prep");
        let wt_a = dir.0.join("wt-a");
        let wt_b = dir.0.join("wt-b");
        fs::create_dir_all(&wt_a).unwrap();
        fs::create_dir_all(&wt_b).unwrap();
        let launch_one = |identifier: &str, session: &str, worktree: &PathBuf| {
            let base = canned_server(repo_calls());
            let deps = make_deps(&base, &dir.0, fake_worktrees(worktree));
            ready(launch(session_request(row(session, Some("repo-pair")), &[identifier]), &deps))
        };
        let a = launch_one("EXP-1", "sess-a", &wt_a);
        let b = launch_one("EXP-2", "sess-b", &wt_b);
        assert_eq!(a.branch, "exp/EXP-1");
        assert_eq!(b.branch, "exp/EXP-2");
        assert_ne!(a.worktree, b.worktree);
        assert_ne!(a.session_id, b.session_id);
        assert_eq!(a.tab_title, "claude · Code review");
        assert_eq!(b.tab_title, "claude · Code review");
        assert_eq!(a.spawn.cwd.as_deref(), Some(wt_a.as_path()));
        assert_eq!(b.spawn.cwd.as_deref(), Some(wt_b.as_path()));
    }

    /// EXP-773: the EXP-474 ignore guard belongs to the FILE-writing arm — the
    /// agent SHELL. A repo whose committed `.gitignore` re-includes
    /// `.exp-mcp.json` still refuses a shell there, but a session writes no
    /// file at all (the key rides the child env), so it must launch.
    #[cfg(unix)]
    #[test]
    fn the_key_file_guard_gates_the_agent_shell_only() {
        let dir = temp_dir("guard-transport");
        let worktree = dir.0.join("wt");
        fs::create_dir_all(&worktree).unwrap();
        let git = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .args(args)
                .current_dir(&worktree)
                .env("GIT_AUTHOR_NAME", "t")
                .env("GIT_AUTHOR_EMAIL", "t@example.com")
                .env("GIT_COMMITTER_NAME", "t")
                .env("GIT_COMMITTER_EMAIL", "t@example.com")
                .output()
                .unwrap();
            assert!(out.status.success(), "git {args:?} failed");
        };
        git(&["init", "--quiet", "-b", "main"]);
        fs::write(worktree.join(".gitignore"), "!.exp-mcp.json\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "--quiet", "-m", "reinclude"]);

        guard_agent_mcp(CodingAgent::Claude, &worktree, false)
            .expect("a session has no file to guard");
        let direct = guard_agent_mcp(CodingAgent::Claude, &worktree, true).unwrap_err();
        assert!(matches!(direct, CodingError::Git(_)), "wrong error: {direct:?}");

        let base = canned_server(repo_calls());
        let mut deps = make_deps(&base, &dir.0, fake_worktrees(&worktree));
        deps.settings.claude_path = acp_ready_claude_stub(&dir.0).to_string_lossy().into_owned();
        ready(launch(session_request(row("s-guard", Some("repo-guard")), &["EXP-42"]), &deps));
        assert!(!worktree.join(crate::mcp_json::MCP_JSON_FILE).exists());
    }

    // ---- EXP-792: the team servers ----

    /// A pick the member cannot use (here: the resolve call fails outright)
    /// never refuses a launch: the resolver warns and hands back no servers.
    /// The call names the row and no ids — the pick lives on the row.
    #[test]
    fn an_unusable_mcp_pick_degrades_to_a_warning_never_a_blocker() {
        let dir = temp_dir("mcp-degraded");
        let (base, captured) =
            canned_server_recording(vec![(500, r#"{"error":{"message":"boom"}}"#.to_string())]);
        let deps = make_deps(&base, &dir.0, fake_worktrees(&dir.0.join("unused")));
        let resolved = resolve_mcp_servers(&deps, "sess-1");
        assert!(resolved.servers.is_empty() && resolved.env.is_empty());
        assert_eq!(resolved.warnings.len(), 1, "{:?}", resolved.warnings);
        let requests = captured.lock().unwrap();
        assert!(requests[0].contains("mcpServers.resolveForLaunch"));
        assert!(!requests[0].contains("serverIds"), "{}", requests[0]);
        assert!(
            requests[0].to_ascii_lowercase().contains("x-exp-session-id: sess-1"),
            "{}",
            requests[0]
        );
    }

    /// The warning RIDES the prepared launch so the hosts can show it; the
    /// run still prepares, with only the device's own code-mode server.
    #[cfg(unix)]
    #[test]
    fn a_skipped_mcp_pick_rides_the_prepared_launch() {
        let dir = temp_dir("mcp-degraded-prepared");
        let base = canned_server(vec![(
            200,
            r#"{"result":{"data":{"servers":[],"skipped":[{"id":"srv-9","name":"Linear","reason":"not connected"}],"warnings":[]}}}"#.to_string(),
        )]);
        let mut deps = make_deps(&base, &dir.0, fake_worktrees(&dir.0.join("unused")));
        deps.settings.claude_path = acp_ready_claude_stub(&dir.0).to_string_lossy().into_owned();
        let prepared = ready(launch(session_request(row("s-skip", None), &[]), &deps));
        assert_eq!(
            prepared.mcp_warnings,
            vec![
                "MCP server Linear: not connected; starting without it. Connect it in Settings → MCP servers."
                    .to_string()
            ]
        );
        let names: Vec<&str> = prepared.acp.servers.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["codemode"]);
        assert_eq!(prepared.acp.servers[0].token_env.as_deref(), Some(codemode::TOKEN_ENV));
        assert!(!prepared.acp.mcp_secrets.is_empty());
        assert!(prepared.acp.system_append.contains("# Code mode"));
    }

    // ---- EXP-1158: the last used stamp ----

    /// Only a thread ROOT a person sent stamps last used — with the account
    /// they NAMED, or the agent alone. A child (an agent's), a team agent's
    /// channel and every resume never do.
    #[test]
    fn a_person_started_root_stamps_last_used_and_nothing_else_does() {
        let options = |account: Option<&str>| LaunchOptions {
            agent: CodingAgent::Codex,
            account: account.map(str::to_string),
            ..base_options()
        };
        let root = PrepareRequest::Session(session_request(row("s-root", None), &[]));
        assert_eq!(
            last_used_stamp(&root, &options(Some(" 0a1b2c3d "))),
            Some(LastUsedStamp { agent: CodingAgent::Codex, account: Some("0a1b2c3d".to_string()) })
        );
        assert_eq!(
            last_used_stamp(&root, &options(None)),
            Some(LastUsedStamp { agent: CodingAgent::Codex, account: None })
        );
        let mut child_row = row("s-child", None);
        child_row.parent_id = Some("s-root".to_string());
        let child = PrepareRequest::Session(session_request(child_row, &[]));
        assert_eq!(last_used_stamp(&child, &options(Some("0a1b2c3d"))), None);
        let mut agent_row = row("s-agent", None);
        agent_row.agent_id = Some("ag-1".to_string());
        let agent = PrepareRequest::Session(session_request(agent_row, &[]));
        assert_eq!(last_used_stamp(&agent, &options(None)), None);
        let dir = temp_dir("stamp-resume");
        let mut resumed = session_request(row("sess-old", None), &[]);
        resumed.resume = Some(ResumeRun {
            record: resume_record(&dir.0, "sess-old"),
            prompt: "go on".to_string(),
            message_id: None,
            reconfigure: None,
            fresh: false,
        });
        assert_eq!(last_used_stamp(&PrepareRequest::Session(resumed), &options(None)), None);
    }


    /// The stamp writes the login pointer and `defaultAgent`; the retired
    /// `system` clears the pointer (back to the first profile).
    #[test]
    fn the_stamp_records_the_login_and_the_agent() {
        let dir = temp_dir("stamp-apply");
        let first = crate::agent_profiles::create(&dir.0, CodingAgent::Codex).unwrap();
        let work = crate::agent_profiles::create(&dir.0, CodingAgent::Codex).unwrap();
        apply_last_used_stamp(
            &LastUsedStamp { agent: CodingAgent::Codex, account: Some(work.id.clone()) },
            &dir.0,
        );
        assert_eq!(crate::agent_profiles::active_profile(&dir.0, CodingAgent::Codex), Some(work.id.clone()));
        let path = Settings::default_path(&dir.0);
        assert_eq!(Settings::load(&path).default_agent, CodingAgent::Codex);
        // An agent-only stamp leaves the login alone.
        apply_last_used_stamp(&LastUsedStamp { agent: CodingAgent::Codex, account: None }, &dir.0);
        assert_eq!(crate::agent_profiles::active_profile(&dir.0, CodingAgent::Codex), Some(work.id.clone()));
        apply_last_used_stamp(
            &LastUsedStamp { agent: CodingAgent::Claude, account: Some("system".to_string()) },
            &dir.0,
        );
        assert_eq!(Settings::load(&path).default_agent, CodingAgent::Claude);
        apply_last_used_stamp(
            &LastUsedStamp { agent: CodingAgent::Codex, account: Some("system".to_string()) },
            &dir.0,
        );
        assert_eq!(crate::agent_profiles::active_profile(&dir.0, CodingAgent::Codex), Some(first.id));
    }


    #[test]
    fn default_device_label_is_never_empty() {
        assert!(!default_device_label().trim().is_empty());
    }

    fn agent_shell_request(cwd_override: Option<PathBuf>) -> AgentShellRequest {
        AgentShellRequest {
            options: LaunchOptions {
                agent: CodingAgent::Claude,
                model: "fable".to_string(),
                effort: String::new(),
                ultracode: false,
                plan_mode: false,
                subagent_model: String::new(),
                mcp_server_ids: Vec::new(),
                account: None,
                computer_use: None,
            },
            repository_id: "repo-1".to_string(),
            full_name: "acme/web".to_string(),
            cwd_override,
            team_id: None,
        }
    }

    /// EXP-369: a pinned worktree becomes the agent's cwd (and with it the
    /// `.exp-mcp.json` target); without one the trunk clone is.
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

    // ---- EXP-792: team MCP servers ----

    /// The two-server pick: an OAuth http server whose bearer rides
    /// `${EXP_MCP_TOKEN_1}` and a stdio server with a typed env value.
    fn two_servers() -> Vec<McpServerWire> {
        use crate::argv::McpWireTransport;
        vec![
            McpServerWire {
                id: "srv-1".to_string(),
                name: "linear".to_string(),
                transport: McpWireTransport::Http {
                    url: "https://mcp.linear.app/mcp".to_string(),
                },
                headers: vec![("Authorization".to_string(), "Bearer ${EXP_MCP_TOKEN_1}".to_string())],
                token_env: Some("EXP_MCP_TOKEN_1".to_string()),
                env: Vec::new(),
                actor: None,
            },
            McpServerWire {
                id: "srv-2".to_string(),
                name: "github".to_string(),
                transport: McpWireTransport::Stdio {
                    command: "npx".to_string(),
                    args: vec!["-y".to_string(), "@acme/github-mcp".to_string()],
                },
                headers: Vec::new(),
                token_env: None,
                env: vec![("GITHUB_TOKEN".to_string(), "${GITHUB_TOKEN}".to_string())],
                actor: None,
            },
        ]
    }

    fn resolved_two() -> ResolvedMcp {
        ResolvedMcp {
            servers: two_servers(),
            env: vec![
                ("EXP_MCP_TOKEN_1".to_string(), "oauth-access-token-value-1".to_string()),
                ("GITHUB_TOKEN".to_string(), "ghp_typed_value_2".to_string()),
            ],
            warnings: Vec::new(),
            members: Vec::new(),
            pick: None,
        }
    }

    /// Every resolved pair lands in the spawn env; the server LIST never
    /// does (claude and codex both carry it in their own config), and no
    /// value ever reaches the env under a name the agent could print.
    #[test]
    fn apply_mcp_server_env_lands_every_resolved_pair() {
        let resolved = resolved_two();
        let spawn = apply_mcp_server_env(SpawnSpec::new("agent"), &resolved);
        let env = |name: &str| {
            spawn
                .env
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.as_str())
        };
        assert_eq!(env("EXP_MCP_TOKEN_1"), Some("oauth-access-token-value-1"));
        assert_eq!(env("GITHUB_TOKEN"), Some("ghp_typed_value_2"));
        // An empty pick adds nothing at all — the pre-792 env, byte for byte.
        let bare = apply_mcp_server_env(SpawnSpec::new("agent"), &ResolvedMcp::default());
        assert!(bare.env.is_empty());
    }

    /// The launch's secret set reaches the engine as a value that never
    /// prints, and comes back out as the redactor's exact-match entries.
    #[test]
    fn mcp_secrets_never_print_but_round_trip() {
        let secrets = McpSecrets::new(resolved_two().secret_values());
        assert_eq!(format!("{secrets:?}"), "McpSecrets(<2 redacted>)");
        assert_eq!(secrets.len(), 2);
        assert!(!secrets.is_empty());
        assert_eq!(
            secrets.into_vec(),
            vec!["oauth-access-token-value-1".to_string(), "ghp_typed_value_2".to_string()]
        );
        assert!(McpSecrets::default().is_empty());
    }


    // ---- EXP-1292: the native resume of the SAME row ----

    /// A repo-LESS recorded session — the shortest resume path (no token,
    /// no git), so the tests below exercise identity + wire, not plumbing.
    fn resume_record(dir: &Path, session_id: &str) -> RunRecord {
        let cwd = dir.join("scratch");
        fs::create_dir_all(&cwd).unwrap();
        RunRecord {
            session_id: session_id.to_string(),
            account_id: "acct".to_string(),
            agent: CodingAgent::Claude,
            kind: RunKind::Thread,
            team_id: "ws-1".to_string(),
            issues: Vec::new(),
            title: Some("Code review".to_string()),
            cwd,
            clone: None,
            repo: None,
            repository_id: None,
            branch: None,
            base_branch: None,
            claude_session_id: Some("claude-1".to_string()),
            codex_originator: None,
            model: "fable".to_string(),
            effort: String::new(),
            ultracode: false,
            recorded_at: crate::run_registry::now_secs(),
            extra: std::collections::BTreeMap::new(),
            transport: None,
            acp_session_id: None,
            agent_native_session_id: None,
            acp_child_pid: None,
            host_pid: None,
        }
    }

    const MESSAGE: &str = "Please also fix the flaky test.";

    /// The resume of `record`'s own row, woken by [`MESSAGE`].
    fn resume_request(record: RunRecord) -> SessionLaunchRequest {
        let mut row = row(&record.session_id, record.repository_id.as_deref());
        row.plan_mode = false;
        let mut req = session_request(row, &[]);
        req.resume = Some(ResumeRun {
            record,
            prompt: MESSAGE.to_string(),
            message_id: Some("msg-1".to_string()),
            reconfigure: None,
            fresh: false,
        });
        req
    }

    fn seed_transcript(dir: &Path, deps: &mut CodingDeps, id: &str) {
        let projects = dir.join("claude-projects");
        // The dir name is claude's business (EXP-761: capped + hashed on a
        // long cwd); the launcher finds the file by session id alone.
        let project_dir = projects.join("-Users-u-some-long-worktree-path--71h4ur");
        fs::create_dir_all(&project_dir).unwrap();
        fs::write(project_dir.join(format!("{id}.jsonl")), "{\"turn\":1}\n").unwrap();
        deps.claude_projects_root = Some(projects);
    }

    /// EXP-746 (D8): a session recorded on the ACP path resumes into the
    /// engine seeded with the recorded `session/load` handle, and the waking
    /// message is the first turn — consumed once sent (its id rides along).
    /// Same row, same record key: nothing server-side is created.
    #[cfg(unix)]
    #[test]
    fn an_acp_resume_reopens_the_recorded_session_with_the_message() {
        let dir = temp_dir("resume-acp");
        let (base, captured) = canned_server_recording(vec![(200, RESOLVE_NONE.to_string())]);
        let mut deps = make_deps(&base, &dir.0, fake_worktrees(&dir.0.join("unused")));
        deps.settings.claude_path = acp_ready_claude_stub(&dir.0).to_string_lossy().into_owned();
        let mut record = resume_record(&dir.0, "sess-old");
        record.transport = Some("acp".to_string());
        record.acp_session_id = Some("acp-42".to_string());

        let prepared = ready(launch(resume_request(record), &deps));
        assert_eq!(prepared.session_id, "sess-old", "the SAME row");
        assert_eq!(prepared.acp.resume, Some(ResumeSeed::Acp("acp-42".to_string())));
        assert_eq!(prepared.acp.prompt.as_deref(), Some(MESSAGE));
        assert_eq!(prepared.seed_message_id.as_deref(), Some("msg-1"));
        assert!(!prepared.plan_mode, "a resume never re-enters plan mode");
        assert!(prepared.bypass_permissions);
        assert_eq!(prepared.claude_session_id, None);
        let requests = captured.lock().unwrap();
        assert_eq!(requests.len(), 1, "{requests:?}");
        assert!(requests[0].to_ascii_lowercase().contains("x-exp-session-id: sess-old"));
        drop(requests);
        let record = crate::run_registry::get(&dir.0, "sess-old").expect("record");
        assert_eq!(record.acp_session_id.as_deref(), Some("acp-42"), "kept until the engine upserts");
        assert_eq!(record.host_pid, None);
    }

    /// A session whose conversation did NOT survive starts over from its
    /// brief, with the waking message after it.
    #[cfg(unix)]
    #[test]
    fn a_resume_without_a_conversation_reseeds_the_brief_and_the_message() {
        let dir = temp_dir("resume-fresh");
        let base = canned_server(vec![(200, RESOLVE_NONE.to_string())]);
        let mut deps = make_deps(&base, &dir.0, fake_worktrees(&dir.0.join("unused")));
        deps.settings.claude_path = acp_ready_claude_stub(&dir.0).to_string_lossy().into_owned();
        deps.claude_projects_root = Some(dir.0.join("empty-projects"));
        let mut record = resume_record(&dir.0, "sess-old");
        record.transport = Some("acp".to_string());

        let prepared = ready(launch(resume_request(record), &deps));
        assert_eq!(prepared.acp.resume, None);
        assert_eq!(seed_prompt(&prepared), format!("{BRIEF}\n\n{MESSAGE}"));
        assert_eq!(prepared.seed_message_id.as_deref(), Some("msg-1"));
    }

    /// EXP-773: a session whose agent lost its ACP readiness has no terminal
    /// to fall back to — the resume is REFUSED with the doctor's reason.
    #[test]
    fn a_resume_whose_agent_lost_acp_readiness_is_refused() {
        let dir = temp_dir("resume-acp-not-ready");
        let base = canned_server(vec![]);
        let mut deps = make_deps(&base, &dir.0, fake_worktrees(&dir.0.join("unused")));
        // `git --version` never parses as a claude version, so the doctor
        // leaves `acp` unknown and the ACP gate refuses.
        deps.settings.claude_path = "git".to_string();
        let mut record = resume_record(&dir.0, "sess-old");
        record.transport = Some("acp".to_string());
        record.acp_session_id = Some("acp-42".to_string());
        let before = record.clone();
        crate::run_registry::record(&dir.0, record.clone());

        match launch(resume_request(record), &deps).unwrap() {
            Prepared::Disabled(DisabledReason::AcpUnavailable { label, .. }) => {
                assert_eq!(label, "Claude Code");
            }
            other => panic!("expected AcpUnavailable, got {other:?}"),
        }
        assert_eq!(crate::run_registry::get(&dir.0, "sess-old"), Some(before), "untouched");
    }

    /// The claude native path: the recorded transcript still exists (under a
    /// hash-suffixed project dir), so the resume reopens THAT conversation
    /// through the agent's own handle.
    #[cfg(unix)]
    #[test]
    fn a_resume_reuses_the_recorded_claude_transcript() {
        let dir = temp_dir("resume-claude");
        let base = canned_server(vec![(200, RESOLVE_NONE.to_string())]);
        let mut deps = make_deps(&base, &dir.0, fake_worktrees(&dir.0.join("unused")));
        deps.settings.claude_path = acp_ready_claude_stub(&dir.0).to_string_lossy().into_owned();
        seed_transcript(&dir.0, &mut deps, "claude-1");
        let record = resume_record(&dir.0, "sess-old");
        assert!(!record.is_acp(), "no ACP id to load");

        let prepared = ready(launch(resume_request(record), &deps));
        assert_eq!(prepared.acp.resume, Some(ResumeSeed::Native("claude-1".to_string())));
        assert_eq!(prepared.acp.prompt.as_deref(), Some(MESSAGE));
        let record = crate::run_registry::get(&dir.0, "sess-old").expect("record");
        assert_eq!(record.claude_session_id.as_deref(), Some("claude-1"));
    }

    /// EXP-1280: a plan approved "in a fresh context" waking a session with
    /// no engine (a daemon restart) starts a NEW conversation — the
    /// recorded transcript is not reopened, the brief and the approval seed
    /// it — instead of silently keeping the old context.
    #[cfg(unix)]
    #[test]
    fn a_fresh_approval_resume_starts_a_new_conversation() {
        let dir = temp_dir("resume-fresh");
        let base = canned_server(vec![(200, RESOLVE_NONE.to_string())]);
        let mut deps = make_deps(&base, &dir.0, fake_worktrees(&dir.0.join("unused")));
        deps.settings.claude_path = acp_ready_claude_stub(&dir.0).to_string_lossy().into_owned();
        seed_transcript(&dir.0, &mut deps, "claude-1");
        let mut req = resume_request(resume_record(&dir.0, "sess-old"));
        req.resume.as_mut().unwrap().fresh = true;

        let prepared = ready(launch(req, &deps));
        assert_eq!(prepared.acp.resume, None, "no reopen: a fresh conversation");
        let prompt = prepared.acp.prompt.as_deref().unwrap_or_default();
        assert!(prompt.ends_with(MESSAGE), "{prompt}");
        assert!(prompt.len() > MESSAGE.len(), "seeded from the brief too: {prompt}");
        assert_eq!(prepared.seed_message_id.as_deref(), Some("msg-1"));
    }

    /// EXP-1330: a `/btw` aside resumes with NO MCP servers — the team pick
    /// is never resolved (no call), code mode and computer use stay off, so
    /// nothing of the person's key is reachable from a side question; the
    /// engine renders the claude config empty on top (`engine::aside`).
    #[cfg(unix)]
    #[test]
    fn an_aside_resume_carries_no_mcp_servers() {
        let dir = temp_dir("resume-aside");
        let (base, captured) = canned_server_recording(vec![(200, RESOLVE_NONE.to_string())]);
        let mut deps = make_deps(&base, &dir.0, fake_worktrees(&dir.0.join("unused")));
        deps.settings.claude_path = acp_ready_claude_stub(&dir.0).to_string_lossy().into_owned();
        seed_transcript(&dir.0, &mut deps, "claude-1");
        let mut record = resume_record(&dir.0, "aside-1");
        record.set_aside_of("sess-parent");

        let prepared = ready(launch(resume_request(record), &deps));
        assert!(prepared.acp.servers.is_empty(), "{:?}", prepared.acp.servers);
        assert!(prepared.acp.mcp_secrets.is_empty());
        assert!(prepared.mcp_warnings.is_empty());
        assert!(!prepared.acp.system_append.contains("# Code mode"));
        assert!(captured.lock().unwrap().is_empty(), "an aside never resolves the team pick");
    }

    /// A person may change the model on a live session; the resume runs on
    /// the row's pick and re-records it.
    #[test]
    fn a_resume_takes_the_rows_model() {
        let dir = temp_dir("resume-model");
        let base = canned_server(vec![(200, RESOLVE_NONE.to_string())]);
        let deps = make_deps(&base, &dir.0, fake_worktrees(&dir.0.join("unused")));
        let mut req = resume_request(resume_record(&dir.0, "sess-old"));
        req.session.model = Some("opus".to_string());
        req.session.effort = Some("high".to_string());
        let prepared = ready(launch(req, &deps));
        assert_eq!(prepared.acp.options.model, "opus");
        assert_eq!(prepared.acp.options.effort, "high");
        let record = crate::run_registry::get(&dir.0, "sess-old").expect("record");
        assert_eq!(record.model, "opus");
    }

    /// EXP-849 — an account SWITCH (claude only, `options.account` on a
    /// resume): the resume runs on the target profile, the transcript is
    /// HANDED OVER into that profile's own `projects` tree, no credential
    /// travels, and (EXP-935) the continue instruction closes the turn.
    #[test]
    fn a_resume_hands_the_transcript_to_the_target_account() {
        let dir = temp_dir("resume-switch");
        let base = canned_server(vec![(200, RESOLVE_NONE.to_string())]);
        let mut deps = make_deps(&base, &dir.0, fake_worktrees(&dir.0.join("unused")));
        // The run happened on the AMBIENT login, whose tree the fixture root
        // stands in for.
        seed_transcript(&dir.0, &mut deps, "claude-1");
        let target = crate::agent_profiles::create(&dir.0, CodingAgent::Claude).unwrap();
        let credential = dir.0.join("creds.json");
        fs::write(&credential, "secret").unwrap();

        let mut req = resume_request(resume_record(&dir.0, "sess-old"));
        req.options.account = Some(target.id.clone());
        let prepared = ready(launch(req, &deps));
        assert_eq!(prepared.acp.resume, Some(ResumeSeed::Native("claude-1".to_string())));
        let target_dir =
            crate::agent_profiles::profile_dir(&dir.0, CodingAgent::Claude, &target.id).unwrap();
        let handed = target_dir
            .join("projects")
            .join("-Users-u-some-long-worktree-path--71h4ur")
            .join("claude-1.jsonl");
        assert_eq!(fs::read_to_string(&handed).unwrap(), "{\"turn\":1}\n");
        assert!(!target_dir.join("creds.json").exists());
        assert!(!target_dir.join(".credentials.json").exists());
        assert_eq!(fs::read_to_string(&credential).unwrap(), "secret");
        assert!(prepared
            .spawn
            .env
            .iter()
            .any(|(key, value)| key == "CLAUDE_CONFIG_DIR"
                && value == &target_dir.to_string_lossy().to_string()));
        let prompt = seed_prompt(&prepared);
        assert!(prompt.starts_with(MESSAGE), "{prompt}");
        assert!(prompt.ends_with(crate::prompt::ACCOUNT_SWITCH_CONTINUE_PROMPT), "{prompt}");
        // The next resume re-enters the login that now holds the transcript.
        let record = crate::run_registry::get(&dir.0, "sess-old").expect("record");
        assert_eq!(record.account().as_deref(), Some(target.id.as_str()));
    }

    /// EXP-935: a switch with nothing to say, picked on the usage WALL,
    /// still opens the turn itself (the walled work continues); the same
    /// pick on an idle, unwalled session resumes idle — no turn of its own.
    #[test]
    fn an_account_switch_without_a_message_continues_only_a_walled_session() {
        for walled in [true, false] {
            let dir = temp_dir(if walled { "resume-switch-prompt" } else { "resume-switch-idle" });
            let base = canned_server(vec![(200, RESOLVE_NONE.to_string())]);
            let mut deps = make_deps(&base, &dir.0, fake_worktrees(&dir.0.join("unused")));
            seed_transcript(&dir.0, &mut deps, "claude-1");
            let target = crate::agent_profiles::create(&dir.0, CodingAgent::Claude).unwrap();
            let mut req = resume_request(resume_record(&dir.0, "sess-old"));
            req.options.account = Some(target.id.clone());
            if walled {
                req.session.blocked = Some(serde_json::json!({"kind": "rate_limit"}));
            }
            if let Some(resume) = req.resume.as_mut() {
                resume.prompt = String::new();
                resume.message_id = None;
            }
            let prepared = ready(launch(req, &deps));
            let expected = walled.then_some(crate::prompt::ACCOUNT_SWITCH_CONTINUE_PROMPT);
            assert_eq!(prepared.acp.prompt.as_deref(), expected, "walled = {walled}");
            assert_eq!(prepared.seed_message_id, None);
            // Either way the run now lives on the target login.
            let record = crate::run_registry::get(&dir.0, "sess-old").expect("record");
            assert_eq!(record.account().as_deref(), Some(target.id.as_str()));
        }
    }

    /// EXP-849 — the switches the launcher refuses outright, before it
    /// touches anything: an account this machine does not have, and codex.
    /// Naming the account the run already used is no switch at all.
    #[test]
    fn a_resume_refuses_a_switch_it_cannot_honor() {
        let dir = temp_dir("resume-switch-refused");
        let base = canned_server(vec![(200, RESOLVE_NONE.to_string())]);
        let deps = make_deps(&base, &dir.0, fake_worktrees(&dir.0.join("unused")));

        let mut req = resume_request(resume_record(&dir.0, "sess-old"));
        req.options.account = Some("deadbeef".to_string());
        match launch(req, &deps).unwrap() {
            Prepared::Disabled(DisabledReason::AccountSwitchRefused { message }) => {
                assert!(message.contains("deadbeef"), "{message}");
            }
            other => panic!("expected a refusal, got {other:?}"),
        }

        let target = crate::agent_profiles::create(&dir.0, CodingAgent::Codex).unwrap();
        let mut record = resume_record(&dir.0, "sess-old");
        record.agent = CodingAgent::Codex;
        let mut req = resume_request(record);
        req.options.account = Some(target.id);
        match launch(req, &deps).unwrap() {
            Prepared::Disabled(DisabledReason::AccountSwitchRefused { message }) => {
                assert!(message.to_lowercase().contains("codex"), "{message}");
            }
            other => panic!("expected a refusal, got {other:?}"),
        }

        let mut req = resume_request(resume_record(&dir.0, "sess-old"));
        req.options.account = Some("system".to_string());
        assert!(
            !matches!(
                launch(req, &deps),
                Ok(Prepared::Disabled(DisabledReason::AccountSwitchRefused { .. }))
            ),
            "same-account resume must not be refused"
        );
    }

    /// A REPO-BACKED workspace with no recorded branch has nothing to
    /// re-create from — the resume refuses rather than spawn in a
    /// fabricated directory.
    #[test]
    fn a_resume_refuses_a_missing_branchless_workspace() {
        let dir = temp_dir("resume-gone");
        let base = canned_server(vec![]);
        let deps = make_deps(&base, &dir.0, fake_worktrees(&dir.0.join("unused")));
        let mut record = resume_record(&dir.0, "sess-old3");
        record.clone = Some(dir.0.join("clone"));
        record.cwd = dir.0.join("vanished");
        match launch(resume_request(record), &deps) {
            Err(CodingError::Io(message)) => {
                assert!(message.contains("workspace is gone"), "{message}");
            }
            other => panic!("expected the missing-workspace error, got {other:?}"),
        }
    }

    /// A session's worktree the prune reclaimed once its PR merged: the
    /// resume re-creates it through the provider on the recorded branch,
    /// spawns INTO it, and opens with the reclaimed-workspace note before
    /// the message.
    #[test]
    fn a_resume_recreates_a_reclaimed_worktree() {
        let dir = temp_dir("resume-reclaimed");
        let (base, captured) = canned_server_recording(vec![
            (200, TOKEN_OK.to_string()),
            (200, RESOLVE_NONE.to_string()),
        ]);
        let layout = dir.0.join("repos").join("acme").join("web.worktrees").join("session-1a2b3c4d");
        let worktrees = fake_worktrees(&layout);
        let mut deps = make_deps(&base, &dir.0, worktrees.clone());
        let mut record = resume_record(&dir.0, "sess-old5");
        record.cwd = layout.clone();
        record.clone = Some(dir.0.join("repos").join("acme").join("web"));
        record.repo = Some("acme/web".to_string());
        record.repository_id = Some("repo-resume-reclaimed".to_string());
        record.branch = Some("exp/session-1a2b3c4d".to_string());
        record.base_branch = Some("main".to_string());
        assert!(!layout.exists(), "the worktree is gone");
        assert!(record.resumable() && record.workspace_reclaimed());
        seed_transcript(&dir.0, &mut deps, "claude-1");

        let prepared = ready(launch(resume_request(record), &deps));
        let seen = worktrees.seen.lock().unwrap().clone();
        assert_eq!(seen.len(), 1, "{seen:?}");
        assert_eq!(seen[0].1, "main");
        assert_eq!(seen[0].2, "exp/session-1a2b3c4d");
        assert_eq!(prepared.spawn.cwd.as_deref(), Some(layout.as_path()));
        assert_eq!(prepared.worktree, layout);
        assert_eq!(prepared.acp.resume, Some(ResumeSeed::Native("claude-1".to_string())));
        let prompt = seed_prompt(&prepared);
        assert!(prompt.starts_with("Your worktree was reclaimed"), "{prompt}");
        assert!(prompt.contains("`exp/session-1a2b3c4d` now starts fresh from `origin/main`"), "{prompt}");
        assert!(prompt.ends_with(MESSAGE), "{prompt}");
        assert!(!prompt.contains("acknowledge this in one line"), "{prompt}");
        assert_eq!(
            prepared.run_cleanup.as_ref().map(|c| c.worktree.clone()),
            Some(layout.clone())
        );
        assert_eq!(prepared.repository_id.as_deref(), Some("repo-resume-reclaimed"));
        assert!(captured.lock().unwrap()[0].contains("repositories.installationToken"));
    }

    /// EXP-1292: a repo-less session's purged scratch dir is re-made where
    /// it was (a team agent never ends, so its channel must survive one).
    #[test]
    fn a_resume_re_makes_a_purged_scratch_dir() {
        let dir = temp_dir("resume-scratch-purged");
        let base = canned_server(vec![(200, RESOLVE_NONE.to_string())]);
        let mut deps = make_deps(&base, &dir.0, fake_worktrees(&dir.0.join("unused")));
        deps.claude_projects_root = Some(dir.0.join("empty-projects"));
        let record = resume_record(&dir.0, "sess-old4");
        let scratch = record.cwd.clone();
        fs::remove_dir_all(&scratch).unwrap();
        assert!(record.resumable());
        let prepared = ready(launch(resume_request(record), &deps));
        assert!(scratch.is_dir());
        assert_eq!(prepared.worktree, scratch);
    }

    // ---- EXP-1292: the end of a session's process ----

    /// A thread's agent process is gone: `sessions.report{status: ended}`,
    /// observed; an agent's: `{status: idle}`.
    #[test]
    fn end_and_idle_report_the_status() {
        let (base, captured) = canned_server_recording(vec![
            (200, REPORT_OK.to_string()),
            (200, REPORT_OK.to_string()),
        ]);
        let trpc = TrpcClient::new(&base, Arc::new(api::StaticToken("tok".into())));
        end_session(&trpc, "s-thread").expect("ended");
        idle_session(&trpc, "s-agent").expect("idled");
        let requests = captured.lock().unwrap();
        assert!(requests[0].starts_with("POST /api/trpc/sessions.report"));
        assert!(requests[0].ends_with(r#"{"sessionId":"s-thread","status":"ended"}"#), "{}", requests[0]);
        assert!(requests[1].ends_with(r#"{"sessionId":"s-agent","status":"idle","busy":false}"#), "{}", requests[1]);
    }

    // ---- EXP-1138: the gate judges the login the launch spends ----


    // ---- EXP-1138: the gate judges the login the launch spends ----

    /// Deps whose agent stubs answer sign-in per config dir
    /// (`test_support::auth_stub`): the ambient login is always signed out,
    /// a profile only with its marker.
    #[cfg(unix)]
    fn auth_deps(base: &str, dir: &Path) -> CodingDeps {
        let worktrees = Arc::new(FakeWorktrees {
            worktree: dir.join("unused"),
            seen: Default::default(),
        });
        let mut deps = make_deps(base, dir, worktrees);
        // Only the profiles each test creates.
        let _ = fs::remove_dir_all(dir.join("agents"));
        deps.settings.claude_path =
            crate::test_support::auth_stub(dir, "claude", "9.9.9 (Claude Code)");
        deps.settings.codex_path = crate::test_support::auth_stub(dir, "codex", "9.9.9");
        deps
    }


    fn codex_session(account: Option<&str>) -> SessionLaunchRequest {
        let mut codex = row("s-codex", None);
        codex.cli = Some("codex".to_string());
        codex.model = Some("gpt-5.6-sol".to_string());
        codex.effort = Some("high".to_string());
        codex.account = account.map(str::to_string);
        let mut req = session_request(codex, &[]);
        req.options = LaunchOptions {
            agent: CodingAgent::Codex,
            model: "gpt-5.6-sol".to_string(),
            ..base_options()
        };
        req
    }

    fn codex_home(dir: &Path, profile: &str) -> String {
        crate::agent_profiles::profile_dir(dir, CodingAgent::Codex, profile)
            .unwrap()
            .to_string_lossy()
            .into_owned()
    }

    /// A session naming no account lands on the signed-in profile — never
    /// the ambient login. The retired `system` reads the same.
    #[cfg(unix)]
    #[test]
    fn an_unnamed_launch_lands_on_the_signed_in_profile() {
        for account in [None, Some("system")] {
            let dir = temp_dir("gate-unnamed");
            let base = canned_server(vec![(200, RESOLVE_NONE.to_string())]);
            let deps = auth_deps(&base, &dir.0);
            let work = crate::agent_profiles::create(&dir.0, CodingAgent::Codex).unwrap();
            crate::test_support::sign_in_profile(&dir.0, CodingAgent::Codex, &work.id);

            let prepared = match launch(codex_session(account), &deps).unwrap() {
                Prepared::Ready(prepared) => prepared,
                other => panic!("{account:?}: expected Ready, got {other:?}"),
            };
            assert!(prepared
                .spawn
                .env
                .contains(&("CODEX_HOME".to_string(), codex_home(&dir.0, &work.id))));
            let record = crate::run_registry::get(&dir.0, "s-codex").expect("record");
            assert_eq!(record.account().as_deref(), Some(work.id.as_str()));
        }
    }

    /// EXP-1158: an UNNAMED launch whose LAST USED profile is signed out is
    /// not refused on it — it lands on a login that is signed in. Naming that
    /// profile is still refused by name.
    #[cfg(unix)]
    #[test]
    fn an_unnamed_launch_skips_a_signed_out_last_used_profile() {
        let dir = temp_dir("gate-last-used-signed-out");
        let base = canned_server(vec![(200, RESOLVE_NONE.to_string())]);
        let deps = auth_deps(&base, &dir.0);
        let work = crate::agent_profiles::create(&dir.0, CodingAgent::Codex).unwrap();
        let home = crate::agent_profiles::create(&dir.0, CodingAgent::Codex).unwrap();
        crate::agent_profiles::note_last_used(&dir.0, CodingAgent::Codex, &work.id).unwrap();
        crate::test_support::sign_in_profile(&dir.0, CodingAgent::Codex, &home.id);

        match launch(codex_session(Some(&work.id)), &deps).unwrap() {
            Prepared::Disabled(DisabledReason::DoctorFailed(_)) => {}
            other => panic!("expected DoctorFailed, got {other:?}"),
        }
        let prepared = match launch(codex_session(None), &deps).unwrap() {
            Prepared::Ready(prepared) => prepared,
            other => panic!("expected Ready, got {other:?}"),
        };
        assert!(prepared
            .spawn
            .env
            .contains(&("CODEX_HOME".to_string(), codex_home(&dir.0, &home.id))));
    }

    /// With NO signed-in profile there is nothing to run on: the agent's own
    /// doctor check refuses before any network call.
    #[cfg(unix)]
    #[test]
    fn no_signed_in_profile_refuses_the_launch() {
        let dir = temp_dir("gate-bare");
        let deps = auth_deps("http://127.0.0.1:1", &dir.0);
        match launch(session_request(row("s-bare", Some("repo-bare")), &["EXP-42"]), &deps).unwrap() {
            Prepared::Disabled(reason @ DisabledReason::DoctorFailed(_)) => {
                assert_eq!(
                    reason.message(),
                    "claude is installed but not signed in. Sign in from Settings → Agents, or from the Sign in button on the failed start."
                );
            }
            other => panic!("expected DoctorFailed, got {other:?}"),
        }
    }

    /// A session NAMING the signed-in profile starts inside that profile's dir.
    #[cfg(unix)]
    #[test]
    fn a_named_signed_in_profile_launches_in_its_dir() {
        let dir = temp_dir("gate-profile");
        let base = canned_server(vec![(200, RESOLVE_NONE.to_string())]);
        let deps = auth_deps(&base, &dir.0);
        let work = crate::agent_profiles::create(&dir.0, CodingAgent::Codex).unwrap();
        crate::test_support::sign_in_profile(&dir.0, CodingAgent::Codex, &work.id);
        let prepared = match launch(codex_session(Some(&work.id)), &deps).unwrap() {
            Prepared::Ready(prepared) => prepared,
            other => panic!("expected Ready, got {other:?}"),
        };
        assert!(prepared
            .spawn
            .env
            .contains(&("CODEX_HOME".to_string(), codex_home(&dir.0, &work.id))));
    }

    /// A named account that is signed out refuses by its email.
    #[cfg(unix)]
    #[test]
    fn a_signed_out_named_account_refuses_the_launch() {
        let dir = temp_dir("gate-named-out");
        let deps = auth_deps("http://127.0.0.1:1", &dir.0);
        let work = crate::agent_profiles::create(&dir.0, CodingAgent::Codex).unwrap();
        let home = crate::agent_profiles::create(&dir.0, CodingAgent::Codex).unwrap();
        crate::agent_profiles::stamp_login(&dir.0, CodingAgent::Codex, &home.id, Some("h@acme.test"))
            .unwrap();
        crate::test_support::sign_in_profile(&dir.0, CodingAgent::Codex, &work.id);
        match launch(codex_session(Some(&home.id)), &deps).unwrap() {
            Prepared::Disabled(reason @ DisabledReason::DoctorFailed(_)) => {
                assert_eq!(
                    reason.message(),
                    "codex account h@acme.test is signed out on this machine. Sign in from Settings → Agents, or pick another account."
                );
            }
            other => panic!("expected DoctorFailed, got {other:?}"),
        }
    }

    /// A record that names no login (the retired ambient one) resumes on the
    /// device's signed-in profile: it passes the gate rather than being
    /// refused on a login no run may spend.
    #[cfg(unix)]
    #[test]
    fn an_ambient_record_resumes_on_the_signed_in_profile() {
        let dir = temp_dir("gate-resume");
        let deps = auth_deps("http://127.0.0.1:1", &dir.0);
        let record = resume_record(&dir.0, "sess-old");
        assert_eq!(record.account(), None);
        let req = resume_request(record);
        if let Ok(Prepared::Disabled(reason)) = launch(req.clone(), &deps) {
            assert!(
                reason.message().starts_with("claude is installed but not signed in"),
                "no profile: refused for that, {}",
                reason.message()
            );
        }
        let work = crate::agent_profiles::create(&dir.0, CodingAgent::Claude).unwrap();
        crate::test_support::sign_in_profile(&dir.0, CodingAgent::Claude, &work.id);
        if let Ok(Prepared::Disabled(DisabledReason::DoctorFailed(failed))) = launch(req, &deps) {
            panic!("the resume must pass the gate: {:?}", failed.error);
        }
    }

    /// The agent shell takes the same rules as a session launch.
    #[cfg(unix)]
    #[test]
    fn an_agent_shell_judges_its_login_like_a_session() {
        let dir = temp_dir("gate-shell");
        let deps = auth_deps("http://127.0.0.1:1", &dir.0);
        match prepare_agent_shell(&agent_shell_request(None), &deps).unwrap() {
            PreparedAgentShell::Disabled(reason @ DisabledReason::DoctorFailed(_)) => {
                assert!(
                    reason.message().starts_with("claude is installed but not signed in"),
                    "{}",
                    reason.message()
                );
            }
            other => panic!("expected DoctorFailed, got {other:?}"),
        }
        let work = crate::agent_profiles::create(&dir.0, CodingAgent::Claude).unwrap();
        crate::test_support::sign_in_profile(&dir.0, CodingAgent::Claude, &work.id);
        match prepare_agent_shell(&agent_shell_request(None), &deps) {
            Err(CodingError::Api(ApiError::Transport { .. })) => {}
            other => {
                panic!("expected the launch to pass the gate and reach the network, got {other:?}")
            }
        }
    }
}
