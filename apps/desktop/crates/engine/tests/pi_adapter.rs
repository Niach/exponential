//! EXP-746 (lane E4) — PiAgent driven end to end against a fake
//! `pi --mode rpc`.
//!
//! The fake is a real child process (`tests/fixtures/pi/fake-pi.sh`) replaying
//! `tests/fixtures/pi/*`, so these tests cover the whole adapter: the spawn
//! recipe, the LF-only framing (one fixture line carries a U+2028, which a
//! naive reader would split in half), the response registry, the event
//! mapping, and the `extension_ui_request` round trip that is pi's only
//! interactive channel.
//!
//! Nothing here needs `pi` itself to be installed, so it runs in CI.
//!
//! Unix-only, like `transport.rs`'s own child tests: the fake is a `/bin/sh`
//! script, and pi ships no Windows build to test against anyway.
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agent_client_protocol::schema::v1::{
    AgentNotification, AgentRequest, ClientResponse, ContentBlock, CreateElicitationResponse,
    InitializeRequest, LoadSessionRequest, NewSessionRequest, PromptRequest,
    RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SelectedPermissionOutcome, SessionConfigOptionValue, SessionId, SessionNotification,
    SessionUpdate, SetSessionConfigOptionRequest, StopReason, ToolCallContent, ToolKind,
};
use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::{Agent, Client, ConnectionTo, Error};
use engine::adapters::pi::PiAgent;
use engine::adapters::{AdapterKind, AdapterSpec};
use serde_json::{json, Value};
use terminal::pty::SpawnSpec;

/// The session id the `get_state` fixture reports — pi is file-path keyed, so
/// its session FILE is both the ACP session id and the resume handle.
const FIXTURE_SESSION_FILE: &str = "/sessions/2026-09-06_exp746.jsonl";

/// Everything the client saw.
#[derive(Default)]
struct Recorder {
    updates: Mutex<Vec<SessionNotification>>,
    permissions: Mutex<Vec<RequestPermissionRequest>>,
}

impl Recorder {
    fn updates(&self) -> Vec<SessionUpdate> {
        self.updates
            .lock()
            .expect("the recorder is not poisoned")
            .iter()
            .map(|notification| notification.update.clone())
            .collect()
    }
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pi")
}

/// A scratch worktree with one file the fake pi "edits".
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("exp746-pi-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("the scratch worktree is writable");
    std::fs::write(dir.join("notes.md"), "before\n").expect("the edited file is writable");
    dir
}

fn adapter(cwd: &Path) -> PiAgent {
    let fixtures = fixtures();
    let spawn = SpawnSpec::new(fixtures.join("fake-pi.sh").display().to_string())
        .cwd(cwd)
        .env("EXP_FAKE_PI_DIR", fixtures.display().to_string())
        .env(
            "EXP_FAKE_PI_EDIT",
            cwd.join("notes.md").display().to_string(),
        );
    let mut options = coding::LaunchOptions::defaults(&coding::Settings::default());
    options.agent = coding::CodingAgent::Pi;
    PiAgent::new(AdapterSpec {
        kind: AdapterKind::Pi,
        agent: coding::AgentKind::Builtin(coding::CodingAgent::Pi),
        spawn,
        options,
        mcp: coding::AgentMcp::PiExtension,
        cwd: cwd.to_path_buf(),
        session_id: "sess-1".to_string(),
        prompt: None,
        resume: None,
        personal_key: None,
        reaper_settings_path: None,
    })
    .expect("the fake pi spawns")
}

