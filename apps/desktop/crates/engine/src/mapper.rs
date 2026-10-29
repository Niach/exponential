//! The ONE `SessionUpdate` → wire mapper moved to the agent core (EXP-1426
//! part 4b, `exponential_agent_runtime::engine::mapper`); every item is
//! re-exported here under its old path. What stays is Exponential's half:
//! the [`HostTools`] answering for its MCP surface (the contract's
//! `expToolDisplay` / `expToolPreview` tables) and its in-process code-mode
//! server, and the session-shaped [`MapperConfig`] every caller builds, which
//! carries them into the core's config.

use std::path::PathBuf;
use std::sync::Arc;

pub use exponential_agent_runtime::engine::mapper::*;
use exponential_agent_runtime::engine::{HostTools, ScriptCall, ScriptReport};

/// Exponential's own tools, by the contract tables and the code-mode report
/// channel.
#[derive(Clone, Copy, Debug, Default)]
pub struct ExpHostTools;

impl HostTools for ExpHostTools {
    fn row(&self, title: &str) -> Option<&'static str> {
        steer::exp_tool_row(title)
    }

    fn subject_key(&self, title: &str) -> Option<&'static str> {
        steer::exp_tool_subject_key(title)
    }

    fn preview_spec(&self, title: &str) -> Option<&'static str> {
        steer::exp_tool::exp_tool_preview_spec(title)
    }

    fn take_script_report(&self, session_seed: &str, script: &str) -> Option<ScriptReport> {
        let report = coding::codemode::take_report(session_seed, script)?;
        Some(ScriptReport {
            count: report.count,
            elapsed_ms: report.elapsed_ms,
            calls: report
                .calls
                .into_iter()
                .map(|call| ScriptCall {
                    tool: call.tool,
                    subject: call.subject,
                    ok: call.ok,
                    ms: call.ms,
                })
                .collect(),
        })
    }
}

/// Everything the mapper needs that is constant for a session — the core's
/// config with Exponential's tools ([`ExpHostTools`]) filled in.
pub struct MapperConfig {
    /// Session secrets (installation token, the `expu_` personal key) plus the
    /// static patterns — every wire string passes through it. Shared
    /// (`SessionCtx.redactor`) so the lifecycle's `diff` ticker masks with the
    /// SAME secret set as the mapper (REV2-17).
    pub redactor: Arc<steer::Redactor>,
    /// The worktree, for relativizing tool-call paths.
    pub cwd: PathBuf,
    /// Keys the command catalog and the codex sigil guard.
    pub agent: steer::SessionAgent,
    /// Seed for `steer::synthetic_question_id` when the agent stamps no id.
    pub session_seed: String,
}

