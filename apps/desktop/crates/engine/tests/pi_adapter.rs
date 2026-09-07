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
    InitializeRequest, LoadSessionRequest, NewSessionRequest, PermissionOptionKind, PromptRequest,
    RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SelectedPermissionOutcome, SessionConfigOptionValue, SessionId, SessionModeId,
    SessionNotification, SessionUpdate, SetSessionConfigOptionRequest, SetSessionModeRequest,
    StopReason, ToolCallContent, ToolKind,
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

/// Everything the client saw. `decline` answers every permission with its
/// REJECT option instead of the first one (EXP-752: a plan the user turns
/// down must leave the session planning).
#[derive(Default)]
struct Recorder {
    updates: Mutex<Vec<SessionNotification>>,
    permissions: Mutex<Vec<RequestPermissionRequest>>,
    decline: bool,
}

impl Recorder {
    /// A client that says NO to every dialog.
    fn declining() -> Self {
        Self { decline: true, ..Self::default() }
    }

    fn updates(&self) -> Vec<SessionUpdate> {
        self.updates
            .lock()
            .expect("the recorder is not poisoned")
            .iter()
            .map(|notification| notification.update.clone())
            .collect()
    }

    /// Every `current_mode_update` the client was told about, in order.
    fn modes(&self) -> Vec<String> {
        self.updates()
            .iter()
            .filter_map(|update| match update {
                SessionUpdate::CurrentModeUpdate(update) => {
                    Some(update.current_mode_id.0.to_string())
                }
                _ => None,
            })
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

/// The rpc deadline every test but the timeout one runs on: long enough that
/// a healthy fake never hits it, short enough that a wedged one does not hold
/// the suite for two minutes.
const TEST_RPC_TIMEOUT: Duration = Duration::from_secs(20);

fn adapter(
    cwd: &Path,
    plan_mode: bool,
    env: &[(&str, String)],
    rpc_timeout: Duration,
) -> PiAgent {
    let fixtures = fixtures();
    let mut spawn = SpawnSpec::new(fixtures.join("fake-pi.sh").display().to_string())
        .cwd(cwd)
        .env("EXP_FAKE_PI_DIR", fixtures.display().to_string())
        .env(
            "EXP_FAKE_PI_EDIT",
            cwd.join("notes.md").display().to_string(),
        );
    for (key, value) in env {
        spawn = spawn.env(*key, value);
    }
    let mut options = coding::LaunchOptions::defaults(&coding::Settings::default());
    options.agent = coding::CodingAgent::Pi;
    // The settings default plan mode ON, so every test says which it wants.
    options.plan_mode = plan_mode;
    PiAgent::with_rpc_timeout(
        AdapterSpec {
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
            exit: engine::ChildExitLink::new(),
        },
        rpc_timeout,
    )
    .expect("the fake pi spawns")
}

/// Drive one client connection over the adapter. `main` runs with the ACP
/// connection; everything it returns comes back to the test.
fn drive<T, F>(cwd: &Path, recorder: Arc<Recorder>, main: F) -> T
where
    F: AsyncFnOnce(ConnectionTo<Agent>) -> Result<T, Error>,
{
    drive_with(cwd, recorder, false, &[], main)
}

/// [`drive`] over a launch that asked for plan mode (EXP-752) and/or a fake
/// with extra env (its stdin log, its plan-turn fixture).
fn drive_with<T, F>(
    cwd: &Path,
    recorder: Arc<Recorder>,
    plan_mode: bool,
    env: &[(&str, String)],
    main: F,
) -> T
where
    F: AsyncFnOnce(ConnectionTo<Agent>) -> Result<T, Error>,
{
    drive_timed(cwd, recorder, plan_mode, env, TEST_RPC_TIMEOUT, main)
}

/// [`drive_with`] over an explicit rpc deadline (EXP-758).
fn drive_timed<T, F>(
    cwd: &Path,
    recorder: Arc<Recorder>,
    plan_mode: bool,
    env: &[(&str, String)],
    rpc_timeout: Duration,
    main: F,
) -> T
where
    F: AsyncFnOnce(ConnectionTo<Agent>) -> Result<T, Error>,
{
    let agent = adapter(cwd, plan_mode, env, rpc_timeout);
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
                                let chosen = if recorder.decline {
                                    request
                                        .options
                                        .iter()
                                        .find(|option| {
                                            matches!(
                                                option.kind,
                                                PermissionOptionKind::RejectOnce
                                                    | PermissionOptionKind::RejectAlways
                                            )
                                        })
                                        .unwrap_or(&request.options[0])
                                } else {
                                    &request.options[0]
                                };
                                let option = chosen.option_id.clone();
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
        json!({ "exponentialNativeSessionId": FIXTURE_SESSION_FILE })
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
            "compact", "new", "model", "thinking", "name", "fork", "clone", "export", "review",
            "exp-status"
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

// ---------------------------------------------------------------------------
// EXP-752 — plan mode over `pi --mode rpc`
// ---------------------------------------------------------------------------

#[test]
fn a_plan_mode_session_advertises_plan_and_default_modes() {
    let cwd = scratch("modes");
    let planning = drive_with(
        &cwd,
        Arc::new(Recorder::default()),
        true,
        &[],
        {
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
                Ok(serde_json::to_value(&session).expect("the response serializes"))
            }
        },
    );
    assert_eq!(planning["modes"]["currentModeId"], json!("plan"));
    let ids: Vec<&Value> = planning["modes"]["availableModes"]
        .as_array()
        .expect("the plan session lists its modes")
        .iter()
        .map(|mode| &mode["id"])
        .collect();
    assert_eq!(ids, vec![&json!("default"), &json!("plan")]);

    // Without the plan extension pi has NO modes at all — the config options
    // stay the whole switchable surface.
    let plain = drive(&cwd, Arc::new(Recorder::default()), {
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
            Ok(serde_json::to_value(&session).expect("the response serializes"))
        }
    });
    assert!(
        plain.get("modes").is_none_or(Value::is_null),
        "a plain rpc session advertises no modes: {plain}"
    );
    let _ = std::fs::remove_dir_all(&cwd);
}

#[test]
fn set_mode_between_turns_writes_the_exp_plan_command() {
    let cwd = scratch("set-mode");
    let log = cwd.join("stdin.jsonl");
    let recorder = Arc::new(Recorder::default());
    drive_with(
        &cwd,
        Arc::clone(&recorder),
        true,
        &[("EXP_FAKE_PI_STDIN_LOG", log.display().to_string())],
        {
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
                cx.send_request(SetSessionModeRequest::new(
                    session.session_id.clone(),
                    SessionModeId::new("default"),
                ))
                .block_task()
                .await?;
                Ok(())
            }
        },
    );

