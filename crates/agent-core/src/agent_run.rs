//! Agent-run handshake. The core never spawns the CLI itself: when the pipeline
//! needs an agent run it builds a `RunRequest`, emits it to the host (the GUI,
//! which runs `claude`/`codex` in its libghostty terminal), and blocks until the
//! host calls back with the result via `submit_result` (FFI:
//! `agent_core_submit_run_result`). Correlation is by `run_id` through a
//! registry of one-shot channels.

use crate::mcp_config;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Mutex, OnceLock};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunRequest {
    pub run_id: String,
    pub cwd: String,
    pub mode: String, // "plan" | "code"
    pub program: String,
    pub argv: Vec<String>,
    pub env: Vec<(String, String)>,
    pub mcp_config_path: Option<String>,
    pub system_prompt: String,
    pub user_prompt: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunResult {
    pub exit_code: i32,
    pub final_text: String,
}

static REGISTRY: OnceLock<Mutex<HashMap<String, mpsc::Sender<RunResult>>>> = OnceLock::new();
static COUNTER: AtomicU64 = AtomicU64::new(1);

fn registry() -> &'static Mutex<HashMap<String, mpsc::Sender<RunResult>>> {
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn new_run_id() -> String {
    format!("run-{}", COUNTER.fetch_add(1, Ordering::Relaxed))
}

/// Register the run, hand it to the host via `emit`, then block for the result.
/// Returns a failure result if the channel is dropped (host never answered).
pub fn request_run(req: RunRequest, emit: impl FnOnce(&RunRequest)) -> RunResult {
    let (tx, rx) = mpsc::channel();
    registry().lock().unwrap().insert(req.run_id.clone(), tx);
    emit(&req);
    rx.recv().unwrap_or(RunResult { exit_code: -1, final_text: String::new() })
}

/// Deliver a host's run result to the waiting pipeline. Returns false if no run
/// with that id is pending.
pub fn submit_result(run_id: &str, exit_code: i32, final_text: String) -> bool {
    let tx = registry().lock().unwrap().remove(run_id);
    match tx {
        Some(tx) => tx.send(RunResult { exit_code, final_text }).is_ok(),
        None => false,
    }
}

/// Cancel one pending run (drops its channel → the waiter gets a failure result).
pub fn cancel(run_id: &str) -> bool {
    registry().lock().unwrap().remove(run_id).is_some()
}

/// Drop all pending run channels — unblocks every parked `request_run` caller
/// (used on shutdown so pipeline threads don't hang waiting for the host).
pub fn cancel_all() {
    registry().lock().unwrap().clear();
}

/// Build a claude-CLI run request (`--print`, `--mcp-config`,
/// `--permission-mode plan|acceptEdits`). Plain `--print` (no stream-json) so a
/// headless host can capture stdout directly as the final text; the combined
/// prompt is passed positionally / via the carried prompt fields.
pub fn build_claude_run(cwd: &str, mode: &str, mcp_config_path: &str, system_prompt: &str, user_prompt: &str) -> RunRequest {
    let permission_mode = if mode == "plan" { "plan" } else { "acceptEdits" };
    RunRequest {
        run_id: new_run_id(),
        cwd: cwd.to_string(),
        mode: mode.to_string(),
        program: "claude".to_string(),
        argv: vec![
            "--print".into(),
            "--mcp-config".into(),
            mcp_config_path.into(),
            "--permission-mode".into(),
            permission_mode.into(),
        ],
        env: vec![],
        mcp_config_path: Some(mcp_config_path.to_string()),
        system_prompt: system_prompt.to_string(),
        user_prompt: user_prompt.to_string(),
    }
}

/// Build a codex-CLI run request (`--sandbox read-only|workspace-write`, MCP via
/// a config dir + the token in `EXPONENTIAL_MCP_TOKEN`).
pub fn build_codex_run(cwd: &str, mode: &str, mcp_url: &str, token: &str, system_prompt: &str, user_prompt: &str) -> RunRequest {
    let sandbox = if mode == "plan" { "read-only" } else { "workspace-write" };
    RunRequest {
        run_id: new_run_id(),
        cwd: cwd.to_string(),
        mode: mode.to_string(),
        program: "codex".to_string(),
        argv: vec!["exec".into(), "--sandbox".into(), sandbox.into()],
        env: vec![
            ("EXPONENTIAL_MCP_TOKEN".to_string(), token.to_string()),
            ("EXPONENTIAL_MCP_CONFIG".to_string(), mcp_config::codex_config_toml(mcp_url)),
        ],
        mcp_config_path: None,
        system_prompt: system_prompt.to_string(),
        user_prompt: user_prompt.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn request_run_blocks_until_submit() {
        let req = RunRequest {
            run_id: new_run_id(),
            cwd: "/tmp".into(),
            mode: "plan".into(),
            program: "claude".into(),
            argv: vec![],
            env: vec![],
            mcp_config_path: None,
            system_prompt: String::new(),
            user_prompt: String::new(),
        };
        let id = req.run_id.clone();
        let result = request_run(req, |_r| {
            // Host runs the CLI asynchronously, then submits the result.
            let id = id.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(20));
                assert!(submit_result(&id, 0, "### PLAN\nbody".into()));
            });
        });
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.final_text, "### PLAN\nbody");
    }

    #[test]
    fn submit_unknown_run_is_false() {
        assert!(!submit_result("run-does-not-exist", 0, "x".into()));
    }

    #[test]
    fn run_ids_are_unique() {
        assert_ne!(new_run_id(), new_run_id());
    }

    #[test]
    fn claude_run_has_safe_flags() {
        let r = build_claude_run("/w", "plan", "/w/.mcp.json", "sys", "user");
        assert_eq!(r.program, "claude");
        assert!(r.argv.contains(&"--permission-mode".to_string()));
        let i = r.argv.iter().position(|a| a == "--permission-mode").unwrap();
        assert_eq!(r.argv[i + 1], "plan");
        let code = build_claude_run("/w", "code", "/w/.mcp.json", "s", "u");
        let j = code.argv.iter().position(|a| a == "--permission-mode").unwrap();
        assert_eq!(code.argv[j + 1], "acceptEdits");
    }

    // Parity with companion drivers.test.ts: codex exposes safe sandbox defaults
    // (plan → read-only, code → workspace-write) via `exec --sandbox`.
    #[test]
    fn codex_run_has_safe_flags() {
        let plan = build_codex_run("/w", "plan", "https://x.at/api/mcp", "expk_t", "sys", "user");
        assert_eq!(plan.program, "codex");
        assert_eq!(plan.argv.first().map(String::as_str), Some("exec"));
        let i = plan.argv.iter().position(|a| a == "--sandbox").expect("--sandbox flag");
        assert_eq!(plan.argv[i + 1], "read-only");
        let code = build_codex_run("/w", "code", "https://x.at/api/mcp", "expk_t", "s", "u");
        let j = code.argv.iter().position(|a| a == "--sandbox").expect("--sandbox flag");
        assert_eq!(code.argv[j + 1], "workspace-write");
    }

    // Parity with the companion's UnsafePermissionError contract: the drivers
    // HARD-REFUSE unsafe modes — claude FORBIDDEN_PERMISSION_MODES={bypassPermissions},
    // codex FORBIDDEN_SANDBOX_MODES={danger-full-access}. Our run builders never
    // emit any unsafe flag, in either plan or code mode.
    #[test]
    fn drivers_never_emit_unsafe_modes() {
        const UNSAFE: &[&str] = &[
            "bypassPermissions",
            "--dangerously-skip-permissions",
            "danger-full-access",
            "--dangerously-bypass-approvals-and-sandbox",
            "--yolo",
        ];
        for mode in ["plan", "code"] {
            let claude = build_claude_run("/w", mode, "/w/.mcp.json", "s", "u");
            let codex = build_codex_run("/w", mode, "https://x.at/api/mcp", "expk_t", "s", "u");
            for r in [&claude, &codex] {
                let joined = r.argv.join(" ");
                for bad in UNSAFE {
                    assert!(!joined.contains(bad), "{} emitted unsafe flag {bad} in {mode} mode", r.program);
                }
            }
        }
    }
}