/// Drive one client connection over the adapter. `main` runs with the ACP
/// connection; everything it returns comes back to the test.
fn drive<T, F>(cwd: &Path, recorder: Arc<Recorder>, main: F) -> T
where
    F: AsyncFnOnce(ConnectionTo<Agent>) -> Result<T, Error>,
{
    let agent = adapter(cwd);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("a test runtime starts");
    runtime.block_on(async move {
        let connection = Client
            .builder()
            .name("exponential-test")
            .on_receive_notification(
                {
                    let recorder = Arc::clone(&recorder);
                    async move |notification: AgentNotification, _cx| {
                        if let AgentNotification::SessionNotification(notification) = notification {
                            recorder
                                .updates
                                .lock()
                                .expect("the recorder is not poisoned")
                                .push(notification);
                        }
                        Ok(())
                    }
                },
                agent_client_protocol::on_receive_notification!(),
            )
            .on_receive_request(
                {
                    let recorder = Arc::clone(&recorder);
                    async move |request: AgentRequest, responder, _cx| {
                        let responder = responder.cast::<ClientResponse>();
                        match request {
                            AgentRequest::RequestPermissionRequest(request) => {
                                let option = request.options[0].option_id.clone();
                                recorder
                                    .permissions
                                    .lock()
                                    .expect("the recorder is not poisoned")
                                    .push(request);
                                responder.respond(ClientResponse::RequestPermissionResponse(
                                    RequestPermissionResponse::new(
                                        RequestPermissionOutcome::Selected(
                                            SelectedPermissionOutcome::new(option),
                                        ),
                                    ),
                                ))
                            }
                            AgentRequest::CreateElicitationRequest(_) => responder.respond(
                                ClientResponse::CreateElicitationResponse(
                                    CreateElicitationResponse::new(
                                        agent_client_protocol::schema::v1::ElicitationAction::Cancel,
                                    ),
                                ),
                            ),
                            _ => responder.respond_with_internal_error("not implemented in tests"),
                        }
                    }
                },
                agent_client_protocol::on_receive_request!(),
            )
            .connect_with(agent, async move |cx: ConnectionTo<Agent>| {
                // A wedged adapter must fail the test, never hang the suite.
                match tokio::time::timeout(Duration::from_secs(30), main(cx)).await {
                    Ok(result) => result,
                    Err(_) => Err(Error::internal_error().data(json!("the adapter never answered"))),
                }
            });
        connection.await
    })
    .expect("the connection runs")
}