    // pi has no mode verb: the switch IS the plan extension's command, sent
    // as a prompt (pi runs a registered command instead of prompting the
    // model with it).
    let written = std::fs::read_to_string(&log).expect("the fake logged its stdin");
    assert!(
        written.contains(r#""type":"prompt","message":"/exp-plan off""#),
        "the mode switch rides a prompt: {written}"
    );
    // And the client is told, so the picker shows the mode it now has.
    assert_eq!(recorder.modes(), vec!["default".to_string()]);
    let _ = std::fs::remove_dir_all(&cwd);
}

#[test]
fn an_approved_plan_confirm_flips_the_mode_to_default() {
    let cwd = scratch("plan-confirm");
    let recorder = Arc::new(Recorder::default());
    drive_with(
        &cwd,
        Arc::clone(&recorder),
        true,
        &[("EXP_FAKE_PI_PLAN", "1".to_string())],
        {
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
                cx.send_request(PromptRequest::new(
                    session.session_id.clone(),
                    vec![ContentBlock::from("plan the parser fix")],
                ))
                .block_task()
                .await?;
                Ok(())
            }
        },
    );

    // The plan dialog is the one confirm that is really a mode switch, so the
    // permission card says so.
    let permissions = recorder
        .permissions
        .lock()
        .expect("the recorder is not poisoned");
    assert_eq!(permissions.len(), 1);
    assert_eq!(
        permissions[0].tool_call.fields.title.as_deref(),
        Some("Approve plan?")
    );
    assert_eq!(
        permissions[0].tool_call.fields.kind,
        Some(ToolKind::SwitchMode)
    );
    // Approving it leaves plan mode: the extension has stopped planning, so
    // the advertised mode has to follow.
    assert_eq!(recorder.modes(), vec!["default".to_string()]);
    let _ = std::fs::remove_dir_all(&cwd);
}

