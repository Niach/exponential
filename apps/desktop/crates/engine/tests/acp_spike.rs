//! EXP-746 — the phase-1 SPIKE test (the gate for every other lane).
//!
//! Owned by lane S1. Offline checks (argv vectors, the codex line classifier,
//! pi's discriminator) always run; the LIVE checklist is gated behind
//! `EXP_ACP_SPIKE=1` and self-skips whenever the CLI under test is absent, so
//! a plain `cargo test` stays green on a machine with no agent installed.
//!
//! The live checks are the automatable core of the checklist — a handshake and
//! one turn per agent. The judgement calls (plan approval, AskUserQuestion,
//! MCP headers, subagent attribution, interrupt fences) live in
//! `examples/exp746_acp_spike.rs`, because they need a human reading the
//! per-frame table, not an assertion.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use engine::adapters::claude_wire::{
    self, ClaudeArgs, ClaudeOut, ClaudeProcess, McpConfig, TurnEnd,
};
use engine::adapters::codex_wire::{self, classify_line, AppServer, CodexMode, Incoming};
use engine::adapters::pi_wire::{self, parse_line, PiArgs, PiOut, PiProcess};
use serde_json::json;
use terminal::pty::SpawnSpec;

/// The live half runs only when explicitly asked for.
fn live() -> bool {
    std::env::var("EXP_ACP_SPIKE").is_ok_and(|value| value == "1")
}