impl From<MapperConfig> for exponential_agent_runtime::engine::MapperConfig {
    fn from(config: MapperConfig) -> Self {
        Self {
            redactor: config.redactor,
            cwd: config.cwd,
            agent: config.agent,
            session_seed: config.session_seed,
            host_tools: Arc::new(ExpHostTools),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::schema::v1::{
        SessionId, SessionNotification, SessionUpdate, ToolCall, ToolCallContent, ToolCallId,
        ToolCallStatus, ToolCallUpdate, ToolCallUpdateFields, ToolKind,
    };
    use serde_json::json;
    use steer::{ActivityEvent, ToolKind as WireToolKind};

    fn mapper() -> Mapper {
        Mapper::new(MapperConfig {
            redactor: Arc::new(steer::Redactor::new(vec!["expu_supersecretkey".to_string()])),
            cwd: PathBuf::from("/tmp/worktree"),
            agent: steer::SessionAgent::Claude,
            session_seed: "sess-1".to_string(),
        })
    }

    fn notify(update: SessionUpdate) -> SessionNotification {
        SessionNotification::new(SessionId::new("acp-1"), update)
    }

    /// EXP-1426: the core inlines these contract values; a contract change
    /// fails here instead of drifting.
    #[test]
    fn the_core_caps_are_the_contract_caps() {
        assert_eq!(REPLY_BODY_MAX, domain::contract::SESSIONS_SEND_MESSAGE_BODY_MAX);
        assert_eq!(
            WORKING_TOKEN_TICK_MS,
            domain::contract::STEER_WORKING_TOKEN_TICK_MS.max(0) as u64
        );
        assert_eq!(WORKING_PREVIEW_MAX, domain::contract::STEER_WORKING_PREVIEW_MAX);
        assert_eq!(EMPTY_TURN_NOTICE, domain::session_tree::THREAD.copy.empty_turn);
        assert_eq!(
            exponential_agent_runtime::engine::host_tools::CODEMODE_SERVER_NAME,
            coding::codemode::SERVER_NAME
        );
    }

    /// EXP-1202 / EXP-1220: the CLAUDE adapter's settle of an Exponential
    /// call — the answer text as content AND the `{content:[…]}` envelope as
    /// `raw_output` — publishes the answer's id as `preview.id`; a failed
    /// one, which carries no `raw_output`, publishes no preview.
    #[test]
    fn a_claude_shaped_exp_tool_settle_publishes_the_answer_id() {
        let mut mapper = mapper();
        let title = "mcp__exponential__exponential_attachments_upload";
        let answer = r#"{"id":"7c0b9f3e-2d4a-4e8b-9a61-3f5d2c1b0a99","fileName":"shot.png"}"#;
        for id in ["tc-show-1", "tc-show-2"] {
            let call = ToolCall::new(ToolCallId::new(id), title)
                .kind(ToolKind::Other)
                .status(ToolCallStatus::InProgress)
                .raw_input(json!({"file": "/work/tree/shot.png"}));
            mapper.on_update(&notify(SessionUpdate::ToolCall(call)), &mut MapOut::default());
        }
        let text = vec![ToolCallContent::from(answer.to_string())];

        let mut out = MapOut::default();
        let update = ToolCallUpdate::new(
            ToolCallId::new("tc-show-1"),
            ToolCallUpdateFields::new()
                .status(ToolCallStatus::Completed)
                .content(text.clone())
                .raw_output(json!({"content": [{"type": "text", "text": answer}]})),
        );
        mapper.on_update(&notify(SessionUpdate::ToolCallUpdate(update)), &mut out);
        let preview = out
            .wire
            .iter()
            .find_map(|event| match event {
                ActivityEvent::ToolUpdate { preview: Some(preview), .. } => Some(preview.clone()),
                _ => None,
            })
            .expect("the settle carries a preview");
        assert_eq!(preview.id.as_deref(), Some("7c0b9f3e-2d4a-4e8b-9a61-3f5d2c1b0a99"));

        let mut out = MapOut::default();
        let update = ToolCallUpdate::new(
            ToolCallId::new("tc-show-2"),
            ToolCallUpdateFields::new().status(ToolCallStatus::Failed).content(text),
        );
        mapper.on_update(&notify(SessionUpdate::ToolCallUpdate(update)), &mut out);
        assert!(
            out.wire.iter().any(|event| matches!(
                event,
                ActivityEvent::ToolUpdate { status: Some(_), preview: None, .. }
            )),
            "a failed call settles with no preview: {:?}",
            out.wire
        );
    }

    /// EXP-920: a `$`-rooted spec term reads the retained INPUT — `pr_open`
    /// answers with a url and no issue, the input named it. The preview is
    /// published on the strength of the refs alone.
    #[test]
    fn a_dollar_rooted_spec_reads_the_retained_input() {
        let mut mapper = mapper();
        let mut out = MapOut::default();
        let call = ToolCall::new(ToolCallId::new("tc-refs-2"), "exponential_pr_open")
            .kind(ToolKind::Other)
            .raw_input(json!({"issueId": "EXP-895", "title": "Fix it"}));
        mapper.on_update(&notify(SessionUpdate::ToolCall(call)), &mut out);

        let mut out = MapOut::default();
        let update = ToolCallUpdate::new(
            ToolCallId::new("tc-refs-2"),
            ToolCallUpdateFields::new()
                .status(ToolCallStatus::Completed)
                .raw_output(json!({"ok": true})),
        );
        mapper.on_update(&notify(SessionUpdate::ToolCallUpdate(update)), &mut out);
        let preview = out
            .wire
            .iter()
            .find_map(|event| match event {
                ActivityEvent::ToolUpdate { preview: Some(preview), .. } => Some(preview.clone()),
                _ => None,
            })
            .expect("refs alone are a preview");
        assert_eq!(preview.id, None);
        let mut expected = steer::frames::EntityRef::new("issue", "EXP-895");
        expected.identifier = Some("EXP-895".to_string());
        assert_eq!(preview.refs, vec![expected]);

        // A tool that is not ours never retains its input.
        let mut out = MapOut::default();
        let call = ToolCall::new(ToolCallId::new("tc-refs-3"), "Read")
            .kind(ToolKind::Read)
            .raw_input(json!({"file_path": "/tmp/worktree/big.txt"}));
        mapper.on_update(&notify(SessionUpdate::ToolCall(call)), &mut out);
        assert_eq!(mapper.retains_input("tc-refs-3"), Some(false));
    }

    /// EXP-920: the settle of one of our calls publishes the entity REFS its
    /// answer named beside the legacy subject — the call's `raw_input` is
    /// retained from the `ToolCall` so a `$`-rooted term can read it later.
    #[test]
    fn a_settled_exponential_call_publishes_its_refs() {
        let mut mapper = mapper();
        let mut out = MapOut::default();
        let call = ToolCall::new(
            ToolCallId::new("tc-refs-1"),
            "mcp__exponential__exponential_issues_create",
        )
        .kind(ToolKind::Other)
        .raw_input(json!({"boardId": "b-1", "title": "Fix the flicker"}));
        mapper.on_update(&notify(SessionUpdate::ToolCall(call)), &mut out);
        assert_eq!(mapper.retains_input("tc-refs-1"), Some(true), "the input is retained");

        let mut out = MapOut::default();
        let update = ToolCallUpdate::new(
            ToolCallId::new("tc-refs-1"),
            ToolCallUpdateFields::new()
                .status(ToolCallStatus::Completed)
                .raw_output(json!({
                    "content": [{
                        "type": "text",
                        "text": "{\"id\":\"i-1\",\"identifier\":\"EXP-42\",\"title\":\"Fix the flicker\",\"status\":\"backlog\",\"statusId\":\"s-1\"}"
                    }]
                })),
        );
        mapper.on_update(&notify(SessionUpdate::ToolCallUpdate(update)), &mut out);
        let preview = out
            .wire
            .iter()
            .find_map(|event| match event {
                ActivityEvent::ToolUpdate { preview: Some(preview), .. } => Some(preview.clone()),
                _ => None,
            })
            .expect("the settle carries a preview");
        // The subject rides along: the web settled caption reads it.
        assert_eq!(preview.identifier.as_deref(), Some("EXP-42"));
        let mut expected = steer::frames::EntityRef::new("issue", "i-1");
        expected.identifier = Some("EXP-42".to_string());
        expected.title = Some("Fix the flicker".to_string());
        assert_eq!(preview.refs, vec![expected]);
    }

    /// EXP-846: one of OUR calls publishes the SUBJECT its contract entry names
    /// (`expToolSubjectKeys`) as the row's detail, so the transcript reads
    /// "Creating issue · Fix the sync loop" instead of a bare caption. A tool
    /// whose entry names no subject (a list) publishes none.
    #[test]
    fn an_exponential_call_publishes_its_contract_subject() {
        let mut mapper = mapper();
        let mut out = MapOut::default();
        let call = ToolCall::new(
            ToolCallId::new("tc-mcp-1"),
            "mcp__exponential__exponential_issues_create",
        )
        .kind(ToolKind::Other)
        .raw_input(json!({"boardId": "b-1", "title": "Fix the sync loop"}));
        mapper.on_update(&notify(SessionUpdate::ToolCall(call)), &mut out);
        match &out.wire[0] {
            ActivityEvent::Tool { name, detail, .. } => {
                // The NAME stays the raw tool name — the caption is the
                // client's, off the contract tables.
                assert_eq!(name, "mcp__exponential__exponential_issues_create");
                assert_eq!(detail.as_deref(), Some("Fix the sync loop"));
            }
            other => panic!("expected a tool event, got {other:?}"),
        }

        // A list names no subject: nothing is invented for it.
        let mut out = MapOut::default();
        let call = ToolCall::new(ToolCallId::new("tc-mcp-2"), "exponential_issues_list")
            .kind(ToolKind::Other)
            .raw_input(json!({"boardId": "b-1"}));
        mapper.on_update(&notify(SessionUpdate::ToolCall(call)), &mut out);
        match &out.wire[0] {
            ActivityEvent::Tool { detail, .. } => assert_eq!(detail, &None),
            other => panic!("expected a tool event, got {other:?}"),
        }
    }

    /// EXP-1279: a code-mode `exec` is a `script` row, and its settle
    /// carries the call log the code-mode server filed for this run and this
    /// script, plus the script itself.
    #[test]
    fn a_codemode_exec_is_a_script_row_whose_settle_carries_its_call_log() {
        let mut mapper = mapper();
        let script = "return await tools.exponential.exponential_issues_get({ id: 'EXP-1279-a' });";
        let mut out = MapOut::default();
        // claude announces the call before its input has streamed in.
        let call = ToolCall::new(ToolCallId::new("tc-script-1"), "mcp__codemode__exec")
            .kind(ToolKind::Other)
            .raw_input(json!({}));
        mapper.on_update(&notify(SessionUpdate::ToolCall(call)), &mut out);
        assert!(
            out.wire.iter().any(|event| matches!(
                event,
                ActivityEvent::Tool { tool_kind: Some(WireToolKind::Script), .. }
            )),
            "{:?}",
            out.wire
        );
        let mut out = MapOut::default();
        let input = ToolCallUpdate::new(
            ToolCallId::new("tc-script-1"),
            ToolCallUpdateFields::new().raw_input(json!({ "script": script, "timeout_ms": 5000 })),
        );
        mapper.on_update(&notify(SessionUpdate::ToolCallUpdate(input)), &mut out);

        coding::codemode::file_report(
            "sess-1",
            script,
            coding::codemode::ScriptReport {
                count: 3,
                elapsed_ms: 1200,
                calls: vec![coding::codemode::CallRecord {
                    tool: "exponential.exponential_issues_get".into(),
                    subject: Some("EXP-1279-a".into()),
                    ok: true,
                    ms: Some(40),
                }],
            },
        );
        let mut out = MapOut::default();
        let settle = ToolCallUpdate::new(
            ToolCallId::new("tc-script-1"),
            ToolCallUpdateFields::new()
                .status(ToolCallStatus::Completed)
                .raw_output(json!({ "content": [{ "type": "text", "text": "{\"ok\":true}" }] })),
        );
        mapper.on_update(&notify(SessionUpdate::ToolCallUpdate(settle)), &mut out);
        let log = out
            .wire
            .iter()
            .find_map(|event| match event {
                ActivityEvent::ToolUpdate { script: Some(script), .. } => Some(script.clone()),
                _ => None,
            })
            .expect("the settle carries the log");
        assert_eq!(
            log,
            steer::frames::ScriptLog {
                count: Some(3),
                elapsed_ms: Some(1200),
                calls: vec![steer::frames::ScriptLogCall {
                    tool: "exponential.exponential_issues_get".into(),
                    subject: Some("EXP-1279-a".into()),
                    ok: true,
                    ms: Some(40),
                }],
                source: Some(script.to_string()),
            }
        );
        assert_eq!(coding::codemode::take_report("sess-1", script), None, "the log was taken");
    }
}
