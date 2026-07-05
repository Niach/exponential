//! `coding` — the Start-coding launcher (masterplan-v3 §3.1 / §07, DC-1).
//!
//! Phase 5 lands: git worktree creation via argv `git` (never `gh`, never a
//! git library), the `exp/<IDENTIFIER>` branch, the token-embedded remote
//! (never logged — [`git_worktree::TokenUrl`] redacts, git output is
//! scrubbed), `.mcp.json` + `PROMPT.md`, the tooling doctor, the
//! coding settings (repos root / branch prefix / claude path — never
//! a manual API-key field), and the
//! `claude --dangerously-skip-permissions` spawn into the embedded terminal.
//!
//! ## The one entry point (§7.1)
//!
//! Local button press and relay `start_session` frame run the SAME sequence:
//!
//! ```text
//! // background executor (blocking network + git I/O, gpui-free):
//! let prepared = coding::prepare_launch(&req, &deps)?;
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

pub mod claude_task;
pub mod clone_manager;
pub mod doctor;
pub mod git_worktree;
pub mod launcher;
pub mod mcp_json;
pub mod prompt;
pub mod run_launch;
pub mod scm;
pub mod settings;
pub mod trunk_state;

pub use claude_task::{
    claude_task, create_run_configs_prompt, fix_conflicts_prompt, resolve_pr_prompt, ClaudeTask,
};
pub use clone_manager::CloneEvent;
pub use doctor::{run_doctor, DoctorReport, Tool, ToolCheck};
pub use scm::{
    CommitInfo, ConflictKind, ConflictState, DiffFile, DiffLine, DiffLineKind, FileChange,
    FileStatus, StatusSummary, UnifiedHunk,
};
pub use trunk_state::TrunkState;
pub use git_worktree::{branch_name, clone_path, worktree_path, GitError, TokenUrl};
pub use launcher::{
    default_device_label, end_session_best_effort, prepare_launch, spawn_prepared,
    spawn_prepared_with, CodingDeps, CodingError, DisabledReason, ExitNotify, GitWorktrees,
    IssueSeed, IssueSeedFn, LaunchOrigin, LaunchOutcome, LaunchRequest, Prepared, PreparedLaunch,
    WorktreeProvider,
};
pub use mcp_json::{render_mcp_json, write_mcp_json, MCP_JSON_FILE};
pub use prompt::{render_prompt, write_prompt, PROMPT_FILE, SEED_LINE};
pub use run_launch::{
    format_argv_line, format_env_lines, parse_argv_line, parse_env_lines, play_state, run_root,
    run_spawn_spec, shell_cwd, PlayState, STOP_GRACE,
};
pub use settings::Settings;