#[test]
fn a_typed_plan_command_switches_the_mode_instead_of_wedging_the_turn() {
    let cwd = scratch("typed-plan");
    let log = cwd.join("stdin.jsonl");
    let recorder = Arc::new(Recorder::default());
    // EXP-752: `/exp-plan` is hidden from the picker, but the composer and a
    // remote steer message send free text — pi answers the registered
    // extension command WITHOUT starting a turn, so an adapter that forwarded
    // it as an ordinary prompt would park this request forever.
    let stop = drive_with(
        &cwd,
        Arc::clone(&recorder),
        true,
        &[("EXP_FAKE_PI_STDIN_LOG", log.display().to_string())],
        {
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
                let response = cx
                    .send_request(PromptRequest::new(
                        session.session_id.clone(),
                        vec![ContentBlock::from("  /exp-plan off  ")],
                    ))
                    .block_task()
                    .await?;
                Ok(response.stop_reason)
            }
        },
    );

    // It ENDS (a wedged turn would only ever hit the harness timeout), and it
    // rode the same command `session/set_mode` writes.
    assert_eq!(stop, StopReason::EndTurn);
    let written = std::fs::read_to_string(&log).expect("the fake logged its stdin");
    assert!(
        written.contains(r#""type":"prompt","message":"/exp-plan off""#),
        "the typed switch rides the extension command: {written}"
    );
    // ... and the advertised mode followed it, exactly as the picker's route does.
    assert_eq!(recorder.modes(), vec!["default".to_string()]);
    let _ = std::fs::remove_dir_all(&cwd);
}

#[test]
fn a_plan_switch_mid_turn_is_refused_on_both_routes() {
    let cwd = scratch("mid-turn-mode");
    let recorder = Arc::new(Recorder::default());
    let (from_picker, from_text) = drive_with(
        &cwd,
        Arc::clone(&recorder),
        true,
        // The fake streams a turn and never settles it: the mid-turn state.
        &[("EXP_FAKE_PI_HANG", "1".to_string())],
        {
            let cwd = cwd.clone();
            let recorder = Arc::clone(&recorder);
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
                let session_id = session.session_id.clone();
                let prompting = tokio::spawn({
                    let cx = cx.clone();
                    let session_id = session_id.clone();
                    async move {
                        cx.send_request(PromptRequest::new(
                            session_id,
                            vec![ContentBlock::from("keep going")],
                        ))
                        .block_task()
                        .await
                    }
                });
                // The turn is live once its first chunk lands.
                let deadline = std::time::Instant::now() + Duration::from_secs(10);
                while !recorder
                    .updates()
                    .iter()
                    .any(|update| matches!(update, SessionUpdate::AgentMessageChunk(_)))
                {
                    if std::time::Instant::now() > deadline {
                        return Err(Error::internal_error().data(json!("the fake never streamed")));
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }

                let from_picker = cx
                    .send_request(SetSessionModeRequest::new(
                        session_id.clone(),
                        SessionModeId::new("default"),
                    ))
                    .block_task()
                    .await;
                let from_text = cx
                    .send_request(PromptRequest::new(
                        session_id,
                        vec![ContentBlock::from("/exp-plan off")],
                    ))
                    .block_task()
                    .await;
                // Neither refusal may settle the turn that is still running.
                assert!(
                    !prompting.is_finished(),
                    "a refused switch left the running turn alone"
                );
                prompting.abort();
                Ok((
                    refusal("session/set_mode", from_picker),
                    refusal("a typed /exp-plan", from_text),
                ))
            }
        },
    );

    // A switch queued into a running turn would take effect at the wrong
    // moment, so both routes say no the same way.
    for error in [from_picker, from_text] {
        assert_eq!(error.code, Error::invalid_request().code, "{error:?}");
        assert_eq!(
            error.data,
            Some(json!("pi plan mode switches between turns only")),
            "{error:?}"
        );
    }
    // And nothing lied to the client about the mode it is in.
    assert!(recorder.modes().is_empty(), "{:?}", recorder.modes());
    let _ = std::fs::remove_dir_all(&cwd);
}

#[test]
fn a_declined_plan_confirm_stays_in_plan_mode() {
    let cwd = scratch("plan-declined");
    let recorder = Arc::new(Recorder::declining());
    drive_with(
        &cwd,
        Arc::clone(&recorder),
        true,
        &[("EXP_FAKE_PI_PLAN", "1".to_string())],
        {
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
                cx.send_request(PromptRequest::new(
                    session.session_id.clone(),
                    vec![ContentBlock::from("plan the parser fix")],
                ))
                .block_task()
                .await?;
                Ok(())
            }
        },
    );

    let permissions = recorder
        .permissions
        .lock()
        .expect("the recorder is not poisoned");
    assert_eq!(permissions.len(), 1);
    assert_eq!(
        permissions[0].tool_call.fields.title.as_deref(),
        Some("Approve plan?")
    );
    // A REJECTED plan leaves the extension planning, so the mode must not
    // move: a `current_mode_update` here would leave the picker lying.
    assert!(recorder.modes().is_empty(), "{:?}", recorder.modes());
    let _ = std::fs::remove_dir_all(&cwd);
}

