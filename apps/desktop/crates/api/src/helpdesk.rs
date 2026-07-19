//! Typed `helpdesk.*` tRPC helpers (EXP-180 — the team-level Support inbox).
//! Verified against `apps/web/src/lib/trpc/helpdesk.ts`:
//!
//! - `helpdesk.listThreads({teamId, filter})` → `[{id, teamId, title, status,
//!   linkedIssueId, reporterEmail, reporterName, lastReporterSeenAt,
//!   createdAt, updatedAt, lastMessage: {body, direction, createdAt} | null,
//!   unread}]` (query; `unread` = the reporter spoke last — no per-member
//!   read state)
//! - `helpdesk.getThread({threadId})` → `{thread, messages, linkedIssue}`
//!   (query; messages include member-only internal notes)
//! - `helpdesk.reply({threadId, body})` → `{message}` (emails the reporter)
//! - `helpdesk.note({threadId, body})` → `{message}` (internal, never emailed)
//! - `helpdesk.close({threadId})` → `{ok}` / `helpdesk.reopen` → `{ok}`
//! - `helpdesk.escalate({threadId, boardId, title?})` → `{issue, txId}`
//!   (rejects when the ticket already has a linked issue)
//!
//! Support threads are server-only (tRPC, never Electric-synced) — the
//! Support surfaces poll these queries; the synced `teams.helpdesk_enabled`
//! flag only gates visibility.

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::trpc::TrpcClient;

/// One inbox row from `helpdesk.listThreads`.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportThreadSummary {
    pub id: String,
    #[serde(default)]
    pub team_id: Option<String>,
    pub title: String,
    /// `open` / `resolved`.
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub linked_issue_id: Option<String>,
    #[serde(default)]
    pub reporter_email: Option<String>,
    #[serde(default)]
    pub reporter_name: Option<String>,
    #[serde(default)]
    pub last_reporter_seen_at: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    /// Latest PUBLIC message (internal notes never surface here).
    #[serde(default)]
    pub last_message: Option<SupportLastMessage>,
    /// The reporter spoke last — the inbox dot.
    #[serde(default)]
    pub unread: bool,
}

/// The `lastMessage` projection on a list row.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportLastMessage {
    #[serde(default)]
    pub body: Option<String>,
    /// `inbound` (the reporter) / `outbound` (a member).
    #[serde(default)]
    pub direction: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
}

/// The `thread` object of `helpdesk.getThread` (same fields as a list row
/// minus `lastMessage`/`unread`).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportThreadOut {
    pub id: String,
    #[serde(default)]
    pub team_id: Option<String>,
    pub title: String,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub linked_issue_id: Option<String>,
    #[serde(default)]
    pub reporter_email: Option<String>,
    #[serde(default)]
    pub reporter_name: Option<String>,
    #[serde(default)]
    pub last_reporter_seen_at: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// One conversation message (public reply or internal note).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportMessageOut {
    pub id: String,
    #[serde(default)]
    pub thread_id: Option<String>,
    /// `None` on inbound (reporter) messages.
    #[serde(default)]
    pub author_user_id: Option<String>,
    /// `inbound` / `outbound`.
    #[serde(default)]
    pub direction: Option<String>,
    /// `public` / `internal`.
    #[serde(default)]
    pub visibility: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub email_delivery_id: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// The escalation chip's linked-issue projection.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportLinkedIssue {
    pub id: String,
    #[serde(default)]
    pub identifier: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub board_id: Option<String>,
}

/// `helpdesk.getThread` output.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportThreadDetail {
    pub thread: SupportThreadOut,
    #[serde(default)]
    pub messages: Vec<SupportMessageOut>,
    #[serde(default)]
    pub linked_issue: Option<SupportLinkedIssue>,
}

/// `helpdesk.escalate` output: the freshly filed issue + the sync txId.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EscalateOutput {
    pub issue: EscalatedIssue,
    #[serde(default)]
    pub tx_id: Option<i64>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EscalatedIssue {
    pub id: String,
    #[serde(default)]
    pub identifier: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
}

