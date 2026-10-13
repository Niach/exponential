//! EXP-1294 (Threads v2): the people-side `sessions.*` and `pullRequests.*`
//! tRPC calls (web `apps/web/src/lib/trpc/sessions.ts`). The device-side
//! procedures (`brief`, `claim`, `report`, `reply`, `consume`) live with the
//! session host, not here.
//!
//! - `sessions.send` = THE one entrypoint ([`send`]): a new root (team +
//!   device + text/issues/action), a new AGENT (`agent`), a child
//!   (`parentId`) or a message to an existing session (`sessionId`; to an
//!   idle session it resumes it, to an ended thread it continues it).
//! - `stop` interrupts a turn, `end` ends a session (a root: its tree).
//! - `setTitle` (`null` clears), `linkIssues` / `unlinkIssue`, `answer`
//!   (a question/plan card), `unqueue` (a queued person message).
//! - Agents: `getAgent` (the server-only instructions), `updateAgent`,
//!   `deleteAgent`.
//! - `pullRequests.merge` / `close` / `files`.
//!
//! Plain blocking calls: run them on a background executor only. Rows
//! re-stream over the shapes, so callers fire-and-forget; `txId` also
//! decodes as `txid`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::trpc::TrpcClient;

/// A new agent's facets (`sessions.send({agent})`).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentInput {
    pub name: String,
    /// Icon concept.
    pub icon: String,
    /// `#rrggbb`.
    pub color: String,
    pub instructions: String,
    /// `AgentTrigger[]` jsonb, sent as is.
    pub triggers: Vec<serde_json::Value>,
}

/// `sessions.send` input. Every key is optional and omitted when unset; the
/// server's rules decide which combinations are valid (a message needs
/// `session_id` + text/attachments, a child `parent_id` + `device_id`, a new
/// root `team_id` + `device_id` + text/issues/action, a new agent `agent`).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cli: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_mode: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub computer_use: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ultracode: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_server_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachment_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_id: Option<String>,
    /// The action's picked inputs (needs `action_id`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inputs: Option<BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentInput>,
}

impl SendInput {
    /// A message to an existing session (the thread reply, the session
    /// view, an agent channel).
    pub fn message(session_id: &str, text: &str, attachment_ids: Vec<String>) -> Self {
        Self {
            session_id: Some(session_id.to_string()),
            text: Some(text.to_string()).filter(|text| !text.is_empty()),
            attachment_ids: Some(attachment_ids).filter(|ids| !ids.is_empty()),
            ..Self::default()
        }
    }

    /// A child session under `parent_id` on `device_id`.
    pub fn child(parent_id: &str, device_id: &str, text: &str) -> Self {
        Self {
            parent_id: Some(parent_id.to_string()),
            device_id: Some(device_id.to_string()),
            text: Some(text.to_string()),
            ..Self::default()
        }
    }
}

/// `sessions.send` output.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendOutput {
    /// The session written to (new or existing).
    pub session_id: String,
    /// The person message it stored, if any.
    #[serde(default)]
    pub message_id: Option<String>,
    #[serde(default, alias = "txid")]
    pub tx_id: Option<i64>,
}

/// THE send ([`SendInput`]).
pub fn send(trpc: &TrpcClient, input: &SendInput) -> Result<SendOutput, ApiError> {
    trpc.mutation("sessions.send", input)
}

/// Every plain mutation's `{txid}`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxOutput {
    #[serde(default, alias = "txid")]
    pub tx_id: Option<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionIdInput<'a> {
    session_id: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MessageIdInput<'a> {
    message_id: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PullRequestIdInput<'a> {
    pull_request_id: &'a str,
}

/// Interrupt the session's current turn (it stays alive).
pub fn stop(trpc: &TrpcClient, session_id: &str) -> Result<TxOutput, ApiError> {
    trpc.mutation("sessions.stop", &SessionIdInput { session_id })
}

/// End the session (a root ends its whole tree).
pub fn end(trpc: &TrpcClient, session_id: &str) -> Result<TxOutput, ApiError> {
    trpc.mutation("sessions.end", &SessionIdInput { session_id })
}

