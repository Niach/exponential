//! Typed `codingSessions.*` tRPC helpers (masterplan-v3 §7.1 steps 6 + 8).
//! Shapes verified against `apps/web/src/lib/trpc/coding-sessions.ts`:
//!
//! - `codingSessions.start({issueId, deviceLabel?})` → `{session}` —
//!   **mutation**, called BEFORE the child spawns: the returned row id keys
//!   the terminal tab (§06) and the steer session room (§08). The server
//!   enforces the plan's concurrent-session capacity here
//!   (`assertWithinCodingSessionLimit` → `PRECONDITION_FAILED`/412 with an
//!   upgrade nudge on cloud; unlimited self-hosted) — the desktop never
//!   self-throttles (§7.6).
//! - `codingSessions.end({id})` → `{session}` — **mutation**, idempotent
//!   server-side (ending an already-ended session is a no-op), so firing it
//!   from the child-exit hook is safe even after a relay-side kill already
//!   ended the row.
//!
//! The row is a synced Electric shape: the "coding now" badge everywhere is
//! the synced row itself — no client fabricates it locally.

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::trpc::TrpcClient;

/// The synced `coding_sessions` row as the start/end mutations return it.
/// Only the fields the launcher consumes are typed strictly; the rest are
/// tolerant options so server-side additions never break the decode.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodingSession {
    pub id: String,
    #[serde(default)]
    pub issue_id: Option<String>,
    /// Set for release-scoped orchestrator sessions (EXP-56); `issue_id` is
    /// NULL on those rows.
    #[serde(default)]
    pub release_id: Option<String>,
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub device_label: Option<String>,
    /// `running` | `ended` (contract enum `coding_session_status`).
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub ended_at: Option<String>,
}

#[derive(Deserialize)]
struct SessionEnvelope {
    session: CodingSession,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StartInput<'a> {
    issue_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    device_label: Option<&'a str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StartReleaseInput<'a> {
    release_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    device_label: Option<&'a str>,
}

#[derive(Serialize)]
struct EndInput<'a> {
    id: &'a str,
}

/// `codingSessions.start` — mutation. A 412 (`PRECONDITION_FAILED`) is the
/// plan's concurrent-session cap; the launcher maps it to its `SessionLimit`
/// disabled state with the server's upgrade copy.
pub fn start(
    trpc: &TrpcClient,
    issue_id: &str,
    device_label: Option<&str>,
) -> Result<CodingSession, ApiError> {
    let envelope: SessionEnvelope = trpc.mutation(
        "codingSessions.start",
        &StartInput {
            issue_id,
            device_label,
        },
    )?;
    Ok(envelope.session)
}

/// `codingSessions.start` for a RELEASE-scoped orchestrator session (EXP-56):
/// the server accepts exactly one of `issueId`/`releaseId` and inserts a row
/// with `issue_id`/`project_id` NULL and `workspace_id` from the release.
/// Same 412 semantics as [`start`].
pub fn start_release(
    trpc: &TrpcClient,
    release_id: &str,
    device_label: Option<&str>,
) -> Result<CodingSession, ApiError> {
    let envelope: SessionEnvelope = trpc.mutation(
        "codingSessions.start",
        &StartReleaseInput {
            release_id,
            device_label,
        },
    )?;
    Ok(envelope.session)
}

/// `codingSessions.end` — mutation, idempotent server-side.
pub fn end(trpc: &TrpcClient, id: &str) -> Result<CodingSession, ApiError> {
    let envelope: SessionEnvelope = trpc.mutation("codingSessions.end", &EndInput { id })?;
    Ok(envelope.session)
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

    const SESSION_BODY: &str = r#"{"result":{"data":{"session":{
        "id":"sess-1","issueId":"issue-1","workspaceId":"ws-1","userId":"user-1",
        "deviceLabel":"testbox","status":"running",
        "startedAt":"2026-07-03T10:00:00.000Z","endedAt":null,
        "createdAt":"2026-07-03T10:00:00.000Z","updatedAt":"2026-07-03T10:00:00.000Z"}}}}"#;

    #[test]
    fn start_decodes_session_envelope_and_posts_device_label() {
        let (base, captured) = one_shot_server(200, SESSION_BODY);
        let session = start(&client(&base), "issue-1", Some("testbox")).unwrap();
        assert_eq!(session.id, "sess-1");
        assert_eq!(session.status.as_deref(), Some("running"));
        assert_eq!(session.device_label.as_deref(), Some("testbox"));
        assert_eq!(session.ended_at, None);
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/codingSessions.start HTTP/1.1"));
        assert!(request.ends_with(r#"{"issueId":"issue-1","deviceLabel":"testbox"}"#));
    }

    #[test]
    fn start_omits_absent_device_label() {
        let (base, captured) = one_shot_server(200, SESSION_BODY);
        let _ = start(&client(&base), "issue-1", None).unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(r#"{"issueId":"issue-1"}"#));
    }

    #[test]
    fn start_release_posts_release_id_and_decodes_the_release_row() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"session":{
                "id":"sess-r","issueId":null,"releaseId":"rel-1","workspaceId":"ws-1",
                "userId":"user-1","deviceLabel":"testbox","status":"running"}}}}"#,
        );
        let session = start_release(&client(&base), "rel-1", Some("testbox")).unwrap();
        assert_eq!(session.id, "sess-r");
        assert_eq!(session.release_id.as_deref(), Some("rel-1"));
        assert_eq!(session.issue_id, None);
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/codingSessions.start HTTP/1.1"));
        assert!(request.ends_with(r#"{"releaseId":"rel-1","deviceLabel":"testbox"}"#));
    }

    #[test]
    fn session_limit_surfaces_as_412() {
        let (base, _captured) = one_shot_server(
            412,
            r#"{"error":{"message":"Concurrent coding session limit reached — upgrade to run more.","code":-32012,"data":{"code":"PRECONDITION_FAILED","httpStatus":412}}}"#,
        );
        match start(&client(&base), "issue-1", None) {
            Err(ApiError::Http { status, message }) => {
                assert_eq!(status, 412);
                assert!(message.contains("limit"));
            }
            other => panic!("expected 412 Http error, got {other:?}"),
        }
    }

    #[test]
    fn end_posts_id_and_decodes_session() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"session":{"id":"sess-1","status":"ended","endedAt":"2026-07-03T11:00:00.000Z"}}}}"#,
        );
        let session = end(&client(&base), "sess-1").unwrap();
        assert_eq!(session.id, "sess-1");
        assert_eq!(session.status.as_deref(), Some("ended"));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/codingSessions.end HTTP/1.1"));
        assert!(request.ends_with(r#"{"id":"sess-1"}"#));
    }
}
