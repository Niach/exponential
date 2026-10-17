//! `coding` — the Start-coding launcher (masterplan-v3 §3.1 / §07, DC-1).
//!
//! Phase 5 lands: git worktree creation via argv `git` (never `gh`, never a
//! git library), the `exp/<IDENTIFIER>` branch, ambient git auth via the
//! repo-local credential helper ([`git_credentials`] — EXP-73: `origin` stays
//! bare; the token is never logged — [`git_worktree::TokenUrl`] redacts, git
//! output is scrubbed), `.exp-mcp.json`, the seed prompt, the tooling doctor, the
//! coding settings (repos root / branch prefix / per-agent paths — never
//! a manual API-key field), and the agent spawn into the embedded
//! terminal. EXP-201/EXP-849: two agents — `claude` and `codex`
//! ([`agent::CodingAgent`]).
//!
//! ## The one entry point (§7.1)
//!
//! EXP-1292 (Threads v2): every session is a `sessions` row; a `queued` one
//! naming this device is picked up off the shape ([`pickup`]), claimed, and
//! run through the SAME sequence:
//!
//! ```text
//! // background executor (blocking network + git I/O, gpui-free):
//! let prepared = coding::prepare(&PrepareRequest::Session(req), &deps)?;
//! // foreground:
//! match prepared {
//!     Prepared::Ready(p) => engine::start(p, ..)?,
//!     Prepared::Disabled(reason) => LaunchOutcome::Disabled { reason },
//! }
//! ```
//!
//! EXP-773: the in-process ACP engine (`crates/engine`) is the ONE host —
//! it owns the publisher, the activity vocabulary and the session's end.
//! `coding` deliberately depends on neither `engine` nor `steer` (§3.1
//! dependency direction).
//!
//! The eight steps, their failure surfaces (`DisabledReason` — never
//! falsely block, always explain), and the worktree layout are specified in
//! [`launcher`] / [`git_worktree`].

pub mod account_option;
// EXP-1082: the account-rotation seams EXP-1005 fills.
pub mod account_rotation;
pub mod agent;
pub mod agent_accounts;
pub mod agent_login;
pub mod agent_profiles;
pub mod agent_update;
pub mod agent_usage;
// EXP-1330: a `/btw` aside — a read-only fork of its parent's conversation.
pub mod aside;
pub mod atomic_config;
pub mod argv;
pub mod triggers;
pub mod claude_oauth;
pub mod claude_trust;
pub mod clone_manager;
pub mod codex_app_server;
pub mod codex_sessions;
pub mod codex_trust;
pub mod context_layout;
pub mod device_doctor;
pub mod doctor;
pub mod git_credentials;
pub mod git_worktree;
// EXP-1252: a session moving to another device (the handoff package).
pub mod handoff;
pub mod inventory;
pub mod launch_defaults_sync;
pub mod launch_gate;
pub mod launcher;
pub mod lockfile;
pub mod managed_codex;
// EXP-1099: the rotating log file + the heartbeat failure policy.
pub mod logging;
pub mod mcp_json;
pub mod mcp_servers;
pub mod process;
pub mod prompt;
pub mod pickup;
pub mod prune;
pub mod remote_admin;
pub mod reaper;
pub mod run_cleanup;
pub mod run_registry;
pub mod scratch;
pub mod scm;
pub mod wip_save;
pub mod session_retention;
pub mod settings;
pub mod skill;
#[cfg(test)]
pub(crate) mod test_support;
pub mod token_cache;
pub mod token_refresh;
pub mod token_refresh_host;
pub mod trunk_state;
pub mod usage_cache;
pub mod worktree_agents;

/// EXP-1196: the device's computer-use server, re-exported so the hosts
/// (engine, ui, cli) reach it through the launcher crate they already link.
pub use computer;
/// EXP-1236: the device's code-mode server, re-exported for the same reason.
pub use codemode;

