//! The REAL claude adapter (against `fake-claude.sh`) driven through the REAL
//! engine host (`engine::start_with`): what `claude_adapter.rs` cannot see,
//! because it stops at the ACP notifications — the host's own state (the
//! `agent_busy` mirror, FEED-44) and the rows the ONE mapper publishes for a
//! host-sent prompt (EXP-1098).
//!
//! No network: `publish: false` with a recording sink, and an unroutable
//! tRPC base, so every server call (an attachment download included) fails at
//! once.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use engine::adapters::claude::ClaudeAgent;
use engine::adapters::{AdapterKind, AdapterSpec};
use engine::{
    ChildExitLink, EngineExit, EngineHost, EngineParts, EngineSession, EngineStart, EventSink,
    KillFeed, LocalFeedEvent, RecordingSink,
};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/claude");
const BUDGET: Duration = Duration::from_secs(15);

/// Every claude session attaches to the process-global live usage registry;
/// one at a time, as in `claude_adapter.rs`.
static SESSION_LOCK: Mutex<()> = Mutex::new(());

fn one_session_at_a_time() -> std::sync::MutexGuard<'static, ()> {
    match SESSION_LOCK.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

#[derive(Default)]
struct TestHost;

impl EngineHost for TestHost {
    fn on_exit(&self, _exit: EngineExit) {}
}

struct Harness {
    session: EngineSession,
    sink: Arc<RecordingSink>,
    feed: flume::Receiver<LocalFeedEvent>,
    work: PathBuf,
    _hold: coding::LaunchHold,
    _runtime: Arc<steer::SteerRuntime>,
}

impl Drop for Harness {
    fn drop(&mut self) {
        self.session.kill("killed");
        let _ = self.session.wait_timeout(Duration::from_secs(5));
        let _ = std::fs::remove_dir_all(&self.work);
    }
}

/// Make sure the fake is executable even in a checkout that lost the bit.
fn fake_claude() -> PathBuf {
    use std::os::unix::fs::PermissionsExt as _;
    let path = Path::new(FIXTURES).join("fake-claude.sh");
    if let Ok(metadata) = std::fs::metadata(&path) {
        let mut permissions = metadata.permissions();
        if permissions.mode() & 0o111 == 0 {
            permissions.set_mode(0o755);
            let _ = std::fs::set_permissions(&path, permissions);
        }
    }
    path
}

fn options() -> coding::LaunchOptions {
    coding::LaunchOptions {
        workflow: None,
        agent: coding::CodingAgent::Claude,
        model: "opus".to_string(),
        effort: String::new(),
        ultracode: false,
        plan_mode: false,
        subagent_model: String::new(),
        mcp_server_ids: Vec::new(),
        account: None,
    }
}

fn adapter_spec(scenario: &str, work: &Path, session_id: &str, exit: ChildExitLink) -> AdapterSpec {
    let spawn = terminal::pty::SpawnSpec::new(fake_claude().display().to_string())
        .cwd(work)
        .env("CLAUDE_CONFIG_DIR", work.join("claude-config").display().to_string())
        .env(
            "EXP_FAKE_CLAUDE_DIR",
            Path::new(FIXTURES).join(scenario).display().to_string(),
        )
        .env("EXP_FAKE_CLAUDE_STDIN", work.join("stdin.jsonl").display().to_string())
        .env(coding::MCP_TOKEN_ENV, "expu_test-key");
    AdapterSpec {
        kind: AdapterKind::Claude,
        agent: coding::CodingAgent::Claude,
        spawn,
        options: options(),
        mcp: coding::AgentMcp::ClaudeInline {
            url: "https://app.example/api/mcp".to_string(),
            session_id: Some(session_id.to_string()),
        },
        servers: Vec::new(),
        cwd: work.to_path_buf(),
        session_id: session_id.to_string(),
        // The ENGINE sends the seed (`PreparedLaunch::acp.prompt`), exactly
        // as a real launch does.
        prompt: None,
        resume: None,
        replay: false,
        personal_key: Some("expu_test-key".to_string()),
        reaper_settings_path: Some(work.join("claude-hooks/1/row.settings.json")),
        system_append: coding::skill::system_append(None),
        context_layers: coding::ContextLayers::default(),
        exit,
    }
}

