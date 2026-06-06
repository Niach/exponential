//! Agent-run handshake. The core never spawns the CLI itself: when the pipeline
//! needs an agent run it builds a `RunRequest`, emits it to the host (the GUI,
//! which runs `claude`/`codex` in its libghostty terminal), and blocks until the
//! host calls back with the result via `submit_result` (FFI:
//! `agent_core_submit_run_result`). Correlation is by `run_id` through a
//! registry of one-shot channels.

use crate::mcp_config;
use std::collections::HashMap;
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Generous wall-clock cap for an in-core headless run (plan/code stages take
/// minutes, so this is deliberately large — it's a runaway guard, not the short
/// HTTP `Config::timeout_s`). The legacy host path had no timeout at all.
pub const HEADLESS_RUN_TIMEOUT_S: u64 = 30 * 60;

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
    // Interactive runs launch the CLI WITHOUT `--print` so the user watches and
    // steers it in the embedded terminal; the plan is delivered out-of-band via
    // the MCP plan-submit tool (no stdout parsing). `continue_session_id` resumes
    // a prior session (approve-and-continue) in the reused worktree.
    pub interactive: bool,
    pub continue_session_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunResult {
    pub exit_code: i32,
    pub final_text: String,
    // The CLI session id (claude), captured by the host so an interactive run
    // can later be `--continue`d. None for headless runs / hosts that don't
    // surface it.
    pub session_id: Option<String>,
}

static REGISTRY: OnceLock<Mutex<HashMap<String, mpsc::Sender<RunResult>>>> = OnceLock::new();
static COUNTER: AtomicU64 = AtomicU64::new(1);
// issue_id → the run_id currently in flight for that issue. The dispatcher
// serializes runs per issue (its `running` set), so each issue maps to at most
// one run at a time. Lets the host cancel "the run for this issue" without
// tracking run_ids itself (`cancel_issue` / `agent_core_cancel_issue`).
static ISSUE_RUNS: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

fn registry() -> &'static Mutex<HashMap<String, mpsc::Sender<RunResult>>> {
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn issue_runs() -> &'static Mutex<HashMap<String, String>> {
    ISSUE_RUNS.get_or_init(|| Mutex::new(HashMap::new()))
}

// run_id → the spawned headless child, so cancel/shutdown can kill it. Headless
// runs (the background pipeline's plan/code stages) execute IN-CORE via
// `run_headless`; only interactive runs are delegated to the host's terminal.
static PROCS: OnceLock<Mutex<HashMap<String, Arc<Mutex<Child>>>>> = OnceLock::new();

fn procs() -> &'static Mutex<HashMap<String, Arc<Mutex<Child>>>> {
    PROCS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Kill a headless child AND its descendants. Headless children are spawned as
/// their own process-group leader (`process_group(0)`), so a group SIGKILL reaps
/// grandchildren too — otherwise one that inherited the stdout pipe (e.g. a CLI
/// subprocess) would keep it open and block our reader threads after a kill.
fn kill_child_group(child: &Mutex<Child>) {
    let mut guard = child.lock().unwrap();
    #[cfg(unix)]
    {
        // The child is the group leader, so its pid == pgid; `-pid` targets the
        // whole group.
        let pid = guard.id() as i32;
        unsafe {
            libc::kill(-pid, libc::SIGKILL);
        }
    }
    let _ = guard.kill();
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
    rx.recv().unwrap_or(RunResult { exit_code: -1, final_text: String::new(), session_id: None })
}

/// Like `request_run`, but records the issue → run_id mapping for the duration
/// of the run so `cancel_issue` can target it. The mapping is cleared when the
/// run completes (or is cancelled) and the waiter unblocks.
pub fn request_run_for_issue(issue_id: &str, req: RunRequest, emit: impl FnOnce(&RunRequest)) -> RunResult {
    let run_id = req.run_id.clone();
    issue_runs().lock().unwrap().insert(issue_id.to_string(), run_id.clone());
    let result = request_run(req, emit);
    // Only clear if it still points at this run (a newer run shouldn't be able to
    // start for the same issue while this one blocks, but be defensive).
    let mut map = issue_runs().lock().unwrap();
    if map.get(issue_id).map(String::as_str) == Some(run_id.as_str()) {
        map.remove(issue_id);
    }
    result
}

