//! EXP-1294 (Threads v2): the people-side `sessions.*` and `pullRequests.*`
//! tRPC calls (web `apps/web/src/lib/trpc/sessions.ts`). The device-side
//! procedures (`brief`, `claim`, `report`, `reply`, `consume`) live with the
//! session host, not here.
//!
//! - `sessions.send` = THE one entrypoint ([`send`]): a new root (team +
//!   device + text/issues/prompt), a message to an AGENT (`agentId`: the
//!   caller's own conversation session with it, NEW-2), a child
//!   (`parentId`) or a message to an existing session (`sessionId`; to an
//!   idle session it resumes it, to an ended thread it continues it).
//! - `stop` interrupts a turn, `end` ends a session (a root: its tree).
//! - `linkIssues` / `unlinkIssue`, `answer` (a question/plan card),
//!   `unqueue` (a queued person message).
//! - NEW-2: agent definitions live in [`crate::agents`] (`agents.*`).
//! - `pullRequests.merge` / `close` / `files`.
//! - EXP-1323: `markRead`, the suggestions verbs (`startSuggestion`,
//!   `startAllSuggestions`, `dismissSuggestion`, `fileSuggestion`).
//! - EXP-1211: the quick replies verbs (`sendReply`, `dismissReplies`).
//!
//! Plain blocking calls: run them on a background executor only. Rows
//! re-stream over the shapes, so callers fire-and-forget; `txId` also
//! decodes as `txid`.

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::trpc::TrpcClient;

/// `sessions.send` input. Every key is optional and omitted when unset; the
/// server's rules decide which combinations are valid (a message needs
/// `session_id` + text/attachments/`account` — an `account` on a message is
/// an account SWITCH, EXP-849, and may ride alone — a child `parent_id` +
/// `device_id`, a new root `team_id` + `device_id` + text/issues/prompt, a
/// message to an agent `agent_id` + text).
/// EXP-1341: `issue_ids` also ride a child (`parent_id`: linked to the root
/// and ASSIGNED to the child) and a message (`session_id`: forwarded), and
/// `name` names a new root or child — no client-side refusal of either.
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
    /// NEW-2: a new root run from a saved prompt (its body plus `text` =
    /// the first message).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository_id: Option<String>,
    /// EXP-1341: an explicit name for a new root or child (its stored
    /// title; `domain::session_title`'s first rung).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// NEW-2: a message to an AGENT = into the caller's own conversation
    /// session with it (created on first use; agents never end).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// VAPP-110: a message to an APP = into the caller's own editor session
    /// with it (created on first use on `device_id`; an ended one
    /// continues).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
    /// NEW-57: a meeting turn (`meeting.json` RESIDENCY): the server stamps
    /// the message's `expires_at` (`contract.meeting.retentionDays`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meeting: Option<bool>,
    /// F21: the caller's stable id for this message (at most 64 chars),
    /// reused across retries: the server dedupes a send it already took
    /// (a meeting part whose answer was lost).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_message_id: Option<String>,
}

impl SendInput {
    /// A message to an existing session (the thread reply, the session
    /// view, an agent channel).
    pub fn message(session_id: &str, text: &str, attachment_ids: Vec<String>) -> Self {
        Self {
            session_id: Some(session_id.to_string()),
            // Blank text is no text (the server refuses whitespace alone).
            text: Some(text.to_string()).filter(|text| !text.trim().is_empty()),
            attachment_ids: Some(attachment_ids).filter(|ids| !ids.is_empty()),
            ..Self::default()
        }
    }

    /// NEW-57: a meeting turn: `meeting: true` marks it for the server's
    /// retention (`meeting.json` RESIDENCY). `session_id` = the member's
    /// conversation with the voice agent, or `team_id` + `agent_id` while
    /// there is none yet (set by the caller on the returned input).
    pub fn meeting(mut self) -> Self {
        self.meeting = Some(true);
        self
    }