pub use agent::{claude_model_alias, CodingAgent};
pub use argv::{
    permission_args, shell_args, AgentMcp, LaunchOptions, McpServerWire, McpWireTransport,
    MCP_SESSION_ID_ENV, MCP_TOKEN_ENV,
};
pub use clone_manager::{AutoSyncOutcome, CloneEvent};
pub use context_layout::{CarriedBase, ContextLayers, ProjectMemoryFile};
pub use codex_sessions::default_codex_sessions_root;
pub use doctor::{
    device_caps, parse_claude_version, parse_codex_version, run_doctor,
    AgentAdvertisement, AgentLaunchDefaults, ClaudeAuthStatus, DoctorReport, Tool,
    ToolCheck, DEVICE_CAPS, MIN_CLAUDE_ACP_VERSION, RUN_CAPS,
    MIN_CLAUDE_VERSION, MIN_CODEX_ACP_VERSION,
};
pub use agent_update::{update_agent, AgentUpdateOutcome, AGENT_UPDATE_COMMAND};
pub use account_option::{
    carried_account_option, flatten_accounts, last_used_account_option, parse_account_option_key, AccountLimits,
    AccountModelLimit, AccountOption,
};
pub use agent_accounts::{now_iso, AgentAccount, AgentAccounts, AgentProfileEntry, Health, Importable};
pub use agent_profiles::{profile_id, AgentProfile};
pub use agent_login::{login_plan, LoginPhase, LoginPlan, LoginProgress};
pub use agent_usage::{
    collect_if_due, force_collect, refresh_on_demand, AgentStatusPayload,
    AgentUsage, AgentUsageMap, UsageWindow,
};
pub use mcp_servers::ResolvedMcp;
pub use scm::{
    diff_status, CommitInfo, ConflictKind, ConflictState, DiffFile, DiffHunk, DiffLine,
    DiffLineKind, DiffStatus, FileChange, FileStatus, StatusSummary,
};
pub use git_credentials::{ensure_repo_auth, ensure_repo_auth_with_margin};
pub use token_cache::{token_cache, MintedToken, TokenCache};
pub use token_refresh_host::{clone_refreshers, CloneRefreshers, RefresherHold};
pub use trunk_state::TrunkState;
pub use git_worktree::{
    branch_name, clone_path, shell_cwd, worktree_path, GitError, TokenUrl,
};
pub use launch_gate::LaunchHold;
pub use launcher::{apply_account_env, set_session_end_observer};
pub use launcher::{
    claude_projects_root, claude_transcript_exists, context_layers_for, default_device_label,
    end_session, end_session_best_effort, idle_session, locate_claude_transcript, prepare,
    prepare_agent_shell, record_last_used, session_branch, session_label, session_launch_options,
    AcpLaunch, ACP_TRANSPORT, AgentShellLaunch, AgentShellRequest, CodingDeps, CodingError,
    DisabledReason, GitWorktrees, LaunchOutcome, Prepared, PreparedAgentShell, PrepareRequest,
    PreparedLaunch, ResumeReconfigure, ResumeRun, ResumeSeed, SessionEndObserver, SessionLaunchRequest,
    WorktreeProvider, SESSION_BRANCH_PREFIX,
};
pub use run_cleanup::{remove_if_clean, CleanupOutcome, RunCleanup};
pub use wip_save::{save_wip_commit, WipSave};
pub use mcp_json::{
    remove_stale_legacy_mcp_json, render_mcp_json, write_mcp_json, MCP_JSON_FILE,
};
pub use prune::{prune_landed, PrunePolicy, PruneReport, SkipReason};
pub use inventory::{
    inventory_fingerprint, scan_clones, scan_inventory, CloneRef, WorktreeInventoryEntry,
    MAX_REPORTED_WORKTREES,
};
pub use launch_defaults_sync::{
    defaults_fingerprint, read_marker, reconcile, write_marker, ReconcileAction, SyncMarker,
};
pub use remote_admin::{
    apply_defaults_patch, defaults_wire, overlay_launch_defaults, AgentDefaultsPatch,
    DefaultsPatch,
};
pub use prompt::{reclaimed_workspace_note, ACCOUNT_SWITCH_CONTINUE_PROMPT};
pub use settings::Settings;
pub use token_refresh::{
    next_refresh_delay, refresh_clone_token, REFRESH_LEAD, TOKEN_REFRESH_RETRY,
};
pub use worktree_agents::{
    record_worktree_agent, record_worktree_agent_id, worktree_agents, AGENTS_FILE,
};
