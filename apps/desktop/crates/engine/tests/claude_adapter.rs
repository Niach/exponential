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
    SessionConfigId, SessionConfigOptionValue, SessionModeId,
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
    /// How many notifications the client had already taken when the
    /// `session/prompt` resolved: anything the adapter published BEFORE the
    /// settle has an index below this, anything after does not.
    updates_at_settle: usize,
}

impl Run {
    /// One short label per notification, so a test asserts the whole stream in
    /// one readable vector instead of index arithmetic.
    fn shape(&self) -> Vec<String> {
        self.updates
            .iter()
            // EXP-784: the native-id / rate-limit `_meta` carriers are not
            // feed rows; `rate_limits()` and `native_ids()` read those.
            .filter(|notification| {
                !matches!(notification.update, SessionUpdate::SessionInfoUpdate(_))
            })
            .map(|notification| shape(&notification.update))
            .collect()
    }

    /// EXP-784: the rate-limit slots the adapter published, in order.
    fn rate_limits(&self) -> Vec<Value> {
        self.updates
            .iter()
            .filter_map(|notification| {
                notification.meta.as_ref()?.get(engine::mapper::RATE_LIMIT_META_KEY).cloned()
            })
            .collect()
    }

    /// EXP-784: the native session ids re-published mid-run, in order.
    fn native_ids(&self) -> Vec<String> {
        self.updates
            .iter()
            .filter_map(|notification| {
                notification
                    .meta
                    .as_ref()?
                    .get(engine::NATIVE_SESSION_META_KEY)?
                    .as_str()
                    .map(str::to_string)
            })
            .collect()
    }

    /// The adapter's notifications through the ONE mapper: what the relay
    /// would have seen.
    fn wire(&self) -> Vec<Value> {
        let mut mapper = engine::Mapper::new(engine::MapperConfig {
            redactor: Arc::new(steer::Redactor::new(Vec::new())),
            cwd: PathBuf::from("/tmp/worktree"),
            agent: steer::SessionAgent::Claude,
            session_seed: "sess-1".to_string(),
        });
        let mut out = engine::MapOut::default();
        for notification in &self.updates {
            mapper.on_update(notification, &mut out);
        }
        mapper.on_stop(StopReason::EndTurn, &mut out);
        out.wire
            .iter()
            .map(|event| serde_json::to_value(event).expect("an activity event serializes"))
            .collect()
    }

    fn argv_value(&self, flag: &str) -> Option<&str> {
        let at = self.argv.iter().position(|arg| arg == flag)?;
        self.argv.get(at + 1).map(String::as_str)
    }

    /// The subagent edges the adapter stamped on its no-op tool-call patches,
    /// as `(spawning tool call, status)` in the order the client saw them.
    fn subagent_edges(&self) -> Vec<(String, String)> {
        self.updates
            .iter()
            .filter_map(|notification| {
                let edge = notification.meta.as_ref()?.get("exponentialSubagent")?;
                Some((
                    edge.get("id")?.as_str()?.to_string(),
                    edge.get("status")?.as_str()?.to_string(),
                ))
            })
            .collect()
    }

    /// Where in the stream the client saw the edge reporting `status` for the
    /// tool call `id` — the index the settle is compared against.
    fn subagent_edge_at(&self, id: &str, status: &str) -> Option<usize> {
        self.updates.iter().position(|notification| {
            let Some(edge) =
                notification.meta.as_ref().and_then(|meta| meta.get("exponentialSubagent"))
            else {
                return false;
            };
            edge.get("id").and_then(Value::as_str) == Some(id)
                && edge.get("status").and_then(Value::as_str) == Some(status)
        })
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

/// A scenario the spike never recorded: frames written into the run's own
/// scratch tree and replayed by the same fake, for an edge the CLI only
/// produces under conditions a capture cannot be coaxed into.
fn synthetic(work: &Path, turn1: &[&str], after_answer: &[&str]) -> PathBuf {
    let dir = work.join("synthetic");
    std::fs::create_dir_all(&dir).expect("a synthetic scenario directory");
    std::fs::copy(Path::new(FIXTURES).join("initialize.jsonl"), dir.join("initialize.jsonl"))
        .expect("the shared initialize response");
    std::fs::write(dir.join("turn1.jsonl"), format!("{}\n", turn1.join("\n")))
        .expect("the turn frames");
    std::fs::write(dir.join("after-answer.jsonl"), format!("{}\n", after_answer.join("\n")))
        .expect("the frames that follow the permission answer");
    dir
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
    spec_at(&Path::new(FIXTURES).join(scenario), work, plan_mode)
}

/// The same launch against an arbitrary scenario directory, so a test can
/// hand the fake SYNTHETIC frames it wrote itself for an edge no recording
/// holds (it must carry its own `initialize.jsonl`: the fake's fallback to
/// `../initialize.jsonl` only reaches inside the fixture tree).
fn spec_at(scenario_dir: &Path, work: &Path, plan_mode: bool) -> AdapterSpec {
    let spawn = terminal::pty::SpawnSpec::new(fake_claude().display().to_string())
        .cwd(work)
        // Transcripts are looked up in the CHILD's config dir, so `session/list`
        // and `session/load` read the run's own scratch tree, never the
        // developer's real ~/.claude.
        .env("CLAUDE_CONFIG_DIR", work.join("claude-config").display().to_string())
        .env("EXP_FAKE_CLAUDE_DIR", scenario_dir.display().to_string())
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
            mcp_server_ids: Vec::new(),
            account: None,
            external: None,
        },
        mcp: coding::AgentMcp::ClaudeInline {
            url: "https://app.example/api/mcp".to_string(),
            session_id: Some("row-1".to_string()),
        },
        servers: Vec::new(),
        cwd: work.to_path_buf(),
        session_id: "row-1".to_string(),
        prompt: None,
        resume: None,
        replay: false,
        personal_key: Some("expu_test-key".to_string()),
        reaper_settings_path: Some(work.join("claude-hooks/1/row-1.settings.json")),
        exit: engine::ChildExitLink::new(),
    }
}

