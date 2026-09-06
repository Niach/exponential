//! EXP-746 — recorded `session/update` sequences through the ONE mapper.
//!
//! Fixtures are ACP JSONL (`tests/fixtures/mapper/*.jsonl`, one
//! `SessionNotification` per line), so these tests also lock the DECODE side:
//! a schema shape the engine cannot parse fails here rather than in a live
//! session. The adapters' own recordings live under
//! `tests/fixtures/{claude,codex,pi}/` and are owned by their lanes; these
//! cover the mapper's own rules.
//!
//! Every assertion compares parsed `serde_json::Value`s, never serialized
//! text: `serde_json`'s `preserve_order` is feature-unified ON in this
//! workspace, so a string comparison passes alone and flakes in a multi-crate
//! `cargo test`.

use std::path::{Path, PathBuf};

use agent_client_protocol::schema::v1::{SessionNotification, StopReason};
use engine::{MapOut, Mapper, MapperConfig};
use serde_json::{json, Value};

fn mapper() -> Mapper {
    Mapper::new(MapperConfig {
        // REV2-17: the session's `expu_` key is an exact-match secret.
        redactor: steer::Redactor::new(vec!["expu_supersecretkey".to_string()]),
        cwd: PathBuf::from("/tmp/worktree"),
        agent: steer::SessionAgent::Claude,
        session_seed: "sess-1".to_string(),
    })
}

fn fixture(name: &str) -> Vec<SessionNotification> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/mapper")
        .join(name);
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("fixture {} is readable: {err}", path.display()));
    raw.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str(line)
                .unwrap_or_else(|err| panic!("fixture line decodes: {err}\n{line}"))
        })
        .collect()
}

/// Replay a fixture and end the turn, returning what went ON THE WIRE.
fn wire(name: &str) -> Vec<Value> {
    let mut mapper = mapper();
    let mut out = MapOut::default();
    for notification in fixture(name) {
        mapper.on_update(&notification, &mut out);
    }
    mapper.on_stop(StopReason::EndTurn, &mut out);
    out.wire
        .iter()
        .map(|event| serde_json::to_value(event).expect("an activity event serializes"))
        .collect()
}

/// Replay a fixture and end the turn, returning the LOCAL feed.
fn local(name: &str) -> Vec<engine::LocalFeedEvent> {
    let mut mapper = mapper();
    let mut out = MapOut::default();
    for notification in fixture(name) {
        mapper.on_update(&notification, &mut out);
    }
    mapper.on_stop(StopReason::EndTurn, &mut out);
    out.local
}

#[test]
fn a_recorded_turn_maps_to_the_wire_vector() {
    assert_eq!(
        wire("turn.jsonl"),
        vec![
            // The two chunks of `m1` coalesced; the tool call flushed them.
            json!({"kind": "narration", "text": "Looking at the repo"}),
            json!({"kind": "narration", "text": "They want the tests green"}),
            // The detail is the location path, relative to the worktree.
            json!({"kind": "tool", "name": "Read src/main.rs", "detail": "src/main.rs"}),
            json!({
                "kind": "usage",
                "contextUsed": 12000,
                "contextSize": 200_000,
                "costUsd": 0.12
            }),
            // The turn end flushed `m2`.
            json!({"kind": "narration", "text": "Done."}),
        ]
    );
}

#[test]
fn a_tool_call_update_is_a_local_card_and_never_a_wire_row() {
    let local = local("turn.jsonl");
    let cards: Vec<&engine::LocalFeedEvent> = local
        .iter()
        .filter(|event| matches!(event, engine::LocalFeedEvent::ToolCall { .. }))
        .collect();
    // One for the call, one for its completion.
    assert_eq!(cards.len(), 2);
    match cards[1] {
        engine::LocalFeedEvent::ToolCall { id, status, .. } => {
            assert_eq!(id, "tc-1");
            assert_eq!(*status, engine::ToolCardStatus::Completed);
        }
        other => panic!("expected a tool card, got {other:?}"),
    }
    // A thought is a local item as well as a capped narration.
    assert!(local
        .iter()
        .any(|event| matches!(event, engine::LocalFeedEvent::Thought { .. })));
}