/// `helpdesk.listThreads` — query. `filter` is `"open"` or `"resolved"`.
pub fn helpdesk_list_threads(
    trpc: &TrpcClient,
    team_id: &str,
    filter: &str,
) -> Result<Vec<SupportThreadSummary>, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        team_id: &'a str,
        filter: &'a str,
    }
    trpc.query_with_input("helpdesk.listThreads", &Input { team_id, filter })
}

/// `helpdesk.getThread` — query (full conversation incl. internal notes).
pub fn helpdesk_get_thread(
    trpc: &TrpcClient,
    thread_id: &str,
) -> Result<SupportThreadDetail, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        thread_id: &'a str,
    }
    trpc.query_with_input("helpdesk.getThread", &Input { thread_id })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ThreadBodyInput<'a> {
    thread_id: &'a str,
    body: &'a str,
}

#[derive(Deserialize)]
struct MessageOutput {
    message: SupportMessageOut,
}

/// `helpdesk.reply` — mutation (public reply; the server emails the reporter).
pub fn helpdesk_reply(
    trpc: &TrpcClient,
    thread_id: &str,
    body: &str,
) -> Result<SupportMessageOut, ApiError> {
    let out: MessageOutput = trpc.mutation("helpdesk.reply", &ThreadBodyInput { thread_id, body })?;
    Ok(out.message)
}

/// `helpdesk.note` — mutation (member-only internal note; never emailed).
pub fn helpdesk_note(
    trpc: &TrpcClient,
    thread_id: &str,
    body: &str,
) -> Result<SupportMessageOut, ApiError> {
    let out: MessageOutput = trpc.mutation("helpdesk.note", &ThreadBodyInput { thread_id, body })?;
    Ok(out.message)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ThreadIdInput<'a> {
    thread_id: &'a str,
}

#[derive(Deserialize)]
struct OkOutput {
    #[allow(dead_code)]
    #[serde(default)]
    ok: bool,
}

/// `helpdesk.close` — mutation (resolve the ticket; transcript stays readable).
pub fn helpdesk_close(trpc: &TrpcClient, thread_id: &str) -> Result<(), ApiError> {
    let _: OkOutput = trpc.mutation("helpdesk.close", &ThreadIdInput { thread_id })?;
    Ok(())
}

/// `helpdesk.reopen` — mutation (ticket back to open).
pub fn helpdesk_reopen(trpc: &TrpcClient, thread_id: &str) -> Result<(), ApiError> {
    let _: OkOutput = trpc.mutation("helpdesk.reopen", &ThreadIdInput { thread_id })?;
    Ok(())
}