/// [`drive`] over several sequential prompts (`turn1.jsonl`, `turn2.jsonl`,
/// …); `stop_reason` and `updates_at_settle` are the LAST turn's.
async fn drive_turns(
    scenario: &str,
    work: &Path,
    prompts: &[&str],
    permission: PermissionAnswer,
    elicitation: ElicitationAnswer,
) -> Run {
    let scenario_dir = Path::new(FIXTURES).join(scenario);
    let adapter =
        ClaudeAgent::new(spec_at(&scenario_dir, work, false)).expect("the adapter builds");
    let updates: Arc<Mutex<Vec<SessionNotification>>> = Arc::new(Mutex::new(Vec::new()));
    let permissions: Arc<Mutex<Vec<RequestPermissionRequest>>> = Arc::new(Mutex::new(Vec::new()));
    let elicitations: Arc<Mutex<Vec<CreateElicitationRequest>>> = Arc::new(Mutex::new(Vec::new()));
    let seen_updates = updates.clone();
    let seen_permissions = permissions.clone();
    let seen_elicitations = elicitations.clone();
    let settled_at: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));
    let counted_updates = updates.clone();
    let count_at_settle = settled_at.clone();
    let prompts: Vec<String> = prompts.iter().map(|prompt| prompt.to_string()).collect();

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
            let mut stop_reason = StopReason::EndTurn;
            for prompt in prompts {
                let response = cx
                    .send_request(PromptRequest::new(
                        session.session_id.clone(),
                        vec![ContentBlock::Text(TextContent::new(prompt))],
                    ))
                    .block_task()
                    .await?;
                stop_reason = response.stop_reason;
            }
            *count_at_settle.lock().expect("the settle count") =
                counted_updates.lock().expect("updates").len();
            Ok::<_, Error>((session, stop_reason))
        });
    let (session, stop_reason) = tokio::time::timeout(Duration::from_secs(30), driven)
        .await
        .expect("the turns settle inside the budget")
        .expect("the connection runs cleanly");
    let argv = std::fs::read_to_string(work.join("argv.txt"))
        .map(|text| text.lines().map(str::to_string).collect())
        .unwrap_or_default();
    let stdin = std::fs::read_to_string(work.join("stdin.jsonl"))
        .map(|text| text.lines().filter_map(|line| serde_json::from_str(line).ok()).collect())
        .unwrap_or_default();
    let updates = updates.lock().expect("updates").clone();
    let permissions = permissions.lock().expect("permissions").clone();
    let elicitations = elicitations.lock().expect("elicitations").clone();
    let updates_at_settle = *settled_at.lock().expect("the settle count");
    Run {
        stop_reason,
        session,
        updates,
        permissions,
        elicitations,
        argv,
        stdin,
        updates_at_settle,
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
    drive_at(&Path::new(FIXTURES).join(scenario), work, prompt, plan_mode, permission, elicitation)
        .await
}