fn prepared(session_id: &str, worktree: &Path, seed: Option<String>) -> coding::PreparedLaunch {
    coding::PreparedLaunch {
        session_id: session_id.to_string(),
        issue_identifier: "EXP-12".to_string(),
        worktree: worktree.to_path_buf(),
        clone: worktree.to_path_buf(),
        repository_id: None,
        account_pick: None,
        workflow: None,
        branch: "exp/EXP-12".to_string(),
        base_branch: None,
        base_ref: None,
        run_cleanup: None,
        spawn: terminal::pty::SpawnSpec::new("claude"),
        acp: coding::AcpLaunch {
            prompt: seed,
            options: options(),
            mcp: coding::AgentMcp::ClaudeFile,
            session_id: session_id.to_string(),
            resume: None,
            reaper_settings_path: None,
            system_append: coding::skill::system_append(None),
            context_layers: coding::ContextLayers::default(),
            servers: Vec::new(),
            mcp_secrets: Default::default(),
        },
        tab_title: "claude · EXP-12".to_string(),
        tab_title_prefix: "EXP-12".to_string(),
        heartbeat_scope: api::coding_sessions::HeartbeatScope {
            issue_id: None,
            team_id: None,
            action_id: None,
            action_name: None,
            started_by_id: None,
            device_id: None,
            started_reason: None,
            automation_id: None,
            branch: None,
            batch_issue_ids: Vec::new(),
            agent: None,
            agent_account: None,
            workflow_id: None,
            workflow_node_id: None,
            workflow_role: None,
        },
        action_id: None,
        bypass_permissions: true,
        plan_mode: false,
        agent: coding::CodingAgent::Claude,
        claude_session_id: None,
        codex_originator: None,
        codex_resume_id: None,
        launch_hold: None,
    }
}