/// `helpdesk.escalate` — mutation (file an issue on a board of the ticket's
/// team and link it; the server rejects a second escalation).
pub fn helpdesk_escalate(
    trpc: &TrpcClient,
    thread_id: &str,
    board_id: &str,
    title: Option<&str>,
) -> Result<EscalateOutput, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        thread_id: &'a str,
        board_id: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<&'a str>,
    }
    trpc.mutation(
        "helpdesk.escalate",
        &Input {
            thread_id,
            board_id,
            title,
        },
    )
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

    #[test]
    fn list_threads_is_a_get_query_decoding_rows_with_last_message() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":[{"id":"th-1","teamId":"w-1","title":"App crashes on login","status":"open","linkedIssueId":null,"reporterEmail":"jane@example.com","reporterName":"Jane","lastReporterSeenAt":null,"createdAt":"2026-07-18T09:00:00Z","updatedAt":"2026-07-18T10:00:00Z","lastMessage":{"body":"Still broken","direction":"inbound","createdAt":"2026-07-18T10:00:00Z"},"unread":true},{"id":"th-2","teamId":"w-1","title":"Question","status":"open","linkedIssueId":"i-9","reporterEmail":"bob@example.com","reporterName":null,"lastReporterSeenAt":null,"createdAt":"2026-07-17T09:00:00Z","updatedAt":"2026-07-17T09:30:00Z","lastMessage":null,"unread":false}]}}"#,
        );
        let threads = helpdesk_list_threads(&client(&base), "w-1", "open").unwrap();
        assert_eq!(threads.len(), 2);
        assert_eq!(threads[0].title, "App crashes on login");
        assert!(threads[0].unread);
        assert_eq!(
            threads[0]
                .last_message
                .as_ref()
                .and_then(|m| m.direction.as_deref()),
            Some("inbound")
        );
        assert_eq!(threads[1].linked_issue_id.as_deref(), Some("i-9"));
        assert!(!threads[1].unread);
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("GET /api/trpc/helpdesk.listThreads?input="));
    }

    #[test]
    fn get_thread_decodes_thread_messages_and_linked_issue() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"thread":{"id":"th-1","teamId":"w-1","title":"App crashes","status":"resolved","linkedIssueId":"i-1","reporterEmail":"jane@example.com","reporterName":"Jane"},"messages":[{"id":"m-1","threadId":"th-1","authorUserId":null,"direction":"inbound","visibility":"public","body":"It crashes","emailDeliveryId":null,"createdAt":"2026-07-18T09:00:00Z"},{"id":"m-2","threadId":"th-1","authorUserId":"u-1","direction":"outbound","visibility":"internal","body":"Repro found","createdAt":"2026-07-18T09:30:00Z"}],"linkedIssue":{"id":"i-1","identifier":"EXP-42","title":"Fix crash","status":"in_progress","boardId":"p-1"}}}}"#,
        );
        let detail = helpdesk_get_thread(&client(&base), "th-1").unwrap();
        assert_eq!(detail.thread.status.as_deref(), Some("resolved"));
        assert_eq!(detail.messages.len(), 2);
        assert_eq!(detail.messages[0].direction.as_deref(), Some("inbound"));
        assert_eq!(detail.messages[1].visibility.as_deref(), Some("internal"));
        assert_eq!(
            detail.linked_issue.as_ref().and_then(|i| i.identifier.as_deref()),
            Some("EXP-42")
        );
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("GET /api/trpc/helpdesk.getThread?input="));
    }

    #[test]
    fn reply_and_note_post_thread_id_and_body() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"message":{"id":"m-3","threadId":"th-1","authorUserId":"u-1","direction":"outbound","visibility":"public","body":"On it!"}}}}"#,
        );
        let message = helpdesk_reply(&client(&base), "th-1", "On it!").unwrap();
        assert_eq!(message.visibility.as_deref(), Some("public"));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/helpdesk.reply HTTP/1.1"));
        assert!(request.ends_with(r#"{"threadId":"th-1","body":"On it!"}"#));

        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"message":{"id":"m-4","threadId":"th-1","authorUserId":"u-1","direction":"outbound","visibility":"internal","body":"internal note"}}}}"#,
        );
        let message = helpdesk_note(&client(&base), "th-1", "internal note").unwrap();
        assert_eq!(message.visibility.as_deref(), Some("internal"));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/helpdesk.note HTTP/1.1"));
        assert!(request.ends_with(r#"{"threadId":"th-1","body":"internal note"}"#));
    }

    #[test]
    fn close_and_reopen_post_the_thread_id() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"ok":true}}}"#);
        helpdesk_close(&client(&base), "th-1").unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/helpdesk.close HTTP/1.1"));
        assert!(request.ends_with(r#"{"threadId":"th-1"}"#));

        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"ok":true}}}"#);
        helpdesk_reopen(&client(&base), "th-1").unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/helpdesk.reopen HTTP/1.1"));
        assert!(request.ends_with(r#"{"threadId":"th-1"}"#));
    }

    #[test]
    fn escalate_omits_absent_title_and_decodes_issue_with_tx() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"issue":{"id":"i-7","identifier":"EXP-77","title":"App crashes"},"txId":42}}}"#,
        );
        let out = helpdesk_escalate(&client(&base), "th-1", "p-1", None).unwrap();
        assert_eq!(out.issue.identifier.as_deref(), Some("EXP-77"));
        assert_eq!(out.tx_id, Some(42));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/helpdesk.escalate HTTP/1.1"));
        assert!(request.ends_with(r#"{"threadId":"th-1","boardId":"p-1"}"#));

        // Explicit title rides along.
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"issue":{"id":"i-8","identifier":"EXP-78","title":"Custom"},"txId":43}}}"#,
        );
        let out = helpdesk_escalate(&client(&base), "th-2", "p-1", Some("Custom")).unwrap();
        assert_eq!(out.issue.id, "i-8");
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(r#"{"threadId":"th-2","boardId":"p-1","title":"Custom"}"#));
    }
}