/// Resolve a CLI the way a launch would — through the login shell's PATH, not
/// the (often minimal) PATH a test harness inherits.
fn cli(name: &str) -> Option<PathBuf> {
    let path = terminal::pty::login_path();
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("exp746-spike-test").join(name);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

// ---------------------------------------------------------------------------
// Offline — always run
// ---------------------------------------------------------------------------

#[test]
fn claude_argv_matches_the_sdk_flag_for_flag() {
    let mcp = PathBuf::from("/work/tree/.exp-mcp.json");
    let argv = claude_wire::claude_argv(&ClaudeArgs {
        print_mode: claude_wire::CLAUDE_PRINT_MODE,
        model: Some("claude-opus-4-6"),
        permission_mode: Some("manual"),
        mcp_config: Some(McpConfig::File(&mcp)),
        strict_mcp_config: true,
        session_id: Some("sid"),
        ..ClaudeArgs::default()
    });
    // The SDK's base array, in its order, before anything of ours.
    let base = argv
        .iter()
        .position(|arg| arg == "--output-format")
        .expect("the base array leads");
    assert_eq!(
        argv[base..base + 5],
        [
            "--output-format".to_string(),
            "stream-json".to_string(),
            "--verbose".to_string(),
            "--input-format".to_string(),
            "stream-json".to_string(),
        ]
    );
    // Without this flag the CLI never asks and the whole permission surface
    // silently dies — the single most load-bearing argument on the list.
    assert!(argv.windows(2).any(|pair| pair == ["--permission-prompt-tool", "stdio"]));
    assert!(argv.contains(&"--session-id=sid".to_string()));
    assert!(argv.contains(&"--strict-mcp-config".to_string()));
}

#[test]
fn codex_classify_line_routes_all_three_shapes() {
    assert!(matches!(
        classify_line(r#"{"id":1,"result":{}}"#),
        Incoming::Response { id: 1, .. }
    ));
    assert!(matches!(
        classify_line(r#"{"id":"a","method":"item/permissions/requestApproval","params":{}}"#),
        Incoming::ServerRequest { .. }
    ));
    assert!(matches!(
        classify_line(r#"{"method":"turn/started","params":{}}"#),
        Incoming::Notification { .. }
    ));
    assert!(matches!(classify_line("codex: starting"), Incoming::Junk));
}

#[test]
fn pi_parse_line_ignores_an_unknown_type() {
    assert!(matches!(
        parse_line(r#"{"type":"some_event_pi_added_last_week","payload":1}"#),
        PiOut::Event { .. }
    ));
    assert!(matches!(parse_line("{"), PiOut::Unknown));
}

// ---------------------------------------------------------------------------
// Live — EXP_ACP_SPIKE=1, self-skipping
// ---------------------------------------------------------------------------

/// Checkpoints 1, 2, 11: the subscription-authenticated CLI completes a turn
/// in stream-json mode and stdin survives it.
#[test]
fn claude_stream_json_handshake() {
    if !live() || cli("claude").is_none() {
        return;
    }
    let spec = SpawnSpec::new("claude")
        .args(claude_wire::claude_argv(&ClaudeArgs::default()))
        .cwd(scratch("claude"));
    let process = ClaudeProcess::spawn(&spec).expect("claude spawns");
    process.send_user("Reply with the single word ok.", None).expect("stdin");
    let mut saw_init = false;
    let mut saw_assistant = false;
    let end = process.pump_turn(Instant::now() + Duration::from_secs(180), |frame, _raw| {
        match frame {
            ClaudeOut::System(system) if system.subtype == "init" => saw_init = true,
            ClaudeOut::Assistant(_) => saw_assistant = true,
            _ => {}
        }
        Vec::new()
    });
    assert_eq!(end, TurnEnd::Result, "the turn completed");
    assert!(saw_init, "system/init arrived");
    assert!(saw_assistant, "the model answered");
}

/// Checkpoint 12: the app-server handshake plus one full-access turn.
#[test]
fn codex_app_server_thread_start_and_turn() {
    if !live() || cli("codex").is_none() {
        return;
    }
    let cwd = scratch("codex");
    let spec = SpawnSpec::new("codex").args(["app-server", "--listen", "stdio://"]).cwd(cwd.clone());
    let (server, notifications, _requests, _exit, _pid) = AppServer::spawn(&spec).expect("codex spawns");
    server
        .request_blocking(
            "initialize",
            codex_wire::initialize_params("spike", codex_wire::OPT_OUT_NOTIFICATIONS),
            Duration::from_secs(60),
        )
        .expect("initialize");
    server.notify("initialized", json!({})).expect("initialized");
    let thread = server
        .request_blocking(
            "thread/start",
            codex_wire::thread_start_params(
                &cwd,
                codex_wire::thread_config(None, "spike", std::slice::from_ref(&cwd)),
                None,
            ),
            Duration::from_secs(60),
        )
        .expect("thread/start");
    let thread_id = thread["thread"]["id"].as_str().expect("a thread id").to_string();
    server
        .request_blocking(
            "turn/start",
            codex_wire::turn_start_params(
                &thread_id,
                "Reply with the single word ok.",
                None,
                None,
                CodexMode::AgentFullAccess,
                std::slice::from_ref(&cwd),
            ),
            Duration::from_secs(120),
        )
        .expect("turn/start");
    let deadline = Instant::now() + Duration::from_secs(180);
    let mut completed = false;
    while Instant::now() < deadline && !completed {
        let Ok((method, _params)) = notifications.recv_timeout(Duration::from_secs(10)) else {
            continue;
        };
        completed = method == "turn/completed" || method == "turn/failed";
    }
    assert!(completed, "the turn reached a completion notification");
}

/// Checkpoint 13: the rpc handshake and the idle edge.
#[test]
fn pi_rpc_prompt_and_settled() {
    if !live() || cli("pi").is_none() {
        return;
    }
    let spec = SpawnSpec::new("pi").args(pi_wire::pi_argv(&PiArgs::default())).cwd(scratch("pi"));
    let process = PiProcess::spawn(&spec).expect("pi spawns");
    process.send(&pi_wire::command("1", "get_state", json!({}))).expect("stdin");
    let answered = process.pump(
        Instant::now() + Duration::from_secs(60),
        |_frame, _raw| Vec::new(),
        |frame| matches!(frame, PiOut::Response { command, .. } if command == "get_state"),
    );
    assert!(answered, "get_state answered");
    process
        .send(&pi_wire::command("2", "prompt", json!({ "message": "Reply with the single word ok." })))
        .expect("stdin");
    let settled = process.pump(
        Instant::now() + Duration::from_secs(180),
        |_frame, _raw| Vec::new(),
        // `agent_settled` is the true idle edge; `agent_end` can precede a
        // retry, so it is NOT the one to wait on.
        |frame| matches!(frame, PiOut::Event { kind, .. } if kind == "agent_settled"),
    );
    assert!(settled, "the turn settled");
}
