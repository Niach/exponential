//! EXP-1426 part 4d — the server-free path: the core [`start_with`] driven
//! with [`NoBackend`] and a [`RecordingSink`] against a scripted ACP agent,
//! no tRPC, no relay, no run record. The fake is `engine`'s
//! (`engine/tests/fake_agent.rs`, copied: a test crate cannot share it), a
//! real `ConnectTo<Client>` over the SDK's in-process channel, so this drives
//! the REAL host and session — the path a spezl-style host takes with the
//! user's own `claude` / `codex` adapter instead of the fake.
//!
//! What it guards: a turn streams into the sink and the local feed; a
//! permission is answered locally and resolves the agent's request; a
//! delivered message starts a turn; a stop ends the run exactly once through
//! the backend's end sequence; a resume reopens the session with
//! `session/load`.

#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use agent_client_protocol::v1::schema::{
    AvailableCommand, AvailableCommandsUpdate, CompactionId, CompactionStatus, CompactionUpdate,
    ContentBlock, ContentChunk, CreateElicitationRequest, ElicitationAction, ElicitationContentValue,
    ElicitationFormMode, ElicitationPropertySchema, ElicitationSchema, ElicitationSessionScope,
    InitializeRequest, InitializeResponse, LoadSessionRequest, LoadSessionResponse,
    NewSessionRequest, NewSessionResponse, PermissionOption, PermissionOptionId,
    PermissionOptionKind, PromptRequest, PromptResponse, ReadTextFileRequest,
    RequestPermissionOutcome, RequestPermissionRequest,
    SessionConfigOption, SessionConfigOptionCategory, SessionConfigSelectOption, SessionId,
    SessionInfoUpdate,
    SessionMode, SessionModeId, SessionModeState, SessionNotification, SessionUpdate,
    SetSessionConfigOptionRequest, SetSessionConfigOptionResponse, SetSessionModeRequest,
    SetSessionModeResponse, StopReason, StringPropertySchema,
    TextContent, ToolCall, ToolCallId, ToolKind, UsageUpdate,
};
use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::{
    on_receive_notification, on_receive_request, Agent, Client, ConnectTo, Error,
};
use eagent::agent::CodingAgent;
use eagent::engine::local::{EnginePhase, LocalFeedEvent};
use eagent::engine::{
    start_with, ChildExitLink, EngineExit, EngineHost, EngineSession, EventSink, NoBackend,
    RecordingSink, ResumeHandle, RunBackend, SessionCtx, SessionSpec,
};
use eagent::wire::RemoteAnswer;

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
    /// FEED-153: how many `fail-*` prompts the fake has refused so far.
    failures: AtomicUsize,
    /// EXP-1910: the fake never answers `initialize` (a wedged CLI, a long
    /// `session/load`).
    mute_handshake: AtomicBool,
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
            let init_state = state.clone();
            let prompt_state = state.clone();
            let cancel_state = state.clone();
            let config_state = state.clone();
            let mode_state = state.clone();

            Agent
                .v1()
                .name("fake-agent")
                .on_receive_request(
                    async move |_request: InitializeRequest, responder, _cx| {
                        if init_state.mute_handshake.load(Ordering::SeqCst) {
                            std::future::pending::<()>().await;
                        }
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
                    async move |_notification: agent_client_protocol::v1::schema::CancelNotification,
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
                        // FEED-153: `fail-once` refuses its FIRST send
                        // (an adapter erroring on a prompt it never ran),
                        // `fail-always` every one.
                        if text == "fail-always"
                            || (text == "fail-once" && prompt_state.failures.load(Ordering::SeqCst) == 0)
                        {
                            prompt_state.failures.fetch_add(1, Ordering::SeqCst);
                            let _ = responder.respond_with_error(
                                Error::internal_error().data(serde_json::Value::String("overloaded".to_string())),
                            );
                            return Ok(());
                        }
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
    cx: &agent_client_protocol::v1::ConnectionTo<Client>,
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
                    agent_client_protocol::v1::schema::ToolCallUpdate::new(
                        ToolCallId::new("tc-perm"),
                        agent_client_protocol::v1::schema::ToolCallUpdateFields::new()
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
        // FEED-62: a message claude folds into the running turn. Claude
        // answers it only with that turn's `result`, so it stays open until
        // the test releases the turn, exactly like the steered one.
        text if text.starts_with("fold ") => {
            let deadline = Instant::now() + BUDGET;
            while Instant::now() < deadline && !state.released.load(Ordering::SeqCst) {
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
            meta.insert(eagent::engine::context_layout::CONTEXT_LAYOUT_META_KEY.to_string(), slot);
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
        // EXP-1292: a turn whose LAST top-level message is its reply — the
        // prose before the tool call is not.
        "reply" => {
            for (message_id, text) in [("m-a", "Looking around."), ("m-b", "All "), ("m-b", "done.")] {
                if message_id == "m-b" && text == "All " {
                    let _ = cx.send_notification(SessionNotification::new(
                        session_id.clone(),
                        SessionUpdate::ToolCall(
                            ToolCall::new(ToolCallId::new("tc-reply"), "Read src/lib.rs")
                                .kind(ToolKind::Read),
                        ),
                    ));
                }
                let _ = cx.send_notification(SessionNotification::new(
                    session_id.clone(),
                    SessionUpdate::AgentMessageChunk(
                        ContentChunk::new(ContentBlock::Text(TextContent::new(text)))
                            .message_id(agent_client_protocol::v1::schema::MessageId::new(message_id)),
                    ),
                ));
            }
            StopReason::EndTurn
        }
        "quit" => {
            child_exit.record(eagent::spawn::ChildExit {
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
        eagent::engine::launch::NATIVE_SESSION_META_KEY.to_string(),
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
    _runtime: tokio::runtime::Runtime,
}

fn start_fake(name: &str, resume: Option<ResumeHandle>) -> Harness {
    start_fake_with(name, resume, |host| Arc::new(NoBackend::with_host(host)))
}

fn start_fake_with(
    name: &str,
    resume: Option<ResumeHandle>,
    backend: impl FnOnce(Arc<TestHost>) -> Arc<dyn RunBackend>,
) -> Harness {
    start_adapter(name, resume, None, Arc::new(FakeState::default()), FakeAgent::new, backend)
}

/// [`start_fake_with`] over any adapter, with a seed prompt and the fake's
/// state set up front.
fn start_adapter<A: ConnectTo<Client> + 'static>(
    name: &str,
    resume: Option<ResumeHandle>,
    prompt: Option<String>,
    state: Arc<FakeState>,
    adapter: impl FnOnce(Arc<FakeState>, ChildExitLink) -> A,
    backend: impl FnOnce(Arc<TestHost>) -> Arc<dyn RunBackend>,
) -> Harness {
    let worktree = std::env::temp_dir().join(format!(
        "exp1426-core-{name}-{}-{}",
        std::process::id(),
        Instant::now().elapsed().as_nanos()
    ));
    std::fs::create_dir_all(&worktree).expect("the scratch worktree is creatable");
    // The host's own runtime: the core never spins one up.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("a tokio runtime");
    let sink = RecordingSink::new();
    let host = Arc::new(TestHost::default());
    let child_exit = ChildExitLink::new();
    let adapter = adapter(state.clone(), child_exit.clone());
    let session = SessionSpec {
        session_id: format!("sess-{name}"),
        run: eagent::engine::RunFacts {
            worktree: worktree.clone(),
            ..Default::default()
        },
        runtime: runtime.handle().clone(),
        data_dir: worktree,
        personal_key: None,
        persistent: false,
        seed_message_id: None,
        rotation_host: false,
        publish: false,
        local_sink: None,
        agent: CodingAgent::Claude,
        mcp_secrets: Vec::new(),
        replay: false,
        resume,
        prompt,
        aside: false,
        child_exit,
        attachments: None,
        host_tools: Arc::new(eagent::engine::NoHostTools),
        sink: Some(sink.clone() as Arc<dyn EventSink>),
    };
    let session = start_with(session, adapter, backend(host.clone())).expect("the engine starts");
    Harness {
        session,
        sink,
        state,
        host,
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

/// One run, no server: a turn streams into the sink and the local feed, a
/// permission is answered locally, a delivered message starts a turn, and a
/// stop ends the run once, through `NoBackend`'s end sequence.
#[test]
fn a_server_free_run_streams_answers_queues_and_stops() {
    let harness = start_fake("server-free", None);
    let feed = harness.session.subscribe();
    harness.session.send_prompt("stream".to_string());

    until("the tool event", || !events_of(&harness.sink, "tool").is_empty());
    until("the narration", || !events_of(&harness.sink, "narration").is_empty());
    assert_eq!(events_of(&harness.sink, "tool")[0]["name"], "Read");
    assert_eq!(harness.session.acp_session_id().as_deref(), Some("fake-session"));
    assert_eq!(harness.session.agent_native_session_id().as_deref(), Some("agent-native-1"));
    let mut saw_card = false;
    while let Ok(event) = feed.recv_timeout(Duration::from_millis(100)) {
        if let LocalFeedEvent::ToolCall { id, .. } = event {
            saw_card |= id == "tc-1";
        }
    }
    assert!(saw_card, "the local feed carries the tool card");

    // The permission card is answered HERE, not through a server.
    until("the stream turn's end", || harness.session.turn_signal().is_idle());
    harness.session.send_prompt("permission".to_string());
    until("the question card", || !events_of(&harness.sink, "question").is_empty());
    harness.session.answer(RemoteAnswer {
        question_id: "tc-perm".to_string(),
        ask_id: None,
        keys: vec!["allow".to_string()],
        text: None,
    });
    until("the agent's own resolution", || {
        harness.state.chosen.lock().expect("the choice slot").is_some()
    });
    assert_eq!(harness.state.chosen.lock().unwrap().as_deref(), Some("allow"));
    assert!(harness.session.open_cards().is_empty(), "NoBackend posts no cards");

    // A host message: between turns it starts one, announced as the user row.
    until("the permission turn's end", || harness.session.turn_signal().is_idle());
    harness.session.deliver("m-1".to_string(), "stream".to_string());
    until("the delivered prompt", || {
        harness.state.prompts.lock().unwrap().iter().filter(|prompt| *prompt == "stream").count() == 2
    });

    harness.session.kill("killed");
    let exit = harness.session.wait_timeout(BUDGET).expect("the run ends");
    assert_eq!(exit.outcome, "killed");
    assert_eq!(harness.session.phase(), Some(EnginePhase::Ended));
    until("the host heard", || harness.host.count.load(Ordering::SeqCst) == 1);
    assert_eq!(harness.host.exits.lock().unwrap()[0].0, "killed");
}

/// A resume reopens the conversation with `session/load` (the recorded ACP
/// id), goes live and takes a prompt — no run record behind it.
#[test]
fn a_server_free_resume_reloads_the_session() {
    let harness = start_fake("resume", Some(ResumeHandle::Acp("fake-session".to_string())));
    until("the live phase", || harness.session.phase() == Some(EnginePhase::Live));
    assert_eq!(harness.session.acp_session_id().as_deref(), Some("fake-session"));
    harness.session.send_prompt("stream".to_string());
    until("the tool event", || !events_of(&harness.sink, "tool").is_empty());
    harness.session.kill("killed");
    assert!(harness.session.wait_timeout(BUDGET).is_some());
}

/// EXP-1896: [`NoBackend`] that counts the Esc answers
/// ([`RunBackend::turn_started`]: Exponential's clears the run's computer-use
/// abort there).
struct EscAnswers {
    inner: NoBackend,
    answers: Arc<AtomicUsize>,
}

impl RunBackend for EscAnswers {
    fn end(&self, ctx: &SessionCtx, outcome: &str, child: Option<eagent::spawn::ChildExit>, error: Option<String>) {
        self.inner.end(ctx, outcome, child, error);
    }

    fn turn_started(&self, _session_id: &str) {
        self.answers.fetch_add(1, Ordering::SeqCst);
    }
}

/// EXP-1896: a turn opened by a row no person wrote (a child's report, a CI
/// wake, another session's message) does not answer an Esc that aborted the
/// run's computer use; the person's next message does.
#[test]
fn only_the_persons_message_answers_an_esc() {
    let answers = Arc::new(AtomicUsize::new(0));
    let counted = answers.clone();
    let harness = start_fake_with("esc-answer", None, move |host| {
        Arc::new(EscAnswers { inner: NoBackend::with_host(host), answers: counted })
    });
    let turns = |count: usize| {
        until("the turn's prompt", || harness.state.prompts.lock().unwrap().len() == count);
        until("the turn's end", || harness.session.turn_signal().is_idle());
    };

    // The person presses Esc (the abort lives in the backend), then says
    // nothing: a child's report arrives and runs a turn.
    harness.session.deliver_wake("r-1".to_string(), "stream".to_string());
    turns(1);
    assert_eq!(answers.load(Ordering::SeqCst), 0, "a report answers no Esc");

    // The person's own message is the answer.
    harness.session.deliver("m-1".to_string(), "stream".to_string());
    turns(2);
    assert_eq!(answers.load(Ordering::SeqCst), 1, "the person spoke");

    // And the next report leaves it alone again.
    harness.session.deliver_wake("r-2".to_string(), "stream".to_string());
    turns(3);
    assert_eq!(answers.load(Ordering::SeqCst), 1);

    harness.session.kill("killed");
    assert!(harness.session.wait_timeout(BUDGET).is_some());
}

/// #EXP-1514 (D 10): [`NoBackend`] that records the turn signal's edges
/// ([`RunBackend::turn`]) beside the Esc answers.
struct TurnEdges {
    inner: NoBackend,
    edges: Arc<Mutex<Vec<bool>>>,
    answers: Arc<AtomicUsize>,
}

impl RunBackend for TurnEdges {
    fn end(&self, ctx: &SessionCtx, outcome: &str, child: Option<eagent::spawn::ChildExit>, error: Option<String>) {
        self.inner.end(ctx, outcome, child, error);
    }

    fn turn_started(&self, _session_id: &str) {
        self.answers.fetch_add(1, Ordering::SeqCst);
    }

    fn turn(&self, _session_id: &str, running: bool) {
        self.edges.lock().unwrap().push(running);
    }
}

/// #EXP-1514 (D 10): EVERY turn reaches the backend once as it starts and
/// once as it ends, whoever opened it (a report as well as the person),
/// while the Esc answer stays the person's; the run's start (no turn yet)
/// is no edge.
#[test]
fn every_turn_is_one_start_and_one_end_edge() {
    let edges = Arc::new(Mutex::new(Vec::new()));
    let answers = Arc::new(AtomicUsize::new(0));
    let (heard, counted) = (edges.clone(), answers.clone());
    let harness = start_fake_with("turn-edges", None, move |host| {
        Arc::new(TurnEdges { inner: NoBackend::with_host(host), edges: heard, answers: counted })
    });
    until("the live phase", || harness.session.phase() == Some(EnginePhase::Live));
    assert!(edges.lock().unwrap().is_empty(), "no turn yet: {:?}", edges.lock().unwrap());
    let turns = |count: usize| {
        until("the turn's prompt", || harness.state.prompts.lock().unwrap().len() == count);
        until("the turn's end", || harness.session.turn_signal().is_idle());
        until("the end edge", || edges.lock().unwrap().len() == 2 * count);
    };
    harness.session.deliver_wake("r-1".to_string(), "stream".to_string());
    turns(1);
    harness.session.deliver("m-1".to_string(), "stream".to_string());
    turns(2);
    assert_eq!(*edges.lock().unwrap(), [true, false, true, false]);
    assert_eq!(answers.load(Ordering::SeqCst), 1, "the Esc answer stays the person's");
    harness.session.kill("killed");
    assert!(harness.session.wait_timeout(BUDGET).is_some());
    assert_eq!(edges.lock().unwrap().len(), 4, "a run that ends between turns has no edge left");
}

/// #EXP-2069: [`NoBackend`] whose turn note marks any turn that said
/// "Working" (the `stream` turn), recording every turn's texts.
struct TurnNotes {
    inner: NoBackend,
    seen: Arc<Mutex<Vec<Vec<String>>>>,
}

impl RunBackend for TurnNotes {
    fn end(&self, ctx: &SessionCtx, outcome: &str, child: Option<eagent::spawn::ChildExit>, error: Option<String>) {
        self.inner.end(ctx, outcome, child, error);
    }

    fn turn_note(&self, texts: &[String]) -> Option<String> {
        self.seen.lock().unwrap().push(texts.to_vec());
        texts.iter().any(|text| text.contains("Working")).then(|| "[view] NOTE".to_string())
    }
}

/// #EXP-2069: the backend's turn note leads the NEXT input the agent gets,
/// once, then a blank line; the feed's row stays the input alone. A `/`
/// command passes it on to the next real input; a fresh context drops it.
#[test]
fn a_turn_note_leads_the_next_input_once() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let heard = seen.clone();
    let harness = start_fake_with("turn-notes", None, move |host| {
        Arc::new(TurnNotes { inner: NoBackend::with_host(host), seen: heard })
    });
    until("the live phase", || harness.session.phase() == Some(EnginePhase::Live));
    let prompts = || harness.state.prompts.lock().unwrap().clone();
    let turn = |count: usize| {
        until("the turn's prompt", || prompts().len() == count);
        until("the turn's end", || harness.state.answered.lock().unwrap().len() == count && harness.session.turn_signal().is_idle());
    };
    harness.session.send_prompt("stream".to_string());
    turn(1);
    assert_eq!(*seen.lock().unwrap(), vec![vec!["Working on it".to_string()]]);
    harness.session.deliver("m-c".to_string(), "/foo".to_string());
    turn(2);
    assert_eq!(prompts()[1], "/foo", "a command never carries the note");
    harness.session.deliver("m-1".to_string(), "hello".to_string());
    turn(3);
    assert_eq!(prompts()[2], "[view] NOTE\n\nhello");
    let rows: Vec<serde_json::Value> = events_of(&harness.sink, "user_message");
    assert!(rows.iter().any(|row| row["text"] == "hello"), "{rows:?}");
    assert!(!rows.iter().any(|row| row["text"].as_str().is_some_and(|text| text.contains("NOTE"))), "{rows:?}");
    harness.session.deliver_wake("r-1".to_string(), "again".to_string());
    turn(4);
    assert_eq!(prompts()[3], "again", "once");
    harness.session.send_prompt("stream".to_string());
    turn(5);
    harness.session.deliver_fresh("m-f".to_string(), "the plan".to_string());
    turn(6);
    assert_eq!(prompts()[5], "the plan", "a fresh context has no last turn");
    harness.session.deliver("m-2".to_string(), "after".to_string());
    turn(7);
    assert_eq!(prompts()[6], "after", "dropped, not held");
    harness.session.kill("killed");
    assert!(harness.session.wait_timeout(BUDGET).is_some());
}

/// EXP-1910: a kill sent while the handshake is still open (a wedged CLI, a
/// long `session/load`) ends the run at once, and the seed prompt never goes
/// out. It used to sit unread until the handshake's own deadline (3 or 10
/// minutes).
#[test]
fn a_kill_during_the_handshake_ends_the_run_at_once() {
    let state = Arc::new(FakeState::default());
    state.mute_handshake.store(true, Ordering::SeqCst);
    let harness = start_adapter("mute-handshake", None, Some("stream".to_string()), state, FakeAgent::new, |host| {
        Arc::new(NoBackend::with_host(host))
    });
    until("the connecting phase", || harness.session.phase() == Some(EnginePhase::Connecting));
    harness.session.kill("killed");
    let exit = harness.session.wait_timeout(BUDGET).expect("the kill is read during the handshake");
    assert_eq!(exit.outcome, "killed");
    assert!(harness.state.prompts.lock().unwrap().is_empty(), "the seed never went out");
}

/// EXP-1910: an adapter that blows up on the engine thread.
struct PanickingAgent;

impl ConnectTo<Client> for PanickingAgent {
    fn connect_to(
        self,
        _client: impl ConnectTo<Agent>,
    ) -> impl std::future::Future<Output = Result<(), Error>> + Send {
        async move { panic!("the adapter blew up") }
    }
}

/// EXP-1910: a panic on the engine thread still runs the backend's end
/// sequence, once (grants, the publisher, the tickers and the row all hang
/// off it); it used to flip the exit slot and nothing else.
#[test]
fn an_engine_panic_still_runs_the_end_sequence() {
    let harness = start_adapter(
        "panic",
        None,
        None,
        Arc::new(FakeState::default()),
        |_, _| PanickingAgent,
        |host| Arc::new(NoBackend::with_host(host)),
    );
    let exit = harness.session.wait_timeout(BUDGET).expect("the run ends");
    assert_eq!(exit.error.as_deref(), Some("the engine thread panicked"));
    until("the host heard", || harness.host.count.load(Ordering::SeqCst) == 1);
}