/// Rename a root; `None` sends `null` (back to the derived title).
pub fn set_title(trpc: &TrpcClient, session_id: &str, title: Option<&str>) -> Result<TxOutput, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        session_id: &'a str,
        title: Option<&'a str>,
    }
    trpc.mutation("sessions.setTitle", &Input { session_id, title })
}

/// Link issues to a root (1..=30).
pub fn link_issues(trpc: &TrpcClient, session_id: &str, issue_ids: &[String]) -> Result<TxOutput, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        session_id: &'a str,
        issue_ids: &'a [String],
    }
    trpc.mutation("sessions.linkIssues", &Input { session_id, issue_ids })
}

/// Unlink one issue from a root.
pub fn unlink_issue(trpc: &TrpcClient, session_id: &str, issue_id: &str) -> Result<TxOutput, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        session_id: &'a str,
        issue_id: &'a str,
    }
    trpc.mutation("sessions.unlinkIssue", &Input { session_id, issue_id })
}

/// `sessions.answer` input: a question card takes `answer`, a plan card
/// `decision` (`approved` | `rejected`) + optional `feedback`.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerInput {
    pub message_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feedback: Option<String>,
}

/// Answer a question/plan card in place.
pub fn answer(trpc: &TrpcClient, input: &AnswerInput) -> Result<TxOutput, ApiError> {
    trpc.mutation("sessions.answer", input)
}

/// Take back a queued (not yet consumed) person message.
pub fn unqueue(trpc: &TrpcClient, message_id: &str) -> Result<TxOutput, ApiError> {
    trpc.mutation("sessions.unqueue", &MessageIdInput { message_id })
}

/// `sessions.getAgent` output: the server-only instructions.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDetails {
    pub session_id: String,
    #[serde(default)]
    pub instructions: String,
}

/// Read an agent's instructions (never synced).
pub fn get_agent(trpc: &TrpcClient, session_id: &str) -> Result<AgentDetails, ApiError> {
    trpc.query_with_input("sessions.getAgent", &SessionIdInput { session_id })
}

/// `sessions.updateAgent` input: unset keys are omitted; `account`/`model`
/// take `Some(None)` to send `null` (back to the device's default).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAgentInput {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub triggers: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<f64>,
}

pub fn update_agent(trpc: &TrpcClient, input: &UpdateAgentInput) -> Result<TxOutput, ApiError> {
    trpc.mutation("sessions.updateAgent", input)
}

pub fn delete_agent(trpc: &TrpcClient, session_id: &str) -> Result<TxOutput, ApiError> {
    trpc.mutation("sessions.deleteAgent", &SessionIdInput { session_id })
}

/// `pullRequests.merge` output: `queued` = a merge queue took it, `error` =
/// GitHub refused (the caller shows it).
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeOutput {
    #[serde(default)]
    pub merged: bool,
    #[serde(default)]
    pub queued: bool,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default, alias = "txid")]
    pub tx_id: Option<i64>,
}

/// Merge a pull request; `stack` = merge THROUGH it (every open PR below
/// lands first).
pub fn pr_merge(trpc: &TrpcClient, pull_request_id: &str, stack: bool) -> Result<MergeOutput, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        pull_request_id: &'a str,
        stack: bool,
    }
    trpc.mutation("pullRequests.merge", &Input { pull_request_id, stack })
}

pub fn pr_close(trpc: &TrpcClient, pull_request_id: &str) -> Result<TxOutput, ApiError> {
    trpc.mutation("pullRequests.close", &PullRequestIdInput { pull_request_id })
}

/// One changed file of a pull request (`patch` = the GitHub patch, `None`
/// for binaries / oversized files).
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrFile {
    pub path: String,
    #[serde(default)]
    pub additions: i64,
    #[serde(default)]
    pub deletions: i64,
    #[serde(default)]
    pub patch: Option<String>,
}