fn start(scenario: &str, seed: Option<String>) -> Harness {
    let work = std::env::temp_dir().join(format!(
        "exp-claude-engine-{scenario}-{}",
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::create_dir_all(&work).expect("a scratch worktree");
    let hold = coding::launch_gate::hold(&work);
    let session_id = format!("sess-{scenario}");
    let runtime = steer::SteerRuntime::new().expect("a steer runtime");
    let sink = RecordingSink::new();
    let child_exit = ChildExitLink::new();
    let adapter = ClaudeAgent::new(adapter_spec(scenario, &work, &session_id, child_exit.clone()))
        .expect("the adapter builds");
    let session = engine::start_with(
        EngineStart {
            prepared: prepared(&session_id, &work, seed),
            trpc: Arc::new(api::TrpcClient::new("http://127.0.0.1:1", Arc::new(|| None))),
            runtime: Arc::clone(&runtime),
            data_dir: work.clone(),
            account_id: "acct-1".to_string(),
            own_user_id: Some("user-1".to_string()),
            personal_key: Some("expu_test-key".to_string()),
            issue_id: Some("issue-1".to_string()),
            foreign_host: false,
            rotation_host: false,
            publish: false,
            kill: KillFeed::inert(),
            local_sink: None,
        },
        Arc::new(TestHost),
        EngineParts {
            adapter,
            child_exit,
            sink: Some(sink.clone() as Arc<dyn EventSink>),
        },
    )
    .expect("the engine starts");
    // Subscribed before the handshake finishes, so the seed's rows are seen.
    let feed = session.subscribe();
    Harness {
        session,
        sink,
        feed,
        work,
        _hold: hold,
        _runtime: runtime,
    }
}

fn until(what: &str, mut check: impl FnMut() -> bool) {
    let deadline = Instant::now() + BUDGET;
    while Instant::now() < deadline {
        if check() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting for {what}");
}

fn events_of(sink: &RecordingSink, kind: &str) -> Vec<serde_json::Value> {
    sink.snapshot()
        .iter()
        .filter_map(|event| serde_json::to_value(event).ok())
        .filter(|event| event.get("kind").and_then(|kind| kind.as_str()) == Some(kind))
        .collect()
}

/// How many turns have ENDED after starting (the seeded `ended` does not
/// count).
fn ended_turns(sink: &RecordingSink) -> usize {
    let states: Vec<String> = events_of(sink, "turn")
        .iter()
        .map(|event| event["state"].as_str().unwrap_or_default().to_string())
        .collect();
    let mut open = false;
    let mut ended = 0;
    for state in states {
        match state.as_str() {
            "started" => open = true,
            "ended" if open => {
                open = false;
                ended += 1;
            }
            _ => {}
        }
    }
    ended
}

fn has_agent_task(list: &serde_json::Value) -> bool {
    list["tasks"]
        .as_array()
        .is_some_and(|tasks| tasks.iter().any(|task| task["kind"] == "agent"))
}

/// FEED-44: the agent launched a subagent with `run_in_background` and ENDED
/// its own turn to wait for the completion notification. The turn is over
/// (the queue may drain, an account switch may run) but the run is still
/// working: `agent_busy` — the synced column every list spins on — holds
/// until the CLI's background-task list stops naming an agent.
///
/// The fixture is synthesized from the `basic` capture plus the
/// `background_tasks_changed` frame recorded in `duplicate-agent`
/// (`task_type: local_agent`). The `task_started` frame is left out on
/// purpose: a LIVE task of the current turn defers the turn's settle for up
/// to `TASK_MAX_LIFETIME` (10 min), and the state this test pins — turn
/// settled, agent still listed — is exactly where that deferral ends.
#[test]
fn background_subagents_keep_the_run_busy_past_the_turn_end() {
    let _session = one_session_at_a_time();
    let harness = start("background-agent", None);
    until("the seeded turn slot", || !events_of(&harness.sink, "turn").is_empty());
    assert!(!harness.session.agent_busy(), "a fresh session is idle");

    harness
        .session
        .send_prompt("Launch the slowpoke agent in the background and wait for it.".to_string());
    until("the first turn to end", || ended_turns(&harness.sink) >= 1);
    until("the agent-kind background list", || {
        events_of(&harness.sink, "background_tasks").iter().any(has_agent_task)
    });
    assert!(harness.session.turn_signal().is_idle(), "the turn itself ended");
    // Held, not a race with the edge: still busy a few ticker periods on.
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        harness.session.agent_busy(),
        "a live background subagent keeps the run busy after its turn ended"
    );

    // The next list no longer names the agent: the run goes idle with it.
    harness.session.send_prompt("Is the slowpoke agent done?".to_string());
    until("the second turn to end", || ended_turns(&harness.sink) >= 2);
    let lists = events_of(&harness.sink, "background_tasks");
    assert!(
        lists.last().is_some_and(|list| !has_agent_task(list)),
        "the strip closed: {lists:?}"
    );
    until("the run to read idle", || !harness.session.agent_busy());
}

/// EXP-1098: a seed prompt with an image — the host announces the person's
/// text (`![image](/api/attachments/…)`, the issue ref, the mention) and the
/// agent receives the LOCALIZED text (`Image #1: …` manifest). Claude's
/// `--replay-user-messages` echo carries the localized spelling, and it must
/// retire the announced row's echo instead of landing as a SECOND row —
/// on the wire AND in the local feed.
#[test]
fn an_image_seed_publishes_exactly_one_user_row() {
    let _session = one_session_at_a_time();
    let attachment = "0f1e2d3c-4b5a-4968-8776-655443322110";
    let seed = format!(
        "Fix the header like #EXP-12 says, @dennis@example.com has the details:\n![image](/api/attachments/{attachment})"
    );
    let harness = start("image-seed", Some(seed.clone()));
    until("the seed turn to end", || ended_turns(&harness.sink) >= 1);
    // The fake received the localized text (the download failed, so the
    // manifest line carries the URL — deterministic, and still a spelling
    // that differs from the announce).
    let stdin = std::fs::read_to_string(harness.work.join("stdin.jsonl")).unwrap_or_default();
    assert!(stdin.contains("Image #1: "), "the agent got the manifest: {stdin}");
    // Let the flush tick publish anything the replay coalesced.
    std::thread::sleep(Duration::from_millis(600));

    let rows = events_of(&harness.sink, "user_message");
    assert_eq!(rows.len(), 1, "exactly ONE initial message on the wire: {rows:?}");
    assert_eq!(rows[0]["text"], serde_json::json!(seed));

    let mut local = Vec::new();
    while let Ok(event) = harness.feed.try_recv() {
        if let LocalFeedEvent::Activity { event, .. } = event {
            let value = serde_json::to_value(&event).expect("an activity event serializes");
            if value["kind"] == "user_message" {
                local.push(value);
            }
        }
    }
    assert_eq!(local.len(), 1, "exactly ONE initial message in the local feed: {local:?}");
    assert_eq!(local[0]["text"], serde_json::json!(seed));
}
