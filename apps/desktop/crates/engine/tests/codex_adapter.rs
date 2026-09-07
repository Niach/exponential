//! EXP-746 — the CodexAgent adapter driven end to end against a FAKE
//! app-server. Owned by lane E3.
//!
//! The fake replays `fixtures/codex/*.jsonl` — real frame shapes, recorded
//! from the installed codex 0.144.5 and the schema dump — and answers our
//! requests by id, which is exactly what a shell script replaying a file
//! cannot do. Everything above the fake is the production path: the same
//! `AppServer` router, the same adapter, a real ACP `Client` on the other end.
//!
//! What these tests exist to catch:
//!
//! - a notification that stops becoming a `SessionUpdate` (the whole feed),
//! - an approval that stops reaching the client, or whose answer stops
//!   reaching codex (codex then waits forever with no symptom),
//! - the STALE FENCE regressing: `turn/interrupt` returns `Ok` and late
//!   deltas keep arriving, and they must never reach the feed.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use agent_client_protocol::schema::v1::{
    CancelNotification, ClientCapabilities, ContentBlock, CreateElicitationRequest,
    CreateElicitationResponse, ElicitationAcceptAction, ElicitationContentValue,
    ElicitationMode, InitializeRequest, NewSessionRequest,
    PermissionOptionId, PromptRequest, RequestPermissionOutcome, RequestPermissionRequest,
    RequestPermissionResponse, SelectedPermissionOutcome, SessionConfigOptionValue, SessionId,
    SessionModeId, SessionNotification, SessionUpdate, SetSessionConfigOptionRequest,
    SetSessionModeRequest, StopReason, TextContent, ToolCallContent, ToolKind,
};
use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::{Client, ConnectionTo};
use engine::adapters::codex::{CodexAgent, CodexConnection};
use engine::adapters::codex_wire::{AppServer, LineSink};
use serde_json::{json, Value};

const SETTLE: Duration = Duration::from_secs(5);

// ---------------------------------------------------------------------------
// The fake app-server
// ---------------------------------------------------------------------------

struct FakeState {
    /// One script per `turn/start`, in order: the notifications that turn
    /// produces once it has been acknowledged.
    turns: VecDeque<Vec<Value>>,
    /// Replayed after `turn/interrupt` — the late frames the spike measured.
    late: Vec<Value>,
    /// Replayed once we answer a server request (codex resumes the turn).
    after_answer: Vec<Value>,
    /// Every method we were sent, in order.
    seen: Vec<String>,
    /// Every result the adapter sent back for a server request.
    answers: Vec<Value>,
    turn: u32,
    /// EXP-758: a method the fake answers only when the test says so, i.e. the
    /// slow app-server call a handler must not sit on the dispatch loop for.
    hold: Option<String>,
    /// The answers (and their replay frames) [`FakeServer::release`] owes.
    held: Vec<(Value, Value, Vec<Value>)>,
}

struct FakeServer {
    /// Taken by [`FakeServer::crash`]: this is the app-server's stdout, and
    /// dropping the last sender is exactly what a dead child looks like to the
    /// router.
    lines: Mutex<Option<flume::Sender<String>>>,
    state: Mutex<FakeState>,
}

impl FakeServer {
    fn new(turns: Vec<Vec<Value>>, late: Vec<Value>, after_answer: Vec<Value>) -> (Arc<FakeServer>, CodexConnection) {
        let (lines, incoming) = flume::unbounded();
        let fake = Arc::new(FakeServer {
            lines: Mutex::new(Some(lines)),
            state: Mutex::new(FakeState {
                turns: turns.into(),
                late,
                after_answer,
                seen: Vec::new(),
                answers: Vec::new(),
                turn: 0,
                hold: None,
                held: Vec::new(),
            }),
        });
        let sink: Arc<dyn LineSink> = fake.clone();
        let (server, notifications, requests) =
            AppServer::attach(incoming, sink).expect("the router starts");
        (
            fake,
            CodexConnection {
                server,
                notifications,
                requests,
                exit: None,
                pid: None,
            },
        )
    }

    /// EXP-758: hold every answer to `method` until [`FakeServer::release`].
    /// Nothing SLEEPS: a blocking fake would stall the single-threaded test
    /// runtime whichever side of the fix the handler is on, and prove
    /// nothing.
    fn hold(&self, method: &str) {
        if let Ok(mut state) = self.state.lock() {
            state.hold = Some(method.to_string());
        }
    }

    /// Answer everything [`FakeServer::hold`] held back.
    fn release(&self) {
        let held = match self.state.lock() {
            Ok(mut state) => {
                state.hold = None;
                std::mem::take(&mut state.held)
            }
            Err(_) => Vec::new(),
        };
        for (id, result, replay) in held {
            self.reply(&id, result);
            for frame in &replay {
                self.push(frame);
            }
        }
    }

