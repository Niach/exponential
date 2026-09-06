//! EXP-746 — the engine core against a scripted ACP agent.
//!
//! `FakeAgent` is a real `ConnectTo<Client>` over the SDK's own in-process
//! channel, so these tests drive the REAL `host.rs` + `session.rs` +
//! `lifecycle.rs` — the same code path a claude/codex/pi adapter takes,
//! minus the child process. What it guards:
//!
//! - a turn's updates reach the relay sink AND the local feed;
//! - a permission becomes an answerable card keyed on the tool-call id, and
//!   answering it resolves the ACP request;
//! - **a cancel during a live turn reaches the adapter** — the regression
//!   test for the dispatch-loop deadlock (a handler that forgot to spawn
//!   hangs here and nowhere else);
//! - `set_config`/`set_mode` re-emit `config_state`, which IS the
//!   confirmation (D4);
//! - the child going away ends the run exactly once, with `exit:<code>`;
//! - an error response does not tear the connection down;
//! - `start` never touches the launch hold the host took (EXP-478).
//!
//! No network: `publish: false` with a recording sink in its place.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use agent_client_protocol::schema::v1::{
    AvailableCommand, AvailableCommandsUpdate, ContentBlock, ContentChunk, InitializeRequest,
    InitializeResponse, LoadSessionRequest, LoadSessionResponse, NewSessionRequest,
    NewSessionResponse, PermissionOption, PermissionOptionId, PermissionOptionKind, PromptRequest,
    PromptResponse, ReadTextFileRequest, RequestPermissionOutcome, RequestPermissionRequest,
    SessionConfigOption, SessionConfigOptionCategory, SessionConfigSelectOption, SessionId,
    SessionMode, SessionModeId, SessionModeState, SessionNotification, SessionUpdate,
    SetSessionConfigOptionRequest, SetSessionConfigOptionResponse, SetSessionModeRequest,
    SetSessionModeResponse, StopReason, TextContent, ToolCall, ToolCallId, ToolKind,
};
use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::{
    on_receive_notification, on_receive_request, Agent, Client, ConnectTo, Error,
};
use engine::{
    ChildExitLink, EngineExit, EngineHost, EngineParts, EngineSession, EngineStart, EventSink,
    KillFeed, LocalFeedEvent, RecordingSink,
};

const BUDGET: Duration = Duration::from_secs(10);

// ---------------------------------------------------------------------------
// The scripted agent
// ---------------------------------------------------------------------------

/// What the fake did, readable from the test thread.
#[derive(Default)]
struct FakeState {
    prompts: Mutex<Vec<String>>,
    /// The permission option the client picked.
    chosen: Mutex<Option<String>>,
    /// The cancel notification arrived (the deadlock guard).
    cancelled: AtomicBool,
    /// The client answered `fs/read_text_file` with an error.
    read_failed: AtomicBool,
    config_set: Mutex<Option<(String, String)>>,
    mode_set: Mutex<Option<String>>,
}

struct FakeAgent {
    state: Arc<FakeState>,
    child_exit: ChildExitLink,
    quit: (flume::Sender<()>, flume::Receiver<()>),
    cancel: (flume::Sender<()>, flume::Receiver<()>),
}

impl FakeAgent {
    fn new(state: Arc<FakeState>, child_exit: ChildExitLink) -> FakeAgent {
        FakeAgent {
            state,
            child_exit,
            quit: flume::unbounded(),
            cancel: flume::unbounded(),
        }
    }
}

fn modes() -> SessionModeState {
    SessionModeState::new(
        SessionModeId::new("default"),
        vec![
            SessionMode::new(SessionModeId::new("default"), "Default"),
            SessionMode::new(SessionModeId::new("plan"), "Plan"),
        ],
    )
}