/// Deliver a host's run result to the waiting pipeline. Returns false if no run
/// with that id is pending. `session_id` is the CLI session (for `--continue`).
pub fn submit_result(run_id: &str, exit_code: i32, final_text: String, session_id: Option<String>) -> bool {
    let tx = registry().lock().unwrap().remove(run_id);
    match tx {
        Some(tx) => tx.send(RunResult { exit_code, final_text, session_id }).is_ok(),
        None => false,
    }
}

/// Cancel one run: kill its in-core headless child (if any) AND drop its host
/// channel (if any). Returns true if either was found.
pub fn cancel(run_id: &str) -> bool {
    let proc = procs().lock().unwrap().remove(run_id);
    let killed = if let Some(child) = proc {
        kill_child_group(&child);
        true
    } else {
        false
    };
    registry().lock().unwrap().remove(run_id).is_some() || killed
}

/// Cancel the run currently in flight for an issue (the desktop "Cancel" button).
/// Looks up the issue's run_id and drops its channel; the parked pipeline thread
/// then unblocks with a failure result and the issue stops running. Returns
/// false if no run is in flight for that issue.
pub fn cancel_issue(issue_id: &str) -> bool {
    let run_id = issue_runs().lock().unwrap().get(issue_id).cloned();
    match run_id {
        Some(id) => cancel(&id),
        None => false,
    }
}

/// Drop all pending run channels + kill all in-core children — unblocks every
/// parked `request_run` caller and stops every headless run (used on shutdown so
/// pipeline threads don't hang).
pub fn cancel_all() {
    registry().lock().unwrap().clear();
    issue_runs().lock().unwrap().clear();
    let drained: Vec<_> = procs().lock().unwrap().drain().map(|(_, c)| c).collect();
    for child in drained {
        kill_child_group(&child);
    }
}

/// Combined system+user prompt — matches the host wrapper's `buildPrompt` so the
/// CLI sees identical input whether a run executes in-core or (legacy) on a host.
fn combined_prompt(system: &str, user: &str) -> String {
    format!("{system}\n\n<user_issue>\n{user}\n</user_issue>")
}

