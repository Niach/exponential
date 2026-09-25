//! EXP-746 — the engine core against a scripted ACP agent.
//!
//! `FakeAgent` is a real `ConnectTo<Client>` over the SDK's own in-process
//! channel, so these tests drive the REAL `host.rs` + `session.rs` +
//! `lifecycle.rs` — the same code path a claude/codex adapter takes,
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
    AvailableCommand, AvailableCommandsUpdate, CompactionId, CompactionStatus, CompactionUpdate,
    ContentBlock, ContentChunk, CreateElicitationRequest, CreateTerminalRequest, ElicitationAction, ElicitationContentValue,
    ElicitationFormMode, ElicitationPropertySchema, ElicitationSchema, ElicitationSessionScope,
    InitializeRequest, InitializeResponse, LoadSessionRequest, LoadSessionResponse,
    NewSessionRequest, NewSessionResponse, PermissionOption, PermissionOptionId,
    PermissionOptionKind, PromptRequest, PromptResponse, ReadTextFileRequest,
    ReleaseTerminalRequest, RequestPermissionOutcome, RequestPermissionRequest,
    SessionConfigOption, SessionConfigOptionCategory, SessionConfigSelectOption, SessionId,
    SessionInfoUpdate,
    SessionMode, SessionModeId, SessionModeState, SessionNotification, SessionUpdate,
    SetSessionConfigOptionRequest, SetSessionConfigOptionResponse, SetSessionModeRequest,
    SetSessionModeResponse, StopReason, StringPropertySchema, Terminal, TerminalOutputRequest,
    TextContent, ToolCall, ToolCallContent, ToolCallId, ToolKind, UsageUpdate,
    WaitForTerminalExitRequest,
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
    /// EXP-873: lets the test end the compaction the "compact" turn holds open.
    fold_released: AtomicBool,
    /// EXP-873: while set, the fake does NOT replay a prompt's text on
    /// receipt — the replay waits until the gate clears, the way claude
    /// takes a mid-turn message in at its next tool boundary. Clear (the
    /// default) = replay at once.
    replay_gate: AtomicBool,
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
                        // EXP-873: claude replays every user message the
                        // moment it takes it in (`--replay-user-messages`);
                        // so does the fake — at once, or once the test's gate
                        // clears. "silent" is a message the agent folded in
                        // WITHOUT echoing it.
                        if text != "silent" {
                            let replay_cx = cx.clone();
                            let replay_state = prompt_state.clone();
                            let replay_session = request.session_id.clone();
                            let replay_text = text.clone();
                            cx.spawn(async move {
                                let deadline = Instant::now() + BUDGET;
                                while Instant::now() < deadline
                                    && replay_state.replay_gate.load(Ordering::SeqCst)
                                {
                                    tokio::time::sleep(Duration::from_millis(10)).await;
                                }
                                if !replay_state.replay_gate.load(Ordering::SeqCst) {
                                    let _ = replay_cx.send_notification(SessionNotification::new(
                                        replay_session,
                                        SessionUpdate::UserMessageChunk(ContentChunk::new(
                                            ContentBlock::Text(TextContent::new(replay_text)),
                                        )),
                                    ));
                                }
                                Ok(())
                            })?;
                        }
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
        // EXP-1051: the context bar. The rest of the prompt is the slot the
        // adapter would stamp, sent TWICE on a no-op `session_info_update` —
        // the mapper's latest-wins dedupe is what turns two identical frames
        // into one published event.
        text if text.starts_with("context-layout ") => {
            let slot: serde_json::Value =
                serde_json::from_str(text.trim_start_matches("context-layout ").trim())
                    .expect("the test's slot parses");
            let mut meta = serde_json::Map::new();
            meta.insert(engine::CONTEXT_LAYOUT_META_KEY.to_string(), slot);
            for _ in 0..2 {
                let _ = cx.send_notification(
                    SessionNotification::new(
                        session_id.clone(),
                        SessionUpdate::SessionInfoUpdate(SessionInfoUpdate::new()),
                    )
                    .meta(meta.clone()),
                );
            }
            StopReason::EndTurn
        }
        // EXP-936: a turn that fills most of the window — the meter the
        // compaction policy reads — and then holds until released, the way
        // the run's own tool call keeps its turn open.
        "usage-high" => {
            let _ = cx.send_notification(SessionNotification::new(
                session_id.clone(),
                SessionUpdate::UsageUpdate(UsageUpdate::new(150_000, 200_000)),
            ));
            let deadline = Instant::now() + BUDGET;
            while Instant::now() < deadline && !state.released.load(Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            StopReason::EndTurn
        }
        // EXP-936: the agent's own `/compact <keep>` as the host sends it —
        // a fold that opens and closes within the turn, like claude's.
        text if text.starts_with("/compact") => {
            let _ = cx.send_notification(SessionNotification::new(
                session_id.clone(),
                SessionUpdate::CompactionUpdate(CompactionUpdate::new(
                    CompactionId::new("c-own"),
                    CompactionStatus::InProgress,
                )),
            ));
            let _ = cx.send_notification(SessionNotification::new(
                session_id.clone(),
                SessionUpdate::CompactionUpdate(CompactionUpdate::new(
                    CompactionId::new("c-own"),
                    CompactionStatus::Completed,
                )),
            ));
            let _ = cx.send_notification(SessionNotification::new(
                session_id.clone(),
                SessionUpdate::UsageUpdate(UsageUpdate::new(12_000, 200_000)),
            ));
            StopReason::EndTurn
        }
        // EXP-969: the same fold, but its `completed` edge NEVER comes — a
        // manual `/compact` that lands no `compact_boundary`. The turn ends
        // with the gate still open, which is what used to strand the bar.
        "compact-silent" => {
            let _ = cx.send_notification(SessionNotification::new(
                session_id.clone(),
                SessionUpdate::CompactionUpdate(CompactionUpdate::new(
                    CompactionId::new("c-1"),
                    CompactionStatus::InProgress,
                )),
            ));
            let deadline = Instant::now() + BUDGET;
            while Instant::now() < deadline && !state.fold_released.load(Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            StopReason::EndTurn
        }
        // EXP-873: a compaction held open until the test releases it, the
        // way claude's auto-compaction holds a turn; the turn ends with it.
        "compact" => {
            let _ = cx.send_notification(SessionNotification::new(
                session_id.clone(),
                SessionUpdate::CompactionUpdate(CompactionUpdate::new(
                    CompactionId::new("c-1"),
                    CompactionStatus::InProgress,
                )),
            ));
            let deadline = Instant::now() + BUDGET;
            while Instant::now() < deadline && !state.fold_released.load(Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            let _ = cx.send_notification(SessionNotification::new(
                session_id.clone(),
                SessionUpdate::CompactionUpdate(CompactionUpdate::new(
                    CompactionId::new("c-1"),
                    CompactionStatus::Completed,
                )),
            ));
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
                workflow: None,
                agent: coding::CodingAgent::Claude,
                model: String::new(),
                effort: String::new(),
                ultracode: false,
                plan_mode: false,
                subagent_model: String::new(),
                mcp_server_ids: Vec::new(),
                account: None,
            },
            mcp: coding::AgentMcp::ClaudeFile,
            session_id: session_id.to_string(),
            resume: None,
            reaper_settings_path: None,
            system_append: coding::skill::system_append(None),
            context_layers: coding::ContextLayers::default(),
            servers: Vec::new(),
            mcp_secrets: Default::default(),
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
            batch_issue_ids: Vec::new(),
            agent: None,
            agent_account: None,
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

/// Every published event, in order — for assertions about what came first.
fn all_events(sink: &RecordingSink) -> Vec<serde_json::Value> {
    sink.snapshot()
        .iter()
        .filter_map(|event| serde_json::to_value(event).ok())
        .collect()
}

/// The newest `queue` frame's message list.
fn last_queue(sink: &RecordingSink) -> Option<Vec<serde_json::Value>> {
    events_of(sink, "queue")
        .pop()
        .and_then(|slot| slot["messages"].as_array().cloned())
}

fn queue_texts(messages: &[serde_json::Value]) -> Vec<String> {
    messages
        .iter()
        .filter_map(|message| message["text"].as_str().map(str::to_string))
        .collect()
}

fn has_user_row(sink: &RecordingSink, text: &str) -> bool {
    events_of(sink, "user_message")
        .iter()
        .any(|event| event["text"] == text)
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

/// EXP-848: the turn slot, end to end through the REAL host. The session
/// start SEEDS `ended`, a prompt opens `started`, and the turn settling closes
/// it again — which is the one signal every client's "Working…" spinner and
/// Stop button read.
#[test]
fn a_turn_publishes_its_started_and_ended_edges() {
    let harness = start_fake("turn-edges");
    // The handshake seeds the slot: a viewer that joins before any prompt
    // reads `ended` rather than inferring it.
    until("the seeded turn slot", || {
        !events_of(&harness.sink, "turn").is_empty()
    });
    assert_eq!(events_of(&harness.sink, "turn")[0]["state"], "ended");

    harness.session.send_prompt("stream".to_string());
    until("the started edge", || {
        events_of(&harness.sink, "turn")
            .iter()
            .any(|event| event["state"] == "started")
    });
    // …and the turn settling closes it. The fake answers `stream` on its own,
    // so no release is needed.
    until("the ended edge after the turn", || {
        let states: Vec<String> = events_of(&harness.sink, "turn")
            .iter()
            .map(|event| event["state"].as_str().unwrap_or_default().to_string())
            .collect();
        states.iter().rposition(|state| state == "ended")
            > states.iter().position(|state| state == "started")
    });
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

/// EXP-873: a message that arrives while a turn is running goes to the agent
/// AT ONCE (claude folds it in at its next tool boundary) and waits in the
/// `queue` slot as `sent`, with no `user_message` row of its own; the agent's
/// REPLAY of it is the moment it read it — the bar closes and the row
/// appears, in that order. (EXP-861 held the message on the device until the
/// idle edge, which is not how the CLI's own queue behaves: a steer lands
/// mid-turn, between two tool calls, not after the turn.)
#[test]
fn a_mid_turn_message_goes_to_the_agent_at_once_and_its_row_lands_on_the_replay() {
    let harness = start_fake("queue-sent");
    let signal = harness.session.turn_signal();
    let prompts = || harness.state.prompts.lock().expect("the prompt log is not poisoned").len();
    harness.state.replay_gate.store(true, Ordering::SeqCst);

    harness.session.send_prompt("steered".to_string());
    until("the first turn", || prompts() >= 1);
    harness.session.steer("also check the tests".to_string());
    until("the message reaches the agent mid-turn", || prompts() >= 2);
    assert!(!signal.is_idle(), "the first turn is still running");
    until("the queue slot", || last_queue(&harness.sink).is_some_and(|m| m.len() == 1));
    let slot = last_queue(&harness.sink).expect("a queue frame");
    assert_eq!(slot[0]["text"], "also check the tests");
    assert_eq!(slot[0]["sent"], true, "sent to the agent, awaiting its replay");
    assert!(slot[0]["id"].as_str().is_some_and(|id| !id.is_empty()));

    // No row while the agent has not taken it in: the bar is its only home.
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        !has_user_row(&harness.sink, "also check the tests"),
        "a message the agent has not read has no user row"
    );

    // The replay: the row lands, and the bar closed FIRST.
    harness.state.replay_gate.store(false, Ordering::SeqCst);
    until("the read message's row", || has_user_row(&harness.sink, "also check the tests"));
    let events = all_events(&harness.sink);
    let row_at = events
        .iter()
        .position(|event| event["kind"] == "user_message" && event["text"] == "also check the tests")
        .expect("the row");
    let closed_at = events
        .iter()
        .rposition(|event| {
            event["kind"] == "queue" && event["messages"].as_array().is_some_and(Vec::is_empty)
        })
        .expect("the bar closing");
    assert!(closed_at < row_at, "the bar closes before the row appears");
    assert!(
        !signal.is_idle(),
        "the turn the message joined is still running: answered {:?}, turns {:?}",
        harness.state.answered.lock().expect("the answer log is not poisoned"),
        events_of(&harness.sink, "turn")
    );

    harness.state.released.store(true, Ordering::SeqCst);
    until("the idle edge", || signal.is_idle());
    assert_eq!(prompts(), 2);
    assert_eq!(
        events_of(&harness.sink, "user_message")
            .iter()
            .filter(|event| event["text"] == "also check the tests")
            .count(),
        1,
        "one row, never a second from the echo"
    );
    harness.session.kill("killed");
}

/// EXP-873: an agent that folded a message in WITHOUT echoing it still read
/// it — on the idle edge every sent line the mapper stopped awaiting becomes
/// its row and leaves the bar, so no message the agent answered sits
/// "queued" forever.
#[test]
fn a_sent_message_without_a_replay_lands_on_the_idle_edge() {
    let harness = start_fake("queue-silent");
    let signal = harness.session.turn_signal();
    let prompts = || harness.state.prompts.lock().expect("the prompt log is not poisoned").len();

    harness.session.send_prompt("steered".to_string());
    until("the first turn", || prompts() >= 1);
    harness.session.steer("silent".to_string());
    until("the message reaches the agent", || prompts() >= 2);
    until("the sent line", || {
        last_queue(&harness.sink).is_some_and(|m| m.len() == 1 && m[0]["sent"] == true)
    });
    std::thread::sleep(Duration::from_millis(200));
    assert!(!has_user_row(&harness.sink, "silent"), "no replay, no row yet");

    harness.state.released.store(true, Ordering::SeqCst);
    until("the idle edge", || signal.is_idle());
    until("the row on the idle edge", || has_user_row(&harness.sink, "silent"));
    assert_eq!(last_queue(&harness.sink).map(|m| m.len()), Some(0));
    harness.session.kill("killed");
}

/// EXP-861/EXP-873: a message sent while a COMPACTION is open is HELD on the
/// device — no `sent` flag, the agent sees nothing — and goes to the agent
/// the moment the fold ends.
#[test]
fn a_message_sent_mid_compaction_is_held_until_the_fold_ends() {
    let harness = start_fake("queue-compact");
    let signal = harness.session.turn_signal();
    let prompts = || harness.state.prompts.lock().expect("the prompt log is not poisoned").len();

    harness.session.send_prompt("compact".to_string());
    until("the compaction to open", || {
        events_of(&harness.sink, "compaction")
            .iter()
            .any(|event| event["phase"] == "started")
    });
    harness.session.steer("after the fold".to_string());
    until("the held line", || last_queue(&harness.sink).is_some_and(|m| m.len() == 1));
    let slot = last_queue(&harness.sink).expect("a queue frame");
    assert_eq!(slot[0]["text"], "after the fold");
    assert!(slot[0].get("sent").is_none(), "held on the device, not sent");
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(prompts(), 1, "a held message never reaches the agent mid-compaction");
    assert!(!has_user_row(&harness.sink, "after the fold"));

    harness.state.fold_released.store(true, Ordering::SeqCst);
    until("the held message reaches the agent", || prompts() >= 2);
    until("its row", || has_user_row(&harness.sink, "after the fold"));
    until("the idle edge", || signal.is_idle());
    assert_eq!(last_queue(&harness.sink).map(|m| m.len()), Some(0));
    harness.session.kill("killed");
}

/// EXP-969: a fold whose `completed` edge never arrives (a manual
/// `/compact`) must not strand the bar — the turn ending closes the gate and
/// the held message goes to the agent as the next turn, instead of sitting
/// there while the run reads as busy forever.
#[test]
fn a_held_message_drains_when_a_silent_compaction_turn_ends() {
    let harness = start_fake("queue-compact-silent");
    let signal = harness.session.turn_signal();
    let prompts = || harness.state.prompts.lock().expect("the prompt log is not poisoned").len();

    harness.session.send_prompt("compact-silent".to_string());
    until("the compaction to open", || {
        events_of(&harness.sink, "compaction")
            .iter()
            .any(|event| event["phase"] == "started")
    });
    harness.session.steer("after the fold".to_string());
    until("the held line", || last_queue(&harness.sink).is_some_and(|m| m.len() == 1));
    assert!(
        last_queue(&harness.sink).expect("a queue frame")[0].get("sent").is_none(),
        "held while the gate is open"
    );

    // The turn ends with no `completed` edge at all.
    harness.state.fold_released.store(true, Ordering::SeqCst);
    until("the held message reaches the agent", || prompts() >= 2);
    until("its row", || has_user_row(&harness.sink, "after the fold"));
    until("the idle edge", || signal.is_idle());
    assert!(
        events_of(&harness.sink, "compaction")
            .iter()
            .any(|event| event["phase"] == "ended"),
        "the turn end closed the strip too"
    );
    assert_eq!(last_queue(&harness.sink).map(|m| m.len()), Some(0));
    harness.session.kill("killed");
}

/// EXP-936: the run's own compaction ask. Refused `too_early` while the
/// meter is low; accepted once it is high — and then the host sends the
/// agent's `/compact <keep>` ONLY after the running turn ends, follows the
/// compaction with the continuation prompt, and refuses a second ask as
/// `cooldown` throughout.
#[test]
fn a_compaction_ask_runs_compact_then_continue_at_the_turn_boundaries() {
    let harness = start_fake("compact-ask");
    let signal = harness.session.turn_signal();
    let prompts = || harness.state.prompts.lock().expect("the prompt log is not poisoned").clone();
    until("the session to be live", || signal.is_idle());

    // No meter yet: nothing to show for the claim.
    assert_eq!(
        harness.session.request_compaction(Some("open threads".to_string())),
        steer::CompactVerdict::Refused(steer::CompactRefusal::TooEarly)
    );

    harness.session.send_prompt("usage-high".to_string());
    until("the meter", || {
        events_of(&harness.sink, "usage")
            .last()
            .is_some_and(|event| event["contextUsed"] == 150_000)
    });
    // Mid-turn: accepted, but nothing goes to the agent until the turn ends.
    assert_eq!(
        harness.session.request_compaction(Some("open threads".to_string())),
        steer::CompactVerdict::Accepted
    );
    assert_eq!(
        harness.session.request_compaction(None),
        steer::CompactVerdict::Refused(steer::CompactRefusal::Cooldown)
    );
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(prompts(), vec!["usage-high"], "the `/compact` waits for the turn boundary");

    harness.state.released.store(true, Ordering::SeqCst);
    until("the /compact and the continuation", || prompts().len() >= 3);
    assert_eq!(
        prompts(),
        vec![
            "usage-high".to_string(),
            "/compact open threads".to_string(),
            engine::COMPACT_CONTINUE_PROMPT.to_string(),
        ]
    );
    until("the idle edge", || signal.is_idle());
    assert!(
        events_of(&harness.sink, "compaction")
            .iter()
            .any(|event| event["phase"] == "ended"),
        "the fold opened and closed on the feed"
    );
    assert!(has_user_row(&harness.sink, "/compact open threads"));
    assert!(has_user_row(&harness.sink, engine::COMPACT_CONTINUE_PROMPT));
    // Right after: the meter is low again AND the cooldown holds.
    assert!(matches!(
        harness.session.request_compaction(None),
        steer::CompactVerdict::Refused(_)
    ));
    harness.session.kill("killed");
}

/// EXP-861: the × on the bar — an `unqueue` drops a HELD message; the slot
/// says so and the agent never sees it. EXP-873: a SENT line cannot be taken
/// back from the agent, so the slot is republished as it stands and the line
/// a client dropped optimistically comes back.
#[test]
fn an_unqueue_drops_a_held_message_but_not_a_sent_one() {
    let harness = start_fake("queue-revoke");
    let signal = harness.session.turn_signal();
    let prompts = || harness.state.prompts.lock().expect("the prompt log is not poisoned").len();
    let queue_frames = || events_of(&harness.sink, "queue").len();
    harness.state.replay_gate.store(true, Ordering::SeqCst);

    // A sent line: the × changes nothing but the slot restating it.
    harness.session.send_prompt("steered".to_string());
    until("the first turn", || prompts() >= 1);
    harness.session.steer("sent one".to_string());
    until("the sent line", || {
        last_queue(&harness.sink).is_some_and(|m| m.len() == 1 && m[0]["sent"] == true)
    });
    let sent_id = last_queue(&harness.sink).expect("a slot")[0]["id"]
        .as_str()
        .expect("an id")
        .to_string();
    let before = queue_frames();
    harness.session.unqueue(&sent_id);
    until("the slot restated", || queue_frames() > before);
    assert_eq!(
        queue_texts(&last_queue(&harness.sink).expect("a slot")),
        vec!["sent one"],
        "a sent line stays"
    );
    harness.state.replay_gate.store(false, Ordering::SeqCst);
    harness.state.released.store(true, Ordering::SeqCst);
    until("the idle edge", || signal.is_idle());
    assert!(has_user_row(&harness.sink, "sent one"));

    // A held line: the × drops it, the agent never sees it.
    harness.session.send_prompt("compact".to_string());
    until("the compaction to open", || {
        events_of(&harness.sink, "compaction")
            .iter()
            .any(|event| event["phase"] == "started")
    });
    harness.session.steer("drop me".to_string());
    harness.session.steer("keep me".to_string());
    until("two held lines", || last_queue(&harness.sink).is_some_and(|m| m.len() == 2));
    let held_id = last_queue(&harness.sink).expect("a slot")[0]["id"]
        .as_str()
        .expect("an id")
        .to_string();
    harness.session.unqueue(&held_id);
    // An unknown id changes nothing.
    harness.session.unqueue("no-such-id");
    until("the slot without it", || last_queue(&harness.sink).is_some_and(|m| m.len() == 1));
    assert_eq!(queue_texts(&last_queue(&harness.sink).expect("a slot")), vec!["keep me"]);

    harness.state.fold_released.store(true, Ordering::SeqCst);
    until("the kept message reaches the agent", || prompts() >= 4);
    until("the idle edge", || signal.is_idle());
    let seen = harness.state.prompts.lock().expect("the prompt log is not poisoned").clone();
    assert_eq!(seen, vec!["steered", "sent one", "compact", "keep me"]);
    assert!(!has_user_row(&harness.sink, "drop me"), "a revoked message has no row");
    harness.session.kill("killed");
}

/// EXP-861/EXP-873: a Stop drops every unread message — held AND sent (the
/// CLI's own `cancel_queued`) — so nothing typed behind a turn the person
/// just killed sneaks in, and a dropped message never renders as a row: the
/// clients hand its text back to the composer instead.
#[test]
fn a_stop_drops_the_unread_messages_without_a_row() {
    let harness = start_fake("queue-stop");
    let signal = harness.session.turn_signal();
    let prompts = || harness.state.prompts.lock().expect("the prompt log is not poisoned").len();
    harness.state.replay_gate.store(true, Ordering::SeqCst);

    harness.session.send_prompt("hang".to_string());
    until("the turn to start", || prompts() >= 1);
    harness.session.steer("never mind".to_string());
    until("the sent line", || {
        last_queue(&harness.sink).is_some_and(|m| m.len() == 1 && m[0]["sent"] == true)
    });
    harness.session.cancel_turn();
    until("the adapter's cancel", || harness.state.cancelled.load(Ordering::SeqCst));
    until("the idle edge", || signal.is_idle());
    assert_eq!(last_queue(&harness.sink).map(|m| m.len()), Some(0));
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        !has_user_row(&harness.sink, "never mind"),
        "a message the Stop dropped never renders as read"
    );
    harness.session.kill("killed");
}

/// EXP-784/EXP-873: `TURN_SLOTS` still bounds how many `session/prompt`s are
/// open at once — past two, a mid-turn message parks inside its task until a
/// slot frees — and every message keeps send order on the transcript, its
/// row landing on its replay.
#[test]
fn mid_turn_messages_keep_send_order_under_the_turn_slot_bound() {
    assert_eq!(engine::host::TURN_SLOTS, 2, "the test below assumes two slots");
    let harness = start_fake("queue-order");
    let signal = harness.session.turn_signal();
    let prompts = || harness.state.prompts.lock().expect("the prompt log is not poisoned").len();

    harness.session.send_prompt("steered".to_string());
    until("the first turn", || prompts() >= 1);
    for text in ["one", "two", "three"] {
        harness.session.steer(text.to_string());
    }
    until("every message reaches the agent", || prompts() >= 4);
    harness.state.released.store(true, Ordering::SeqCst);
    until("the idle edge", || signal.is_idle());
    let seen = harness.state.prompts.lock().expect("the prompt log is not poisoned").clone();
    assert_eq!(seen, vec!["steered", "one", "two", "three"]);
    let rows: Vec<String> = events_of(&harness.sink, "user_message")
        .iter()
        .filter_map(|event| event["text"].as_str().map(str::to_string))
        .collect();
    assert_eq!(rows, vec!["steered", "one", "two", "three"]);
    assert_eq!(last_queue(&harness.sink).map(|m| m.len()), Some(0));
    harness.session.kill("killed");
}

/// EXP-861: a run that ends with messages still unread does NOT close the bar
/// on its way out — the last `queue` frame lists them, so a client can move
/// the text somewhere the person can still reach it (the composer draft) on
/// the `ended` edge instead of losing it. No row for them: nothing says the
/// agent read them.
#[test]
fn a_run_ending_with_unread_messages_keeps_them_in_its_last_queue_frame() {
    let harness = start_fake("queue-end");
    let prompts = || harness.state.prompts.lock().expect("the prompt log is not poisoned").len();
    harness.state.replay_gate.store(true, Ordering::SeqCst);

    // The seed: an empty slot from the very first moment, so a resumed
    // run's inherited history never replays a predecessor's full bar.
    until("the seeded slot", || {
        events_of(&harness.sink, "queue")
            .first()
            .is_some_and(|slot| slot["messages"].as_array().is_some_and(|m| m.is_empty()))
    });
    harness.session.send_prompt("hang".to_string());
    until("the turn to start", || prompts() >= 1);
    for text in ["one", "two"] {
        harness.session.steer(text.to_string());
    }
    until("two unread lines", || last_queue(&harness.sink).is_some_and(|m| m.len() == 2));

    harness.session.kill("killed");
    until("the exit", || harness.session.is_done());
    assert_eq!(queue_texts(&last_queue(&harness.sink).expect("a slot")), vec!["one", "two"]);
    assert!(
        !has_user_row(&harness.sink, "one") && !has_user_row(&harness.sink, "two"),
        "an unread message has no row"
    );
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

    // EXP-877: `set_config` reaches the agent AND the answer's option list
    // folds into the snapshot — the `model` value is the one thing the wire
    // carries about options now (EXP-772 took the rest away for good).
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
    until("the switched model to reach the snapshot", || {
        events_of(&harness.sink, "config_state")
            .last()
            .and_then(|state| state["options"][0]["value"].as_str().map(str::to_string))
            == Some("sonnet".to_string())
    });
    assert!(events_of(&harness.sink, "config_state").iter().all(|state| {
        state["options"]
            .as_array()
            .is_some_and(|options| options.len() == 1 && options[0]["id"] == "model")
    }));
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

// ---------------------------------------------------------------------------
// EXP-1051 — the context-layout slot, end to end
// ---------------------------------------------------------------------------

/// The slot an adapter would stamp, as the fake agent's prompt carries it.
fn context_layout_prompt(segments: serde_json::Value) -> String {
    format!(
        "context-layout {}",
        serde_json::json!({ "segments": segments, "model": "opus" })
    )
}

/// The slot reaches the wire ONCE even though the agent stamped it twice:
/// `context_layout` is latest-wins state and an identical re-emit is not news.
#[test]
fn a_context_layout_slot_publishes_once_and_an_identical_re_emit_is_dropped() {
    let harness = start_fake("stream");
    harness.session.send_prompt(context_layout_prompt(serde_json::json!([
        { "key": "base", "tokens": 21_000, "source": "measured" },
        { "key": "playbook", "tokens": 1_500, "source": "estimated" },
    ])));

    until("the context_layout event", || {
        !events_of(&harness.sink, "context_layout").is_empty()
    });
    // Give the second, identical notification every chance to arrive.
    std::thread::sleep(Duration::from_millis(200));
    let events = events_of(&harness.sink, "context_layout");
    assert_eq!(events.len(), 1, "two identical frames publish once: {events:?}");
    assert_eq!(events[0]["segments"][0]["key"], "base");
    assert_eq!(events[0]["segments"][0]["tokens"], 21_000);
    assert_eq!(events[0]["segments"][0]["source"], "measured");
    assert_eq!(events[0]["segments"][1]["key"], "playbook");
}

/// The relay drops a frame whose numbers fall outside its zod bounds WHOLE,
/// so the device clamps first — and an unknown key costs its frame nothing.
#[test]
fn an_out_of_bounds_segment_is_clamped_and_an_unknown_key_dropped() {
    let harness = start_fake("stream");
    harness.session.send_prompt(context_layout_prompt(serde_json::json!([
        { "key": "moon", "tokens": 10, "source": "estimated" },
        { "key": "playbook", "tokens": 2_000_000_000i64, "source": "estimated" },
        // Out of contract order on purpose: the mapper sorts.
        { "key": "base", "tokens": -5, "source": "measured" },
        // A duplicate keeps the FIRST occurrence.
        { "key": "playbook", "tokens": 1, "source": "estimated" },
    ])));

    until("the context_layout event", || {
        !events_of(&harness.sink, "context_layout").is_empty()
    });
    let events = events_of(&harness.sink, "context_layout");
    let segments = events[0]["segments"].as_array().expect("an array");
    let keys: Vec<&str> = segments
        .iter()
        .map(|segment| segment["key"].as_str().expect("a key"))
        .collect();
    assert_eq!(keys, vec!["base", "playbook"], "`moon` is dropped, the rest sorted");
    assert_eq!(segments[0]["tokens"], 0, "a negative token count floors at zero");
    assert_eq!(
        segments[1]["tokens"], 1_000_000_000i64,
        "the relay's ceiling, not the nonsense number"
    );
}

/// A payload that is not a segment list at all publishes nothing — and never
/// panics the mapping step, which would take the whole run with it.
#[test]
fn a_nonsense_context_layout_payload_publishes_nothing() {
    let harness = start_fake("stream");
    harness
        .session
        .send_prompt("context-layout {\"segments\": \"not a list\"}".to_string());
    until("the turn to settle", || {
        kinds(&harness.sink).iter().any(|kind| kind == "turn")
    });
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        events_of(&harness.sink, "context_layout").is_empty(),
        "a malformed slot is dropped whole"
    );
}