fn config_options(current: &str) -> Vec<SessionConfigOption> {
    vec![SessionConfigOption::select(
        "model",
        "Model",
        current.to_string(),
        vec![
            SessionConfigSelectOption::new("opus", "Opus"),
            SessionConfigSelectOption::new("sonnet", "Sonnet"),
        ],
    )
    .category(SessionConfigOptionCategory::Model)]
}

impl ConnectTo<Client> for FakeAgent {
    fn connect_to(
        self,
        client: impl ConnectTo<Agent>,
    ) -> impl std::future::Future<Output = Result<(), Error>> + Send {
        async move {
            let FakeAgent {
                state,
                child_exit,
                quit,
                cancel,
            } = self;
            let (quit_tx, quit_rx) = quit;
            let (cancel_tx, cancel_rx) = cancel;
            let prompt_state = state.clone();
            let cancel_state = state.clone();
            let config_state = state.clone();
            let mode_state = state.clone();

            Agent
                .builder()
                .name("fake-agent")
                .on_receive_request(
                    async move |_request: InitializeRequest, responder, _cx| {
                        responder.respond(InitializeResponse::new(ProtocolVersion::V1))
                    },
                    on_receive_request!(),
                )
                .on_receive_request(
                    async move |_request: NewSessionRequest, responder, _cx| {
                        responder.respond(
                            NewSessionResponse::new(SessionId::new("fake-session"))
                                .modes(modes())
                                .config_options(config_options("opus"))
                                .meta(native_id_meta("agent-native-1")),
                        )
                    },
                    on_receive_request!(),
                )
                .on_receive_request(
                    async move |_request: LoadSessionRequest, responder, _cx| {
                        responder.respond(LoadSessionResponse::new().modes(modes()))
                    },
                    on_receive_request!(),
                )
                .on_receive_request(
                    async move |request: SetSessionConfigOptionRequest, responder, _cx| {
                        let value = request
                            .value
                            .as_value_id()
                            .map(|id| id.0.to_string())
                            .unwrap_or_default();
                        *config_state
                            .config_set
                            .lock()
                            .expect("the config slot is not poisoned") =
                            Some((request.config_id.0.to_string(), value.clone()));
                        responder.respond(SetSessionConfigOptionResponse::new(config_options(
                            &value,
                        )))
                    },
                    on_receive_request!(),
                )
                .on_receive_request(
                    async move |request: SetSessionModeRequest, responder, _cx| {
                        *mode_state.mode_set.lock().expect("the mode slot is not poisoned") =
                            Some(request.mode_id.0.to_string());
                        responder.respond(SetSessionModeResponse::new())
                    },
                    on_receive_request!(),
                )
                .on_receive_notification(
                    async move |_notification: agent_client_protocol::schema::v1::CancelNotification,
                                _cx| {
                        cancel_state.cancelled.store(true, Ordering::SeqCst);
                        let _ = cancel_tx.send(());
                        Ok(())
                    },
                    on_receive_notification!(),
                )
                .on_receive_request(
                    async move |request: PromptRequest, responder, cx| {
                        let text = prompt_text(&request);
                        prompt_state
                            .prompts
                            .lock()
                            .expect("the prompt log is not poisoned")
                            .push(text.clone());
                        let session_id = request.session_id.clone();
                        let state = prompt_state.clone();
                        let child_exit = child_exit.clone();
                        let quit_tx = quit_tx.clone();
                        let cancel_rx = cancel_rx.clone();
                        let turn_cx = cx.clone();
                        // A real adapter spawns its turn; so does this one,
                        // or `session/cancel` could never be dispatched.
                        cx.spawn(async move {
                            let stop = run_turn(
                                &turn_cx, &session_id, &text, &state, &child_exit, &quit_tx,
                                &cancel_rx,
                            )
                            .await;
                            let _ = responder.respond(PromptResponse::new(stop));
                            Ok(())
                        })?;
                        Ok(())
                    },
                    on_receive_request!(),
                )
                // The agent half stays alive until the scripted quit — the
                // client's `incoming_closed` then fires, exactly like a child
                // closing its stdout.
                .connect_with(client, async move |_cx| {
                    let _ = quit_rx.recv_async().await;
                    Ok(())
                })
                .await
                .map(|_: ()| ())
        }
    }
}