/// Execute a HEADLESS run IN-CORE (the background pipeline's plan/code stages):
/// spawn the CLI with the prompt as the final positional arg — mirroring the
/// host's `<program> <argv> "$(cat prompt)" 2>&1` wrapper — drain stdout+stderr
/// (threaded, so a full pipe can't deadlock the child), enforce `timeout_s`, and
/// return the combined output + exit code. Killable via `cancel`/`cancel_issue`/
/// `cancel_all`. The host is no longer involved in headless runs at all.
pub fn run_headless(req: &RunRequest, timeout_s: u64) -> RunResult {
    let prompt = combined_prompt(&req.system_prompt, &req.user_prompt);
    let mut cmd = Command::new(&req.program);
    cmd.args(&req.argv)
        .arg(&prompt)
        .current_dir(&req.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in &req.env {
        cmd.env(k, v);
    }
    // Own process group so cancel/timeout can SIGKILL the whole tree (the CLI may
    // spawn subprocesses that inherit — and keep open — the stdout pipe).
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return RunResult {
                exit_code: -1,
                final_text: format!("failed to spawn {}: {e}", req.program),
                session_id: None,
            }
        }
    };

    // Drain both pipes off-thread so a full pipe buffer can't deadlock the child.
    let mut out = child.stdout.take();
    let mut err = child.stderr.take();
    let out_h = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(o) = out.as_mut() {
            let _ = o.read_to_string(&mut s);
        }
        s
    });
    let err_h = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(e) = err.as_mut() {
            let _ = e.read_to_string(&mut s);
        }
        s
    });

    let child = Arc::new(Mutex::new(child));
    procs().lock().unwrap().insert(req.run_id.clone(), Arc::clone(&child));

    let deadline = Instant::now() + Duration::from_secs(timeout_s.max(1));
    let mut exit_code = -1;
    loop {
        // Cancelled? (cancel/cancel_all removed us from PROCS.)
        if !procs().lock().unwrap().contains_key(&req.run_id) {
            kill_child_group(&child);
            break;
        }
        match child.lock().unwrap().try_wait() {
            Ok(Some(status)) => {
                exit_code = status.code().unwrap_or(-1);
                break;
            }
            Ok(None) => {}
            Err(_) => break,
        }
        if Instant::now() >= deadline {
            kill_child_group(&child);
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    procs().lock().unwrap().remove(&req.run_id);
    // Reap (no-op if already exited) so we don't leave a zombie after a kill.
    let _ = child.lock().unwrap().wait();

    let mut text = out_h.join().unwrap_or_default();
    let err_text = err_h.join().unwrap_or_default();
    if !err_text.is_empty() {
        text.push_str(&err_text);
    }
    RunResult { exit_code, final_text: text, session_id: None }
}

/// `run_headless`, recording the issue→run_id mapping so `cancel_issue` can
/// target it (parity with `request_run_for_issue`).
pub fn run_headless_for_issue(issue_id: &str, req: &RunRequest, timeout_s: u64) -> RunResult {
    issue_runs()
        .lock()
        .unwrap()
        .insert(issue_id.to_string(), req.run_id.clone());
    let result = run_headless(req, timeout_s);
    let mut map = issue_runs().lock().unwrap();
    if map.get(issue_id).map(String::as_str) == Some(req.run_id.as_str()) {
        map.remove(issue_id);
    }
    result
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
        interactive: false,
        continue_session_id: None,
    }
}