#[test]
fn a_pi_turn_maps_onto_acp_updates_and_answers_a_confirm() {
    let cwd = scratch("turn");
    let recorder = Arc::new(Recorder::default());
    let (session_meta, session_id, stop) = drive(&cwd, Arc::clone(&recorder), {
        let cwd = cwd.clone();
        async move |cx: ConnectionTo<Agent>| {
            let initialize = cx
                .send_request(
                    InitializeRequest::new(ProtocolVersion::V1)
                        .client_capabilities(engine::client_capabilities()),
                )
                .block_task()
                .await?;
            assert!(
                initialize.agent_capabilities.load_session,
                "pi replays a session file, so load_session is advertised"
            );
            let session = cx
                .send_request(NewSessionRequest::new(cwd.clone()))
                .block_task()
                .await?;
            let prompt = cx
                .send_request(PromptRequest::new(
                    session.session_id.clone(),
                    vec![ContentBlock::from("edit the notes")],
                ))
                .block_task()
                .await?;
            Ok((
                serde_json::to_value(&session).expect("the response serializes"),
                session.session_id,
                prompt.stop_reason,
            ))
        }
    });

    // The ACP session id IS pi's session file (D8), and the same value rides
    // the meta as the agent-native handle.
    assert_eq!(session_id.0.as_ref(), FIXTURE_SESSION_FILE);
    assert_eq!(
        session_meta["_meta"],
        json!({ "exp.nativeSessionId": FIXTURE_SESSION_FILE })
    );
    let option_ids: Vec<&Value> = session_meta["configOptions"]
        .as_array()
        .expect("the new session carries config options")
        .iter()
        .map(|option| &option["id"])
        .collect();
    assert_eq!(
        option_ids,
        vec![
            &json!("model"),
            &json!("thinking_level"),
            &json!("steering_mode"),
            &json!("follow_up_mode"),
            &json!("auto_compaction"),
        ]
    );
    assert_eq!(stop, StopReason::EndTurn);

    let updates = recorder.updates();

    // A thought and a message, both keyed by the assistant message so the
    // mapper's coalescer can fold them.
    let thought = updates
        .iter()
        .find_map(|update| match update {
            SessionUpdate::AgentThoughtChunk(chunk) => Some(chunk),
            _ => None,
        })
        .expect("the turn streamed a thought");
    assert_eq!(
        serde_json::to_value(thought).expect("the chunk serializes"),
        json!({
            "content": { "type": "text", "text": "weighing the edit" },
            "messageId": "resp-1"
        })
    );
    let message = updates
        .iter()
        .find_map(|update| match update {
            SessionUpdate::AgentMessageChunk(chunk) => Some(chunk),
            _ => None,
        })
        .expect("the turn streamed a message");
    // U+2028 is legal inside a JSON string: an LF-only reader keeps the frame
    // whole, and the delta arrives with the separator intact.
    assert_eq!(
        serde_json::to_value(message).expect("the chunk serializes"),
        json!({
            "content": { "type": "text", "text": "first\u{2028}second" },
            "messageId": "resp-1"
        })
    );

    // The tool call surfaces with its kind and the file it names.
    let call = updates
        .iter()
        .find_map(|update| match update {
            SessionUpdate::ToolCall(call) => Some(call),
            _ => None,
        })
        .expect("the turn surfaced a tool call");
    assert_eq!(call.tool_call_id.0.as_ref(), "call-1");
    assert_eq!(call.kind, ToolKind::Edit);
    assert_eq!(call.locations[0].path, cwd.join("notes.md"));

    // pi reports no patch of its own, so the adapter's before/after snapshot
    // IS the diff card.
    let diff = updates
        .iter()
        .find_map(|update| match update {
            SessionUpdate::ToolCallUpdate(update) => update
                .fields
                .content
                .as_ref()
                .and_then(|content| content.first())
                .and_then(|content| match content {
                    ToolCallContent::Diff(diff) => Some(diff),
                    _ => None,
                }),
            _ => None,
        })
        .expect("the edit produced a diff");
    assert_eq!(diff.path, cwd.join("notes.md"));
    assert_eq!(diff.old_text.as_deref(), Some("before\n"));
    assert_eq!(diff.new_text, "after\n");

    // pi is the ONE agent that reports money.
    let usage = updates
        .iter()
        .find_map(|update| match update {
            SessionUpdate::UsageUpdate(usage) => Some(usage),
            _ => None,
        })
        .expect("the turn reported usage");
    assert_eq!(
        serde_json::to_value(usage).expect("the usage serializes"),
        json!({
            "used": 1234,
            "size": 272_000,
            "cost": { "amount": 0.0125, "currency": "USD" }
        })
    );

    // The command catalog: pi's verb-only commands first, then whatever
    // `get_commands` advertised, deduped by name.
    let commands = updates
        .iter()
        .find_map(|update| match update {
            SessionUpdate::AvailableCommandsUpdate(update) => Some(update),
            _ => None,
        })
        .expect("the session published its commands");
    let names: Vec<&str> = commands
        .available_commands
        .iter()
        .map(|command| command.name.as_str())
        .collect();
    assert_eq!(
        names,
        vec![
            "compact", "new", "model", "thinking", "name", "fork", "clone", "export", "review"
        ]
    );

    // The dialog reached the client as an answerable permission, and the
    // answer let pi finish the turn (the tool end and settle only arrive
    // after the fake reads our response).
    let permissions = recorder
        .permissions
        .lock()
        .expect("the recorder is not poisoned");
    assert_eq!(permissions.len(), 1);
    assert_eq!(
        permissions[0].tool_call.fields.title.as_deref(),
        Some("Apply the edit?")
    );
    let options: Vec<&str> = permissions[0]
        .options
        .iter()
        .map(|option| option.option_id.0.as_ref())
        .collect();
    assert_eq!(options, vec!["allow", "reject"]);
    let _ = std::fs::remove_dir_all(&cwd);
}