async fn drive_at(
    scenario_dir: &Path,
    work: &Path,
    prompt: &str,
    plan_mode: bool,
    permission: PermissionAnswer,
    elicitation: ElicitationAnswer,
) -> Run {
    let adapter =
        ClaudeAgent::new(spec_at(scenario_dir, work, plan_mode)).expect("the adapter builds");
    let updates: Arc<Mutex<Vec<SessionNotification>>> = Arc::new(Mutex::new(Vec::new()));
    let permissions: Arc<Mutex<Vec<RequestPermissionRequest>>> = Arc::new(Mutex::new(Vec::new()));
    let elicitations: Arc<Mutex<Vec<CreateElicitationRequest>>> = Arc::new(Mutex::new(Vec::new()));

    let seen_updates = updates.clone();
    let seen_permissions = permissions.clone();
    let seen_elicitations = elicitations.clone();
    // Read the moment the prompt resolves, so a test can tell an update the
    // adapter published BEFORE the settle from one it published after.
    let settled_at: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));
    let counted_updates = updates.clone();
    let count_at_settle = settled_at.clone();
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
            *count_at_settle.lock().expect("the settle count") =
                counted_updates.lock().expect("updates").len();
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

    let updates_at_settle = *settled_at.lock().expect("the settle count");
    let updates = std::mem::take(&mut *updates.lock().expect("updates"));

    Run {
        stop_reason: response.stop_reason,
        session,
        updates,
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
        updates_at_settle,
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
            // EXP-772: the vocabulary is empty — no option chips anywhere.
            "config:0".to_string(),
            // No mode event: the init frame reports the CLI's own spelling
            // (`auto`), which clamps to the `bypassPermissions` this run
            // already started in. Only the two modes `available_modes`
            // advertises are ever announced.
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
    // EXP-784: the init frame's session id differs from the pinned one in a
    // replayed capture, so a native-id re-record (a no-op
    // `session_info_update`) may precede the catalog.
    let first_catalog = run
        .updates
        .iter()
        .find(|notification| !matches!(notification.update, SessionUpdate::SessionInfoUpdate(_)))
        .expect("an update past the id re-record");
    match &first_catalog.update {
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
    // EXP-763: the run playbook, appended to the system prompt. The fixture
    // records argv one LINE per entry, so only the playbook's first line is
    // addressable here.
    assert_eq!(
        run.argv_value("--append-system-prompt"),
        coding::skill::RUN_SKILL.lines().next()
    );
    // EXP-784: the `--session-id` pin is claude's OWN uuid, decoupled from
    // the ACP session id (a `/clear` moves the former, never the latter);
    // it reaches the run record through the response's `_meta`.
    let pinned = run
        .argv
        .iter()
        .find_map(|arg| arg.strip_prefix("--session-id="))
        .expect("--session-id= is pinned");
    assert_ne!(pinned, run.session.session_id.0.as_ref());
    assert!(uuid::Uuid::parse_str(pinned).is_ok(), "{pinned}");
    assert_eq!(
        run.session.meta.as_ref().and_then(|meta| meta.get(engine::NATIVE_SESSION_META_KEY)),
        Some(&serde_json::json!(pinned))
    );
    // EXP-784: subagent prose reaches stdout only with this flag.
    assert!(run.argv.iter().any(|arg| arg == "--forward-subagent-text"));
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
    // EXP-772: plan on, plan off, and nothing else.
    assert_eq!(ids, vec!["plan".to_string(), "bypassPermissions".to_string()]);
    assert_eq!(modes.current_mode_id.0.as_ref(), "bypassPermissions");

    // …and no option chips at all: model / effort / fast / agent left the
    // mid-session steering UI on every client.
    let options = run.session.config_options.as_ref().expect("config options");
    assert!(options.is_empty(), "{options:?}");
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
    // EXP-772: permissions are bypassed in every mode, so an ordinary tool is
    // allowed HERE — no card ever reaches the client.
    assert!(run.permissions.is_empty(), "{:?}", run.permissions);
    let answer = run.control_responses().first().copied().expect("an answer went back");
    let payload = &answer["response"]["response"];
    assert_eq!(payload["behavior"], serde_json::json!("allow"));
    assert_eq!(payload["decisionClassification"], serde_json::json!("user_temporary"));
    assert_eq!(payload["toolUseID"], serde_json::json!("toolu_01U7SzXzR2hsSy8k3Yuei2Dq"));
    // The input goes back verbatim: nothing rewrote what the model asked for.
    assert_eq!(payload["updatedInput"]["command"], serde_json::json!("rm -f /work/tree/x"));
}

/// EXP-772: the plan approval is one of the only two dialogs left, so it is
/// what the cancellation contract is locked against.
#[tokio::test]
async fn a_cancelled_permission_is_denied_rather_than_left_hanging() {
    let work = workdir("permission-cancel");
    let run = drive(
        "plan",
        &work.0,
        "Append world to note.txt. Plan first.",
        true,
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

/// EXP-772: the CLI replays MACHINERY as `user` entries — system reminders,
/// hook and task notifications, local-command echoes, the summary a
/// compaction hands the fresh context. None of it may reach a feed as the
/// human's own words; a real turn beside them still does.
#[tokio::test]
async fn replayed_machinery_never_becomes_a_user_message() {
    let work = workdir("user-filter");
    let scenario = synthetic(
        &work.0,
        &[
            r#"{"type":"system","subtype":"init","cwd":"/work/tree","session_id":"11111111-2222-3333-4444-555555555555","tools":["Bash"],"model":"claude-opus-5[1m]","permissionMode":"bypassPermissions","slash_commands":["compact"],"agents":[],"uuid":"00000000-0000-4000-8000-000000000001"}"#,
            r#"{"type":"user","isMeta":true,"message":{"content":[{"type":"text","text":"The user opened a file."}]},"session_id":"11111111-2222-3333-4444-555555555555","uuid":"00000000-0000-4000-8000-000000000002"}"#,
            r#"{"type":"user","isCompactSummary":true,"message":{"content":[{"type":"text","text":"Summary of the conversation so far."}]},"session_id":"11111111-2222-3333-4444-555555555555","uuid":"00000000-0000-4000-8000-000000000003"}"#,
            r#"{"type":"user","message":{"content":[{"type":"text","text":"<system-reminder>Never do that.</system-reminder>"},{"type":"text","text":"and now the migration"}]},"session_id":"11111111-2222-3333-4444-555555555555","uuid":"00000000-0000-4000-8000-000000000004"}"#,
            r#"{"type":"user","message":{"content":[{"type":"text","text":"<task-notification>agent finished</task-notification>"}]},"session_id":"11111111-2222-3333-4444-555555555555","uuid":"00000000-0000-4000-8000-000000000005"}"#,
            r#"{"type":"result","subtype":"success","is_error":false,"stop_reason":"end_turn","result":"Done.","session_id":"11111111-2222-3333-4444-555555555555","uuid":"00000000-0000-4000-8000-000000000006","queued_turn_count":0}"#,
        ],
        &[],
    );
    let run = drive_at(
        &scenario,
        &work.0,
        "Start the migration.",
        false,
        reject_all(),
        cancel_elicitations(),
    )
    .await;

    let users: Vec<String> = run
        .updates
        .iter()
        .map(|notification| shape(&notification.update))
        .filter(|shape| shape.starts_with("user:"))
        .collect();
    assert_eq!(users, vec!["user:and now the migration".to_string()], "{users:?}");
}

/// EXP-772: even in PLAN mode an ordinary tool is allowed right here — the
/// only two dialogs left are the plan approval and a question. `reject_all`
/// would refuse a card if one were raised, and the CLI would see a deny.
#[tokio::test]
async fn a_tool_in_plan_mode_is_auto_allowed_without_a_card() {
    let work = workdir("plan-auto-allow");
    let scenario = synthetic(
        &work.0,
        &[
            r#"{"type":"system","subtype":"init","cwd":"/work/tree","session_id":"11111111-2222-3333-4444-555555555555","tools":["Bash"],"model":"claude-opus-5[1m]","permissionMode":"plan","slash_commands":["compact"],"agents":[],"uuid":"00000000-0000-4000-8000-000000000001"}"#,
            r#"{"type":"control_request","request_id":"0f1c2d3e-4a5b-6c7d-8e9f-000000000009","request":{"subtype":"can_use_tool","tool_name":"Bash","display_name":"Bash","input":{"command":"rg todo","description":"Search for todos"},"description":"Search for todos","tool_use_id":"toolu_theplanmodebash"}}"#,
        ],
        &[
            r#"{"type":"result","subtype":"success","is_error":false,"stop_reason":"end_turn","result":"Searched.","session_id":"11111111-2222-3333-4444-555555555555","uuid":"00000000-0000-4000-8000-000000000004","queued_turn_count":0}"#,
        ],
    );
    let run = drive_at(
        &scenario,
        &work.0,
        "Look for todos, then plan.",
        true,
        reject_all(),
        cancel_elicitations(),
    )
    .await;

    assert!(run.permissions.is_empty(), "{:?}", run.permissions);
    let answer = run.control_responses().first().copied().expect("an answer went back");
    let payload = &answer["response"]["response"];
    assert_eq!(payload["behavior"], serde_json::json!("allow"));
    assert_eq!(payload["toolUseID"], serde_json::json!("toolu_theplanmodebash"));
}

/// EXP-772: a plan-mode launch keeps `--permission-mode plan` AND the
/// dangerous flag — without the flag at spawn the CLI refuses the
/// `bypassPermissions` switch the approved plan makes.
#[tokio::test]
async fn a_plan_launch_still_bypasses_permissions() {
    let work = workdir("plan-argv");
    let run = drive(
        "plan",
        &work.0,
        "Append world to note.txt. Plan first.",
        true,
        pick("exit-plan-bypass"),
        cancel_elicitations(),
    )
    .await;

    assert_eq!(run.argv_value("--permission-mode"), Some("plan"));
    assert!(run.argv.iter().any(|arg| arg == "--allow-dangerously-skip-permissions"));
}

#[tokio::test]
async fn the_plan_approval_is_a_switch_mode_card_with_the_plan_and_its_options() {
    let work = workdir("plan");
    let run = drive(
        "plan",
        &work.0,
        "Append world to note.txt. Plan first.",
        true,
        pick("exit-plan-bypass"),
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
    // EXP-772: coding the plan is the only elevated answer left; EXP-788:
    // the plain "Yes" is index 0, the primary on every client.
    assert_eq!(
        options,
        vec![
            "exit-plan-bypass".to_string(),
            "exit-plan-clear-bypass".to_string(),
            "reject".to_string(),
        ]
    );

    // A non-clearing approval ALLOWS and carries the mode switch with it.
    let answer = run.control_responses().first().copied().expect("an answer went back");
    let payload = &answer["response"]["response"];
    assert_eq!(payload["behavior"], serde_json::json!("allow"));
    assert_eq!(
        payload["updatedPermissions"],
        serde_json::json!([
            { "type": "setMode", "mode": "bypassPermissions", "destination": "session" }
        ])
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
        pick("exit-plan-clear-bypass"),
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

/// EXP-784: the stall watchdog's interrupt says `cancel_queued: false` on
/// the notification, and that reaches the CLI's `interrupt` verbatim — the
/// steers queued behind the wedged turn are what it is rescuing.
#[tokio::test]
async fn a_stall_interrupt_keeps_the_queued_steers() {
    let work = workdir("stall");
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
            let turn = cx.send_request(PromptRequest::new(
                session.session_id.clone(),
                vec![ContentBlock::Text(TextContent::new("Work on something long."))],
            ));
            let mut meta = serde_json::Map::new();
            meta.insert(
                engine::host::CANCEL_QUEUED_META_KEY.to_string(),
                serde_json::json!(false),
            );
            cx.send_notification(CancelNotification::new(session.session_id.clone()).meta(meta))?;
            let response = turn.block_task().await?;
            Ok::<_, Error>(response.stop_reason)
        })
        .await
        .expect("the connection runs cleanly");
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
    assert_eq!(interrupt["request"]["cancel_queued"], serde_json::json!(false));
}

/// EXP-784: claude's rate-limit notice is STATE, not prose. The recorded
/// shape: a `rate_limit_event` (`rejected`), then the CLI's `<synthetic>`
/// assistant frame repeated per request (`You've hit your session limit ·
/// resets 12:10pm (Europe/Berlin)`), then a `result` that repeats it once
/// more — which used to be three identical bubbles. Now: ONE slot with the
/// message, ZERO narration rows, and the next real answer clears it.
#[tokio::test]
async fn a_rate_limit_notice_is_a_slot_and_never_a_bubble() {
    let work = workdir("rate-limit");
    let run = drive_turns(
        "rate-limit",
        &work.0,
        &["Do the thing.", "Try again."],
        reject_all(),
        cancel_elicitations(),
    )
    .await;
    let notice = "You've hit your session limit · resets 12:10pm (Europe/Berlin)";

    let narrations: Vec<&String> = run.shape().into_iter().collect::<Vec<_>>().leak().iter()
        .filter(|shape| shape.starts_with("agent:"))
        .collect();
    assert_eq!(narrations, vec!["agent:Back to work."], "no bubble for the notice");

    assert_eq!(
        run.rate_limits(),
        vec![
            serde_json::json!({"status": "rejected", "resetsAt": 1_788_703_200_000i64}),
            serde_json::json!({"status": "rejected", "resetsAt": 1_788_703_200_000i64, "message": notice}),
            serde_json::json!({"status": "ok"}),
        ],
        "the event, the notice once, the clear on the next real answer"
    );

    // On the wire: exactly one `rate_limit` with the message, then the clear.
    let wire: Vec<Value> =
        run.wire().into_iter().filter(|event| event["kind"] == "rate_limit").collect();
    assert_eq!(wire.len(), 3, "{wire:?}");
    assert_eq!(wire[1]["message"], serde_json::json!(notice));
    assert_eq!(wire[1]["status"], serde_json::json!("rejected"));
    assert_eq!(wire[2], serde_json::json!({"kind": "rate_limit", "status": ""}));
    let with_message = wire.iter().filter(|event| event.get("message").is_some()).count();
    assert_eq!(with_message, 1);
}

/// EXP-784: the ACP session id is the host's STABLE handle; claude's own
/// moves. Two `system/init`s (a `/clear` re-inits under a fresh uuid): the
/// ACP id on every notification stays put, the `--session-id` pin was a
/// different uuid to begin with, and each init that reports a NEW native id
/// re-publishes it so `runs.json` follows the live conversation.
#[tokio::test]
async fn a_clear_moves_the_native_id_but_never_the_acp_id() {
    let work = workdir("clear-reset");
    let run = drive_turns(
        "clear-reset",
        &work.0,
        &["First.", "/clear then second."],
        reject_all(),
        cancel_elicitations(),
    )
    .await;
    let acp = run.session.session_id.0.to_string();
    assert!(run.updates.iter().all(|notification| notification.session_id.0.as_ref() == acp));
    let pinned = run
        .argv
        .iter()
        .find_map(|arg| arg.strip_prefix("--session-id="))
        .expect("--session-id= is pinned");
    assert_ne!(pinned, acp);
    assert_eq!(
        run.native_ids(),
        vec![
            "11111111-2222-3333-4444-555555555555".to_string(),
            "22222222-3333-4444-5555-666666666666".to_string(),
        ]
    );
    assert_eq!(
        run.shape().iter().filter(|shape| shape.starts_with("agent:")).count(),
        2,
        "{:?}",
        run.shape()
    );
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

/// EXP-753 — subagent attribution, end to end, off a LIVE recording.
///
/// Two facts the `subagent/` capture settles, because neither is documented
/// anywhere the CLI publishes:
///
/// 1. `can_use_tool.agent_id` IS the `task_id` of the earlier
///    `system/task_started` — measured, not inferred. That is the whole
///    reason a permission raised inside a subagent can be attributed at all:
///    the id resolves to the task, and the task carries the `tool_use_id` of
///    the Task call every nested row already names.
/// 2. The terminal `task_updated`/`task_notification` pair arrives BEFORE the
///    turn's `result`. The nested rows are the prompt it was handed, its
///    Bash call and (EXP-784, `--forward-subagent-text`) the one line of
///    prose it wrote — an `assistant` frame carrying `parent_tool_use_id`,
///    which lands as a narration scoped to the subagent.
#[tokio::test]
async fn a_subagents_permission_chunks_and_edges_carry_the_parent_tool_use() {
    let work = workdir("subagent");
    let run = drive(
        "subagent",
        &work.0,
        "Use the Task tool to launch one subagent that appends probe to probe.txt.",
        false,
        pick("allow-once"),
        cancel_elicitations(),
    )
    .await;

    assert_eq!(run.stop_reason, StopReason::EndTurn);
    // The Task tool call the whole subagent hangs off: what the edge is keyed
    // on and what every row it produced names.
    let parent = "toolu_01R79m5CpGKpY22SFu6n5MSZ";
    // Both terminal frames (`task_updated` AND `task_notification`) report the
    // same completion. EXP-780: the adapter remembers the status it last
    // published per task, so the repeat is dropped and the edge goes out ONCE
    // — a second completed subagent row is not something every client should
    // have to fold away.
    assert_eq!(
        run.subagent_edges(),
        vec![
            (parent.to_string(), "started".to_string()),
            (parent.to_string(), "completed".to_string()),
        ]
    );

    let nested: Vec<(String, String)> = run
        .updates
        .iter()
        .filter_map(|notification| {
            let id = notification.meta.as_ref()?.get("subagentId")?.as_str()?.to_string();
            Some((shape(&notification.update), id))
        })
        .collect();
    assert!(nested.iter().all(|(_, id)| id == parent), "{nested:?}");
    let shapes: Vec<&str> = nested.iter().map(|(shape, _)| shape.as_str()).collect();
    assert_eq!(shapes.len(), 4, "{nested:?}");
    assert!(shapes[0].starts_with("user:Your only job"), "{shapes:?}");
    assert_eq!(shapes[1], "tool:echo probe >> probe.txt");
    assert_eq!(shapes[2], "tool_update:Completed");
    assert_eq!(shapes[3], "agent:Done: probe appended.");
    // ... and through the mapper it is a narration row with `subagentId`.
    let prose = run
        .wire()
        .into_iter()
        .find(|event| event["kind"] == "narration" && event["text"] == "Done: probe appended.")
        .expect("the subagent's prose is a narration");
    assert_eq!(prose["subagentId"], serde_json::json!(parent));

    // EXP-772: the permission the subagent raised is allowed without a card,
    // keyed by ITS tool use — nothing reaches the client to attribute.
    assert!(run.permissions.is_empty(), "{:?}", run.permissions);
    let answer = run.control_responses().first().copied().expect("an answer went back");
    assert_eq!(answer["response"]["response"]["behavior"], serde_json::json!("allow"));

    // In THIS recording the task completes before the turn's `result`, so the
    // completed edge is simply mid-stream: the answer the CLI streamed after
    // it still follows. The ordering that matters when the outcome is
    // DEFERRED (publish, then settle) is locked by the `subagent-deferred`
    // test below, which this recording cannot exercise.
    let completed = run.subagent_edge_at(parent, "completed").expect("a completed edge");
    assert!(completed < run.updates.len() - 1, "{completed} of {}", run.updates.len());
}

/// EXP-753 — a BACKGROUNDED subagent never adopts a MAIN-THREAD permission.
///
/// The CLI sends no `agent_id` on a main-thread `can_use_tool` (every
/// permission the spike recorded omits it), so attribution cannot guess from
/// "the one live task": a Task started with `is_backgrounded: true` stays
/// live while the main thread keeps working, and nesting that Bash approval
/// under its card files the permission inside a subagent the user never
/// connected it to. Synthetic frames, because a recording of a backgrounded
/// Task racing a main-thread approval is not reproducible on demand.
#[tokio::test]
async fn a_backgrounded_subagent_never_adopts_a_main_thread_permission() {
    let work = workdir("subagent-background");
    let scenario = synthetic(
        &work.0,
        &[
            r#"{"type":"system","subtype":"init","cwd":"/work/tree/subagent","session_id":"11111111-2222-3333-4444-555555555555","tools":["Task","Bash"],"model":"claude-opus-5[1m]","permissionMode":"default","slash_commands":["compact"],"agents":["general-purpose"],"uuid":"00000000-0000-4000-8000-000000000001"}"#,
            r#"{"type":"system","subtype":"task_started","task_id":"bg-task-1","tool_use_id":"toolu_thebackgroundtask","description":"Tail the dev server","subagent_type":"general-purpose","is_backgrounded":true,"spawn_depth":1,"task_type":"local_agent","uuid":"00000000-0000-4000-8000-000000000002","session_id":"11111111-2222-3333-4444-555555555555"}"#,
            // EXP-772: the plan approval, because it is one of the only two
            // dialogs a client still sees — an ordinary tool is auto-allowed
            // and has no card to attribute at all.
            r#"{"type":"control_request","request_id":"0f1c2d3e-4a5b-6c7d-8e9f-000000000001","request":{"subtype":"can_use_tool","tool_name":"ExitPlanMode","display_name":"ExitPlanMode","input":{"plan":"Plan: append main to main.txt"},"description":"Append main to main.txt","tool_use_id":"toolu_themainthreadplan"}}"#,
        ],
        &[
            r#"{"type":"system","subtype":"task_updated","task_id":"bg-task-1","patch":{"status":"completed"},"uuid":"00000000-0000-4000-8000-000000000003","session_id":"11111111-2222-3333-4444-555555555555"}"#,
            r#"{"type":"result","subtype":"success","is_error":false,"stop_reason":"end_turn","result":"Appended.","session_id":"11111111-2222-3333-4444-555555555555","uuid":"00000000-0000-4000-8000-000000000004","queued_turn_count":0}"#,
        ],
    );
    let run = drive_at(
        &scenario,
        &work.0,
        "Tail the dev server in the background and append main to main.txt.",
        false,
        pick("reject"),
        cancel_elicitations(),
    )
    .await;

    assert_eq!(run.stop_reason, StopReason::EndTurn);
    // The background task WAS live when the permission was raised: its started
    // edge is on the stream and nothing ended it until the answer went back.
    assert_eq!(
        run.subagent_edges().first().cloned(),
        Some(("toolu_thebackgroundtask".to_string(), "started".to_string()))
    );

    // And the main thread's permission stays where it belongs: top level.
    let permission = run.permissions.first().expect("the main thread's permission");
    let meta = permission.tool_call.meta.as_ref().expect("the permission carries meta");
    assert_eq!(meta.get("subagentId"), None, "{meta:?}");
    assert_eq!(
        meta.get("permission").and_then(|meta| meta.get("description")),
        Some(&serde_json::json!("Append main to main.txt"))
    );
    // Nothing else on the stream claims the subagent either.
    let nested: Vec<String> = run
        .updates
        .iter()
        .filter(|notification| {
            notification.meta.as_ref().and_then(|meta| meta.get("subagentId")).is_some()
        })
        .map(|notification| shape(&notification.update))
        .collect();
    assert!(nested.is_empty(), "{nested:?}");
}

/// EXP-753 — the DEFERRED outcome, and the edge that releases it.
///
/// `subagent-deferred/` is the `subagent/` capture with its terminal
/// `task_updated`/`task_notification` moved AFTER the turn's `result`: the
/// task is still live when the result lands, so the outcome is HELD BACK and
/// the terminal frame is what releases it. The recording never gets there (it
/// completes the task first, so the result settles directly), which is what
/// left the whole deferral path untested — drop the `settle_deferred` call in
/// the task handler and THIS turn hangs to the drive budget while the
/// recorded one still passes. It is the shape a backgrounded subagent
/// produces on every run.
#[tokio::test]
async fn a_deferred_turn_publishes_the_completed_edge_before_it_settles() {
    let work = workdir("subagent-deferred");
    let run = drive(
        "subagent-deferred",
        &work.0,
        "Use the Task tool to launch one subagent that appends probe to probe.txt.",
        false,
        pick("allow-once"),
        cancel_elicitations(),
    )
    .await;

    // It settles at all: the deferred outcome is released by the terminal task
    // frame, so a subagent that completes after the `result` never wedges the
    // turn (the drive budget is what a hang would spend).
    assert_eq!(run.stop_reason, StopReason::EndTurn);

    let parent = "toolu_01R79m5CpGKpY22SFu6n5MSZ";
    let completed = run.subagent_edge_at(parent, "completed").expect("a completed edge");
    // The client had that edge in hand BEFORE the prompt resolved: settling
    // first ends the run with the subagent card still spinning. (The pump
    // publishes and settles inside ONE frame with no await between, so the
    // wire order only diverges once one creeps in — this pins it there.)
    assert!(
        completed < run.updates_at_settle,
        "completed edge at {completed}, settle after {}",
        run.updates_at_settle
    );
}

// ---------------------------------------------------------------------------
// EXP-758: the CLI dying under a control request, the `/usage` resume, and a
// cancel with no turn behind it
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_control_request_in_flight_fails_when_the_cli_dies() {
    // EXP-758: the pump settled every TURN when stdout closed but nothing was
    // ever going to answer the control requests either, so a `set_mode` in
    // flight sat out the whole 90 s `CONTROL_TIMEOUT` first. The fake closes
    // its stdout and lingers, which is the shape that keeps the connection
    // open long enough (`CHILD_EXIT_GRACE`) for the adapter's own answer to
    // be the one the client sees.
    let work = workdir("control-drain");
    let mut adapter_spec = spec("basic", &work.0, false);
    adapter_spec.spawn =
        adapter_spec.spawn.clone().env("EXP_FAKE_CLAUDE_DIE_ON", "set_permission_mode");
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
            // The fake exits without answering this one.
            let result = cx
                .send_request(SetSessionModeRequest::new(
                    session.session_id.clone(),
                    SessionModeId::new("plan"),
                ))
                .block_task()
                .await;
            Ok::<_, Error>(result.err())
        });

    // Well inside `CONTROL_TIMEOUT`.
    let error = tokio::time::timeout(Duration::from_secs(20), driven)
        .await
        .expect("the dead child fails the control request instead of waiting it out")
        .expect("the connection runs cleanly");
    let error = error.expect("a control request nobody can answer is an error");
    assert!(
        error.data.clone().unwrap_or_default().to_string().contains("claude exited"),
        "the drain answers the control request itself, rather than leaving the \
         client to watch the transport go: {error:?}"
    );
}

#[tokio::test]
async fn usage_after_a_replay_resumes_the_same_conversation() {
    // EXP-758: `/usage` spawns the CLI lazily like any first prompt, and it
    // used to spawn it with a FRESH `--session-id`. The next real prompt then
    // ran in an empty conversation, silently forked off the replayed one.
    let work = workdir("usage-resume");
    record_transcript(&work.0, "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeef", &work.0);

    let adapter = ClaudeAgent::new(spec("basic", &work.0, false)).expect("the adapter builds");
    let listed = work.0.clone();
    Client
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
            let sessions = cx
                .send_request(ListSessionsRequest::new().cwd(listed.clone()))
                .block_task()
                .await?;
            let loaded = sessions.sessions[0].session_id.clone();
            cx.send_request(LoadSessionRequest::new(loaded.clone(), listed))
                .block_task()
                .await?;
            // No child yet: `/usage` is the thing that spawns one.
            cx.send_request(PromptRequest::new(
                loaded,
                vec![ContentBlock::Text(TextContent::new("/usage"))],
            ))
            .block_task()
            .await?;
            Ok::<_, Error>(())
        })
        .await
        .expect("the connection runs cleanly");

    let argv: Vec<String> = std::fs::read_to_string(work.0.join("argv.txt"))
        .expect("the CLI was spawned")
        .lines()
        .map(str::to_string)
        .collect();
    // The handle is the one the PROMPT path computes (the adapter's own ACP
    // session id); what matters is that `/usage` no longer pins a fresh one.
    assert!(
        argv.iter().any(|arg| arg.starts_with("--resume=")),
        "`/usage` resumes rather than starting a second conversation: {argv:?}"
    );
    assert!(
        !argv.iter().any(|arg| arg.starts_with("--session-id")),
        "claude refuses --session-id together with --resume: {argv:?}"
    );
}

#[tokio::test]
async fn two_prompts_racing_on_a_loaded_session_spawn_one_cli() {
    // EXP-766: the lazy spawn checked `child.is_none()` and stored the child
    // in two separate lock windows, so two prompts arriving together each
    // started a CLI. The second one silently orphaned the first: two
    // processes on one worktree, one of them talking to nobody.
    let work = workdir("start-race");
    record_transcript(&work.0, "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeef1", &work.0);

    let adapter = ClaudeAgent::new(spec("basic", &work.0, false)).expect("the adapter builds");
    let listed = work.0.clone();
    Client
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
            let sessions = cx
                .send_request(ListSessionsRequest::new().cwd(listed.clone()))
                .block_task()
                .await?;
            let loaded = sessions.sessions[0].session_id.clone();
            cx.send_request(LoadSessionRequest::new(loaded.clone(), listed))
                .block_task()
                .await?;
            // No child yet, and both of these spawn one. `/usage` is answered
            // from a control request, so neither needs a recorded turn.
            let first = cx
                .send_request(PromptRequest::new(
                    loaded.clone(),
                    vec![ContentBlock::Text(TextContent::new("/usage"))],
                ))
                .block_task();
            let second = cx
                .send_request(PromptRequest::new(
                    loaded,
                    vec![ContentBlock::Text(TextContent::new("/usage"))],
                ))
                .block_task();
            let (first, second) = tokio::join!(first, second);
            first?;
            second?;
            Ok::<_, Error>(())
        })
        .await
        .expect("the connection runs cleanly");

    // Every spawn APPENDS its stdin log, and every start opens with one
    // `initialize` control request: two of them would be two CLIs.
    let stdin = std::fs::read_to_string(work.0.join("stdin.jsonl")).expect("the CLI was spawned");
    let initializes = stdin.matches(r#""subtype":"initialize""#).count();
    assert_eq!(initializes, 1, "exactly one CLI was started: {stdin}");
}