    fn saw(&self, method: &str) -> bool {
        self.state
            .lock()
            .map(|state| state.seen.iter().any(|seen| seen == method))
            .unwrap_or(false)
    }

    fn answers(&self) -> Vec<Value> {
        self.state
            .lock()
            .map(|state| state.answers.clone())
            .unwrap_or_default()
    }

    fn push(&self, frame: &Value) {
        if let Ok(lines) = self.lines.lock() {
            if let Some(lines) = lines.as_ref() {
                let _ = lines.send(frame.to_string());
            }
        }
    }

    /// The app-server dies: stdout ends and nothing will ever answer again.
    fn crash(&self) {
        if let Ok(mut lines) = self.lines.lock() {
            lines.take();
        }
    }

    fn reply(&self, id: &Value, result: Value) {
        self.push(&json!({ "jsonrpc": "2.0", "id": id, "result": result }));
    }
}

impl LineSink for FakeServer {
    fn write_line(&self, line: &str) -> std::io::Result<()> {
        let frame: Value = serde_json::from_str(line).expect("the adapter writes JSON");
        let id = frame.get("id").cloned();
        let method = frame.get("method").and_then(Value::as_str).map(str::to_string);
        match (id, method) {
            // A response to one of the fake's own server requests.
            (Some(id), None) => {
                let after = {
                    let mut state = self.state.lock().expect("state");
                    state
                        .answers
                        .push(frame.get("result").cloned().unwrap_or(Value::Null));
                    let _ = id;
                    std::mem::take(&mut state.after_answer)
                };
                for frame in &after {
                    self.push(frame);
                }
            }
            // A request from the adapter.
            (Some(id), Some(method)) => {
                let replay = {
                    let mut state = self.state.lock().expect("state");
                    state.seen.push(method.clone());
                    match method.as_str() {
                        "turn/start" => {
                            state.turn += 1;
                            state.turns.pop_front().unwrap_or_default()
                        }
                        // `turn/steer` supersedes the live turn with a new id
                        // and takes the next script with it.
                        "turn/steer" => state.turns.pop_front().unwrap_or_default(),
                        "turn/interrupt" => std::mem::take(&mut state.late),
                        _ => Vec::new(),
                    }
                };
                let turn = self.state.lock().map(|state| state.turn).unwrap_or(1);
                let result = match method.as_str() {
                    "initialize" => json!({ "codexHome": "/tmp/codex", "userAgent": "fake" }),
                    "thread/start" | "thread/resume" => json!({
                        "thread": { "id": "thread_1", "cwd": "/work/tree" },
                        "model": "gpt-5.4-codex",
                        "reasoningEffort": "medium",
                    }),
                    "model/list" => json!({
                        "data": [{
                            "id": "gpt-5.4-codex",
                            "model": "gpt-5.4-codex",
                            "displayName": "Codex",
                            "isDefault": true,
                            "defaultReasoningEffort": "medium",
                            "supportedReasoningEfforts": [
                                { "reasoningEffort": "low", "description": "" },
                                { "reasoningEffort": "high", "description": "" },
                            ],
                            "additionalSpeedTiers": ["fast"],
                        }],
                        "nextCursor": Value::Null,
                    }),
                    "turn/start" => json!({ "turn": { "id": format!("turn_{turn}"), "status": "inProgress" } }),
                    "turn/steer" => json!({ "turnId": format!("turn_{turn}b") }),
                    _ => json!({}),
                };
                let held = self
                    .state
                    .lock()
                    .map(|state| state.hold.as_deref() == Some(method.as_str()))
                    .unwrap_or(false);
                if held {
                    if let Ok(mut state) = self.state.lock() {
                        state.held.push((id, result, replay));
                    }
                    return Ok(());
                }
                self.reply(&id, result);
                for frame in &replay {
                    self.push(frame);
                }
            }
            // A notification from the adapter (`initialized`).
            (None, Some(method)) => {
                if let Ok(mut state) = self.state.lock() {
                    state.seen.push(method);
                }
            }
            (None, None) => {}
        }
        Ok(())
    }
}

fn frames(name: &str) -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/codex")
        .join(name);
    let body = std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("{path:?}: {err}"));
    body.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("a fixture frame is JSON"))
        .collect()
}

fn spec() -> engine::adapters::AdapterSpec {
    engine::adapters::AdapterSpec {
        kind: engine::adapters::AdapterKind::Codex,
        agent: coding::AgentKind::Builtin(coding::CodingAgent::Codex),
        spawn: terminal::pty::SpawnSpec::new("codex"),
        options: coding::LaunchOptions {
            agent: coding::CodingAgent::Codex,
            model: String::new(),
            effort: String::new(),
            ultracode: false,
            plan_mode: false,
            external: None,
        },
        mcp: coding::AgentMcp::CodexOverrides {
            url: "https://example.test/api/mcp".to_string(),
            session_id: Some("row-1".to_string()),
        },
        cwd: PathBuf::from("/work/tree"),
        session_id: "row-1".to_string(),
        prompt: None,
        resume: None,
        personal_key: Some("expu_test".to_string()),
        reaper_settings_path: None,
        exit: engine::ChildExitLink::new(),
    }
}