/// A pull request's changed files.
pub fn pr_files(trpc: &TrpcClient, pull_request_id: &str) -> Result<Vec<PrFile>, ApiError> {
    #[derive(Deserialize)]
    struct Output {
        #[serde(default)]
        files: Vec<PrFile>,
    }
    let output: Output = trpc.query_with_input("pullRequests.files", &PullRequestIdInput { pull_request_id })?;
    Ok(output.files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trpc::tests::one_shot_server;
    use crate::StaticToken;
    use std::sync::Arc;
    use std::time::Duration;

    fn client(base: &str) -> TrpcClient {
        TrpcClient::new(base, Arc::new(StaticToken("tok".to_string())))
    }

    fn request(captured: &flume::Receiver<String>) -> String {
        captured.recv_timeout(Duration::from_secs(5)).unwrap()
    }

    #[test]
    fn send_a_message_omits_every_unset_key() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"sessionId":"s-1","messageId":"m-1","txid":9}}}"#,
        );
        let out = send(&client(&base), &SendInput::message("s-1", "Ship it", Vec::new())).unwrap();
        assert_eq!(out.session_id, "s-1");
        assert_eq!(out.message_id.as_deref(), Some("m-1"));
        assert_eq!(out.tx_id, Some(9));
        let request = request(&captured);
        assert!(request.starts_with("POST /api/trpc/sessions.send HTTP/1.1"), "{request}");
        assert!(request.ends_with(r#"{"sessionId":"s-1","text":"Ship it"}"#), "{request}");
    }

    #[test]
    fn send_a_new_root_and_a_new_agent() {
        let (base, captured) =
            one_shot_server(200, r#"{"result":{"data":{"sessionId":"s-2","messageId":null,"txId":3}}}"#);
        let input = SendInput {
            team_id: Some("t-1".into()),
            device_id: Some("dev-1".into()),
            cli: Some("claude".into()),
            plan_mode: Some(true),
            text: Some("Fix it".into()),
            issue_ids: Some(vec!["i-1".into()]),
            attachment_ids: Some(vec!["a-1".into()]),
            ..SendInput::default()
        };
        let out = send(&client(&base), &input).unwrap();
        assert_eq!(out.message_id, None);
        assert_eq!(out.tx_id, Some(3));
        assert!(
            request(&captured).ends_with(
                r#"{"teamId":"t-1","deviceId":"dev-1","cli":"claude","planMode":true,"text":"Fix it","attachmentIds":["a-1"],"issueIds":["i-1"]}"#
            )
        );

        let (base, captured) =
            one_shot_server(200, r#"{"result":{"data":{"sessionId":"ag-1","messageId":null,"txid":4}}}"#);
        let input = SendInput {
            team_id: Some("t-1".into()),
            device_id: Some("dev-1".into()),
            agent: Some(AgentInput {
                name: "Triage".into(),
                icon: "bot".into(),
                color: "#22c55e".into(),
                instructions: String::new(),
                triggers: vec![serde_json::json!({"kind": "schedule"})],
            }),
            ..SendInput::default()
        };
        send(&client(&base), &input).unwrap();
        assert!(request(&captured).ends_with(
            r##"{"teamId":"t-1","deviceId":"dev-1","agent":{"name":"Triage","icon":"bot","color":"#22c55e","instructions":"","triggers":[{"kind":"schedule"}]}}"##
        ));
    }

    #[test]
    fn send_a_child_and_action_inputs() {
        let (base, captured) =
            one_shot_server(200, r#"{"result":{"data":{"sessionId":"c-1","messageId":"m-2","txid":1}}}"#);
        send(&client(&base), &SendInput::child("s-1", "dev-1", "Review")).unwrap();
        assert!(request(&captured).ends_with(r#"{"parentId":"s-1","deviceId":"dev-1","text":"Review"}"#));

        let (base, captured) =
            one_shot_server(200, r#"{"result":{"data":{"sessionId":"s-3","messageId":null,"txid":1}}}"#);
        let input = SendInput {
            team_id: Some("t-1".into()),
            device_id: Some("dev-1".into()),
            action_id: Some("act-1".into()),
            inputs: Some(BTreeMap::from([("repo".to_string(), "r-1".to_string())])),
            ..SendInput::default()
        };
        send(&client(&base), &input).unwrap();
        assert!(request(&captured)
            .ends_with(r#"{"teamId":"t-1","deviceId":"dev-1","actionId":"act-1","inputs":{"repo":"r-1"}}"#));
    }

    #[test]
    fn stop_end_and_delete_post_the_session_id() {
        for (path, call) in [
            ("sessions.stop", stop as fn(&TrpcClient, &str) -> Result<TxOutput, ApiError>),
            ("sessions.end", end),
            ("sessions.deleteAgent", delete_agent),
        ] {
            let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txid":5}}}"#);
            assert_eq!(call(&client(&base), "s-1").unwrap().tx_id, Some(5));
            let request = request(&captured);
            assert!(request.starts_with(&format!("POST /api/trpc/{path} HTTP/1.1")), "{request}");
            assert!(request.ends_with(r#"{"sessionId":"s-1"}"#), "{request}");
        }
    }

    #[test]
    fn set_title_sends_null_to_clear() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txid":1}}}"#);
        set_title(&client(&base), "s-1", None).unwrap();
        assert!(request(&captured).ends_with(r#"{"sessionId":"s-1","title":null}"#));
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txid":1}}}"#);
        set_title(&client(&base), "s-1", Some("Release")).unwrap();
        assert!(request(&captured).ends_with(r#"{"sessionId":"s-1","title":"Release"}"#));
    }

    #[test]
    fn link_and_unlink_issues() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txid":1}}}"#);
        link_issues(&client(&base), "s-1", &["i-1".to_string(), "i-2".to_string()]).unwrap();
        let request_text = request(&captured);
        assert!(request_text.starts_with("POST /api/trpc/sessions.linkIssues HTTP/1.1"));
        assert!(request_text.ends_with(r#"{"sessionId":"s-1","issueIds":["i-1","i-2"]}"#));
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txid":1}}}"#);
        unlink_issue(&client(&base), "s-1", "i-1").unwrap();
        assert!(request(&captured).ends_with(r#"{"sessionId":"s-1","issueId":"i-1"}"#));
    }

    #[test]
    fn answer_and_unqueue() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txid":1}}}"#);
        let input = AnswerInput {
            message_id: "m-1".into(),
            decision: Some("rejected".into()),
            feedback: Some("Smaller".into()),
            ..AnswerInput::default()
        };
        answer(&client(&base), &input).unwrap();
        assert!(request(&captured).ends_with(r#"{"messageId":"m-1","decision":"rejected","feedback":"Smaller"}"#));
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txid":1}}}"#);
        unqueue(&client(&base), "m-2").unwrap();
        let request_text = request(&captured);
        assert!(request_text.starts_with("POST /api/trpc/sessions.unqueue HTTP/1.1"));
        assert!(request_text.ends_with(r#"{"messageId":"m-2"}"#));
    }

    #[test]
    fn get_and_update_an_agent() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"sessionId":"ag-1","instructions":"Keep it tidy."}}}"#,
        );
        let details = get_agent(&client(&base), "ag-1").unwrap();
        assert_eq!(details.instructions, "Keep it tidy.");
        assert!(request(&captured).starts_with(
            "GET /api/trpc/sessions.getAgent?input=%7B%22sessionId%22%3A%22ag-1%22%7D HTTP/1.1"
        ));

        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txid":2}}}"#);
        let input = UpdateAgentInput {
            session_id: "ag-1".into(),
            name: Some("Docs".into()),
            account: Some(None),
            model: Some(Some("opus".into())),
            ..UpdateAgentInput::default()
        };
        update_agent(&client(&base), &input).unwrap();
        assert!(request(&captured)
            .ends_with(r#"{"sessionId":"ag-1","name":"Docs","account":null,"model":"opus"}"#));
    }

    #[test]
    fn merge_close_and_files() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"merged":false,"queued":true,"txid":4}}}"#,
        );
        let out = pr_merge(&client(&base), "pr-1", true).unwrap();
        assert!(!out.merged && out.queued);
        assert_eq!(out.error, None);
        let request_text = request(&captured);
        assert!(request_text.starts_with("POST /api/trpc/pullRequests.merge HTTP/1.1"));
        assert!(request_text.ends_with(r#"{"pullRequestId":"pr-1","stack":true}"#));

        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txid":5}}}"#);
        pr_close(&client(&base), "pr-1").unwrap();
        assert!(request(&captured).ends_with(r#"{"pullRequestId":"pr-1"}"#));

        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"files":[{"path":"a.rs","additions":3,"deletions":1,"patch":"@@"},{"path":"b.png","additions":0,"deletions":0,"patch":null}]}}}"#,
        );
        let files = pr_files(&client(&base), "pr-1").unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].additions, 3);
        assert_eq!(files[1].patch, None);
        assert!(request(&captured).starts_with("GET /api/trpc/pullRequests.files?input="));
    }
}
