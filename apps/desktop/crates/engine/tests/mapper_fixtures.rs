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
        redactor: std::sync::Arc::new(steer::Redactor::new(vec![
            "expu_supersecretkey".to_string(),
        ])),
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
            json!({"kind": "tool", "name": "Read", "detail": "src/main.rs"}),
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
        vec![json!({"kind": "tool", "name": "Bash", "detail": "bun"})]
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

/// EXP-750: a `ToolCallContent::Terminal` is a LOCAL binding edge — the
/// terminal id names the live command the card renders, and neither the id
/// nor the command line it came from may reach the relay.
#[test]
fn a_terminal_tool_call_binds_locally_and_never_reaches_the_wire() {
    let bindings: Vec<(String, String)> = local("terminal.jsonl")
        .iter()
        .filter_map(|event| match event {
            engine::LocalFeedEvent::TerminalBound {
                tool_call_id,
                terminal_id,
            } => Some((tool_call_id.clone(), terminal_id.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        bindings,
        vec![
            ("tc-7".to_string(), "term-1".to_string()),
            // The completion repeats the embed; binding is idempotent
            // downstream (the engine flushes once).
            ("tc-7".to_string(), "term-1".to_string()),
        ]
    );

    let wire = wire("terminal.jsonl");
    assert_eq!(
        wire,
        vec![json!({"kind": "tool", "name": "Bash", "detail": "bun"})]
    );
    let serialized = serde_json::to_string(&wire).expect("the vector serializes");
    assert!(
        !serialized.contains("term-1"),
        "a terminal id is local plumbing: {serialized}"
    );
    assert!(
        !serialized.contains("expu_"),
        "no secret may reach the relay: {serialized}"
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
fn form_request(
    message: &str,
    properties: Vec<(&str, agent_client_protocol::schema::v1::ElicitationPropertySchema)>,
) -> agent_client_protocol::schema::v1::CreateElicitationRequest {
    use agent_client_protocol::schema::v1::{
        CreateElicitationRequest, ElicitationFormMode, ElicitationSchema, ElicitationSessionScope,
        SessionId,
    };
    let mut schema = ElicitationSchema::new();
    for (name, property) in properties {
        schema = schema.property(name, property, false);
    }
    CreateElicitationRequest::new(
        ElicitationFormMode::new(ElicitationSessionScope::new(SessionId::new("acp-1")), schema),
        message,
    )
}

fn choice(title: &str) -> agent_client_protocol::schema::v1::ElicitationPropertySchema {
    use agent_client_protocol::schema::v1::{ElicitationPropertySchema, StringPropertySchema};
    ElicitationPropertySchema::String(
        StringPropertySchema::new()
            .title(title)
            .enum_values(vec!["rewrite".to_string(), "patch".to_string()]),
    )
}

fn answer(id: &str, keys: &[&str], text: Option<&str>) -> steer::RemoteAnswer {
    steer::RemoteAnswer {
        question_id: id.to_string(),
        ask_id: Some("ask-1".to_string()),
        keys: keys.iter().map(|key| key.to_string()).collect(),
        text: text.map(str::to_string),
    }
}

/// A form with ONE property is one card: the request message is the
/// question, the property title its header, and answering it IS the submit
/// (the one tap the PTY card took).
#[test]
fn a_lone_step_form_is_one_card_that_submits_on_its_answer() {
    let mut mapper = mapper();
    let mut out = MapOut::default();
    let request = form_request("The agent has a question", vec![("approach", choice("Which approach?"))]);
    let key = mapper.on_elicitation("ask-1", &request, &mut out);
    assert_eq!(key.question_id, "ask-1#0");
    assert_eq!(key.ask_id.as_deref(), Some("ask-1"));
    assert_eq!(
        serde_json::to_value(&out.wire[0]).expect("the step serializes"),
        json!({
            "kind": "question",
            "text": "The agent has a question",
            "options": [
                {"label": "rewrite", "key": "rewrite"},
                {"label": "patch", "key": "patch"}
            ],
            "id": "ask-1#0",
            "askId": "ask-1",
            "index": 1,
            "total": 1,
            "header": "Which approach?"
        })
    );

    let mut answered = MapOut::default();
    let answer = answer("ask-1#0", &["patch"], None);
    assert_eq!(
        mapper.on_answer(&mapper.ask_key(&answer), &answer, &mut answered),
        engine::AnswerDecision::Elicitation {
            fields: json!({"approach": "patch"}),
            submit: true
        }
    );
    let kinds: Vec<Value> = answered.wire.iter().map(|event| serde_json::to_value(event).unwrap()["kind"].clone()).collect();
    assert_eq!(kinds, vec![json!("answer_ack"), json!("question_resolved")], "no submit card for a lone step");
    assert_eq!(answered.needs_input, Some(false));
}

/// Several properties step through `<ask>#<n>` and end on the `#submit`
/// marker (index and total absent), which hands the whole form back.
#[test]
fn a_multi_step_form_steps_then_submits() {
    let mut mapper = mapper();
    let mut out = MapOut::default();
    let request = form_request(
        "Please answer the following questions.",
        vec![("approach", choice("Which approach?")), ("scope", choice("Which scope?"))],
    );
    mapper.on_elicitation("ask-1", &request, &mut out);
    let first = serde_json::to_value(&out.wire[0]).unwrap();
    assert_eq!(first["text"], "Which approach?");
    assert_eq!(first["index"], 1);
    assert_eq!(first["total"], 2);
    assert_eq!(first.get("header"), None);

    let mut stepped = MapOut::default();
    let answer1 = answer("ask-1#0", &["patch"], None);
    assert_eq!(
        mapper.on_answer(&mapper.ask_key(&answer1), &answer1, &mut stepped),
        engine::AnswerDecision::Elicitation { fields: json!({"approach": "patch"}), submit: false }
    );
    let second = serde_json::to_value(stepped.wire.last().unwrap()).unwrap();
    assert_eq!(second["id"], "ask-1#1");
    assert_eq!(second["index"], 2);

    let mut stepped = MapOut::default();
    let answer2 = answer("ask-1#1", &["rewrite"], None);
    mapper.on_answer(&mapper.ask_key(&answer2), &answer2, &mut stepped);
    let submit = serde_json::to_value(stepped.wire.last().expect("the submit step")).unwrap();
    assert_eq!(submit["id"], "ask-1#submit");
    assert_eq!(submit.get("index"), None);
    assert_eq!(submit.get("total"), None);

    let mut submitted = MapOut::default();
    let answer3 = answer("ask-1#submit", &["submit"], None);
    assert_eq!(
        mapper.on_answer(&mapper.ask_key(&answer3), &answer3, &mut submitted),
        engine::AnswerDecision::Elicitation {
            fields: json!({"approach": "patch", "scope": "rewrite"}),
            submit: true
        }
    );
    assert_eq!(submitted.needs_input, Some(false));
}

/// Claude's AskUserQuestion form pairs every choice `question_<n>` with a
/// plain-string `question_<n>_custom`: that sibling folds into the choice's
/// card as its "Type something." row, and a typed answer lands on the custom
/// field (custom wins, as the CLI's own picker has it) while picking the row
/// with nothing typed answers nothing — never the literal row key.
#[test]
fn a_choice_with_a_custom_sibling_is_one_card_with_a_free_text_row() {
    use agent_client_protocol::schema::v1::{ElicitationPropertySchema, EnumOption, StringPropertySchema};
    let form = || {
        form_request(
            "Which color do you prefer?",
            vec![
                (
                    "question_0",
                    ElicitationPropertySchema::String(StringPropertySchema::new().title("Color").one_of(vec![
                        EnumOption::new("Red", "Red").description("Choose red."),
                        EnumOption::new("Blue", "Blue"),
                    ])),
                ),
                (
                    "question_0_custom",
                    ElicitationPropertySchema::String(StringPropertySchema::new().title("Other")),
                ),
            ],
        )
    };

    let mut m = mapper();
    let mut out = MapOut::default();
    m.on_elicitation("ask-1", &form(), &mut out);
    assert_eq!(
        serde_json::to_value(&out.wire[0]).expect("the card serializes"),
        json!({
            "kind": "question",
            "text": "Which color do you prefer?",
            "options": [
                {"label": "Red", "key": "Red", "description": "Choose red."},
                {"label": "Blue", "key": "Blue"},
                {"label": "Type something.", "key": "text", "freeText": true}
            ],
            "id": "ask-1#0",
            "askId": "ask-1",
            "index": 1,
            "total": 1,
            "header": "Color"
        })
    );
    let mut answered = MapOut::default();
    let pick = answer("ask-1#0", &["Red"], None);
    assert_eq!(
        m.on_answer(&m.ask_key(&pick), &pick, &mut answered),
        engine::AnswerDecision::Elicitation { fields: json!({"question_0": "Red"}), submit: true }
    );

    let mut m = mapper();
    m.on_elicitation("ask-1", &form(), &mut MapOut::default());
    let mut answered = MapOut::default();
    let typed = answer("ask-1#0", &["text"], Some("Green"));
    assert_eq!(
        m.on_answer(&m.ask_key(&typed), &typed, &mut answered),
        engine::AnswerDecision::Elicitation { fields: json!({"question_0_custom": "Green"}), submit: true }
    );
    let resolved = serde_json::to_value(answered.wire.last().unwrap()).unwrap();
    assert_eq!(resolved["answers"], json!(["Green"]));

    let mut m = mapper();
    m.on_elicitation("ask-1", &form(), &mut MapOut::default());
    let mut answered = MapOut::default();
    let empty = answer("ask-1#0", &["text"], Some(""));
    assert_eq!(
        m.on_answer(&m.ask_key(&empty), &empty, &mut answered),
        engine::AnswerDecision::Elicitation { fields: json!({}), submit: true }
    );
    let resolved = serde_json::to_value(answered.wire.last().unwrap()).unwrap();
    assert_eq!(resolved["answers"], json!([]));
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


/// EXP-758: the meter is clamped to the relay's zod bounds, and an
/// unchanged one is not re-sent.
///
/// The relay's `activityEvent` is a discriminated union: a `contextUsed` a
/// `u64 as i64` cast turned NEGATIVE (or a cost below zero) fails the schema
/// and the WHOLE frame is dropped in silence, freezing every viewer's meter
/// for the rest of the run.
#[test]
fn a_nonsense_meter_is_clamped_into_the_relays_bounds_and_never_repeated() {
    assert_eq!(
        wire("usage.jsonl"),
        vec![
            json!({
                "kind": "usage",
                "contextUsed": 1_000_000_000,
                "contextSize": 1_000_000_000,
                "costUsd": 0.0
            }),
            // The identical second frame said nothing and went nowhere.
            json!({
                "kind": "usage",
                "contextUsed": 12000,
                "contextSize": 200_000,
                "costUsd": 0.12
            }),
        ]
    );
}

/// EXP-758: an agent's `/` command catalog is built from files in the REPO,
/// so its three labels are redacted like every other published string; and
/// an unchanged `config_state` is not published twice (one `set_config` used
/// to emit two identical frames, one `set_mode` three).
#[test]
fn command_labels_are_redacted_and_an_unchanged_snapshot_is_not_republished() {
    let wire = wire("commands.jsonl");
    assert_eq!(wire.len(), 1, "the identical second snapshot is dropped");
    assert_eq!(
        wire[0],
        json!({
            "kind": "config_state",
            "options": [],
            "commands": [{
                "name": "deploy",
                "description": "Ship it with [redacted]",
                "hint": "target [redacted]"
            }]
        })
    );
}

/// EXP-758: two id-less chunks are two whole messages (pi's error narration,
/// codex's `codex error:` lines), never one glued string; chunks that share a
/// `message_id` still coalesce into one.
#[test]
fn id_less_chunks_are_separated_and_streamed_ones_still_coalesce() {
    assert_eq!(
        wire("idless.jsonl"),
        vec![
            json!({
                "kind": "narration",
                "text": "pi: Codex error: stream closed\npi: Codex error: no response"
            }),
            json!({"kind": "narration", "text": "Looking at the repo"}),
        ]
    );
}

/// EXP-753 — a subagent's edge and the rows it produced name the SAME id.
///
/// The claude adapter rides the edge on a no-op patch of the Task call that
/// spawned the subagent and stamps `subagentId` on everything that subagent
/// did, so a client can nest the rows under the card without a second
/// lookup; this replays that shape through the mapper and locks the wire.
#[test]
fn subagent_edges_and_their_tool_rows_carry_the_parent_id() {
    assert_eq!(
        wire("subagent.jsonl"),
        vec![
            // The Task call itself is an ordinary tool row: the edge that
            // follows is what turns it into a subagent card.
            json!({"kind": "tool", "name": "Find the failing test"}),
            json!({"kind": "subagent", "id": "tc-parent", "agentType": "explore", "status": "started"}),
            // What the subagent ran, attributed to the card, never a top-level
            // row of its own.
            json!({"kind": "tool", "name": "Bash", "detail": "bun", "subagentId": "tc-parent"}),
            // EXP-748: the completed edge carries the tool-call count — the
            // mapper's own count of attributed calls, or the adapter's
            // `toolCalls` meta when that is larger.
            json!({"kind": "subagent", "id": "tc-parent", "agentType": "explore", "status": "completed", "toolCalls": 1}),
        ]
    );
}