/// Everything the client recorded, so an assertion can read the feed the way a
/// viewer would.
#[derive(Clone, Default)]
struct Recorded(Arc<Mutex<Vec<SessionUpdate>>>);

impl Recorded {
    fn push(&self, update: SessionUpdate) {
        if let Ok(mut updates) = self.0.lock() {
            updates.push(update);
        }
    }

    fn all(&self) -> Vec<SessionUpdate> {
        self.0.lock().map(|updates| updates.clone()).unwrap_or_default()
    }

    fn texts(&self) -> Vec<String> {
        self.all()
            .iter()
            .filter_map(|update| match update {
                SessionUpdate::AgentMessageChunk(chunk) | SessionUpdate::AgentThoughtChunk(chunk) => {
                    match &chunk.content {
                        ContentBlock::Text(text) => Some(text.text.clone()),
                        _ => None,
                    }
                }
                _ => None,
            })
            .collect()
    }
}

async fn settle(mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + SETTLE;
    while Instant::now() < deadline {
        if ready() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("the fake app-server never got there");
}

fn text(body: &str) -> ContentBlock {
    ContentBlock::Text(TextContent::new(body))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// EXP-754: every `CodexAgent` attaches a session to the PROCESS-GLOBAL live
/// usage registry (`coding::agent_usage::live`), so the tests run one at a
/// time — the session count a test reads is then its own.
static SESSION_LOCK: Mutex<()> = Mutex::new(());

fn one_session_at_a_time() -> std::sync::MutexGuard<'static, ()> {
    match SESSION_LOCK.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// What this machine's codex sessions have published so far.
fn live_usage() -> coding::agent_usage::live::LiveUsage {
    coding::agent_usage::live::snapshot(coding::CodingAgent::Codex).unwrap_or_default()
}

#[tokio::test]
async fn a_turn_becomes_tool_calls_narration_a_plan_and_usage() {
    let _session = one_session_at_a_time();
    let (fake, connection) =
        FakeServer::new(vec![frames("turn.jsonl")], Vec::new(), Vec::new());
    let agent = CodexAgent::with_connection(spec(), connection);
    let usage = agent.usage();
    let recorded = Recorded::default();
    let sink = recorded.clone();

    Client
        .builder()
        .on_receive_notification(
            async move |notification: SessionNotification, _cx| {
                sink.push(notification.update);
                Ok(())
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .connect_with(agent, async move |cx: ConnectionTo<agent_client_protocol::Agent>| {
            cx.send_request(
                InitializeRequest::new(ProtocolVersion::V1)
                    .client_capabilities(ClientCapabilities::new()),
            )
            .block_task()
            .await?;
            let session = cx
                .send_request(NewSessionRequest::new(PathBuf::from("/work/tree")))
                .block_task()
                .await?;
            // The three approval x sandbox presets are ACP MODES, and the
            // model/effort/fast chips are config options.
            let modes = session.modes.clone().expect("the presets are modes");
            assert_eq!(modes.available_modes.len(), 3);
            assert_eq!(modes.current_mode_id.0.as_ref(), "agent-full-access");
            let options = session.config_options.clone().expect("config options");
            let ids: Vec<String> = options.iter().map(|option| option.id.0.to_string()).collect();
            assert!(ids.contains(&"model".to_string()), "{ids:?}");
            assert!(ids.contains(&"reasoning_effort".to_string()), "{ids:?}");
            assert!(ids.contains(&"fast-mode".to_string()), "{ids:?}");

            let response = cx
                .send_request(PromptRequest::new(
                    session.session_id.clone(),
                    vec![text("run the tests")],
                ))
                .block_task()
                .await?;
            assert_eq!(response.stop_reason, StopReason::EndTurn);
            Ok(())
        })
        .await
        .expect("the session runs");

    let updates = recorded.all();
    let calls: Vec<&SessionUpdate> = updates
        .iter()
        .filter(|update| matches!(update, SessionUpdate::ToolCall(_)))
        .collect();
    let SessionUpdate::ToolCall(call) = calls.first().expect("a tool call") else {
        unreachable!()
    };
    assert_eq!(call.title, "cargo test -p engine");
    assert_eq!(call.kind, ToolKind::Execute);

    // The output rides as CONTENT: the client advertises `terminal: false`.
    let output = updates.iter().any(|update| match update {
        SessionUpdate::ToolCallUpdate(patch) => patch
            .fields
            .content
            .as_ref()
            .is_some_and(|content| matches!(content.first(), Some(ToolCallContent::Content(_)))),
        _ => false,
    });
    assert!(output, "the command output reached the feed");

    assert!(recorded.texts().iter().any(|text| text == "The tests pass."));
    assert!(updates
        .iter()
        .any(|update| matches!(update, SessionUpdate::Plan(plan) if plan.entries.len() == 1)));
    let usage_update = updates.iter().find_map(|update| match update {
        SessionUpdate::UsageUpdate(usage) => Some((usage.used, usage.size)),
        _ => None,
    });
    assert_eq!(usage_update, Some((1200, 200000)));
    // `account/rateLimits/updated` produces no feed event; it feeds the usage
    // sheet's windows instead.
    assert_eq!(usage.windows().len(), 1);
    assert_eq!(usage.windows()[0].percent, 4);
    // `initialized` is mandatory: the app-server answers nothing else until
    // it lands.
    assert!(fake.saw("initialized"));
    assert!(fake.saw("thread/start"));
    // The opt-out list is advisory (a fake ignores it), so the two frames we
    // never read must still produce nothing.
    assert!(recorded
        .texts()
        .iter()
        .all(|text| !text.contains("aGk=") && !text.contains("reverted")));
}

#[tokio::test]
async fn an_approval_becomes_a_permission_request_and_its_answer_reaches_codex() {
    let _session = one_session_at_a_time();
    let (fake, connection) = FakeServer::new(
        vec![frames("approval.jsonl")],
        Vec::new(),
        frames("approval-continued.jsonl"),
    );
    let agent = CodexAgent::with_connection(spec(), connection);
    let asked: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let seen = asked.clone();

    Client
        .builder()
        .on_receive_notification(
            async move |_notification: SessionNotification, _cx| Ok(()),
            agent_client_protocol::on_receive_notification!(),
        )
        .on_receive_request(
            async move |request: RequestPermissionRequest, responder, _cx| {
                // The card carries the command as its title and the options
                // carry ACP option ids, never keystrokes (D3).
                if let Ok(mut seen) = seen.lock() {
                    seen.push(request.tool_call.fields.title.clone().unwrap_or_default());
                    seen.extend(
                        request
                            .options
                            .iter()
                            .map(|option| option.option_id.0.to_string()),
                    );
                }
                responder.respond(RequestPermissionResponse::new(
                    RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
                        PermissionOptionId::new("allow_once"),
                    )),
                ))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_with(agent, async move |cx: ConnectionTo<agent_client_protocol::Agent>| {
            cx.send_request(
                InitializeRequest::new(ProtocolVersion::V1)
                    .client_capabilities(ClientCapabilities::new()),
            )
            .block_task()
            .await?;
            let session = cx
                .send_request(NewSessionRequest::new(PathBuf::from("/work/tree")))
                .block_task()
                .await?;
            let response = cx
                .send_request(PromptRequest::new(
                    session.session_id.clone(),
                    vec![text("clean the build")],
                ))
                .block_task()
                .await?;
            assert_eq!(response.stop_reason, StopReason::EndTurn);
            Ok(())
        })
        .await
        .expect("the session runs");

    let asked = asked.lock().expect("the card").clone();
    assert_eq!(asked.first().map(String::as_str), Some("rm -rf build"));
    assert!(asked.contains(&"allow_once".to_string()), "{asked:?}");
    assert!(asked.contains(&"cancel".to_string()), "{asked:?}");
    // The decision has to land on codex, or the turn hangs forever with no
    // symptom at all.
    assert_eq!(fake.answers(), vec![json!({ "decision": "accept" })]);
}

#[tokio::test]
async fn a_cancelled_turn_drops_every_frame_that_arrives_after_it() {
    let _session = one_session_at_a_time();
    let (fake, connection) = FakeServer::new(
        vec![frames("interrupt.jsonl"), frames("second-turn.jsonl")],
        frames("interrupt-late.jsonl"),
        Vec::new(),
    );
    let agent = CodexAgent::with_connection(spec(), connection);
    let recorded = Recorded::default();
    let sink = recorded.clone();
    let watcher = recorded.clone();
    let fence = fake.clone();

    Client
        .builder()
        .on_receive_notification(
            async move |notification: SessionNotification, _cx| {
                sink.push(notification.update);
                Ok(())
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .connect_with(agent, async move |cx: ConnectionTo<agent_client_protocol::Agent>| {
            cx.send_request(
                InitializeRequest::new(ProtocolVersion::V1)
                    .client_capabilities(ClientCapabilities::new()),
            )
            .block_task()
            .await?;
            let session = cx
                .send_request(NewSessionRequest::new(PathBuf::from("/work/tree")))
                .block_task()
                .await?;

            let prompt = cx.send_request(PromptRequest::new(
                session.session_id.clone(),
                vec![text("start something long")],
            ));
            settle(|| watcher.texts().iter().any(|text| text == "before the interrupt")).await;
            cx.send_notification(CancelNotification::new(session.session_id.clone()))?;
            let cancelled = prompt.block_task().await?;
            assert_eq!(cancelled.stop_reason, StopReason::Cancelled);
            settle(|| fence.saw("turn/interrupt")).await;

            // A second turn proves the pump kept running AND, because one pump
            // drains one channel in order, that the late frames were already
            // processed by the time this one's text arrives.
            let second = cx
                .send_request(PromptRequest::new(
                    session.session_id.clone(),
                    vec![text("carry on")],
                ))
                .block_task()
                .await?;
            assert_eq!(second.stop_reason, StopReason::EndTurn);
            Ok(())
        })
        .await
        .expect("the session runs");

    let texts = recorded.texts();
    assert!(texts.iter().any(|text| text == "before the interrupt"), "{texts:?}");
    assert!(texts.iter().any(|text| text == "after the fence"), "{texts:?}");
    // THE FENCE: `turn/interrupt` answers `Ok` and codex keeps talking. Three
    // late deltas were measured in the spike; not one may reach the feed.
    assert!(
        texts.iter().all(|text| text != "after the interrupt"),
        "a cancelled turn leaked into the feed: {texts:?}"
    );
}

#[tokio::test]
async fn a_request_for_user_input_becomes_one_elicitation_with_a_property_per_question() {
    let _session = one_session_at_a_time();
    let (fake, connection) = FakeServer::new(
        vec![frames("question.jsonl")],
        Vec::new(),
        frames("question-continued.jsonl"),
    );
    let agent = CodexAgent::with_connection(spec(), connection);
    let form: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let seen = form.clone();

    Client
        .builder()
        .on_receive_notification(
            async move |_notification: SessionNotification, _cx| Ok(()),
            agent_client_protocol::on_receive_notification!(),
        )
        .on_receive_request(
            async move |request: CreateElicitationRequest, responder, _cx| {
                // ONE elicitation, one property per question: codex sends every
                // question of a request under the same itemId, so a permission
                // request each would collide on the id.
                let ElicitationMode::Form(form) = &request.mode else {
                    panic!("expected a form elicitation");
                };
                if let Ok(mut seen) = seen.lock() {
                    seen.extend(form.requested_schema.properties.keys().cloned());
                }
                let mut content = std::collections::BTreeMap::new();
                content.insert(
                    "q1".to_string(),
                    ElicitationContentValue::String("spaces".to_string()),
                );
                responder.respond(CreateElicitationResponse::new(
                    ElicitationAcceptAction::new().content(content),
                ))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_with(agent, async move |cx: ConnectionTo<agent_client_protocol::Agent>| {
            cx.send_request(
                InitializeRequest::new(ProtocolVersion::V1)
                    .client_capabilities(ClientCapabilities::new()),
            )
            .block_task()
            .await?;
            let session = cx
                .send_request(NewSessionRequest::new(PathBuf::from("/work/tree")))
                .block_task()
                .await?;
            let response = cx
                .send_request(PromptRequest::new(
                    session.session_id.clone(),
                    vec![text("reformat the file")],
                ))
                .block_task()
                .await?;
            assert_eq!(response.stop_reason, StopReason::EndTurn);
            Ok(())
        })
        .await
        .expect("the session runs");

    assert_eq!(form.lock().expect("the form").clone(), vec!["q1".to_string()]);
    // The answer goes back keyed by the QUESTION id, in codex's own shape.
    assert_eq!(
        fake.answers(),
        vec![json!({ "answers": { "q1": { "answers": ["spaces"] } } })]
    );
}

#[tokio::test]
async fn steering_supersedes_the_live_turn_and_resolves_both_prompts() {
    let _session = one_session_at_a_time();
    let (fake, connection) = FakeServer::new(
        vec![frames("interrupt.jsonl"), frames("steer.jsonl")],
        Vec::new(),
        Vec::new(),
    );
    let agent = CodexAgent::with_connection(spec(), connection);
    let recorded = Recorded::default();
    let sink = recorded.clone();
    let watcher = recorded.clone();

    Client
        .builder()
        .on_receive_notification(
            async move |notification: SessionNotification, _cx| {
                sink.push(notification.update);
                Ok(())
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .connect_with(agent, async move |cx: ConnectionTo<agent_client_protocol::Agent>| {
            cx.send_request(
                InitializeRequest::new(ProtocolVersion::V1)
                    .client_capabilities(ClientCapabilities::new()),
            )
            .block_task()
            .await?;
            let session = cx
                .send_request(NewSessionRequest::new(PathBuf::from("/work/tree")))
                .block_task()
                .await?;

            let first = cx.send_request(PromptRequest::new(
                session.session_id.clone(),
                vec![text("start something long")],
            ));
            settle(|| watcher.texts().iter().any(|text| text == "before the interrupt")).await;
            // A prompt while a turn is live IS the steer: it rides
            // `turn/steer`, whose expectedTurnId is a precondition.
            let second = cx.send_request(PromptRequest::new(
                session.session_id.clone(),
                vec![text("actually, do this instead")],
            ));
            let second = second.block_task().await?;
            assert_eq!(second.stop_reason, StopReason::EndTurn);
            // The prompt parked on the SUPERSEDED turn must resolve too, or a
            // steered session leaks a request that never answers.
            let first = first.block_task().await?;
            assert_eq!(first.stop_reason, StopReason::EndTurn);
            Ok(())
        })
        .await
        .expect("the session runs");

    assert!(fake.saw("turn/steer"));
    assert!(recorded.texts().iter().any(|text| text == "steered mid turn"));
}

#[tokio::test]
async fn the_app_server_going_away_closes_the_connection() {
    let _session = one_session_at_a_time();
    // The crash path: codex is gone (OOM, `kill -9`, the reaper) while the
    // client still holds the connection. Nothing but this close ends the run —
    // the bye, the heartbeat and `coding::end_session` all hang off it, so a
    // connection that stays open is a session that stays "coding now" forever
    // (EXP-746 E1).
    let (fake, connection) = FakeServer::new(vec![frames("turn.jsonl")], Vec::new(), Vec::new());
    let agent = CodexAgent::with_connection(spec(), connection);
    let crashing = fake.clone();

    let driven = Client
        .builder()
        .on_receive_notification(
            async move |_notification: SessionNotification, _cx| Ok(()),
            agent_client_protocol::on_receive_notification!(),
        )
        .connect_with(agent, async move |cx: ConnectionTo<agent_client_protocol::Agent>| {
            cx.send_request(
                InitializeRequest::new(ProtocolVersion::V1)
                    .client_capabilities(ClientCapabilities::new()),
            )
            .block_task()
            .await?;
            let session = cx
                .send_request(NewSessionRequest::new(PathBuf::from("/work/tree")))
                .block_task()
                .await?;
            let response = cx
                .send_request(PromptRequest::new(
                    session.session_id.clone(),
                    vec![text("run the tests")],
                ))
                .block_task()
                .await?;
            assert_eq!(response.stop_reason, StopReason::EndTurn);
            // Between turns, the way a crash usually lands.
            crashing.crash();
            // The host's own loop: it waits on the adapter and nothing else.
            cx.incoming_closed().await;
            Ok(())
        });

    tokio::time::timeout(Duration::from_secs(30), driven)
        .await
        .expect("the dead app-server closes the connection")
        .expect("the session runs");
}

/// EXP-754 — a live codex session IS this machine's usage source: every
/// `account/rateLimits/updated` frame publishes into
/// `coding::agent_usage::live` (which is what lets the usage poller skip
/// spawning a second `codex app-server`), and the run's END releases the
/// session slot even though the host still holds the handle — a dead session
/// must stop answering for numbers it can no longer refresh.
#[tokio::test]
async fn a_codex_session_publishes_live_usage_and_detaches_on_end() {
    let _session = one_session_at_a_time();
    let (fake, connection) = FakeServer::new(vec![frames("turn.jsonl")], Vec::new(), Vec::new());
    let agent = CodexAgent::with_connection(spec(), connection);
    // The handle a host keeps for the whole run: releasing the registry slot
    // is the SESSION's end, not this handle's drop.
    let usage = agent.usage();
    let during = Arc::new(Mutex::new(None));
    let recorded = during.clone();
    let ending = fake.clone();

    let driven = Client
        .builder()
        .on_receive_notification(
            async move |_notification: SessionNotification, _cx| Ok(()),
            agent_client_protocol::on_receive_notification!(),
        )
        .connect_with(agent, async move |cx: ConnectionTo<agent_client_protocol::Agent>| {
            cx.send_request(
                InitializeRequest::new(ProtocolVersion::V1)
                    .client_capabilities(ClientCapabilities::new()),
            )
            .block_task()
            .await?;
            let session = cx
                .send_request(NewSessionRequest::new(PathBuf::from("/work/tree")))
                .block_task()
                .await?;
            cx.send_request(PromptRequest::new(
                session.session_id.clone(),
                vec![text("run the tests")],
            ))
            .block_task()
            .await?;
            settle(|| !live_usage().windows.is_empty()).await;
            if let Ok(mut slot) = recorded.lock() {
                *slot = Some(live_usage());
            }
            // The session ends the way every run ends: the app-server goes.
            ending.crash();
            cx.incoming_closed().await;
            Ok(())
        });

    tokio::time::timeout(Duration::from_secs(30), driven)
        .await
        .expect("the session ends")
        .expect("the session runs");

    let during = during
        .lock()
        .expect("the recorded snapshot")
        .clone()
        .expect("a live snapshot");
    assert_eq!(during.windows.first().map(|window| window.percent), Some(4));
    assert_eq!(
        during.windows,
        usage.windows(),
        "the adapter's own slot and the machine registry agree"
    );
    assert!(during.sessions >= 1, "a running session holds a slot");

    // Ended: the slot is back even though `usage` is still alive, and the
    // numbers stay (the collector ages them out by their own stamp).
    settle(|| live_usage().sessions == 0).await;
    assert_eq!(live_usage().windows, usage.windows());
}

// ---------------------------------------------------------------------------
// EXP-758: handlers off the dispatch loop, the elicitation fence, and what a
// failed turn stops with
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_slow_thread_start_does_not_block_the_dispatch_loop() {
    let _session = one_session_at_a_time();
    // EXP-758: `session/new` is `thread/start` plus up to eight `model/list`
    // pages, each with a 120 s deadline. Run inline it holds the dispatch
    // loop, and a handler on the dispatch loop is a `session/cancel` that
    // cannot be delivered, which is the crate's one hard rule.
    let (fake, connection) = FakeServer::new(vec![frames("turn.jsonl")], Vec::new(), Vec::new());
    fake.hold("thread/start");
    let agent = CodexAgent::with_connection(spec(), connection);
    let watcher = fake.clone();

    Client
        .builder()
        .on_receive_notification(
            async move |_notification: SessionNotification, _cx| Ok(()),
            agent_client_protocol::on_receive_notification!(),
        )
        .connect_with(agent, async move |cx: ConnectionTo<agent_client_protocol::Agent>| {
            cx.send_request(
                InitializeRequest::new(ProtocolVersion::V1)
                    .client_capabilities(ClientCapabilities::new()),
            )
            .block_task()
            .await?;
            let opening = tokio::spawn({
                let cx = cx.clone();
                async move {
                    cx.send_request(NewSessionRequest::new(PathBuf::from("/work/tree")))
                        .block_task()
                        .await
                }
            });
            settle(|| watcher.saw("thread/start")).await;

            // The app-server has not answered and will not until we say so.
            // Another request has to be dispatched anyway.
            let answered = tokio::time::timeout(
                SETTLE,
                cx.send_request(SetSessionModeRequest::new(
                    SessionId::new("thread_1"),
                    SessionModeId::new("agent"),
                ))
                .block_task(),
            )
            .await;
            assert!(
                answered.is_ok(),
                "a request behind a pending thread/start was never dispatched"
            );
            answered.expect("dispatched").expect("session/set_mode answers");

            watcher.release();
            opening.await.expect("the open task runs").expect("the session opens");
            Ok(())
        })
        .await
        .expect("the session runs");
}

#[tokio::test]
async fn a_cancel_resolves_an_open_question_the_way_it_resolves_an_approval() {
    let _session = one_session_at_a_time();
    // EXP-758: a `requestUserInput` was raised OUTSIDE the cancel fence, so it
    // never entered `in_flight`, so `session/cancel` answered every approval
    // and left this one open, with the app-server waiting on a turn that was
    // already gone.
    let (fake, connection) =
        FakeServer::new(vec![frames("question.jsonl")], Vec::new(), Vec::new());
    let agent = CodexAgent::with_connection(spec(), connection);
    // A person who never answers: the responder is parked, exactly like an
    // unanswered card on screen.
    let parked: Arc<Mutex<Vec<CreateElicitationRequest>>> = Arc::new(Mutex::new(Vec::new()));
    let seen = parked.clone();
    let watcher = fake.clone();

    let stop = Client
        .builder()
        .on_receive_notification(
            async move |_notification: SessionNotification, _cx| Ok(()),
            agent_client_protocol::on_receive_notification!(),
        )
        .on_receive_request(
            async move |request: CreateElicitationRequest, responder, _cx| {
                if let Ok(mut seen) = seen.lock() {
                    seen.push(request);
                }
                // Never answered, and never dropped: the fence has to be what
                // resolves this.
                std::mem::forget(responder);
                Ok(())
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_with(agent, async move |cx: ConnectionTo<agent_client_protocol::Agent>| {
            cx.send_request(
                InitializeRequest::new(ProtocolVersion::V1)
                    .client_capabilities(ClientCapabilities::new()),
            )
            .block_task()
            .await?;
            let session = cx
                .send_request(NewSessionRequest::new(PathBuf::from("/work/tree")))
                .block_task()
                .await?;
            let prompting = tokio::spawn({
                let cx = cx.clone();
                let session_id = session.session_id.clone();
                async move {
                    cx.send_request(PromptRequest::new(session_id, vec![text("reformat the file")]))
                        .block_task()
                        .await
                }
            });
            settle(|| parked.lock().map(|parked| !parked.is_empty()).unwrap_or(false)).await;
            cx.send_notification(CancelNotification::new(session.session_id.clone()))?;
            // The fence answers the question codex is still waiting on.
            settle(|| !watcher.answers().is_empty()).await;
            let stop = prompting.await.expect("the prompt task runs")?.stop_reason;
            Ok(stop)
        })
        .await
        .expect("the session runs");

    assert_eq!(stop, StopReason::Cancelled);
    // codex's own "no answers" shape, the same one an approval's cancel uses.
    assert_eq!(fake.answers(), vec![json!({ "answers": {} })]);
}

#[tokio::test]
async fn a_failed_turn_stops_terminally_without_claiming_a_refusal() {
    let _session = one_session_at_a_time();
    // EXP-758: `turn/failed` is not the model DECLINING. ACP attaches a
    // behaviour to `Refusal` (the prompt and everything after it leaves the
    // next context) that is simply untrue of a provider error, and codex
    // keeps that history. The error itself already rode the feed.
    let (_fake, connection) =
        FakeServer::new(vec![frames("turn-failed.jsonl")], Vec::new(), Vec::new());
    let agent = CodexAgent::with_connection(spec(), connection);
    let recorded = Recorded::default();
    let sink = recorded.clone();

    let stop = Client
        .builder()
        .on_receive_notification(
            async move |notification: SessionNotification, _cx| {
                sink.push(notification.update);
                Ok(())
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .connect_with(agent, async move |cx: ConnectionTo<agent_client_protocol::Agent>| {
            cx.send_request(
                InitializeRequest::new(ProtocolVersion::V1)
                    .client_capabilities(ClientCapabilities::new()),
            )
            .block_task()
            .await?;
            let session = cx
                .send_request(NewSessionRequest::new(PathBuf::from("/work/tree")))
                .block_task()
                .await?;
            let response = cx
                .send_request(PromptRequest::new(
                    session.session_id.clone(),
                    vec![text("run the tests")],
                ))
                .block_task()
                .await?;
            Ok(response.stop_reason)
        })
        .await
        .expect("the session runs");

    assert_eq!(stop, StopReason::EndTurn);
    assert_ne!(stop, StopReason::Refusal);
    // And the reason is in the feed, which is where it belongs.
    assert!(
        recorded
            .texts()
            .iter()
            .any(|text| text.contains("the model provider returned 500")),
        "{:?}",
        recorded.texts()
    );
}

#[tokio::test]
async fn a_slow_config_update_does_not_block_the_dispatch_loop() {
    let _session = one_session_at_a_time();
    // EXP-758: the other handler that ran an app-server call inline.
    // `collaboration_mode` is a `thread/settings/update` round trip, and the
    // rule is the same one the prompt handler already follows.
    let (fake, connection) = FakeServer::new(vec![frames("turn.jsonl")], Vec::new(), Vec::new());
    let agent = CodexAgent::with_connection(spec(), connection);
    let watcher = fake.clone();

    Client
        .builder()
        .on_receive_notification(
            async move |_notification: SessionNotification, _cx| Ok(()),
            agent_client_protocol::on_receive_notification!(),
        )
        .connect_with(agent, async move |cx: ConnectionTo<agent_client_protocol::Agent>| {
            cx.send_request(
                InitializeRequest::new(ProtocolVersion::V1)
                    .client_capabilities(ClientCapabilities::new()),
            )
            .block_task()
            .await?;
            let session = cx
                .send_request(NewSessionRequest::new(PathBuf::from("/work/tree")))
                .block_task()
                .await?;
            watcher.hold("thread/settings/update");
            let updating = tokio::spawn({
                let cx = cx.clone();
                let session_id = session.session_id.clone();
                async move {
                    cx.send_request(SetSessionConfigOptionRequest::new(
                        session_id,
                        "collaboration_mode",
                        SessionConfigOptionValue::value_id("plan"),
                    ))
                    .block_task()
                    .await
                }
            });
            settle(|| watcher.saw("thread/settings/update")).await;

            let answered = tokio::time::timeout(
                SETTLE,
                cx.send_request(SetSessionModeRequest::new(
                    session.session_id.clone(),
                    SessionModeId::new("agent"),
                ))
                .block_task(),
            )
            .await;
            assert!(
                answered.is_ok(),
                "a request behind a pending thread/settings/update was never dispatched"
            );
            answered.expect("dispatched").expect("session/set_mode answers");

            watcher.release();
            updating.await.expect("the update task runs").expect("the option is set");
            Ok(())
        })
        .await
        .expect("the session runs");
}