    /// A child session under `parent_id` on `device_id`.
    pub fn child(parent_id: &str, device_id: &str, text: &str) -> Self {
        Self {
            parent_id: Some(parent_id.to_string()),
            device_id: Some(device_id.to_string()),
            text: Some(text.to_string()).filter(|text| !text.trim().is_empty()),
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

/// `sessions.stop`'s `{txid, delivered}` (thread 58cb32df): `delivered`
/// is false when the relay reached no device for a live, non-queued
/// session; an older server sends none (`None` = delivered).
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StopOutput {
    #[serde(default, alias = "txid")]
    pub tx_id: Option<i64>,
    #[serde(default)]
    pub delivered: Option<bool>,
}

impl StopOutput {
    /// The stop reached no device (`chats.json` `copy.toasts.stopNotDelivered`).
    pub fn not_delivered(&self) -> bool {
        self.delivered == Some(false)
    }
}

/// Interrupt the session's current turn (it stays alive).
pub fn stop(trpc: &TrpcClient, session_id: &str) -> Result<StopOutput, ApiError> {
    trpc.mutation("sessions.stop", &SessionIdInput { session_id })
}

/// Thread 58cb32df: `sessions.delete` — a ROOT chat only: the server ends
/// what is live, then deletes the tree and its messages.
pub fn delete(trpc: &TrpcClient, session_id: &str) -> Result<TxOutput, ApiError> {
    trpc.mutation("sessions.delete", &SessionIdInput { session_id })
}

/// End the session (a root ends its whole tree).
pub fn end(trpc: &TrpcClient, session_id: &str) -> Result<TxOutput, ApiError> {
    trpc.mutation("sessions.end", &SessionIdInput { session_id })
}

/// EXP-1334: `sessions.configure` — a live session's launch options,
/// changed mid-session. `None` leaves a field unchanged; `effort:
/// Some(None)` clears it (the agent's default); `mcp_server_ids:
/// Some(vec![])` picks none. The host device applies them: claude's plan
/// mode at once, the rest at the next turn.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigureInput {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_mode: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ultracode: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub computer_use: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_server_ids: Option<Vec<String>>,
    /// The model (a runner model id, `sessionsConfigureInputSchema.model`);
    /// the host switches the live run at once.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// `sessions.configure` — mutation.
pub fn configure(trpc: &TrpcClient, input: &ConfigureInput) -> Result<TxOutput, ApiError> {
    trpc.mutation("sessions.configure", input)
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

/// `sessions.answer` input: a question card takes `answer` (EXP-1280: or
/// `answers`, its picked labels), a plan card `decision` (`approved` |
/// `rejected`) + optional `feedback` and `answers` (`{questionId: labels}`);
/// either may carry `surfaceData`, the inline surface's data model at
/// answer time (`fixtures/plan-card.json`).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerInput {
    pub message_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<String>,
    /// An approval that starts in a fresh context (ask-dock.json
    /// `planOptions`); only with `decision: approved`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fresh: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feedback: Option<String>,
    /// EXP-1280: a plan card's `{questionId: labels[]}`, a question card's
    /// `labels[]`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answers: Option<serde_json::Value>,
    /// EXP-1280: the inline surface's data model at answer time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub surface_data: Option<serde_json::Value>,
}

/// Answer a question/plan card in place.
pub fn answer(trpc: &TrpcClient, input: &AnswerInput) -> Result<TxOutput, ApiError> {
    trpc.mutation("sessions.answer", input)
}

/// EXP-1316: the DEVICE's write-back of a card its engine resolved itself
/// (a relay answer from the session view): stamps the same answered card +
/// `answeredAt` while still open, `{ok: true}` when already answered (never
/// CONFLICT), enqueues nothing. Same input shape as [`answer`].
pub fn resolve_card(trpc: &TrpcClient, input: &AnswerInput) -> Result<serde::de::IgnoredAny, ApiError> {
    trpc.mutation("sessions.resolveCard", input)
}

/// Take back a queued (not yet consumed) person message.
pub fn unqueue(trpc: &TrpcClient, message_id: &str) -> Result<TxOutput, ApiError> {
    trpc.mutation("sessions.unqueue", &MessageIdInput { message_id })
}

/// EXP-1323: mark `session_id` read for the viewer (`session_reads`; a
/// root = the thread, a child = that session's view).
pub fn mark_read(trpc: &TrpcClient, session_id: &str) -> Result<serde::de::IgnoredAny, ApiError> {
    trpc.mutation("sessions.markRead", &SessionIdInput { session_id })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MarkStoriesSeenInput<'a> {
    session_id: &'a str,
    at: i64,
}

/// S-3: a story was SHOWN in the viewer: raise the viewer's stories mark of
/// the root `root_id` to `at` (the story's createdAt, epoch ms; the server
/// keeps the greatest).
pub fn mark_stories_seen(trpc: &TrpcClient, root_id: &str, at: i64) -> Result<TxOutput, ApiError> {
    trpc.mutation("sessions.markStoriesSeen", &MarkStoriesSeenInput { session_id: root_id, at })
}

/// #EXP-1388 `sessions.setRead` / `sessions.setArchived` input (up to 200
/// root ids per call).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SetReadInput<'a> {
    session_ids: &'a [String],
    read: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SetArchivedInput<'a> {
    session_ids: &'a [String],
    archived: bool,
}

/// #EXP-1388: the chats selection's Mark read (`read`) / Mark unread
/// (`!read`: the server reads the root just before its newest agent
/// message) for the viewer.
pub fn set_read(trpc: &TrpcClient, session_ids: &[String], read: bool) -> Result<TxOutput, ApiError> {
    trpc.mutation("sessions.setRead", &SetReadInput { session_ids, read })
}

/// #EXP-1388: the chats selection's Archive / Unarchive (per viewer,
/// `session_reads.archived_at`).
pub fn set_archived(trpc: &TrpcClient, session_ids: &[String], archived: bool) -> Result<TxOutput, ApiError> {
    trpc.mutation("sessions.setArchived", &SetArchivedInput { session_ids, archived })
}

/// EXP-1323: where a suggested task starts (`sessions.startSuggestion`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionWhere {
    /// A child session (`Start`, the Options menu's Start on… / with…).
    Child,
    /// A message to the posting session (`Do it here`).
    Here,
}

/// `sessions.startSuggestion` input.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartSuggestionInput {
    pub message_id: String,
    pub item_id: String,
    #[serde(rename = "where")]
    pub where_: SuggestionWhere,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

impl StartSuggestionInput {
    pub fn new(message_id: &str, item_id: &str, where_: SuggestionWhere) -> Self {
        Self {
            message_id: message_id.to_string(),
            item_id: item_id.to_string(),
            where_,
            device_id: None,
            account: None,
            model: None,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SuggestionItemInput<'a> {
    message_id: &'a str,
    item_id: &'a str,
}

/// EXP-1323: start one suggested task (a child, or Do it here).
pub fn start_suggestion(trpc: &TrpcClient, input: &StartSuggestionInput) -> Result<serde::de::IgnoredAny, ApiError> {
    trpc.mutation("sessions.startSuggestion", input)
}

/// EXP-1323: start every open item of a suggestions card as children.
pub fn start_all_suggestions(trpc: &TrpcClient, message_id: &str) -> Result<serde::de::IgnoredAny, ApiError> {
    trpc.mutation("sessions.startAllSuggestions", &MessageIdInput { message_id })
}

/// EXP-1323: drop one suggested task (×, Options → Dismiss).
pub fn dismiss_suggestion(trpc: &TrpcClient, message_id: &str, item_id: &str) -> Result<serde::de::IgnoredAny, ApiError> {
    trpc.mutation("sessions.dismissSuggestion", &SuggestionItemInput { message_id, item_id })
}

/// EXP-1323: file one suggested task as an issue (Options → File as issue).
pub fn file_suggestion(trpc: &TrpcClient, message_id: &str, item_id: &str) -> Result<serde::de::IgnoredAny, ApiError> {
    trpc.mutation("sessions.fileSuggestion", &SuggestionItemInput { message_id, item_id })
}

/// EXP-1211: send one quick reply as the viewer's message to the card's
/// session (the item flips `sent`).
pub fn send_reply(trpc: &TrpcClient, message_id: &str, item_id: &str) -> Result<serde::de::IgnoredAny, ApiError> {
    trpc.mutation("sessions.sendReply", &SuggestionItemInput { message_id, item_id })
}

/// EXP-1211: drop a quick replies card (the row's ×).
pub fn dismiss_replies(trpc: &TrpcClient, message_id: &str) -> Result<serde::de::IgnoredAny, ApiError> {
    trpc.mutation("sessions.dismissReplies", &MessageIdInput { message_id })
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

    /// NEW-57: a meeting turn carries `meeting: true`; nothing else does.
    #[test]
    fn a_meeting_turn_says_so() {
        let input = SendInput::message("s-1", "Meeting transcript", Vec::new()).meeting();
        assert_eq!(
            serde_json::to_string(&input).unwrap(),
            r#"{"sessionId":"s-1","text":"Meeting transcript","meeting":true}"#
        );
        let plain = SendInput::message("s-1", "Hi", Vec::new());
        assert!(!serde_json::to_string(&plain).unwrap().contains("meeting"));
    }

    /// EXP-1341: a named child carrying its assigned issues goes out as is.
    #[test]
    fn a_named_child_with_issues_serializes() {
        let input = SendInput {
            issue_ids: Some(vec!["i-1".into(), "i-2".into()]),
            name: Some("Port the strip".into()),
            ..SendInput::child("p-1", "dev-1", "Do it")
        };
        assert_eq!(
            serde_json::to_string(&input).unwrap(),
            r#"{"parentId":"p-1","deviceId":"dev-1","text":"Do it","issueIds":["i-1","i-2"],"name":"Port the strip"}"#
        );
        let forward = SendInput {
            issue_ids: Some(vec!["i-3".into()]),
            ..SendInput::message("s-1", "", Vec::new())
        };
        assert_eq!(serde_json::to_string(&forward).unwrap(), r#"{"sessionId":"s-1","issueIds":["i-3"]}"#);
    }

    #[test]
    fn send_a_new_root_and_a_message_to_an_agent() {
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

        // NEW-2: a message to an agent rides `agentId` (the server routes
        // it into the caller's own conversation session).
        let (base, captured) =
            one_shot_server(200, r#"{"result":{"data":{"sessionId":"c-1","messageId":"m-1","txid":4}}}"#);
        let input = SendInput {
            agent_id: Some("ag-1".into()),
            text: Some("Triage the inbox".into()),
            ..SendInput::default()
        };
        let out = send(&client(&base), &input).unwrap();
        assert_eq!(out.session_id, "c-1");
        assert!(request(&captured).ends_with(r#"{"text":"Triage the inbox","agentId":"ag-1"}"#));
    }

    #[test]
    fn blank_text_is_no_text() {
        assert_eq!(SendInput::message("s-1", "  \n", Vec::new()).text, None);
        assert_eq!(SendInput::child("s-1", "dev-1", " ").text, None);
        assert_eq!(SendInput::message("s-1", "  code", Vec::new()).text.as_deref(), Some("  code"));
    }

    #[test]
    fn send_a_child_and_a_prompt() {
        let (base, captured) =
            one_shot_server(200, r#"{"result":{"data":{"sessionId":"c-1","messageId":"m-2","txid":1}}}"#);
        send(&client(&base), &SendInput::child("s-1", "dev-1", "Review")).unwrap();
        assert!(request(&captured).ends_with(r#"{"parentId":"s-1","deviceId":"dev-1","text":"Review"}"#));

        let (base, captured) =
            one_shot_server(200, r#"{"result":{"data":{"sessionId":"s-3","messageId":null,"txid":1}}}"#);
        let input = SendInput {
            team_id: Some("t-1".into()),
            device_id: Some("dev-1".into()),
            prompt_id: Some("p-1".into()),
            text: Some("only the web".into()),
            ..SendInput::default()
        };
        send(&client(&base), &input).unwrap();
        assert!(request(&captured)
            .ends_with(r#"{"teamId":"t-1","deviceId":"dev-1","text":"only the web","promptId":"p-1"}"#));
    }

    /// #EXP-1388: the selection's batch read + archive writes.
    #[test]
    fn set_read_and_set_archived_post_their_inputs() {
        let ids = vec!["s-1".to_string(), "s-2".to_string()];
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txid":7}}}"#);
        assert_eq!(set_read(&client(&base), &ids, false).unwrap().tx_id, Some(7));
        let text = request(&captured);
        assert!(text.starts_with("POST /api/trpc/sessions.setRead HTTP/1.1"), "{text}");
        assert!(text.ends_with(r#"{"sessionIds":["s-1","s-2"],"read":false}"#), "{text}");

        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txid":8}}}"#);
        set_archived(&client(&base), &ids, true).unwrap();
        let text = request(&captured);
        assert!(text.starts_with("POST /api/trpc/sessions.setArchived HTTP/1.1"), "{text}");
        assert!(text.ends_with(r#"{"sessionIds":["s-1","s-2"],"archived":true}"#), "{text}");
    }

    /// EXP-1323: markRead and the suggestion verbs.
    #[test]
    fn mark_read_and_suggestions_post_their_inputs() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txid":2}}}"#);
        mark_read(&client(&base), "s-1").unwrap();
        let text = request(&captured);
        assert!(text.starts_with("POST /api/trpc/sessions.markRead HTTP/1.1"), "{text}");
        assert!(text.ends_with(r#"{"sessionId":"s-1"}"#), "{text}");

        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"sessionId":"c-1"}}}"#);
        let input = StartSuggestionInput {
            device_id: Some("dev-1".into()),
            model: Some("opus".into()),
            ..StartSuggestionInput::new("m-1", "i-1", SuggestionWhere::Child)
        };
        start_suggestion(&client(&base), &input).unwrap();
        assert!(request(&captured)
            .ends_with(r#"{"messageId":"m-1","itemId":"i-1","where":"child","deviceId":"dev-1","model":"opus"}"#));

        for (path, call) in [
            (
                "sessions.dismissSuggestion",
                dismiss_suggestion as fn(&TrpcClient, &str, &str) -> Result<serde::de::IgnoredAny, ApiError>,
            ),
            ("sessions.fileSuggestion", file_suggestion),
            ("sessions.sendReply", send_reply),
        ] {
            let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txid":3}}}"#);
            call(&client(&base), "m-1", "i-2").unwrap();
            let text = request(&captured);
            assert!(text.starts_with(&format!("POST /api/trpc/{path} HTTP/1.1")), "{text}");
            assert!(text.ends_with(r#"{"messageId":"m-1","itemId":"i-2"}"#), "{text}");
        }

        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"started":2}}}"#);
        start_all_suggestions(&client(&base), "m-1").unwrap();
        let text = request(&captured);
        assert!(text.starts_with("POST /api/trpc/sessions.startAllSuggestions HTTP/1.1"), "{text}");
        assert!(text.ends_with(r#"{"messageId":"m-1"}"#), "{text}");

        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txid":4}}}"#);
        dismiss_replies(&client(&base), "m-1").unwrap();
        let text = request(&captured);
        assert!(text.starts_with("POST /api/trpc/sessions.dismissReplies HTTP/1.1"), "{text}");
        assert!(text.ends_with(r#"{"messageId":"m-1"}"#), "{text}");
    }

    #[test]
    fn stop_and_end_post_the_session_id() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txid":5,"delivered":false}}}"#);
        let out = stop(&client(&base), "s-1").unwrap();
        assert_eq!(out.tx_id, Some(5));
        assert!(out.not_delivered());
        let request_text = request(&captured);
        assert!(request_text.starts_with("POST /api/trpc/sessions.stop HTTP/1.1"), "{request_text}");
        assert!(request_text.ends_with(r#"{"sessionId":"s-1"}"#), "{request_text}");
        // An older server sends no `delivered`: delivered.
        let (base, _captured) = one_shot_server(200, r#"{"result":{"data":{"txid":5}}}"#);
        assert!(!stop(&client(&base), "s-1").unwrap().not_delivered());

        for (path, call) in [
            ("sessions.end", end as fn(&TrpcClient, &str) -> Result<TxOutput, ApiError>),
            ("sessions.delete", delete),
        ] {
            let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txid":5}}}"#);
            assert_eq!(call(&client(&base), "s-1").unwrap().tx_id, Some(5));
            let request = request(&captured);
            assert!(request.starts_with(&format!("POST /api/trpc/{path} HTTP/1.1")), "{request}");
            assert!(request.ends_with(r#"{"sessionId":"s-1"}"#), "{request}");
        }
    }

    #[test]
    fn resolve_card_posts_the_answer() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"ok":true}}}"#);
        let input = AnswerInput {
            message_id: "m-1".into(),
            answer: Some("Postgres".into()),
            ..AnswerInput::default()
        };
        resolve_card(&client(&base), &input).unwrap();
        let request_text = request(&captured);
        assert!(request_text.starts_with("POST /api/trpc/sessions.resolveCard HTTP/1.1"));
        assert!(request_text.ends_with(r#"{"messageId":"m-1","answer":"Postgres"}"#));
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
        let input = AnswerInput {
            message_id: "m-2".into(),
            decision: Some("approved".into()),
            fresh: Some(true),
            ..AnswerInput::default()
        };
        answer(&client(&base), &input).unwrap();
        assert!(request(&captured).ends_with(r#"{"messageId":"m-2","decision":"approved","fresh":true}"#));
        // EXP-1280: the form's answers + the surface's data, camelCase.
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txid":1}}}"#);
        let input = AnswerInput {
            message_id: "m-3".into(),
            decision: Some("approved".into()),
            answers: Some(serde_json::json!({"layout": ["Two columns"]})),
            surface_data: Some(serde_json::json!({"size": "lg"})),
            ..AnswerInput::default()
        };
        answer(&client(&base), &input).unwrap();
        assert!(request(&captured).ends_with(
            r#"{"messageId":"m-3","decision":"approved","answers":{"layout":["Two columns"]},"surfaceData":{"size":"lg"}}"#
        ));
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txid":1}}}"#);
        unqueue(&client(&base), "m-2").unwrap();
        let request_text = request(&captured);
        assert!(request_text.starts_with("POST /api/trpc/sessions.unqueue HTTP/1.1"));
        assert!(request_text.ends_with(r#"{"messageId":"m-2"}"#));
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