#[test]
fn a_command_never_reaches_the_wire_and_a_secret_never_reaches_a_detail() {
    let wire = wire("execute.jsonl");
    assert_eq!(
        wire,
        vec![json!({"kind": "tool", "name": "Run the tests", "detail": "bun"})]
    );
    let serialized = serde_json::to_string(&wire).expect("the vector serializes");
    assert!(
        !serialized.contains("expu_"),
        "no secret may reach the relay: {serialized}"
    );
    assert!(
        !serialized.contains("--token"),
        "no raw command may reach the relay: {serialized}"
    );
}

#[test]
fn command_output_and_edit_diffs_stay_local() {
    let local = local("execute.jsonl");
    let output = local
        .iter()
        .find_map(|event| match event {
            engine::LocalFeedEvent::Output {
                tool_call_id,
                chunk,
                exit_code,
            } => Some((tool_call_id.clone(), chunk.clone(), *exit_code)),
            _ => None,
        })
        .expect("the execute call's output is a local card");
    assert_eq!(output, ("tc-9".to_string(), "12 passed".to_string(), Some(0)));

    let diff = local
        .iter()
        .find_map(|event| match event {
            engine::LocalFeedEvent::EditDiff {
                tool_call_id,
                path,
                new_text,
                ..
            } => Some((tool_call_id.clone(), path.clone(), new_text.clone())),
            _ => None,
        })
        .expect("the edit's diff is a local card");
    assert_eq!(
        diff,
        (
            "tc-2".to_string(),
            PathBuf::from("/tmp/worktree/src/lib.rs"),
            "new".to_string()
        )
    );
}

/// D3 + the four clients' "Plan ready" card: `planMode` plus the plan
/// markdown as `text` is what makes an ExitPlanMode approval render as a plan
/// instead of a generic question. Byte-exact per `frames.rs` conventions
/// (absent optionals are omitted, `key` is the ACP option id).
#[test]
fn the_plan_approval_card_is_byte_exact() {
    use agent_client_protocol::schema::v1::{
        ContentBlock, PermissionOption, PermissionOptionId, PermissionOptionKind,
        RequestPermissionRequest, SessionId, TextContent, ToolCallContent, ToolCallId,
        ToolCallUpdate, ToolCallUpdateFields, ToolKind,
    };
    let mut mapper = mapper();
    let mut out = MapOut::default();
    let request = RequestPermissionRequest::new(
        SessionId::new("acp-1"),
        ToolCallUpdate::new(
            ToolCallId::new("toolu_01plan"),
            ToolCallUpdateFields::new()
                .kind(ToolKind::SwitchMode)
                .title("Ready to code?")
                .content(vec![ToolCallContent::from(ContentBlock::Text(
                    TextContent::new("## Plan\n\n1. Write the mapper\n2. Test it"),
                ))]),
        ),
        vec![
            PermissionOption::new(
                PermissionOptionId::new("reject"),
                "No, keep planning",
                PermissionOptionKind::RejectOnce,
            ),
            PermissionOption::new(
                PermissionOptionId::new("accept_edits"),
                "Yes, and auto-accept edits",
                PermissionOptionKind::AllowAlways,
            ),
            PermissionOption::new(
                PermissionOptionId::new("accept"),
                "Yes",
                PermissionOptionKind::AllowOnce,
            ),
        ],
    );
    let key = mapper.on_permission(&request, &mut out);
    assert_eq!(key.question_id, "toolu_01plan");
    assert_eq!(
        serde_json::to_value(&out.wire[0]).expect("the question serializes"),
        json!({
            "kind": "question",
            "text": "## Plan\n\n1. Write the mapper\n2. Test it",
            "options": [
                {"label": "Yes", "key": "accept"},
                {"label": "Yes, and auto-accept edits", "key": "accept_edits"},
                {"label": "No, keep planning", "key": "reject"}
            ],
            "planMode": true,
            "id": "toolu_01plan"
        })
    );
    assert_eq!(out.needs_input, Some(true));
}