/// Build an INTERACTIVE claude run: no `--print` (the user watches/steers it in
/// the embedded terminal), `--dangerously-skip-permissions`, and the MCP config.
/// The plan/code is delivered out-of-band (the agent calls the Exponential MCP
/// tools inside the session). Plan stage uses `--permission-mode plan`; the
/// continue (code) stage uses `acceptEdits` + `--continue` to resume the same
/// session in the reused worktree.
pub fn build_claude_interactive_run(
    cwd: &str,
    mcp_config_path: &str,
    system_prompt: &str,
    user_prompt: &str,
    continue_session_id: Option<&str>,
) -> RunRequest {
    let continuing = continue_session_id.is_some();
    let permission_mode = if continuing { "acceptEdits" } else { "plan" };
    let mut argv = vec![
        "--dangerously-skip-permissions".into(),
        "--permission-mode".into(),
        permission_mode.into(),
        "--mcp-config".into(),
        mcp_config_path.into(),
    ];
    if continuing {
        argv.push("--continue".into());
    }
    RunRequest {
        run_id: new_run_id(),
        cwd: cwd.to_string(),
        mode: if continuing { "code".into() } else { "plan".into() },
        program: "claude".to_string(),
        argv,
        env: vec![],
        mcp_config_path: Some(mcp_config_path.to_string()),
        system_prompt: system_prompt.to_string(),
        user_prompt: user_prompt.to_string(),
        interactive: true,
        continue_session_id: continue_session_id.map(|s| s.to_string()),
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
        interactive: false,
        continue_session_id: None,
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
            interactive: false,
            continue_session_id: None,
        };
        let id = req.run_id.clone();
        let result = request_run(req, |_r| {
            // Host runs the CLI asynchronously, then submits the result.
            let id = id.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(20));
                assert!(submit_result(&id, 0, "### PLAN\nbody".into(), Some("sess-1".into())));
            });
        });
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.final_text, "### PLAN\nbody");
        assert_eq!(result.session_id.as_deref(), Some("sess-1"));
    }

    #[test]
    fn submit_unknown_run_is_false() {
        assert!(!submit_result("run-does-not-exist", 0, "x".into(), None));
    }

    #[test]
    fn cancel_issue_unblocks_the_runs_waiter() {
        let issue_id = format!("issue-{}", new_run_id());
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
            interactive: false,
            continue_session_id: None,
        };
        let issue = issue_id.clone();
        let result = request_run_for_issue(&issue_id, req, |_r| {
            // The "cancel button": cancel by issue id while the run is parked.
            let issue = issue.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(20));
                assert!(cancel_issue(&issue));
            });
        });
        // Cancelling drops the channel → the waiter gets the failure result.
        assert_eq!(result.exit_code, -1);
        // The mapping is cleared once the waiter unblocks.
        assert!(!cancel_issue(&issue_id));
    }

    #[test]
    fn cancel_issue_unknown_is_false() {
        assert!(!cancel_issue("issue-does-not-exist"));
    }

    #[test]
    fn interactive_builder_has_no_print_and_unsafe_plan_mode() {
        let plan = build_claude_interactive_run("/w", "/w/.mcp.json", "sys", "user", None);
        assert!(plan.interactive);
        assert!(!plan.argv.iter().any(|a| a == "--print"));
        assert!(plan.argv.iter().any(|a| a == "--dangerously-skip-permissions"));
        let i = plan.argv.iter().position(|a| a == "--permission-mode").unwrap();
        assert_eq!(plan.argv[i + 1], "plan");
        assert!(!plan.argv.iter().any(|a| a == "--continue"));

        // Continuing (approve-and-continue) switches to acceptEdits + --continue.
        let cont = build_claude_interactive_run("/w", "/w/.mcp.json", "sys", "user", Some("sess-1"));
        let j = cont.argv.iter().position(|a| a == "--permission-mode").unwrap();
        assert_eq!(cont.argv[j + 1], "acceptEdits");
        assert!(cont.argv.iter().any(|a| a == "--continue"));
        assert_eq!(cont.continue_session_id.as_deref(), Some("sess-1"));
        assert_eq!(cont.mode, "code");
    }

    #[test]
    fn run_ids_are_unique() {
        assert_ne!(new_run_id(), new_run_id());
    }

    fn sh_req(script: &str) -> RunRequest {
        RunRequest {
            run_id: new_run_id(),
            cwd: "/tmp".into(),
            mode: "plan".into(),
            program: "sh".into(),
            argv: vec!["-c".into(), script.into()],
            env: vec![],
            mcp_config_path: None,
            system_prompt: String::new(),
            user_prompt: String::new(),
            interactive: false,
            continue_session_id: None,
        }
    }

    #[test]
    fn run_headless_captures_stdout_stderr_and_exit_code() {
        // `sh -c <script> <prompt>` — the appended prompt is $0 (ignored here).
        let req = sh_req("printf 'hello-out'; printf 'oops' 1>&2; exit 3");
        let r = run_headless(&req, 30);
        assert_eq!(r.exit_code, 3);
        assert!(r.final_text.contains("hello-out"), "stdout: {:?}", r.final_text);
        assert!(r.final_text.contains("oops"), "stderr: {:?}", r.final_text);
        assert_eq!(r.session_id, None);
    }

    #[test]
    fn run_headless_is_cancellable_by_issue() {
        let issue = format!("issue-{}", new_run_id());
        let req = sh_req("sleep 30");
        let issue_t = issue.clone();
        let handle = std::thread::spawn(move || run_headless_for_issue(&issue_t, &req, 30));
        // Let it spawn + register before cancelling.
        std::thread::sleep(Duration::from_millis(300));
        assert!(cancel_issue(&issue), "cancel_issue should find the in-flight run");
        let r = handle.join().unwrap();
        assert_ne!(r.exit_code, 0, "a killed run must not report success");
        // Mapping cleared once the run returns.
        assert!(!cancel_issue(&issue));
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
