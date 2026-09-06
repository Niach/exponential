//! EXP-746 E2 — the claude adapter driven end to end against a FAKE claude.
//!
//! `tests/fixtures/claude/fake-claude.sh` replays recorded stream-json frames
//! (scrubbed captures from the phase-1 spike) and answers control requests, so
//! these tests exercise the real adapter over the real wire: the argv it
//! spawns, the `session/update` notifications it produces, the permission and
//! elicitation requests it raises, and the control responses it writes back.
//!
//! Everything the adapter WROTE to the CLI is asserted from the fake's stdin
//! log, because the answer that never reaches claude is the failure mode that
//! looks fine from the client side.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::schema::v1::{
    ContentBlock, CreateElicitationRequest, CreateElicitationResponse, ElicitationAcceptAction,
    ElicitationAction, ElicitationContentValue, ElicitationMode, InitializeRequest,
    CancelNotification, ListSessionsRequest, LoadSessionRequest, NewSessionRequest,
    NewSessionResponse, PromptRequest, RequestPermissionOutcome,
    RequestPermissionRequest, RequestPermissionResponse, SelectedPermissionOutcome,
    SessionConfigId, SessionConfigKind, SessionConfigOptionValue, SessionModeId,
    SessionNotification, SessionUpdate, SetSessionConfigOptionRequest, SetSessionModeRequest,
    StopReason, TextContent, ToolCallContent, ToolKind,
};
use agent_client_protocol::{
    on_receive_notification, on_receive_request, Client, ConnectionTo, Error,
};
use engine::adapters::claude::ClaudeAgent;
use engine::adapters::{AdapterKind, AdapterSpec};
use serde_json::Value;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/claude");

/// A whole run: what the client saw and what the CLI was told.
struct Run {
    stop_reason: StopReason,
    session: NewSessionResponse,
    updates: Vec<SessionNotification>,
    permissions: Vec<RequestPermissionRequest>,
    elicitations: Vec<CreateElicitationRequest>,
    argv: Vec<String>,
    /// Every line the adapter wrote to claude's stdin, parsed.
    stdin: Vec<Value>,
}

impl Run {
    /// One short label per notification, so a test asserts the whole stream in
    /// one readable vector instead of index arithmetic.
    fn shape(&self) -> Vec<String> {
        self.updates.iter().map(|notification| shape(&notification.update)).collect()
    }

    fn argv_value(&self, flag: &str) -> Option<&str> {
        let at = self.argv.iter().position(|arg| arg == flag)?;
        self.argv.get(at + 1).map(String::as_str)
    }

    /// The control responses the adapter wrote back to claude.
    fn control_responses(&self) -> Vec<&Value> {
        self.stdin
            .iter()
            .filter(|line| line.get("type").and_then(Value::as_str) == Some("control_response"))
            .collect()
    }
}

fn shape(update: &SessionUpdate) -> String {
    match update {
        SessionUpdate::AvailableCommandsUpdate(update) => {
            format!("commands:{}", update.available_commands.len())
        }
        SessionUpdate::ConfigOptionUpdate(update) => {
            format!("config:{}", update.config_options.len())
        }
        SessionUpdate::CurrentModeUpdate(update) => format!("mode:{}", update.current_mode_id.0),
        SessionUpdate::UserMessageChunk(chunk) => format!("user:{}", chunk_text(chunk)),
        SessionUpdate::AgentMessageChunk(chunk) => format!("agent:{}", chunk_text(chunk)),
        SessionUpdate::AgentThoughtChunk(chunk) => format!("thought:{}", chunk_text(chunk)),
        SessionUpdate::ToolCall(call) => format!("tool:{}", call.title),
        SessionUpdate::ToolCallUpdate(update) => match &update.fields.status {
            Some(status) => format!("tool_update:{status:?}"),
            None => "tool_update".to_string(),
        },
        SessionUpdate::UsageUpdate(usage) => format!("usage:{}/{}", usage.used, usage.size),
        SessionUpdate::Plan(plan) => format!("plan:{}", plan.entries.len()),
        other => format!("{other:?}"),
    }
}

fn chunk_text(chunk: &agent_client_protocol::schema::v1::ContentChunk) -> String {
    match &chunk.content {
        ContentBlock::Text(text) => text.text.clone(),
        other => format!("{other:?}"),
    }
}

type PermissionAnswer =
    Arc<dyn Fn(&RequestPermissionRequest) -> RequestPermissionOutcome + Send + Sync>;
type ElicitationAnswer =
    Arc<dyn Fn(&CreateElicitationRequest) -> ElicitationAction + Send + Sync>;