/// An elicitation is a STEPPER: `<ask>#<n>` with index/total, then
/// `<ask>#submit` with BOTH absent — that absence is the submit marker (D3).
#[test]
fn an_elicitation_steps_then_submits() {
    use agent_client_protocol::schema::v1::{
        CreateElicitationRequest, ElicitationFormMode, ElicitationPropertySchema, ElicitationSchema,
        ElicitationSessionScope, SessionId, StringPropertySchema,
    };
    let mut mapper = mapper();
    let mut out = MapOut::default();
    let request = CreateElicitationRequest::new(
        ElicitationFormMode::new(
            ElicitationSessionScope::new(SessionId::new("acp-1")),
            ElicitationSchema::new().property(
                "approach",
                ElicitationPropertySchema::String(
                    StringPropertySchema::new()
                        .title("Which approach?")
                        .enum_values(vec!["rewrite".to_string(), "patch".to_string()]),
                ),
                true,
            ),
        ),
        "The agent has a question",
    );
    let key = mapper.on_elicitation("ask-1", &request, &mut out);
    assert_eq!(key.question_id, "ask-1#0");
    assert_eq!(key.ask_id.as_deref(), Some("ask-1"));
    assert_eq!(
        serde_json::to_value(&out.wire[0]).expect("the step serializes"),
        json!({
            "kind": "question",
            "text": "Which approach?",
            "options": [
                {"label": "rewrite", "key": "rewrite"},
                {"label": "patch", "key": "patch"}
            ],
            "id": "ask-1#0",
            "askId": "ask-1",
            "index": 1,
            "total": 1
        })
    );

    // Answering the step publishes the submit marker and resolves nothing.
    let mut stepped = MapOut::default();
    let answer = steer::RemoteAnswer {
        question_id: "ask-1#0".to_string(),
        ask_id: Some("ask-1".to_string()),
        keys: vec!["patch".to_string()],
        text: None,
    };
    let decision = mapper.on_answer(&mapper.ask_key(&answer), &answer, &mut stepped);
    assert_eq!(
        decision,
        engine::AnswerDecision::Elicitation {
            fields: json!({"approach": "patch"}),
            submit: false
        }
    );
    let submit = serde_json::to_value(stepped.wire.last().expect("the submit step"))
        .expect("the submit step serializes");
    assert_eq!(submit["id"], "ask-1#submit");
    assert_eq!(submit.get("index"), None);
    assert_eq!(submit.get("total"), None);

    // The submit answer hands the whole form back.
    let mut submitted = MapOut::default();
    let answer = steer::RemoteAnswer {
        question_id: "ask-1#submit".to_string(),
        ask_id: Some("ask-1".to_string()),
        keys: vec!["submit".to_string()],
        text: None,
    };
    assert_eq!(
        mapper.on_answer(&mapper.ask_key(&answer), &answer, &mut submitted),
        engine::AnswerDecision::Elicitation {
            fields: json!({"approach": "patch"}),
            submit: true
        }
    );
    assert_eq!(submitted.needs_input, Some(false));
}

/// The relay drops an over-cap frame WHOLE (its schema is a discriminated
/// union), so nothing may exceed the caps — in UTF-8 bytes.
#[test]
fn every_wire_string_is_capped() {
    use agent_client_protocol::schema::v1::{
        ContentBlock, ContentChunk, SessionId, SessionNotification, SessionUpdate, TextContent,
        ToolCall, ToolCallId, ToolKind,
    };
    let mut mapper = mapper();
    let mut out = MapOut::default();
    let long = "ü".repeat(steer::activity::NARRATION_MAX);
    mapper.on_update(
        &SessionNotification::new(
            SessionId::new("acp-1"),
            SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(
                TextContent::new(&long),
            ))),
        ),
        &mut out,
    );
    mapper.on_update(
        &SessionNotification::new(
            SessionId::new("acp-1"),
            SessionUpdate::ToolCall(
                ToolCall::new(ToolCallId::new("tc-long"), "x".repeat(1024))
                    .kind(ToolKind::Other)
                    .raw_input(json!({"description": "y".repeat(4096)})),
            ),
        ),
        &mut out,
    );
    mapper.on_stop(StopReason::EndTurn, &mut out);

    for event in &out.wire {
        match event {
            steer::ActivityEvent::Narration { text, .. } => {
                assert!(text.len() <= steer::activity::NARRATION_MAX, "{}", text.len())
            }
            steer::ActivityEvent::Tool { name, detail, .. } => {
                assert!(name.len() <= steer::activity::TOOL_NAME_MAX, "{}", name.len());
                let detail = detail.as_deref().unwrap_or_default();
                assert!(
                    detail.len() <= steer::activity::TOOL_DETAIL_MAX,
                    "{}",
                    detail.len()
                );
            }
            other => panic!("unexpected event {other:?}"),
        }
    }
}