/// One scripted turn, keyed on the prompt text.
async fn run_turn(
    cx: &agent_client_protocol::ConnectionTo<Client>,
    session_id: &SessionId,
    text: &str,
    state: &Arc<FakeState>,
    child_exit: &ChildExitLink,
    quit: &flume::Sender<()>,
    cancel: &flume::Receiver<()>,
) -> StopReason {
    match text {
        "stream" => {
            let _ = cx.send_notification(SessionNotification::new(
                session_id.clone(),
                SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(
                    TextContent::new("Working on it"),
                ))),
            ));
            let _ = cx.send_notification(SessionNotification::new(
                session_id.clone(),
                SessionUpdate::ToolCall(
                    ToolCall::new(ToolCallId::new("tc-1"), "Read src/main.rs")
                        .kind(ToolKind::Read)
                        .raw_input(serde_json::json!({"file_path": "src/main.rs"})),
                ),
            ));
            let _ = cx.send_notification(SessionNotification::new(
                session_id.clone(),
                SessionUpdate::AvailableCommandsUpdate(AvailableCommandsUpdate::new(vec![
                    AvailableCommand::new("compact", "Compact the context"),
                ])),
            ));
            StopReason::EndTurn
        }
        "permission" => {
            let outcome = cx
                .send_request(RequestPermissionRequest::new(
                    session_id.clone(),
                    agent_client_protocol::schema::v1::ToolCallUpdate::new(
                        ToolCallId::new("tc-perm"),
                        agent_client_protocol::schema::v1::ToolCallUpdateFields::new()
                            .title("Write src/main.rs"),
                    ),
                    vec![
                        PermissionOption::new(
                            PermissionOptionId::new("allow"),
                            "Yes",
                            PermissionOptionKind::AllowOnce,
                        ),
                        PermissionOption::new(
                            PermissionOptionId::new("reject"),
                            "No",
                            PermissionOptionKind::RejectOnce,
                        ),
                    ],
                ))
                .block_task()
                .await;
            if let Ok(response) = outcome {
                if let RequestPermissionOutcome::Selected(selected) = response.outcome {
                    *state.chosen.lock().expect("the choice slot is not poisoned") =
                        Some(selected.option_id.0.to_string());
                }
            }
            StopReason::EndTurn
        }
        "hang" => {
            // Held until the client cancels: if the client's cancel never
            // arrives (a handler blocked the dispatch loop), this waits the
            // whole budget and the test fails on the assertion, not here.
            let _ = tokio::time::timeout(BUDGET, cancel.recv_async()).await;
            StopReason::Cancelled
        }
        "readfile" => {
            let result = cx
                .send_request(ReadTextFileRequest::new(
                    session_id.clone(),
                    PathBuf::from("/nonexistent/exp-746/never.txt"),
                ))
                .block_task()
                .await;
            state.read_failed.store(result.is_err(), Ordering::SeqCst);
            StopReason::EndTurn
        }
        "quit" => {
            child_exit.record(terminal::pty::ChildExit {
                code: 3,
                success: false,
                signal: None,
            });
            let _ = quit.send(());
            StopReason::EndTurn
        }
        _ => StopReason::EndTurn,
    }
}

