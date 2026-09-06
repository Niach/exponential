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
    RequestPermissionResponse, SelectedPermissionOutcome, SessionNotification, SessionUpdate,
    StopReason, TextContent, ToolCallContent, ToolKind,
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
}

struct FakeServer {
    lines: flume::Sender<String>,
    state: Mutex<FakeState>,
}

impl FakeServer {
    fn new(turns: Vec<Vec<Value>>, late: Vec<Value>, after_answer: Vec<Value>) -> (Arc<FakeServer>, CodexConnection) {
        let (lines, incoming) = flume::unbounded();
        let fake = Arc::new(FakeServer {
            lines,
            state: Mutex::new(FakeState {
                turns: turns.into(),
                late,
                after_answer,
                seen: Vec::new(),
                answers: Vec::new(),
                turn: 0,
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
            },
        )
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
        let _ = self.lines.send(frame.to_string());
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

#[tokio::test]
async fn a_turn_becomes_tool_calls_narration_a_plan_and_usage() {
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