#[tokio::test]
async fn a_cancel_with_no_turn_behind_it_never_interrupts_the_next_one() {
    // EXP-758: a cancel that finds no child is REMEMBERED so the turn it
    // raced can still be stopped. This is the other half of that rule: a
    // cancel with nothing behind it is consumed and dropped, never replayed
    // at whatever turn happens to come next.
    let work = workdir("stale-cancel");
    let session_id = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeef0";
    record_transcript(&work.0, session_id, &work.0);

    let adapter = ClaudeAgent::new(spec("basic", &work.0, false)).expect("the adapter builds");
    let listed = work.0.clone();
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
            let sessions = cx
                .send_request(ListSessionsRequest::new().cwd(listed.clone()))
                .block_task()
                .await?;
            let loaded = sessions.sessions[0].session_id.clone();
            cx.send_request(LoadSessionRequest::new(loaded.clone(), listed))
                .block_task()
                .await?;
            // Nothing is running and there is no child: this one has nowhere
            // to go.
            cx.send_notification(CancelNotification::new(loaded.clone()))?;
            let response = cx
                .send_request(PromptRequest::new(
                    loaded,
                    vec![ContentBlock::Text(TextContent::new("Reply with the single word ok."))],
                ))
                .block_task()
                .await?;
            Ok::<_, Error>(response.stop_reason)
        })
        .await
        .expect("the connection runs cleanly");

    assert_eq!(stop_reason, StopReason::EndTurn);
    let stdin = std::fs::read_to_string(work.0.join("stdin.jsonl")).unwrap_or_default();
    assert!(
        !stdin.contains(r#""subtype":"interrupt""#),
        "the stale cancel was not replayed at the next turn: {stdin}"
    );
}
