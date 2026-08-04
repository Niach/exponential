//! `coding` — the Start-coding launcher (masterplan-v3 §3.1 / §07, DC-1).
//!
//! Phase 5 lands: git worktree creation via argv `git` (never `gh`, never a
//! git library), the `exp/<IDENTIFIER>` branch, ambient git auth via the
//! repo-local credential helper ([`git_credentials`] — EXP-73: `origin` stays
//! bare; the token is never logged — [`git_worktree::TokenUrl`] redacts, git
//! output is scrubbed), `.exp-mcp.json`, the seed prompt, the tooling doctor, the
//! coding settings (repos root / branch prefix / per-agent paths — never
//! a manual API-key field), and the agent spawn into the embedded
//! terminal. EXP-201: three agents — `claude`, `codex`, and `pi`
//! ([`agent::CodingAgent`]). (EXP-259 deleted the one-shot claude-task
//! primitive — conflict fixing became the builtin "Fix merge conflicts"
//! ACTION run.)
//!
//! ## The one entry point (§7.1)
//!
//! Local button press and relay `start_session` frame run the SAME sequence:
//!
//! ```text
//! // background executor (blocking network + git I/O, gpui-free):
//! let prepared = coding::prepare(&req, &deps)?;
//! // foreground (gpui):
//! match prepared {
//!     Prepared::Ready(p) => coding::spawn_prepared(p, &terminal_manager, cx, trpc)?,
//!     Prepared::Disabled(reason) => LaunchOutcome::Disabled { reason },
//! }
//! ```
//!
//! On `LaunchOutcome::Spawned { session_id, .. }` the app/ui layer hands the
//! session id to the steer publisher (§08; the PTY tee was removed with the
//! binary mirror in EXP-249 — only the scrubbed activity stream publishes) —
//! `coding` deliberately does not depend on `steer` (§3.1 dependency
//! direction).
//!
//! The eight steps, their failure surfaces (`DisabledReason` — never
//! falsely block, always explain), and the worktree layout are specified in
//! [`launcher`] / [`git_worktree`].

pub mod action_prompt;
pub mod agent;
pub mod argv;
pub mod batch_launcher;
pub mod batch_prompt;
pub mod clone_manager;
pub mod codex_sessions;
pub mod codex_trust;
pub mod doctor;
pub mod git_credentials;
pub mod git_worktree;
pub mod launcher;
pub mod mcp_json;
pub mod pi_bridge;
pub mod prompt;
pub mod reaper;
pub mod scm;
pub mod settings;
#[cfg(test)]
pub(crate) mod test_support;
pub mod token_cache;
pub mod token_refresh;
pub mod trunk_state;
pub mod worktree_agents;

pub use agent::CodingAgent;
pub use argv::{
    permission_args, session_args, AgentMcp, LaunchOptions, SessionTail, HOOK_PORT_ENV,
    HOOK_TOKEN_ENV, MCP_TOKEN_ENV, MCP_URL_ENV, OBSERVER_TOKEN_ENV, OBSERVER_URL_ENV,
};
pub use batch_launcher::{
    batch_branch_name, new_batch_id, BatchIssueSpec, BatchLaunchRequest, RepoGroup,
};
pub use action_prompt::{
    create_action_prompt, fix_pr_conflicts_prompt, render_action_prompt, ActionInputValue,
};
pub use batch_prompt::{render_batch_prompt, BatchPromptArgs};
pub use clone_manager::{AutoSyncOutcome, CloneEvent};
pub use codex_sessions::{default_codex_sessions_root, find_latest_codex_session_id};
pub use doctor::{
    parse_claude_version, run_doctor, AgentAdvertisement, DoctorReport, Tool, ToolCheck,
    MIN_CLAUDE_VERSION,
};
pub use scm::{
    CommitInfo, ConflictKind, ConflictState, DiffFile, DiffLine, DiffLineKind, FileChange,
    FileStatus, StatusSummary, UnifiedHunk,
};
pub use git_credentials::{ensure_repo_auth, ensure_repo_auth_with_margin};
pub use token_cache::{token_cache, MintedToken, TokenCache};
pub use trunk_state::TrunkState;
pub use git_worktree::{
    branch_name, clone_path, shell_cwd, worktree_path, GitError, TokenUrl,
};
#[cfg(feature = "gpui")]
pub use launcher::{spawn_prepared, spawn_prepared_with, ExitNotify};
pub use launcher::SESSION_HEARTBEAT_INTERVAL;
pub use launcher::{
    default_device_label, end_session_best_effort, prepare, prepare_agent_shell,
    prepare_with_hooks, ActionLaunchRequest, ActionRunKind,
    AgentShellLaunch, AgentShellRequest, CodingDeps, CodingError, DisabledReason,
    GitWorktrees, HookSetup, IssueSeed, IssueSeedFn, LaunchOrigin, LaunchOutcome, LaunchRequest,
    ObserverSetup,
    Prepared, PreparedAgentShell, PrepareRequest, PreparedLaunch, WorktreeProvider,
};
pub use mcp_json::{
    remove_stale_legacy_mcp_json, render_mcp_json, write_mcp_json, MCP_JSON_FILE,
};
pub use pi_bridge::{write_pi_bridge, write_pi_observer, PI_BRIDGE_FILE, PI_OBSERVER_FILE};
pub use prompt::{
    deliver_prompt, deliver_prompt_file, render_prompt, render_resume_prompt,
    write_rendered_prompt, PromptDelivery, PROMPT_ARGV_MAX_BYTES, PROMPT_FILE, SEED_LINE,
};
pub use settings::Settings;
pub use token_refresh::{
    next_refresh_delay, refresh_clone_token, REFRESH_LEAD, TOKEN_REFRESH_RETRY,
};
pub use worktree_agents::{record_worktree_agent, worktree_agents, AGENTS_FILE};