#[test]
fn setting_a_config_option_re_reports_the_whole_snapshot() {
    let cwd = scratch("config");
    let recorder = Arc::new(Recorder::default());
    let response = drive(&cwd, Arc::clone(&recorder), {
        let cwd = cwd.clone();
        async move |cx: ConnectionTo<Agent>| {
            cx.send_request(
                InitializeRequest::new(ProtocolVersion::V1)
                    .client_capabilities(engine::client_capabilities()),
            )
            .block_task()
            .await?;
            let session = cx
                .send_request(NewSessionRequest::new(cwd.clone()))
                .block_task()
                .await?;
            let updated = cx
                .send_request(SetSessionConfigOptionRequest::new(
                    session.session_id.clone(),
                    "thinking_level",
                    SessionConfigOptionValue::value_id("high"),
                ))
                .block_task()
                .await?;
            Ok(serde_json::to_value(&updated).expect("the response serializes"))
        }
    });

    // The re-emitted snapshot IS the confirmation (D4): it is whole, never a
    // delta.
    let ids: Vec<&Value> = response["configOptions"]
        .as_array()
        .expect("the response carries the options")
        .iter()
        .map(|option| &option["id"])
        .collect();
    assert_eq!(
        ids,
        vec![
            &json!("model"),
            &json!("thinking_level"),
            &json!("steering_mode"),
            &json!("follow_up_mode"),
            &json!("auto_compaction"),
        ]
    );
    // pi answers a level change with an EVENT as well, and that lands as a
    // config update of its own — carrying the NEW level, whole. (The fixture
    // `get_state` keeps answering `medium`, so the refresh that follows the
    // setter republishes that; a real pi reports the level it just took.)
    let updates = recorder.updates();
    let levels: Vec<Value> = updates
        .iter()
        .filter_map(|update| match update {
            SessionUpdate::ConfigOptionUpdate(update) => update
                .config_options
                .iter()
                .find(|option| option.id.0.as_ref() == "thinking_level")
                .map(|option| {
                    serde_json::to_value(option).expect("the option serializes")["currentValue"]
                        .clone()
                }),
            _ => None,
        })
        .collect();
    assert!(
        levels.contains(&json!("high")),
        "the thinking_level_changed event republishes the snapshot: {levels:?}"
    );
    let _ = std::fs::remove_dir_all(&cwd);
}

#[test]
fn loading_a_session_replays_its_transcript() {
    let cwd = scratch("load");
    let recorder = Arc::new(Recorder::default());
    drive(&cwd, Arc::clone(&recorder), {
        let cwd = cwd.clone();
        async move |cx: ConnectionTo<Agent>| {
            cx.send_request(
                InitializeRequest::new(ProtocolVersion::V1)
                    .client_capabilities(engine::client_capabilities()),
            )
            .block_task()
            .await?;
            cx.send_request(LoadSessionRequest::new(
                SessionId::new(FIXTURE_SESSION_FILE),
                cwd.clone(),
            ))
            .block_task()
            .await?;
            Ok(())
        }
    });

    let updates = recorder.updates();
    let replayed: Vec<Value> = updates
        .iter()
        .filter_map(|update| match update {
            SessionUpdate::UserMessageChunk(chunk) | SessionUpdate::AgentMessageChunk(chunk) => {
                Some(serde_json::to_value(&chunk.content).expect("the chunk serializes"))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        replayed,
        vec![
            json!({ "type": "text", "text": "fix the parser" }),
            json!({ "type": "text", "text": "done" }),
        ]
    );
    let _ = std::fs::remove_dir_all(&cwd);
}
