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
//! - EXP-750: a `terminal/*` round trip streams into the LOCAL feed and
//!   settles its exit, and a Stop kills the child;
//! - `start` never touches the launch hold the host took (EXP-478).
//!
//! No network: `publish: false` with a recording sink in its place.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use agent_client_protocol::schema::v1::{
    AvailableCommand, AvailableCommandsUpdate, ContentBlock, ContentChunk,
    CreateElicitationRequest, CreateTerminalRequest, ElicitationAction, ElicitationContentValue,
    ElicitationFormMode, ElicitationPropertySchema, ElicitationSchema, ElicitationSessionScope,
    InitializeRequest, InitializeResponse, LoadSessionRequest, LoadSessionResponse,
    NewSessionRequest, NewSessionResponse, PermissionOption, PermissionOptionId,
    PermissionOptionKind, PromptRequest, PromptResponse, ReadTextFileRequest,
    ReleaseTerminalRequest, RequestPermissionOutcome, RequestPermissionRequest,
    SessionConfigOption, SessionConfigOptionCategory, SessionConfigSelectOption, SessionId,
    SessionMode, SessionModeId, SessionModeState, SessionNotification, SessionUpdate,
    SetSessionConfigOptionRequest, SetSessionConfigOptionResponse, SetSessionModeRequest,
    SetSessionModeResponse, StopReason, StringPropertySchema, Terminal, TerminalOutputRequest,
    TextContent, ToolCall, ToolCallContent, ToolCallId, ToolKind, WaitForTerminalExitRequest,
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
    /// The prompts the fake has ANSWERED, in the order it answered them.
    answered: Mutex<Vec<String>>,
    /// Lets the test end the turn that waits on it.
    released: AtomicBool,
    /// The permission option the client picked.
    chosen: Mutex<Option<String>>,
    /// The form field the client sent back for the elicitation.
    elicited: Mutex<Option<String>>,
    /// The cancel notification arrived (the deadlock guard).
    cancelled: AtomicBool,
    /// The client answered `fs/read_text_file` with an error.
    read_failed: AtomicBool,
    config_set: Mutex<Option<(String, String)>>,
    mode_set: Mutex<Option<String>>,
    /// EXP-750: what the client answered the `terminal/*` round trip with.
    terminal_id: Mutex<Option<String>>,
    terminal_output: Mutex<Option<String>>,
    /// The exit code `terminal/wait_for_exit` reported (`-1` = a signal
    /// death, which is what a Stop looks like from here).
    terminal_exit: Mutex<Option<i32>>,
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
                            state
                                .answered
                                .lock()
                                .expect("the answer log is not poisoned")
                                .push(text.clone());
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
        // The ask an agent raises mid-tool: session-scoped, carrying the id
        // of the tool call it interrupts.
        "ask" => {
            let response = cx
                .send_request(CreateElicitationRequest::new(
                    ElicitationFormMode::new(
                        ElicitationSessionScope::new(session_id.clone())
                            .tool_call_id(ToolCallId::new("tc-ask")),
                        ElicitationSchema::new().property(
                            "approach",
                            ElicitationPropertySchema::String(
                                StringPropertySchema::new().title("Which approach?").enum_values(
                                    vec!["rewrite".to_string(), "patch".to_string()],
                                ),
                            ),
                            false,
                        ),
                    ),
                    "The agent has a question",
                ))
                .block_task()
                .await;
            if let Ok(response) = response {
                if let ElicitationAction::Accept(accept) = response.action {
                    *state.elicited.lock().expect("the form slot is not poisoned") = accept
                        .content
                        .and_then(|content| match content.get("approach") {
                            Some(ElicitationContentValue::String(text)) => Some(text.clone()),
                            _ => None,
                        });
                }
            }
            StopReason::EndTurn
        }
        // The mid-turn steer pair: this turn ends the moment the steered
        // prompt reaches the agent, so the follow-up is genuinely in flight
        // when the first turn answers.
        "await-steer" => {
            let deadline = Instant::now() + BUDGET;
            while Instant::now() < deadline
                && state.prompts.lock().expect("the prompt log is not poisoned").len() < 2
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            StopReason::EndTurn
        }
        // The steered turn: held until the test releases it.
        "steered" => {
            let deadline = Instant::now() + BUDGET;
            while Instant::now() < deadline && !state.released.load(Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_millis(10)).await;
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
        // EXP-750 — the terminal round trip: create, publish the tool call
        // that EMBEDS the terminal (which is what binds it to a card), wait
        // for the exit, read the retained output, release.
        "terminal" | "terminal-sleep" => {
            let command = if text == "terminal" {
                "printf a; printf b; exit 3"
            } else {
                "sleep 30"
            };
            let created = cx
                .send_request(
                    CreateTerminalRequest::new(session_id.clone(), "sh")
                        .args(vec!["-c".to_string(), command.to_string()])
                        .output_byte_limit(64u64 * 1024),
                )
                .block_task()
                .await;
            let Ok(created) = created else {
                return StopReason::EndTurn;
            };
            let terminal_id = created.terminal_id.clone();
            *state.terminal_id.lock().expect("the terminal slot is not poisoned") =
                Some(terminal_id.0.to_string());
            let _ = cx.send_notification(SessionNotification::new(
                session_id.clone(),
                SessionUpdate::ToolCall(
                    ToolCall::new(ToolCallId::new("tc-term"), "Run the tests")
                        .kind(ToolKind::Execute)
                        .content(vec![ToolCallContent::Terminal(Terminal::new(
                            terminal_id.clone(),
                        ))]),
                ),
            ));
            if let Ok(exit) = cx
                .send_request(WaitForTerminalExitRequest::new(
                    session_id.clone(),
                    terminal_id.clone(),
                ))
                .block_task()
                .await
            {
                *state.terminal_exit.lock().expect("the exit slot is not poisoned") =
                    Some(exit.exit_status.exit_code.map_or(-1, |code| code as i32));
            }
            if let Ok(output) = cx
                .send_request(TerminalOutputRequest::new(
                    session_id.clone(),
                    terminal_id.clone(),
                ))
                .block_task()
                .await
            {
                *state.terminal_output.lock().expect("the output slot is not poisoned") =
                    Some(output.output);
            }
            let _ = cx
                .send_request(ReleaseTerminalRequest::new(session_id.clone(), terminal_id))
                .block_task()
                .await;
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
        acp: coding::AcpLaunch {
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
        },
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

fn start_fake(name: &str) -> Harness {
    start_fake_with(name, |_| {})
}

/// `start_fake`, with a hook that shapes the scratch worktree before the
/// engine ever looks at it — a real git repo, the launcher's secrets on disk.
fn start_fake_with(name: &str, setup: impl FnOnce(&Path)) -> Harness {
    let worktree = std::env::temp_dir().join(format!(
        "exp746-engine-{name}-{}-{}",
        std::process::id(),
        Instant::now().elapsed().as_nanos()
    ));
    std::fs::create_dir_all(&worktree).expect("the scratch worktree is creatable");
    setup(&worktree);
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
    // The wire name is the bare verb (the PTY vocabulary), the path rides
    // `detail` — which is DERIVED, never `raw_input` verbatim.
    assert_eq!(tools[0]["name"], "Read");
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

/// EXP-766: a run that ENDS with a card still open dismisses it, exactly like
/// a cancel does. Without it the last thing on the feed is a question nobody
/// can answer any more, and `needs_input` stays true on the ended row.
#[test]
fn a_run_that_ends_with_an_open_card_dismisses_it() {
    let harness = start_fake("open-card");
    harness.session.send_prompt("permission".to_string());
    until("the question card", || {
        !events_of(&harness.sink, "question").is_empty()
    });
    assert!(events_of(&harness.sink, "question_resolved").is_empty());

    harness.session.kill("killed");
    until("the dismissal", || {
        events_of(&harness.sink, "question_resolved")
            .iter()
            .any(|event| event["dismissed"] == serde_json::json!(true))
    });
}

/// D3: an `elicitation/create` card is named by the tool call its SESSION
/// scope carries — `<toolCallId>#<n>` steps, `#submit` at the end — never a
/// synthetic hash, so the ask stays correlatable with the tool event it
/// interrupts.
#[test]
fn an_elicitation_is_a_stepper_keyed_on_its_tool_call() {
    let harness = start_fake("elicitation");
    harness.session.send_prompt("ask".to_string());
    until("the question card", || {
        !events_of(&harness.sink, "question").is_empty()
    });
    let question = events_of(&harness.sink, "question").remove(0);
    assert_eq!(question["id"], "tc-ask#0");
    assert_eq!(question["askId"], "tc-ask");
    assert_eq!(question["options"][0]["key"], "rewrite");

    harness.session.answer(steer::RemoteAnswer {
        question_id: "tc-ask#0".to_string(),
        ask_id: Some("tc-ask".to_string()),
        keys: vec!["patch".to_string()],
        text: None,
    });

    // A lone-step form submits on its answer: the agent gets the field back.
    until("the agent's accepted form", || {
        harness
            .state
            .elicited
            .lock()
            .expect("the form slot is not poisoned")
            .is_some()
    });
    assert_eq!(
        harness
            .state
            .elicited
            .lock()
            .expect("the form slot is not poisoned")
            .as_deref(),
        Some("patch")
    );
    harness.session.kill("killed");
}

/// EXP-746: a viewer steering mid-turn sends a second `session/prompt`. It is
/// a TURN like any other here, so the run stays busy until the follow-up
/// answers — an idle edge at the first turn's answer would tell every client
/// the agent is between turns and would let an `AfterTurn` kill (EXP-637)
/// SIGKILL the agent in the middle of the steered answer.
#[test]
fn a_mid_turn_steer_keeps_the_run_busy_until_the_follow_up_answers() {
    let harness = start_fake("steer-turn");
    let signal = harness.session.turn_signal();
    let prompts = || harness.state.prompts.lock().expect("the prompt log is not poisoned").len();
    let answered = |text: &str| {
        harness
            .state
            .answered
            .lock()
            .expect("the answer log is not poisoned")
            .iter()
            .any(|prompt| prompt == text)
    };

    harness.session.send_prompt("await-steer".to_string());
    until("the first turn", || prompts() >= 1);
    harness.session.steer("steered".to_string());
    until("the first turn's answer", || answered("await-steer"));

    // The steered turn is still running: the signal must not have flipped,
    // and must stay put while the follow-up works.
    let deadline = Instant::now() + Duration::from_millis(400);
    while Instant::now() < deadline {
        assert!(!signal.is_idle(), "the steered turn is still running");
        std::thread::sleep(Duration::from_millis(20));
    }

    harness.state.released.store(true, Ordering::SeqCst);
    until("the idle edge", || signal.is_idle());
    assert!(answered("steered"));
    harness.session.kill("killed");
}

/// EXP-784: `TURN_SLOTS` prompts may be in flight; the next one is accepted
/// (its row and the idle edge publish at once) but its `session/prompt` is
/// only SENT once a slot frees — and the command loop never waits for it.
#[test]
fn a_third_prompt_waits_for_a_turn_slot() {
    assert_eq!(engine::host::TURN_SLOTS, 2, "the test below assumes two slots");
    let harness = start_fake("turn-slots");
    let signal = harness.session.turn_signal();
    let prompts = || harness.state.prompts.lock().expect("the prompt log is not poisoned").len();

    // Two turns the fake holds until released, then a third that would end
    // immediately if it reached the agent.
    harness.session.send_prompt("steered".to_string());
    harness.session.steer("steered".to_string());
    until("two turns in flight", || prompts() >= 2);
    harness.session.steer("stream".to_string());
    let deadline = Instant::now() + Duration::from_millis(400);
    while Instant::now() < deadline {
        assert_eq!(prompts(), 2, "the third prompt waits for a slot");
        assert!(!signal.is_idle());
        std::thread::sleep(Duration::from_millis(20));
    }
    // The loop stayed responsive: the user's row for the queued prompt is
    // already on the wire.
    let users = events_of(&harness.sink, "user_message");
    assert!(
        users.iter().any(|event| event["text"] == "stream"),
        "the queued prompt's own row is published at once: {users:?}"
    );

    harness.state.released.store(true, Ordering::SeqCst);
    until("the third turn reaches the agent", || prompts() >= 3);
    until("the idle edge", || signal.is_idle());
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

    // EXP-772: `set_config` still reaches the agent (the wire frame stays
    // accepted for an older publisher), but the snapshot carries NO options —
    // option chips left the steering UI on every client.
    harness
        .session
        .set_config("model", engine::ConfigValue::ValueId("sonnet".to_string()));
    until("the agent to take the config option", || {
        harness
            .state
            .config_set
            .lock()
            .expect("the config slot is not poisoned")
            .is_some()
    });
    assert!(events_of(&harness.sink, "config_state")
        .iter()
        .all(|state| state["options"].as_array().is_some_and(|options| options.is_empty())));
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

/// EXP-750: the whole `terminal/*` round trip through the REAL host. The
/// terminal is created BEFORE the tool call that embeds it exists, so the
/// binding edge is what makes its output renderable — and everything the
/// command wrote before it lands on the card anyway.
#[cfg(unix)]
#[test]
fn a_terminal_round_trip_streams_into_the_local_feed_and_settles_the_exit() {
    let harness = start_fake("terminal");
    let feed = harness.session.subscribe();
    harness.session.send_prompt("terminal".to_string());

    until("the agent's terminal exit", || {
        harness
            .state
            .terminal_exit
            .lock()
            .expect("the exit slot is not poisoned")
            .is_some()
    });
    // The output request is a SECOND round trip that trails the exit one, so
    // it needs its own wait: asserting the slot straight off the exit races
    // the agent on a loaded machine.
    until("the agent's terminal output", || {
        harness
            .state
            .terminal_output
            .lock()
            .expect("the output slot is not poisoned")
            .is_some()
    });
    // The agent read back exactly what the command wrote, and its exit.
    assert_eq!(
        harness
            .state
            .terminal_output
            .lock()
            .expect("the output slot is not poisoned")
            .as_deref(),
        Some("ab")
    );
    assert_eq!(
        *harness
            .state
            .terminal_exit
            .lock()
            .expect("the exit slot is not poisoned"),
        Some(3)
    );

    // The local feed: the bind edge and the output chunks of that call, the
    // last one carrying the exit code that closes the card.
    //
    // The ORDER of the first row is a race with the command, not a contract:
    // `Terminals::bind` flushes whatever the child already wrote as ONE row
    // BEFORE `dispatch` emits the bind edge (`session_extras` reads the pair
    // in exactly that order), while a child that writes after the tool call
    // was published streams its rows behind the edge. So what holds either
    // way — and what this asserts — is that AT MOST ONE row precedes the
    // bind: the flush. A second one would mean chunks forwarded under an id
    // no renderer knows.
    let mut bound: Option<String> = None;
    let mut chunks = String::new();
    let mut closed = false;
    let mut before_bind = 0usize;
    let deadline = Instant::now() + BUDGET;
    while Instant::now() < deadline && !(closed && bound.is_some()) {
        match feed.recv_timeout(Duration::from_millis(100)) {
            Ok(LocalFeedEvent::TerminalBound {
                tool_call_id,
                terminal_id,
            }) => {
                assert_eq!(tool_call_id, "tc-term");
                bound = Some(terminal_id);
            }
            Ok(LocalFeedEvent::Output {
                tool_call_id,
                chunk,
                exit_code,
            }) => {
                assert_eq!(tool_call_id, "tc-term");
                if bound.is_none() {
                    before_bind += 1;
                    assert_eq!(
                        before_bind, 1,
                        "only the bind flush may precede the bind edge"
                    );
                }
                chunks.push_str(&chunk);
                closed |= exit_code == Some(3);
            }
            Ok(_) => {}
            Err(_) => {}
        }
    }
    assert!(bound.is_some(), "the terminal is bound to its tool call");
    assert_eq!(
        bound.as_deref(),
        harness
            .state
            .terminal_id
            .lock()
            .expect("the terminal slot is not poisoned")
            .as_deref()
    );
    assert_eq!(chunks, "ab");
    assert!(closed, "the exit code closes the card");

    // Nothing of the command reached the relay (rule 1).
    let published = serde_json::to_string(&harness.sink.snapshot())
        .expect("the published events serialize");
    assert!(
        !published.contains("printf"),
        "no command may reach the relay: {published}"
    );
    harness.session.kill("killed");
}

/// EXP-750: the Stop button on a live output card — the child dies and the
/// agent's `wait_for_exit` answers instead of hanging for half a minute.
#[cfg(unix)]
#[test]
fn kill_terminal_ends_a_running_command() {
    let harness = start_fake("terminal-kill");
    harness.session.send_prompt("terminal-sleep".to_string());
    until("the created terminal", || {
        harness
            .state
            .terminal_id
            .lock()
            .expect("the terminal slot is not poisoned")
            .is_some()
    });
    let terminal_id = harness
        .state
        .terminal_id
        .lock()
        .expect("the terminal slot is not poisoned")
        .clone()
        .expect("the terminal id is recorded");

    harness.session.kill_terminal(&terminal_id);
    until("the killed command's exit", || {
        harness
            .state
            .terminal_exit
            .lock()
            .expect("the exit slot is not poisoned")
            .is_some()
    });
    harness.session.kill("killed");
}

/// EXP-758: a run that dies before it is live must SAY why. `EngineExit`
/// carried the error to `on_exit` and nowhere else, so the CLI printed
/// nothing and the session tab showed an empty transcript that had simply
/// "ended"; the reason is now a `Failed` phase, emitted before `Ended` and
/// replayed in that order to whoever attaches later.
#[test]
fn a_run_that_dies_in_its_handshake_ends_as_a_failed_phase() {
    /// An agent binary that is gone by the time we speak ACP to it.
    struct DeadAgent;

    impl ConnectTo<Client> for DeadAgent {
        fn connect_to(
            self,
            _client: impl ConnectTo<Agent>,
        ) -> impl std::future::Future<Output = Result<(), Error>> + Send {
            async move {
                Err(Error::internal_error().data(serde_json::Value::String(
                    "the agent binary is gone".to_string(),
                )))
            }
        }
    }

    let worktree = std::env::temp_dir().join(format!(
        "exp758-engine-dead-{}-{}",
        std::process::id(),
        Instant::now().elapsed().as_nanos()
    ));
    std::fs::create_dir_all(&worktree).expect("the scratch worktree is creatable");
    let _hold = coding::launch_gate::hold(&worktree);
    let runtime = steer::SteerRuntime::new().expect("a steer runtime");
    let host = Arc::new(TestHost::default());
    let session = engine::start_with(
        EngineStart {
            prepared: prepared("sess-dead", worktree.clone()),
            trpc: Arc::new(api::TrpcClient::new("http://127.0.0.1:1", Arc::new(|| None))),
            runtime: Arc::clone(&runtime),
            data_dir: worktree.clone(),
            account_id: "acct-1".to_string(),
            own_user_id: Some("user-1".to_string()),
            personal_key: None,
            issue_id: Some("issue-1".to_string()),
            foreign_host: false,
            publish: false,
            kill: KillFeed::inert(),
            local_sink: None,
        },
        host.clone(),
        EngineParts {
            adapter: DeadAgent,
            child_exit: ChildExitLink::new(),
            sink: None,
        },
    )
    .expect("the engine starts");

    let exit = session.wait_timeout(BUDGET).expect("the run ends");
    let error = exit.error.expect("the exit carries the handshake error");
    assert!(!error.is_empty());

    // A view attaching after the end still learns the reason, and learns it
    // BEFORE the end.
    let mut phases = Vec::new();
    let replay = session.subscribe();
    while let Ok(event) = replay.recv_timeout(Duration::from_millis(100)) {
        if let LocalFeedEvent::Phase(phase) = event {
            phases.push(phase);
        }
    }
    assert_eq!(
        phases,
        vec![
            engine::EnginePhase::Failed(error),
            engine::EnginePhase::Ended
        ]
    );
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

/// REV2-17: the debounced worktree `diff` is a published string like any
/// other, and the run's own launcher secrets are exactly what an agent can
/// copy into a tracked file. The mapper masked them from the start; the
/// lifecycle's diff ticker used to build its OWN redactor from the `expu_`
/// key alone, so the credential-file token crossed to the relay verbatim.
/// The planted secret deliberately matches none of the static
/// `SECRET_PATTERNS`: only the shared exact-match set can catch it.
#[test]
fn the_wire_diff_masks_the_runs_launcher_secrets() {
    const SECRET: &str = "n0tapatterntoken-4f2c9ab1d7e6";
    let harness = start_fake_with("diffsecret", |worktree| {
        let git = |args: &[&str]| {
            let output = std::process::Command::new("git")
                .args(args)
                .current_dir(worktree)
                .env("GIT_AUTHOR_NAME", "t")
                .env("GIT_AUTHOR_EMAIL", "t@example.com")
                .env("GIT_COMMITTER_NAME", "t")
                .env("GIT_COMMITTER_EMAIL", "t@example.com")
                .output()
                .expect("git runs");
            assert!(
                output.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        };
        git(&["init", "--quiet", "-b", "main"]);
        std::fs::write(worktree.join("notes.md"), "seed\n").expect("the seed file is writable");
        git(&["add", "notes.md"]);
        git(&["commit", "--quiet", "-m", "seed"]);
        // The EXP-73 credential file the launcher writes into the shared git
        // dir — `secrets_from_worktree` recovers the token from it.
        std::fs::write(
            worktree.join(".git").join("exp-git-credentials"),
            format!("username=x-access-token\npassword={SECRET}\n"),
        )
        .expect("the credential file is writable");
        // ... and the agent copying that token into a TRACKED file, which is
        // what puts it in the worktree patch.
        std::fs::write(worktree.join("notes.md"), format!("seed\ntoken: {SECRET}\n"))
            .expect("the tracked file is writable");
    });

    until("the worktree diff", || {
        !events_of(&harness.sink, "diff").is_empty()
    });
    let published = serde_json::to_string(&events_of(&harness.sink, "diff"))
        .expect("the diff events serialize");
    assert!(
        published.contains("notes.md"),
        "the diff covers the tracked file: {published}"
    );
    assert!(
        !published.contains(SECRET),
        "the credential-file token reached the wire: {published}"
    );
    assert!(
        published.contains("[redacted]"),
        "the token was dropped instead of masked: {published}"
    );
    harness.session.kill("killed");
}
