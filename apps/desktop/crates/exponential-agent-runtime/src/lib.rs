//! exponential-agent-runtime (EXP-1426): the single-account agent core Exponential and
//! spezl share. It spawns the user's own, unmodified `claude`/`codex` binary,
//! streams what the agent does, reads the CLIs' own session history, runs
//! their login in a PTY and wires MCP servers, for ONE local account and with
//! no server. See `README.md` for the design and the extraction order.
//!
//! Two rules everything else follows from:
//!
//! - **No server coupling.** Nothing here knows tRPC, the token store, the
//!   device id, the steer relay or the pickup loop. A host that has a server
//!   (Exponential's `coding` + `engine`) implements the seams (they land with
//!   the modules that need them, see the README); spezl implements them
//!   locally. `tests/dependency_gate.rs` fails the build of
//!   the test suite when an app crate sneaks into `Cargo.toml`.
//! - **Never touch an agent's credentials.** The core spawns the vendor's
//!   binary and lets it own its login; it never reads, copies, moves or
//!   refreshes a token (Anthropic's Claude Code terms forbid intermediating
//!   one). Exponential's multi-account credential handling stays in `coding`.
//!
//! Extraction so far: the pure leaves both hosts already shared (atomic config
//! writes, the lock file, the claude/codex trust seeders, the codex session
//! index, the confinement check, the command head, the Linux run scope and the
//! background `Command` constructor). The old paths (`coding::lockfile`,
//! `engine::run_scope`, `api::atomic_file`, `terminal::process`, …) re-export
//! these modules, so no caller changed.

pub mod atomic_config;
pub mod atomic_file;
// EXP-1426 part 4c: the claude transcript locator (out of `coding::launcher`).
pub mod claude_transcript;
pub mod claude_trust;
pub mod codex_sessions;
pub mod codex_trust;
// EXP-1206: the human-readable head of a command line (`git status`). Pure.
pub mod command_head;
// The cwd-confinement check the adapters answer file/terminal requests with.
pub mod confine;
// EXP-1426 part 4: the ACP engine core (transport, child exit, event sink,
// local feed, run policies), moved out of `engine`.
pub mod engine;
pub mod lockfile;
// EXP-1426 part 4c: the MCP wiring vocabulary (`coding::argv`, `api::mcp_servers`).
pub mod mcp;
// EXP-419: the one `Command` constructor for background spawns.
pub mod process;
/// NEW-17: each agent run in its own systemd scope on Linux.
pub mod run_scope;
// EXP-1426 part 4c: live rate-limit windows + their registry (`coding::agent_usage`).
pub mod usage;
// EXP-1426 part 4: `SpawnSpec`, `ChildExit` and the login PATH (`terminal::pty`).
pub mod spawn;
// EXP-1426 part 2: the event vocabulary (`ActivityEvent`, the frames, the
// redactor, the relay caps), moved out of `steer`.
pub mod wire;

pub use process::{background_command, refresh_windows_path};

// EXP-1426 (part 3): agent identity, detection and the single-account login.
pub mod agent;
pub mod codex_app_server;
pub mod codex_login_picker;
pub mod detect;
pub mod login;
pub mod login_picker;
pub mod login_screen;

pub use agent::{claude_model_alias, CodingAgent};