/// The error a refused plan switch answers with — a switch that is ACCEPTED
/// mid-turn is the defect, so it fails the test right here.
fn refusal<T: std::fmt::Debug>(what: &str, result: Result<T, Error>) -> Error {
    match result {
        Ok(response) => panic!("{what} was accepted mid-turn: {response:?}"),
        Err(error) => error,
    }
}

// ---------------------------------------------------------------------------
// EXP-758: extension commands, the rpc deadline, and the session cost total
// ---------------------------------------------------------------------------

#[test]
fn an_advertised_extension_command_ends_without_a_turn_and_leaves_none_behind() {
    let cwd = scratch("extension-command");
    let log = cwd.join("stdin.jsonl");
    let recorder = Arc::new(Recorder::default());
    // EXP-758: `get_commands` reports a `source` per command, and pi runs an
    // `extension` one ITSELF: no turn, no `agent_settled`. Typed as a prompt
    // it used to park the request forever and leave `turn.active` set, so
    // every later message went out as a `steer` into a turn that never was.
    let (first, second) = drive_with(
        &cwd,
        Arc::clone(&recorder),
        false,
        &[
            ("EXP_FAKE_PI_STDIN_LOG", log.display().to_string()),
            ("EXP_FAKE_PI_NO_TURN", "exp-status".to_string()),
        ],
        {
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
                let first = cx
                    .send_request(PromptRequest::new(
                        session.session_id.clone(),
                        vec![ContentBlock::from("/exp-status")],
                    ))
                    .block_task()
                    .await?;
                // ... and the session is IDLE afterwards, so an ordinary
                // message opens a fresh turn.
                let second = cx
                    .send_request(PromptRequest::new(
                        session.session_id.clone(),
                        vec![ContentBlock::from("edit the notes")],
                    ))
                    .block_task()
                    .await?;
                Ok((first.stop_reason, second.stop_reason))
            }
        },
    );

    assert_eq!(first, StopReason::EndTurn);
    assert_eq!(second, StopReason::EndTurn);
    let written = std::fs::read_to_string(&log).expect("the fake logged its stdin");
    assert!(
        written.contains(r#""type":"prompt","message":"/exp-status""#),
        "the extension command rides a prompt: {written}"
    );
    assert!(
        written.contains(r#""type":"prompt","message":"edit the notes""#),
        "the next message opens a FRESH turn: {written}"
    );
    assert!(
        !written.contains(r#""type":"steer""#),
        "nothing was steered into a turn that never started: {written}"
    );
    let _ = std::fs::remove_dir_all(&cwd);
}

#[test]
fn a_prompt_sourced_command_is_still_an_ordinary_turn() {
    let cwd = scratch("prompt-command");
    let log = cwd.join("stdin.jsonl");
    // `review` is `source: prompt` in the fixture: pi expands it into a real
    // turn, so it must NOT take the extension route.
    let stop = drive_with(
        &cwd,
        Arc::new(Recorder::default()),
        false,
        &[("EXP_FAKE_PI_STDIN_LOG", log.display().to_string())],
        {
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
                let response = cx
                    .send_request(PromptRequest::new(
                        session.session_id.clone(),
                        vec![ContentBlock::from("/review")],
                    ))
                    .block_task()
                    .await?;
                Ok(response.stop_reason)
            }
        },
    );

    // It settled on the fake's `agent_settled`, which only a real turn emits.
    assert_eq!(stop, StopReason::EndTurn);
    let written = std::fs::read_to_string(&log).expect("the fake logged its stdin");
    assert!(
        written.contains(r#""type":"prompt","message":"/review""#),
        "a prompt-sourced command is message text: {written}"
    );
    let _ = std::fs::remove_dir_all(&cwd);
}

#[test]
fn an_unanswered_rpc_command_gives_up_on_its_own_deadline() {
    let cwd = scratch("rpc-timeout");
    // EXP-758: pi has no timeout of its own. A command it never answers used
    // to park its handler task for the life of the run.
    let error = drive_timed(
        &cwd,
        Arc::new(Recorder::default()),
        false,
        &[("EXP_FAKE_PI_SILENT", "compact".to_string())],
        Duration::from_millis(400),
        {
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
                let result = cx
                    .send_request(PromptRequest::new(
                        session.session_id.clone(),
                        vec![ContentBlock::from("/compact")],
                    ))
                    .block_task()
                    .await;
                Ok(refusal("a silenced /compact", result))
            }
        },
    );

    let data = error.data.clone().unwrap_or_default().to_string();
    assert!(
        data.contains("did not answer compact"),
        "the deadline says which command wedged: {error:?}"
    );
    let _ = std::fs::remove_dir_all(&cwd);
}

#[test]
fn a_failed_steer_leaves_the_running_turn_alone() {
    let cwd = scratch("failed-steer");
    let log = cwd.join("stdin.jsonl");
    let recorder = Arc::new(Recorder::default());
    // EXP-758: a steer that pi REJECTS used to settle the whole turn
    // `Cancelled`, answering the first prompt while pi was still working on
    // it, and flipping `turn.active` off underneath it.
    drive_with(
        &cwd,
        Arc::clone(&recorder),
        false,
        &[
            ("EXP_FAKE_PI_HANG", "1".to_string()),
            ("EXP_FAKE_PI_FAIL", "steer".to_string()),
            ("EXP_FAKE_PI_STDIN_LOG", log.display().to_string()),
        ],
        {
            let cwd = cwd.clone();
            let recorder = Arc::clone(&recorder);
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
                let session_id = session.session_id.clone();
                let prompting = tokio::spawn({
                    let cx = cx.clone();
                    let session_id = session_id.clone();
                    async move {
                        cx.send_request(PromptRequest::new(
                            session_id,
                            vec![ContentBlock::from("keep going")],
                        ))
                        .block_task()
                        .await
                    }
                });
                // The turn is live once its first chunk lands.
                let deadline = std::time::Instant::now() + Duration::from_secs(10);
                while !recorder
                    .updates()
                    .iter()
                    .any(|update| matches!(update, SessionUpdate::AgentMessageChunk(_)))
                {
                    if std::time::Instant::now() > deadline {
                        return Err(Error::internal_error().data(json!("the fake never streamed")));
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }

                let rejected = cx
                    .send_request(PromptRequest::new(
                        session_id.clone(),
                        vec![ContentBlock::from("also this")],
                    ))
                    .block_task()
                    .await;
                refusal("a rejected steer", rejected);
                tokio::time::sleep(Duration::from_millis(50)).await;
                assert!(
                    !prompting.is_finished(),
                    "the rejected steer settled the turn pi is still running"
                );

                // And the turn is still THE turn: the next message is another
                // steer, not a prompt opening a second one.
                let again = cx
                    .send_request(PromptRequest::new(
                        session_id,
                        vec![ContentBlock::from("and this")],
                    ))
                    .block_task()
                    .await;
                refusal("a second rejected steer", again);
                prompting.abort();
                Ok(())
            }
        },
    );

    let written = std::fs::read_to_string(&log).expect("the fake logged its stdin");
    let steers = written.matches(r#""type":"steer""#).count();
    assert_eq!(steers, 2, "both messages joined the running turn: {written}");
    assert_eq!(
        written.matches(r#""type":"prompt""#).count(),
        1,
        "only the first message opened a turn: {written}"
    );
    let _ = std::fs::remove_dir_all(&cwd);
}

#[test]
fn per_turn_costs_add_up_into_one_session_total() {
    let cwd = scratch("cost-total");
    let recorder = Arc::new(Recorder::default());
    // EXP-758: pi reports what a TURN cost; ACP's `UsageUpdate.cost` is the
    // SESSION's, and every client renders exactly one figure.
    drive_with(
        &cwd,
        Arc::clone(&recorder),
        false,
        &[("EXP_FAKE_PI_TURNS", "1".to_string())],
        {
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
                for message in ["first", "second"] {
                    cx.send_request(PromptRequest::new(
                        session.session_id.clone(),
                        vec![ContentBlock::from(message)],
                    ))
                    .block_task()
                    .await?;
                }
                Ok(())
            }
        },
    );

    let costs: Vec<f64> = recorder
        .updates()
        .iter()
        .filter_map(|update| match update {
            SessionUpdate::UsageUpdate(usage) => usage.cost.as_ref().map(|cost| cost.amount),
            _ => None,
        })
        .collect();
    assert_eq!(costs.len(), 2, "{costs:?}");
    assert!((costs[0] - 0.030).abs() < 1e-9, "{costs:?}");
    assert!((costs[1] - 0.036).abs() < 1e-9, "{costs:?}");
    let _ = std::fs::remove_dir_all(&cwd);
}