/// Reject every permission unless a test says otherwise.
fn reject_all() -> PermissionAnswer {
    Arc::new(|_request| {
        RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new("reject"))
    })
}

fn pick(option: &'static str) -> PermissionAnswer {
    Arc::new(move |_request| {
        RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(option))
    })
}

fn cancel_elicitations() -> ElicitationAnswer {
    Arc::new(|_request| ElicitationAction::Cancel)
}

struct Workdir(PathBuf);

impl Drop for Workdir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn workdir(scenario: &str) -> Workdir {
    let path = std::env::temp_dir()
        .join(format!("exp746-claude-{scenario}-{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(&path).expect("a scratch directory");
    Workdir(path)
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

fn spec(scenario: &str, work: &Path, plan_mode: bool) -> AdapterSpec {
    let spawn = terminal::pty::SpawnSpec::new(fake_claude().display().to_string())
        .cwd(work)
        // Transcripts are looked up in the CHILD's config dir, so `session/list`
        // and `session/load` read the run's own scratch tree, never the
        // developer's real ~/.claude.
        .env("CLAUDE_CONFIG_DIR", work.join("claude-config").display().to_string())
        .env("EXP_FAKE_CLAUDE_DIR", Path::new(FIXTURES).join(scenario).display().to_string())
        .env("EXP_FAKE_CLAUDE_ARGV", work.join("argv.txt").display().to_string())
        .env("EXP_FAKE_CLAUDE_STDIN", work.join("stdin.jsonl").display().to_string())
        // The key the inline --mcp-config resolves through `${EXP_MCP_TOKEN}`.
        .env(coding::MCP_TOKEN_ENV, "expu_test-key");
    AdapterSpec {
        kind: AdapterKind::Claude,
        agent: coding::AgentKind::Builtin(coding::CodingAgent::Claude),
        spawn,
        options: coding::LaunchOptions {
            agent: coding::CodingAgent::Claude,
            model: "opus".to_string(),
            effort: String::new(),
            ultracode: false,
            plan_mode,
            external: None,
        },
        mcp: coding::AgentMcp::ClaudeInline {
            url: "https://app.example/api/mcp".to_string(),
            session_id: Some("row-1".to_string()),
        },
        cwd: work.to_path_buf(),
        session_id: "row-1".to_string(),
        prompt: None,
        resume: None,
        personal_key: Some("expu_test-key".to_string()),
        reaper_settings_path: Some(work.join("claude-hooks/1/row-1.settings.json")),
        exit: engine::ChildExitLink::new(),
    }
}

async fn drive(
    scenario: &str,
    work: &Path,
    prompt: &str,
    plan_mode: bool,
    permission: PermissionAnswer,
    elicitation: ElicitationAnswer,
) -> Run {
    let adapter = ClaudeAgent::new(spec(scenario, work, plan_mode)).expect("the adapter builds");
    let updates: Arc<Mutex<Vec<SessionNotification>>> = Arc::new(Mutex::new(Vec::new()));
    let permissions: Arc<Mutex<Vec<RequestPermissionRequest>>> = Arc::new(Mutex::new(Vec::new()));
    let elicitations: Arc<Mutex<Vec<CreateElicitationRequest>>> = Arc::new(Mutex::new(Vec::new()));

    let seen_updates = updates.clone();
    let seen_permissions = permissions.clone();
    let seen_elicitations = elicitations.clone();
    let prompt = prompt.to_string();

    let driven = Client
        .builder()
        .name("exp746-test-client")
        .on_receive_notification(
            async move |notification: SessionNotification, _cx| {
                seen_updates.lock().expect("updates").push(notification);
                Ok(())
            },
            on_receive_notification!(),
        )
        .on_receive_request(
            async move |request: RequestPermissionRequest, responder, _cx| {
                seen_permissions.lock().expect("permissions").push(request.clone());
                let outcome = permission(&request);
                responder.respond(RequestPermissionResponse::new(outcome))
            },
            on_receive_request!(),
        )
        .on_receive_request(
            async move |request: CreateElicitationRequest, responder, _cx| {
                seen_elicitations.lock().expect("elicitations").push(request.clone());
                let action = elicitation(&request);
                responder.respond(CreateElicitationResponse::new(action))
            },
            on_receive_request!(),
        )
        .connect_with(adapter, async move |cx: ConnectionTo<agent_client_protocol::Agent>| {
            cx.send_request(
                InitializeRequest::new(ProtocolVersion::V1)
                    .client_capabilities(engine::client_capabilities()),
            )
            .block_task()
            .await?;
            let session = cx
                .send_request(NewSessionRequest::new(std::env::temp_dir()))
                .block_task()
                .await?;
            let response = cx
                .send_request(PromptRequest::new(
                    session.session_id.clone(),
                    vec![ContentBlock::Text(TextContent::new(prompt))],
                ))
                .block_task()
                .await?;
            Ok::<_, Error>((session, response))
        });

    let (session, response) = tokio::time::timeout(Duration::from_secs(30), driven)
        .await
        .expect("the run finishes inside the budget")
        .expect("the connection runs cleanly");

    let argv = std::fs::read_to_string(work.join("argv.txt"))
        .map(|text| text.lines().map(str::to_string).collect())
        .unwrap_or_default();
    let stdin = std::fs::read_to_string(work.join("stdin.jsonl"))
        .map(|text| {
            text.lines()
                .filter(|line| !line.trim().is_empty())
                .filter_map(|line| serde_json::from_str(line).ok())
                .collect()
        })
        .unwrap_or_default();

    Run {
        stop_reason: response.stop_reason,
        session,
        updates: Arc::try_unwrap(updates).expect("one owner").into_inner().expect("updates"),
        permissions: Arc::try_unwrap(permissions)
            .expect("one owner")
            .into_inner()
            .expect("permissions"),
        elicitations: Arc::try_unwrap(elicitations)
            .expect("one owner")
            .into_inner()
            .expect("elicitations"),
        argv,
        stdin,
    }
}

#[tokio::test]
async fn a_plain_turn_streams_once_and_settles_on_end_turn() {
    let work = workdir("basic");
    let run = drive(
        "basic",
        &work.0,
        "Reply with the single word ok.",
        false,
        reject_all(),
        cancel_elicitations(),
    )
    .await;

    assert_eq!(run.stop_reason, StopReason::EndTurn);
    // The consolidated `assistant` message repeats the text that already
    // streamed as deltas: it is diffed against what streamed and only the
    // remainder is forwarded, so "ok" appears exactly once.
    assert_eq!(
        run.shape(),
        vec![
            "commands:2".to_string(),
            "config:3".to_string(),
            "mode:auto".to_string(),
            "user:Reply with the single word ok.".to_string(),
            "usage:18201/1000000".to_string(),
            "agent:ok".to_string(),
            "usage:18201/1000000".to_string(),
            "usage:18201/1000000".to_string(),
        ]
    );
    // The catalog is filtered twice over: `doctor` is terminal-only (the init
    // frame says so, which is why the catalog is re-filtered when it lands)
    // and `clear` is on the hard-coded unsupported list.
    match &run.updates[0].update {
        SessionUpdate::AvailableCommandsUpdate(update) => {
            let names: Vec<&str> = update
                .available_commands
                .iter()
                .map(|command| command.name.as_str())
                .collect();
            assert_eq!(names, vec!["compact", "context"]);
        }
        other => panic!("expected the command list, got {other:?}"),
    }
}

#[tokio::test]
async fn the_argv_pins_the_permission_mode_and_the_reaper_anchor() {
    let work = workdir("argv");
    let run = drive(
        "basic",
        &work.0,
        "Reply with the single word ok.",
        false,
        reject_all(),
        cancel_elicitations(),
    )
    .await;

    // The SDK base array, in the SDK's own order.
    assert_eq!(run.argv[0], "-p");
    assert_eq!(run.argv_value("--output-format"), Some("stream-json"));
    assert_eq!(run.argv_value("--input-format"), Some("stream-json"));
    assert!(run.argv.iter().any(|arg| arg == "--include-partial-messages"));
    assert!(run.argv.iter().any(|arg| arg == "--replay-user-messages"));
    // Without this the CLI never sends can_use_tool and the whole permission
    // surface dies silently.
    assert_eq!(run.argv_value("--permission-prompt-tool"), Some("stdio"));
    // ALWAYS pinned: absent, the user's own settings default wins and an
    // `auto` machine never asks (measured in the spike).
    assert_eq!(run.argv_value("--permission-mode"), Some("bypassPermissions"));
    assert!(run.argv.iter().any(|arg| arg == "--allow-dangerously-skip-permissions"));
    // The reaper's only process-selection anchor (EXP-300).
    assert!(run
        .argv_value("--settings")
        .is_some_and(|path| path.contains("claude-hooks/1/row-1.settings.json")));
    // The MCP server rides inline with the key left as an env reference.
    let mcp = run.argv_value("--mcp-config").expect("--mcp-config is emitted");
    assert!(mcp.contains("${EXP_MCP_TOKEN}"), "the key must not land on the argv: {mcp}");
    assert!(mcp.contains("X-Exp-Session-Id"));
    assert!(!mcp.contains("expu_test-key"));
    assert!(run.argv.iter().any(|arg| arg == "--strict-mcp-config"));
    // The session id pin doubles as the ACP session id, so `--resume=<acp id>`
    // reopens exactly this conversation.
    let pinned = run
        .argv
        .iter()
        .find_map(|arg| arg.strip_prefix("--session-id="))
        .expect("--session-id= is pinned");
    assert_eq!(pinned, run.session.session_id.0.as_ref());
    // Env: the session-state backstop on, the entrypoint label never claimed.
    assert!(run.argv.iter().any(|arg| arg == "env:CLAUDE_CODE_EMIT_SESSION_STATE_EVENTS=1"));
    // Never SET by the adapter (the SDK claims `sdk-ts`; we do not): whatever
    // the child reports is what it inherited from this process.
    let inherited = std::env::var("CLAUDE_CODE_ENTRYPOINT").unwrap_or_default();
    assert!(run
        .argv
        .iter()
        .any(|arg| arg == &format!("env:CLAUDE_CODE_ENTRYPOINT={inherited}")));
    assert!(run.argv.iter().any(|arg| arg == "env:EXP_MCP_TOKEN=expu_test-key"));
}

#[tokio::test]
async fn the_new_session_response_carries_the_modes_and_the_config_options() {
    let work = workdir("config");
    let run = drive(
        "basic",
        &work.0,
        "Reply with the single word ok.",
        false,
        reject_all(),
        cancel_elicitations(),
    )
    .await;

    let modes = run.session.modes.as_ref().expect("modes are advertised");
    let ids: Vec<String> =
        modes.available_modes.iter().map(|mode| mode.id.0.to_string()).collect();
    assert!(ids.contains(&"plan".to_string()));
    assert!(ids.contains(&"acceptEdits".to_string()));
    assert!(!ids.contains(&"dontAsk".to_string()));
    assert_eq!(modes.current_mode_id.0.as_ref(), "bypassPermissions");

    let options = run.session.config_options.as_ref().expect("config options");
    let ids: Vec<String> = options.iter().map(|option| option.id.0.to_string()).collect();
    // Mode is NOT among them: it rides `session/set_mode`, and a second chip
    // for it would render twice on every client. Fast is missing too, and
    // deliberately: the launch flag names the alias `opus`, and only the
    // resolved id the CLI reports on `system/init` says whether the model
    // supports it — the option appears on the config update that follows.
    assert_eq!(ids, vec!["model", "effort"]);
    match &options[0].kind {
        SessionConfigKind::Select(select) => {
            assert_eq!(select.current_value.0.as_ref(), "opus");
        }
        other => panic!("expected a select, got {other:?}"),
    }
}

#[tokio::test]
async fn a_permission_reaches_the_client_and_its_answer_reaches_the_cli() {
    let work = workdir("permission");
    let run = drive(
        "permission",
        &work.0,
        "Run exactly this shell command and nothing else",
        false,
        pick("allow-once"),
        cancel_elicitations(),
    )
    .await;

    assert_eq!(run.stop_reason, StopReason::EndTurn);
    let request = run.permissions.first().expect("one permission request");
    // The card is the tool call itself, so the client renders what will run.
    assert_eq!(request.tool_call.fields.kind, Some(ToolKind::Execute));
    assert!(request
        .tool_call
        .fields
        .title
        .as_ref()
        .is_some_and(|title| title.starts_with("rm -f")));
    let options: Vec<String> =
        request.options.iter().map(|option| option.option_id.0.to_string()).collect();
    // The CLI suggested a rule bundle, so "don't ask again" is offered.
    assert_eq!(options, vec!["allow-once", "allow-with-updates", "reject"]);

    // What actually matters: the allow reached claude, keyed by the tool use.
    let answer = run.control_responses().first().copied().expect("an answer went back");
    let payload = &answer["response"]["response"];
    assert_eq!(payload["behavior"], serde_json::json!("allow"));
    assert_eq!(payload["decisionClassification"], serde_json::json!("user_temporary"));
    assert_eq!(payload["toolUseID"], serde_json::json!("toolu_01U7SzXzR2hsSy8k3Yuei2Dq"));
    assert_eq!(payload["updatedInput"], request.tool_call.fields.raw_input.clone().unwrap());
}

#[tokio::test]
async fn a_cancelled_permission_is_denied_rather_than_left_hanging() {
    let work = workdir("permission-cancel");
    let run = drive(
        "permission",
        &work.0,
        "Run exactly this shell command and nothing else",
        false,
        Arc::new(|_request| RequestPermissionOutcome::Cancelled),
        cancel_elicitations(),
    )
    .await;

    // The ACP cancellation contract answers pending permissions with
    // `Cancelled`; the CLI still needs a decision or the turn never ends.
    let answer = run.control_responses().first().copied().expect("an answer went back");
    let payload = &answer["response"]["response"];
    assert_eq!(payload["behavior"], serde_json::json!("deny"));
    assert_eq!(payload["message"], serde_json::json!("Cancelled by the user"));
    assert_eq!(run.stop_reason, StopReason::EndTurn);
}

#[tokio::test]
async fn the_plan_approval_is_a_switch_mode_card_with_the_plan_and_four_options() {
    let work = workdir("plan");
    let run = drive(
        "plan",
        &work.0,
        "Append world to note.txt. Plan first.",
        true,
        pick("exit-plan-accept-edits"),
        cancel_elicitations(),
    )
    .await;

    assert_eq!(run.argv_value("--permission-mode"), Some("plan"));
    let request = run.permissions.first().expect("the plan approval");
    // All four clients gate their "Plan ready" card on this kind plus the
    // plan markdown the card carries.
    assert_eq!(request.tool_call.fields.kind, Some(ToolKind::SwitchMode));
    assert_eq!(request.tool_call.fields.title.as_deref(), Some("Ready to code?"));
    let plan = request
        .tool_call
        .fields
        .content
        .as_ref()
        .and_then(|content| content.first())
        .expect("the plan rides as content");
    match plan {
        ToolCallContent::Content(content) => match &content.content {
            ContentBlock::Text(text) => assert!(text.text.starts_with("# Plan")),
            other => panic!("expected the plan text, got {other:?}"),
        },
        other => panic!("expected content, got {other:?}"),
    }
    let options: Vec<String> =
        request.options.iter().map(|option| option.option_id.0.to_string()).collect();
    assert_eq!(
        options,
        vec![
            "exit-plan-clear-auto".to_string(),
            "exit-plan-auto".to_string(),
            "exit-plan-default".to_string(),
            "reject".to_string(),
        ]
    );

    // A non-clearing approval ALLOWS and carries the mode switch with it.
    let answer = run.control_responses().first().copied().expect("an answer went back");
    let payload = &answer["response"]["response"];
    assert_eq!(payload["behavior"], serde_json::json!("allow"));
    assert_eq!(
        payload["updatedPermissions"],
        serde_json::json!([{ "type": "setMode", "mode": "acceptEdits", "destination": "session" }])
    );
}

#[tokio::test]
async fn a_clear_context_approval_denies_with_an_interrupt_and_re_prompts_the_plan() {
    let work = workdir("plan-clear");
    let run = drive(
        "plan",
        &work.0,
        "Append world to note.txt. Plan first.",
        true,
        pick("exit-plan-clear-auto"),
        cancel_elicitations(),
    )
    .await;

    let answer = run.control_responses().first().copied().expect("an answer went back");
    let payload = &answer["response"]["response"];
    // Allowing would run ExitPlanMode in the OLD context before the hand-off.
    assert_eq!(payload["behavior"], serde_json::json!("deny"));
    assert_eq!(payload["interrupt"], serde_json::json!(true));
    assert_eq!(
        payload["message"],
        serde_json::json!("User accepted the plan and requested a fresh context")
    );
    // …and the plan is re-prompted on a cleared conversation instead of the
    // turn ending there.
    let prompts: Vec<String> = run
        .stdin
        .iter()
        .filter(|line| line.get("type").and_then(Value::as_str) == Some("user"))
        .filter_map(|line| line["message"]["content"][0]["text"].as_str().map(str::to_string))
        .collect();
    assert!(prompts.iter().any(|text| text == "/clear"), "{prompts:?}");
    assert!(
        prompts.iter().any(|text| text.starts_with("Implement the following plan:")),
        "{prompts:?}"
    );
}

#[tokio::test]
async fn a_question_becomes_an_elicitation_answered_by_the_question_text() {
    let work = workdir("ask");
    let run = drive(
        "ask",
        &work.0,
        "Ask me whether to use tabs or spaces.",
        false,
        reject_all(),
        Arc::new(|_request| {
            let mut content = std::collections::BTreeMap::new();
            content.insert(
                "question_0".to_string(),
                ElicitationContentValue::String("Spaces".to_string()),
            );
            ElicitationAction::Accept(ElicitationAcceptAction::new().content(content))
        }),
    )
    .await;

    let request = run.elicitations.first().expect("one elicitation");
    let schema = match &request.mode {
        ElicitationMode::Form(form) => &form.requested_schema,
        other => panic!("expected a form, got {other:?}"),
    };
    assert!(schema.properties.contains_key("question_0"));
    // The free-text sibling every question gets, so a user can answer off-menu.
    assert!(schema.properties.contains_key("question_0_custom"));
    assert_eq!(request.message, "Should indentation use tabs or spaces?");

    // Answers go back keyed by the question TEXT, not by the field key.
    let answer = run.control_responses().first().copied().expect("an answer went back");
    let payload = &answer["response"]["response"];
    assert_eq!(payload["behavior"], serde_json::json!("allow"));
    assert_eq!(
        payload["updatedInput"]["answers"],
        serde_json::json!({ "Should indentation use tabs or spaces?": "Spaces" })
    );
    assert!(run.permissions.is_empty(), "a question is never a permission card");
}

#[tokio::test]
async fn an_unknown_user_dialog_is_answered_with_silence() {
    let work = workdir("dialog");
    let run = drive(
        "dialog",
        &work.0,
        "Do something the model refuses.",
        false,
        reject_all(),
        cancel_elicitations(),
    )
    .await;

    // The CLI fails closed on a dialog kind no client declared and degrades on
    // its own; a synthesized cancel would kill the flow instead.
    assert!(run.control_responses().is_empty(), "{:?}", run.control_responses());
    assert_eq!(run.stop_reason, StopReason::EndTurn);
    assert!(run.permissions.is_empty());
    assert!(run.elicitations.is_empty());
}

/// One recorded transcript in the child's own `CLAUDE_CONFIG_DIR`, in the
/// shape the CLI persists (`type` + `message`, with the project directory
/// munged from the cwd).
fn record_transcript(work: &Path, session_id: &str, cwd: &Path) {
    let munged = cwd.display().to_string().replace(['/', '.'], "-");
    let dir = work.join("claude-config").join("projects").join(munged);
    std::fs::create_dir_all(&dir).expect("a project directory");
    let cwd = cwd.display().to_string();
    let lines = [
        serde_json::json!({
            "type": "user",
            "cwd": cwd,
            "sessionId": session_id,
            "message": { "role": "user", "content": [{ "type": "text", "text": "hello there" }] },
            "uuid": "00000000-0000-4000-8000-000000000021",
        }),
        serde_json::json!({ "type": "ai-title", "aiTitle": "A recorded run", "sessionId": session_id }),
        serde_json::json!({
            "type": "assistant",
            "cwd": cwd,
            "sessionId": session_id,
            "message": {
                "id": "msg_recorded_1",
                "role": "assistant",
                "content": [{ "type": "text", "text": "hi back" }],
            },
            "uuid": "00000000-0000-4000-8000-000000000022",
        }),
        // A transcript entry type the adapter has no business rendering.
        serde_json::json!({ "type": "file-history-snapshot", "messageId": "x" }),
    ];
    let text = lines
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(dir.join(format!("{session_id}.jsonl")), text + "\n")
        .expect("the transcript is written");
}

#[tokio::test]
async fn a_recorded_transcript_lists_and_replays_without_spawning_the_cli() {
    let work = workdir("history");
    let session_id = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";
    record_transcript(&work.0, session_id, &work.0);

    let adapter = ClaudeAgent::new(spec("basic", &work.0, false)).expect("the adapter builds");
    let updates: Arc<Mutex<Vec<SessionNotification>>> = Arc::new(Mutex::new(Vec::new()));
    let seen = updates.clone();
    let listed = work.0.clone();
    let sessions = Client
        .builder()
        .name("exp746-test-client")
        .on_receive_notification(
            async move |notification: SessionNotification, _cx| {
                seen.lock().expect("updates").push(notification);
                Ok(())
            },
            on_receive_notification!(),
        )
        .connect_with(adapter, async move |cx: ConnectionTo<agent_client_protocol::Agent>| {
            cx.send_request(
                InitializeRequest::new(ProtocolVersion::V1)
                    .client_capabilities(engine::client_capabilities()),
            )
            .block_task()
            .await?;
            let sessions = cx
                .send_request(ListSessionsRequest::new().cwd(listed.clone()))
                .block_task()
                .await?;
            cx.send_request(LoadSessionRequest::new(
                sessions.sessions[0].session_id.clone(),
                listed,
            ))
            .block_task()
            .await?;
            Ok::<_, Error>(sessions)
        })
        .await
        .expect("the connection runs cleanly");

    let row = sessions.sessions.first().expect("the recorded run lists");
    assert_eq!(row.session_id.0.as_ref(), session_id);
    assert_eq!(row.title.as_deref(), Some("A recorded run"));

    // The replay runs through the SAME frame path a live session takes, with
    // no child process: a Past tab renders the run, it does not restart it.
    let shapes: Vec<String> = updates
        .lock()
        .expect("updates")
        .iter()
        .map(|notification| shape(&notification.update))
        .collect();
    assert_eq!(shapes, vec!["user:hello there".to_string(), "agent:hi back".to_string()]);
    assert!(
        !work.0.join("argv.txt").exists(),
        "a transcript replay must not spawn claude"
    );
}

#[tokio::test]
async fn a_cancel_interrupts_the_running_turn_and_settles_it_cancelled() {
    let work = workdir("cancel");
    let adapter = ClaudeAgent::new(spec("cancel", &work.0, false)).expect("the adapter builds");
    let stop_reason = Client
        .builder()
        .name("exp746-test-client")
        .on_receive_notification(
            async move |_notification: SessionNotification, _cx| Ok(()),
            on_receive_notification!(),
        )
        .connect_with(adapter, async move |cx: ConnectionTo<agent_client_protocol::Agent>| {
            cx.send_request(
                InitializeRequest::new(ProtocolVersion::V1)
                    .client_capabilities(engine::client_capabilities()),
            )
            .block_task()
            .await?;
            let session = cx
                .send_request(NewSessionRequest::new(std::env::temp_dir()))
                .block_task()
                .await?;
            // The turn's fixture carries no `result`: it is still running when
            // the cancel arrives, which is the case the interrupt exists for.
            let turn = cx.send_request(PromptRequest::new(
                session.session_id.clone(),
                vec![ContentBlock::Text(TextContent::new("Work on something long."))],
            ));
            cx.send_notification(CancelNotification::new(session.session_id.clone()))?;
            let response = turn.block_task().await?;
            Ok::<_, Error>(response.stop_reason)
        })
        .await
        .expect("the connection runs cleanly");

    // The CLI's own `result` still says `end_turn`; a cancelled session is the
    // adapter's fact, and it outranks the subtype.
    assert_eq!(stop_reason, StopReason::Cancelled);
    let stdin: Vec<Value> = std::fs::read_to_string(work.0.join("stdin.jsonl"))
        .expect("the fake logged stdin")
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    let interrupt = stdin
        .iter()
        .find(|line| line["request"]["subtype"] == serde_json::json!("interrupt"))
        .expect("the interrupt reached the CLI");
    // `cancel_queued` is what makes one Stop halt the whole session rather
    // than only the turn in flight.
    assert_eq!(interrupt["request"]["cancel_queued"], serde_json::json!(true));
}

/// EXP-746: a mid-turn steer is a second `session/prompt`, and the CLI may
/// FOLD it into the running turn instead of queueing it — one `result` for
/// both, `queued_turn_count: 0`. The fixture answers only after both user
/// messages arrived, so both prompts must settle on that one `result`: a
/// stranded one hangs forever and leaves every later turn settling the
/// channel in front of it, which pins the run's `idle` at false for good.
#[tokio::test]
async fn a_steer_the_cli_folds_into_the_running_turn_settles_with_it() {
    let work = workdir("foldin");
    let adapter =
        ClaudeAgent::new(spec("steer-foldin", &work.0, false)).expect("the adapter builds");
    let driven = Client
        .builder()
        .name("exp746-test-client")
        .on_receive_notification(
            async move |_notification: SessionNotification, _cx| Ok(()),
            on_receive_notification!(),
        )
        .connect_with(adapter, async move |cx: ConnectionTo<agent_client_protocol::Agent>| {
            cx.send_request(
                InitializeRequest::new(ProtocolVersion::V1)
                    .client_capabilities(engine::client_capabilities()),
            )
            .block_task()
            .await?;
            let session = cx
                .send_request(NewSessionRequest::new(std::env::temp_dir()))
                .block_task()
                .await?;
            let turn = cx.send_request(PromptRequest::new(
                session.session_id.clone(),
                vec![ContentBlock::Text(TextContent::new("Reply with the single word ok."))],
            ));
            // The steer: sent while the first turn is still streaming, which
            // is what the fixture's missing `result` stands for.
            let steered = cx.send_request(PromptRequest::new(
                session.session_id.clone(),
                vec![ContentBlock::Text(TextContent::new("Make it two words."))],
            ));
            let turn = turn.block_task().await?;
            let steered = steered.block_task().await?;
            Ok::<_, Error>((turn.stop_reason, steered.stop_reason))
        });

    let (turn, steered) = tokio::time::timeout(Duration::from_secs(30), driven)
        .await
        .expect("both prompts settle inside the budget")
        .expect("the connection runs cleanly");
    assert_eq!(turn, StopReason::EndTurn);
    assert_eq!(steered, StopReason::EndTurn);
}

#[tokio::test]
async fn steering_the_mode_and_the_effort_reaches_the_cli_and_echoes_back() {
    let work = workdir("steer");
    let adapter = ClaudeAgent::new(spec("basic", &work.0, false)).expect("the adapter builds");
    let updates: Arc<Mutex<Vec<SessionNotification>>> = Arc::new(Mutex::new(Vec::new()));
    let seen = updates.clone();
    Client
        .builder()
        .name("exp746-test-client")
        .on_receive_notification(
            async move |notification: SessionNotification, _cx| {
                seen.lock().expect("updates").push(notification);
                Ok(())
            },
            on_receive_notification!(),
        )
        .connect_with(adapter, async move |cx: ConnectionTo<agent_client_protocol::Agent>| {
            cx.send_request(
                InitializeRequest::new(ProtocolVersion::V1)
                    .client_capabilities(engine::client_capabilities()),
            )
            .block_task()
            .await?;
            let session = cx
                .send_request(NewSessionRequest::new(std::env::temp_dir()))
                .block_task()
                .await?;
            cx.send_request(SetSessionModeRequest::new(
                session.session_id.clone(),
                SessionModeId::new("plan"),
            ))
            .block_task()
            .await?;
            let options = cx
                .send_request(SetSessionConfigOptionRequest::new(
                    session.session_id.clone(),
                    SessionConfigId::new("effort"),
                    SessionConfigOptionValue::value_id("high"),
                ))
                .block_task()
                .await?;
            Ok::<_, Error>(options)
        })
        .await
        .expect("the connection runs cleanly");

    let stdin: Vec<Value> = std::fs::read_to_string(work.0.join("stdin.jsonl"))
        .expect("the fake logged stdin")
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    let requests: Vec<&Value> = stdin.iter().map(|line| &line["request"]).collect();
    assert!(requests.iter().any(|request| {
        request["subtype"] == serde_json::json!("set_permission_mode")
            && request["mode"] == serde_json::json!("plan")
    }));
    // Effort is a settings-layer flag mid-session; `--effort` is spawn-only.
    assert!(requests.iter().any(|request| {
        request["subtype"] == serde_json::json!("apply_flag_settings")
            && request["settings"]["effortLevel"] == serde_json::json!("high")
    }));

    let shapes: Vec<String> = updates
        .lock()
        .expect("updates")
        .iter()
        .map(|notification| shape(&notification.update))
        .collect();
    // The mode echo the four clients key their chip on, then the full option
    // set both changes re-publish.
    assert!(shapes.contains(&"mode:plan".to_string()), "{shapes:?}");
    assert!(shapes.iter().filter(|shape| shape.starts_with("config:")).count() >= 2, "{shapes:?}");
}

#[tokio::test]
async fn the_child_going_away_closes_the_connection_with_its_exit_code() {
    // The crash path (a `kill -9`, an OOM, the reaper): the CLI is gone while
    // the client is still holding the connection open. Nothing but the child's
    // EOF can end this run, and the engine's whole end sequence — the bye, the
    // heartbeat, `coding::end_session` — hangs off that close (EXP-746 E1).
    let work = workdir("child-exit");
    let mut adapter_spec = spec("basic", &work.0, false);
    adapter_spec.spawn = adapter_spec.spawn.clone().env("EXP_FAKE_CLAUDE_EXIT", "3");
    let exit = adapter_spec.exit.clone();
    let adapter = ClaudeAgent::new(adapter_spec).expect("the adapter builds");

    let driven = Client
        .builder()
        .name("exp746-test-client")
        .on_receive_notification(
            async move |_notification: SessionNotification, _cx| Ok(()),
            on_receive_notification!(),
        )
        .connect_with(adapter, async move |cx: ConnectionTo<agent_client_protocol::Agent>| {
            cx.send_request(
                InitializeRequest::new(ProtocolVersion::V1)
                    .client_capabilities(engine::client_capabilities()),
            )
            .block_task()
            .await?;
            let session = cx
                .send_request(NewSessionRequest::new(std::env::temp_dir()))
                .block_task()
                .await?;
            // The turn replays and the fake then exits 3, so this may settle
            // either way; what matters is what happens next.
            let _ = cx
                .send_request(PromptRequest::new(
                    session.session_id.clone(),
                    vec![ContentBlock::Text(TextContent::new("Reply with the single word ok."))],
                ))
                .block_task()
                .await;
            // The host's own loop: it waits on the adapter and on nothing
            // else, so a connection that never closes is a run that never ends.
            cx.incoming_closed().await;
            Ok::<_, Error>(())
        });

    tokio::time::timeout(Duration::from_secs(30), driven)
        .await
        .expect("the dead child closes the connection")
        .expect("the connection runs cleanly");

    // And it closed LATE enough for the exit code to be on the link: the end
    // sequence turns this into the `exit:3` bye rather than a bare `ended`.
    assert_eq!(exit.get().map(|exit| exit.code), Some(3));
}
