//! `coding` — the Start-coding launcher (masterplan-v3 §3.1 / §07, DC-1).
//!
//! Phase 5 lands: git worktree creation via argv `git` (never `gh`, never a
//! git library), the `exp/<IDENTIFIER>` branch, ambient git auth via the
//! repo-local credential helper ([`git_credentials`] — EXP-73: `origin` stays
//! bare; the token is never logged — [`git_worktree::TokenUrl`] redacts, git
//! output is scrubbed), `.exp-mcp.json`, the seed prompt, the tooling doctor, the
//! coding settings (repos root / branch prefix / claude path — never
//! a manual API-key field), and the `claude` spawn into the embedded
//! terminal. Claude-only: the experimental codex adapter is deleted.
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
//! session id + PTY tee to the steer publisher (§08) — `coding` deliberately
//! does not depend on `steer` (§3.1 dependency direction).
//!
//! The eight steps, their failure surfaces (`DisabledReason` — never
//! falsely block, always explain), and the worktree layout are specified in
//! [`launcher`] / [`git_worktree`].

pub mod argv;
pub mod batch_launcher;
pub mod batch_prompt;
pub mod claude_task;
pub mod clone_manager;
pub mod doctor;
pub mod git_credentials;
pub mod git_worktree;
pub mod launcher;
pub mod mcp_json;
pub mod prompt;
pub mod run_launch;
pub mod scm;
pub mod settings;
#[cfg(test)]
pub(crate) mod test_support;
pub mod token_cache;
pub mod token_refresh;
pub mod trunk_state;

pub use argv::{permission_args, session_args, LaunchOptions};
pub use batch_launcher::{
    batch_branch_name, new_batch_id, BatchIssueSpec, BatchLaunchRequest, RepoGroup,
};
pub use batch_prompt::{render_batch_prompt, BatchPromptArgs};
pub use claude_task::{
    claude_task, claude_task_with_mcp, create_run_configs_prompt, fix_conflicts_prompt,
    resolve_pr_prompt, ClaudeTask,
};
pub use clone_manager::{AutoSyncOutcome, CloneEvent};
pub use doctor::{
    parse_claude_version, run_doctor, DoctorReport, Tool, ToolCheck, MIN_CLAUDE_VERSION,
};
pub use scm::{
    CommitInfo, ConflictKind, ConflictState, DiffFile, DiffLine, DiffLineKind, FileChange,
    FileStatus, StashEntry, StatusSummary, UnifiedHunk,
};
pub use git_credentials::{ensure_repo_auth, ensure_repo_auth_with_margin};
pub use token_cache::{token_cache, MintedToken, TokenCache};
pub use trunk_state::TrunkState;
pub use git_worktree::{branch_name, clone_path, worktree_path, GitError, TokenUrl};
pub use launcher::{
    default_device_label, end_session_best_effort, prepare, spawn_prepared, spawn_prepared_with,
    CodingDeps, CodingError, DisabledReason, ExitNotify, GitWorktrees, IssueSeed, IssueSeedFn,
    LaunchOrigin, LaunchOutcome, LaunchRequest, Prepared, PrepareRequest, PreparedLaunch,
    WorktreeProvider,
};
pub use mcp_json::{
    remove_stale_legacy_mcp_json, render_mcp_json, write_mcp_json, MCP_JSON_FILE,
};
pub use prompt::{
    deliver_prompt, deliver_prompt_file, render_prompt, write_rendered_prompt, PromptDelivery,
    PROMPT_ARGV_MAX_BYTES, PROMPT_FILE, SEED_LINE,
};
pub use run_launch::{
    format_argv_line, format_env_lines, parse_argv_line, parse_env_lines, play_state, run_root,
    run_spawn_spec, shell_cwd, PlayState, STOP_GRACE,
};
pub use settings::Settings;
pub use token_refresh::{
    next_refresh_delay, refresh_clone_token, REFRESH_LEAD, TOKEN_REFRESH_RETRY,
};