fn prompt_text(request: &PromptRequest) -> String {
    request
        .prompt
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text(text) => Some(text.text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

fn native_id_meta(id: &str) -> serde_json::Map<String, serde_json::Value> {
    let mut meta = serde_json::Map::new();
    meta.insert(
        engine::NATIVE_SESSION_META_KEY.to_string(),
        serde_json::Value::String(id.to_string()),
    );
    meta
}

// ---------------------------------------------------------------------------
// The harness
// ---------------------------------------------------------------------------

#[derive(Default)]
struct TestHost {
    exits: Mutex<Vec<(String, Option<i32>)>>,
    count: AtomicUsize,
}

impl EngineHost for TestHost {
    fn on_exit(&self, exit: EngineExit) {
        self.count.fetch_add(1, Ordering::SeqCst);
        self.exits
            .lock()
            .expect("the exit log is not poisoned")
            .push((exit.outcome, exit.child.map(|child| child.code)));
    }
}

struct Harness {
    session: EngineSession,
    sink: Arc<RecordingSink>,
    state: Arc<FakeState>,
    host: Arc<TestHost>,
    /// Kept alive: dropping it would release the gate under test.
    _hold: coding::LaunchHold,
    _runtime: Arc<steer::SteerRuntime>,
}

fn prepared(session_id: &str, worktree: PathBuf) -> coding::PreparedLaunch {
    coding::PreparedLaunch {
        session_id: session_id.to_string(),
        issue_identifier: "EXP-746".to_string(),
        worktree: worktree.clone(),
        clone: worktree.clone(),
        repository_id: None,
        branch: "exp/EXP-746".to_string(),
        base_branch: None,
        base_ref: Some("origin/master".to_string()),
        run_cleanup: None,
        spawn: terminal::pty::SpawnSpec::new("claude"),
        transport: coding::LaunchTransport::Acp,
        acp: Some(coding::AcpLaunch {
            prompt: None,
            options: coding::LaunchOptions {
                agent: coding::CodingAgent::Claude,
                model: String::new(),
                effort: String::new(),
                ultracode: false,
                plan_mode: false,
                external: None,
            },
            mcp: coding::AgentMcp::ClaudeFile,
            session_id: session_id.to_string(),
            resume: None,
            reaper_settings_path: None,
        }),
        tab_title: "claude · EXP-746".to_string(),
        tab_title_prefix: "EXP-746".to_string(),
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
            agent: None,
        },
        tab_kind: terminal::tab::TabKind::Claude,
        bypass_permissions: true,
        plan_mode: false,
        agent: coding::CodingAgent::Claude,
        claude_session_id: None,
        codex_originator: None,
        codex_resume_id: None,
        launch_hold: None,
    }
}

fn start_fake(name: &str) -> Harness {
    let worktree = std::env::temp_dir().join(format!(
        "exp746-engine-{name}-{}-{}",
        std::process::id(),
        Instant::now().elapsed().as_nanos()
    ));
    std::fs::create_dir_all(&worktree).expect("the scratch worktree is creatable");
    // EXP-478: the host takes the hold BEFORE start and keeps it until the
    // session is registered — the harness stands in for that host.
    let hold = coding::launch_gate::hold(&worktree);

    let runtime = steer::SteerRuntime::new().expect("a steer runtime");
    let sink = RecordingSink::new();
    let state = Arc::new(FakeState::default());
    let host = Arc::new(TestHost::default());
    let child_exit = ChildExitLink::new();
    let adapter = FakeAgent::new(state.clone(), child_exit.clone());

    let session = engine::start_with(
        EngineStart {
            prepared: prepared(&format!("sess-{name}"), worktree.clone()),
            // Nothing in these tests talks to a server; an unroutable base
            // makes every stray call fail immediately instead of hanging.
            trpc: Arc::new(api::TrpcClient::new("http://127.0.0.1:1", Arc::new(|| None))),
            runtime: Arc::clone(&runtime),
            data_dir: worktree.clone(),
            account_id: "acct-1".to_string(),
            own_user_id: Some("user-1".to_string()),
            personal_key: Some("expu_supersecretkey".to_string()),
            issue_id: Some("issue-1".to_string()),
            foreign_host: false,
            publish: false,
            kill: KillFeed::inert(),
            local_sink: None,
        },
        host.clone(),
        EngineParts {
            adapter,
            child_exit,
            sink: Some(sink.clone() as Arc<dyn EventSink>),
        },
    )
    .expect("the engine starts");

    Harness {
        session,
        sink,
        state,
        host,
        _hold: hold,
        _runtime: runtime,
    }
}

/// Poll until `check` holds or the budget runs out.
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

fn kinds(sink: &RecordingSink) -> Vec<String> {
    sink.snapshot()
        .iter()
        .filter_map(|event| {
            serde_json::to_value(event)
                .ok()?
                .get("kind")?
                .as_str()
                .map(str::to_string)
        })
        .collect()
}

fn events_of(sink: &RecordingSink, kind: &str) -> Vec<serde_json::Value> {
    sink.snapshot()
        .iter()
        .filter_map(|event| serde_json::to_value(event).ok())
        .filter(|event| event.get("kind").and_then(|kind| kind.as_str()) == Some(kind))
        .collect()
}

// ---------------------------------------------------------------------------
// The tests
// ---------------------------------------------------------------------------

#[test]
fn a_turn_publishes_its_updates_and_feeds_the_local_screen() {
    let harness = start_fake("stream");
    let feed = harness.session.subscribe();
    harness.session.send_prompt("stream".to_string());

    until("the tool event", || kinds(&harness.sink).iter().any(|kind| kind == "tool"));
    until("the coalesced narration", || {
        kinds(&harness.sink).iter().any(|kind| kind == "narration")
    });

    let tools = events_of(&harness.sink, "tool");
    assert_eq!(tools[0]["name"], "Read src/main.rs");
    // The detail is DERIVED, never `raw_input` verbatim.
    assert_eq!(tools[0]["detail"], "src/main.rs");

    // `session/new` published the first snapshot; the commands update
    // replaced it wholesale.
    let config = events_of(&harness.sink, "config_state");
    assert!(!config.is_empty(), "the handshake publishes config_state");
    until("the agent commands", || {
        events_of(&harness.sink, "config_state")
            .last()
            .map(|state| state.get("commands").is_some())
            .unwrap_or(false)
    });

    // The local feed carried the same events plus the tool CARD.
    let mut saw_activity = false;
    let mut saw_card = false;
    while let Ok(event) = feed.recv_timeout(Duration::from_millis(100)) {
        match event {
            LocalFeedEvent::Activity { .. } => saw_activity = true,
            LocalFeedEvent::ToolCall { id, .. } => saw_card |= id == "tc-1",
            _ => {}
        }
    }
    assert!(saw_activity, "the local feed mirrors the wire");
    assert!(saw_card, "the local feed carries the tool card");

    assert_eq!(
        harness.session.acp_session_id().as_deref(),
        Some("fake-session")
    );
    assert_eq!(
        harness.session.agent_native_session_id().as_deref(),
        Some("agent-native-1")
    );
    harness.session.kill("killed");
}

#[test]
fn a_permission_is_answerable_and_resolves_the_agents_request() {
    let harness = start_fake("permission");
    harness.session.send_prompt("permission".to_string());
    until("the question card", || {
        !events_of(&harness.sink, "question").is_empty()
    });
    let question = events_of(&harness.sink, "question").remove(0);
    // D3: the card id IS the ACP tool-call id, and the option key IS the ACP
    // option id — never a keystroke.
    assert_eq!(question["id"], "tc-perm");
    assert_eq!(question["options"][0]["key"], "allow");

    harness.session.answer(steer::RemoteAnswer {
        question_id: "tc-perm".to_string(),
        ask_id: None,
        keys: vec!["allow".to_string()],
        text: None,
    });

    until("the ack", || !events_of(&harness.sink, "answer_ack").is_empty());
    until("the resolution", || {
        !events_of(&harness.sink, "question_resolved").is_empty()
    });
    until("the agent's own resolution", || {
        harness
            .state
            .chosen
            .lock()
            .expect("the choice slot is not poisoned")
            .is_some()
    });
    assert_eq!(
        harness
            .state
            .chosen
            .lock()
            .expect("the choice slot is not poisoned")
            .as_deref(),
        Some("allow")
    );
    harness.session.kill("killed");
}

/// The dispatch-loop regression test: a client handler that forgot to spawn
/// would never dispatch this cancel, and the agent would sit in "hang".
#[test]
fn a_cancel_during_a_turn_reaches_the_adapter() {
    let harness = start_fake("cancel");
    harness.session.send_prompt("hang".to_string());
    until("the turn to start", || {
        !harness
            .state
            .prompts
            .lock()
            .expect("the prompt log is not poisoned")
            .is_empty()
    });
    harness.session.cancel_turn();
    until("the adapter's cancel", || {
        harness.state.cancelled.load(Ordering::SeqCst)
    });
    harness.session.kill("killed");
}

#[test]
fn set_config_and_set_mode_re_emit_the_config_state() {
    let harness = start_fake("config");
    until("the first snapshot", || {
        !events_of(&harness.sink, "config_state").is_empty()
    });

    harness
        .session
        .set_config("model", engine::ConfigValue::ValueId("sonnet".to_string()));
    until("the re-emitted option", || {
        events_of(&harness.sink, "config_state")
            .last()
            .map(|state| state["options"][0]["value"] == "sonnet")
            .unwrap_or(false)
    });
    assert_eq!(
        harness
            .state
            .config_set
            .lock()
            .expect("the config slot is not poisoned")
            .clone(),
        Some(("model".to_string(), "sonnet".to_string()))
    );

    harness.session.set_mode("plan");
    until("the re-emitted mode", || {
        events_of(&harness.sink, "config_state")
            .last()
            .map(|state| state["currentMode"] == "plan")
            .unwrap_or(false)
    });
    assert_eq!(
        harness
            .state
            .mode_set
            .lock()
            .expect("the mode slot is not poisoned")
            .as_deref(),
        Some("plan")
    );
    harness.session.kill("killed");
}

#[test]
fn the_child_going_away_ends_the_run_exactly_once() {
    let harness = start_fake("eof");
    harness.session.send_prompt("quit".to_string());
    until("the exit", || harness.session.is_done());
    let exit = harness
        .session
        .wait_timeout(BUDGET)
        .expect("the exit is recorded");
    assert_eq!(exit.outcome, "exit:3");
    assert_eq!(exit.child.map(|child| child.code), Some(3));
    // Give any second teardown a chance to be wrong.
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(harness.host.count.load(Ordering::SeqCst), 1);
    assert_eq!(
        harness
            .host
            .exits
            .lock()
            .expect("the exit log is not poisoned")
            .first()
            .cloned(),
        Some(("exit:3".to_string(), Some(3)))
    );
}

#[test]
fn an_error_response_does_not_tear_the_connection_down() {
    let harness = start_fake("error");
    harness.session.send_prompt("readfile".to_string());
    until("the failed read", || {
        harness.state.read_failed.load(Ordering::SeqCst)
    });
    // The connection is still good: the next turn runs.
    harness.session.send_prompt("stream".to_string());
    until("the second turn", || {
        harness
            .state
            .prompts
            .lock()
            .expect("the prompt log is not poisoned")
            .len()
            == 2
    });
    until("its tool event", || {
        kinds(&harness.sink).iter().any(|kind| kind == "tool")
    });
    harness.session.kill("killed");
}

#[test]
fn starting_never_releases_the_hosts_launch_hold() {
    let harness = start_fake("hold");
    until("the handshake", || {
        harness.session.acp_session_id().is_some()
    });
    // EXP-478: the gate stays held while the harness (standing in for the
    // host) owns it — an exclusive prune must not be able to run.
    let ran = coding::launch_gate::try_exclusive(harness.session.worktree(), || ());
    assert!(ran.is_none(), "the launch hold is still live after start");
    harness.session.kill("killed");
}
