//! Typed `comments.*` tRPC helpers (masterplan-v3 §4.2 issue-detail timeline
//! composer + author-or-admin edit/delete). Verified against
//! `apps/web/src/lib/trpc/comments.ts`:
//!
//! - `comments.create({issueId, body, attachmentIds?, parentId?, audience?})`
//!   → `{txId, comment, mentionedUserIds, reporterEmailed}` (`body` is GFM
//!   markdown; the SERVER resolves `@email` mentions and auto-subscribes —
//!   the desktop only produces the `@email` source text, §4.6; `parentId`
//!   makes it a reply under that top-level comment, EXP-741; SLOP-4
//!   `audience: 'reporter'` emails the widget reporter — accepted ONLY on a
//!   top-level comment of an issue whose submission has a reporter email,
//!   else BAD_REQUEST — and `reporterEmailed` says whether that mail went
//!   out: `true`/`false` on a reporter comment, `null` on a team one)
//! - `comments.update({id, body, attachmentIds?})` → `{txId, comment}`
//!   (author-only; a reporter's comment is edited by nobody)
//! - `comments.delete({id})` → `{txId}` (author-only, or ANY member for a
//!   reporter's comment — moderation; the server hard-deletes the comment's
//!   linked attachments)
//!
//! EXP-554: `attachmentIds` links already-uploaded `attachments` rows to the
//! comment (max 10, same issue, uploaded by the caller). OMITTING it on
//! `update` leaves the existing links untouched; sending an array makes it the
//! FULL desired set — rows missing from it are hard-deleted server-side. An
//! empty `body` is allowed when the id list is non-empty.

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::labels::TxOutput;
use crate::trpc::TrpcClient;

/// Slim camelCase mirror of the comment row a mutation returns.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentOut {
    pub id: String,
    #[serde(default)]
    pub issue_id: Option<String>,
    #[serde(default)]
    pub author_id: Option<String>,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    /// SLOP-4: `team` | `reporter`.
    #[serde(default)]
    pub audience: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub edited_at: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentsCreateOutput {
    pub comment: CommentOut,
    #[serde(default)]
    pub mentioned_user_ids: Vec<String>,
    #[serde(default)]
    pub tx_id: Option<i64>,
    /// SLOP-4: whether the reporter email was delivered — `Some(true)` =
    /// sent, `Some(false)` = saved but no transport / delivery failed,
    /// `None` = a team comment (nothing to send). Missing on an older
    /// server reads as `None`.
    #[serde(default)]
    pub reporter_emailed: Option<bool>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentsUpdateOutput {
    pub comment: CommentOut,
    #[serde(default)]
    pub tx_id: Option<i64>,
}

/// `comments.create` — mutation. Blocking; background executor only (§3.5).
/// `attachment_ids` links freshly uploaded attachments (EXP-554); `None`
/// omits the field entirely, keeping the wire body byte-identical to the
/// pre-EXP-554 one for plain text comments. `parent_id` (EXP-741) makes the
/// comment a reply under that top-level comment; `None` omits it likewise.
/// `audience` (SLOP-4) = `Some("reporter")` emails the widget reporter;
/// `None` omits the field (the server default `team`).
pub fn comments_create(
    trpc: &TrpcClient,
    issue_id: &str,
    body: &str,
    attachment_ids: Option<&[String]>,
    parent_id: Option<&str>,
    audience: Option<&str>,
) -> Result<CommentsCreateOutput, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        issue_id: &'a str,
        body: &'a str,
        #[serde(rename = "attachmentIds", skip_serializing_if = "Option::is_none")]
        attachment_ids: Option<&'a [String]>,
        #[serde(rename = "parentId", skip_serializing_if = "Option::is_none")]
        parent_id: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        audience: Option<&'a str>,
    }
    trpc.mutation(
        "comments.create",
        &Input {
            issue_id,
            body,
            attachment_ids,
            parent_id,
            audience,
        },
    )
}

/// `comments.update` — mutation (author-or-admin; FORBIDDEN otherwise).
/// `attachment_ids` = `None` leaves the comment's attachments alone; `Some`
/// is the full desired set (removed rows are hard-deleted server-side).
pub fn comments_update(
    trpc: &TrpcClient,
    id: &str,
    body: &str,
    attachment_ids: Option<&[String]>,
) -> Result<CommentsUpdateOutput, ApiError> {
    #[derive(Serialize)]
    struct Input<'a> {
        id: &'a str,
        body: &'a str,
        #[serde(rename = "attachmentIds", skip_serializing_if = "Option::is_none")]
        attachment_ids: Option<&'a [String]>,
    }
    trpc.mutation(
        "comments.update",
        &Input {
            id,
            body,
            attachment_ids,
        },
    )
}

/// `comments.delete` — mutation (author-or-admin).
pub fn comments_delete(trpc: &TrpcClient, id: &str) -> Result<TxOutput, ApiError> {
    #[derive(Serialize)]
    struct Input<'a> {
        id: &'a str,
    }
    trpc.mutation("comments.delete", &Input { id })
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
    fn create_round_trips_gfm_body_and_mentions() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"txId":21,"comment":{"id":"c-1","issueId":"i-1","authorId":"u-1","body":"ping @a@b.com"},"mentionedUserIds":["u-2"]}}}"#,
        );
        let out =
            comments_create(&client(&base), "i-1", "ping @a@b.com", None, None, None).unwrap();
        assert_eq!(out.comment.body.as_deref(), Some("ping @a@b.com"));
        assert_eq!(out.mentioned_user_ids, vec!["u-2".to_string()]);
        assert_eq!(out.tx_id, Some(21));
        // An older server without `reporterEmailed` reads as a team comment.
        assert_eq!(out.reporter_emailed, None);
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/comments.create HTTP/1.1"));
        // No attachments → the field is omitted entirely (the pre-EXP-554
        // body, byte for byte).
        assert!(request.ends_with(r#"{"issueId":"i-1","body":"ping @a@b.com"}"#));
    }

    /// EXP-554: attachment ids ride `attachmentIds`; an attachment-only
    /// comment sends an empty body (the server allows it when the list is
    /// non-empty).
    #[test]
    fn create_and_update_send_attachment_ids_when_present() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"txId":24,"comment":{"id":"c-2","issueId":"i-1"},"mentionedUserIds":[]}}}"#,
        );
        let ids = vec!["att-1".to_string(), "att-2".to_string()];
        comments_create(&client(&base), "i-1", "", Some(&ids), None, None).unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(
            r#"{"issueId":"i-1","body":"","attachmentIds":["att-1","att-2"]}"#
        ));

        // An EMPTY array is meaningful on update — it unlinks (and hard-
        // deletes) every attachment the comment had.
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"txId":25,"comment":{"id":"c-2","body":"edited"}}}}"#,
        );
        comments_update(&client(&base), "c-2", "edited", Some(&[])).unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(r#"{"id":"c-2","body":"edited","attachmentIds":[]}"#));
    }

    /// SLOP-4: the "Reply to reporter" send rides `audience: "reporter"` and
    /// reads the delivery verdict back; a team comment omits the field and
    /// gets `null`.
    #[test]
    fn create_sends_audience_and_reads_reporter_emailed() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"txId":26,"comment":{"id":"c-3","issueId":"i-1","audience":"reporter"},"mentionedUserIds":[],"reporterEmailed":true}}}"#,
        );
        let out = comments_create(&client(&base), "i-1", "On it.", None, None, Some("reporter"))
            .unwrap();
        assert_eq!(out.reporter_emailed, Some(true));
        assert_eq!(out.comment.audience.as_deref(), Some("reporter"));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(r#"{"issueId":"i-1","body":"On it.","audience":"reporter"}"#));

        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"txId":27,"comment":{"id":"c-4","issueId":"i-1"},"mentionedUserIds":[],"reporterEmailed":null}}}"#,
        );
        let out = comments_create(&client(&base), "i-1", "team only", None, None, None).unwrap();
        assert_eq!(out.reporter_emailed, None);
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(r#"{"issueId":"i-1","body":"team only"}"#));
    }

    #[test]
    fn update_and_delete_use_plain_id_inputs() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"txId":22,"comment":{"id":"c-1","body":"edited","editedAt":"2026-07-03T10:00:00Z"}}}}"#,
        );
        let out = comments_update(&client(&base), "c-1", "edited", None).unwrap();
        assert_eq!(out.comment.edited_at.as_deref(), Some("2026-07-03T10:00:00Z"));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        // Omitted `attachmentIds` = "don't touch the attachments" (MCP/old
        // server parity).
        assert!(request.ends_with(r#"{"id":"c-1","body":"edited"}"#));

        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txId":23}}}"#);
        let out = comments_delete(&client(&base), "c-1").unwrap();
        assert_eq!(out.tx_id, Some(23));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(r#"{"id":"c-1"}"#));
    }

    #[test]
    fn forbidden_edit_surfaces_server_message() {
        let (base, _captured) = one_shot_server(
            403,
            r#"{"error":{"message":"Only the author can edit this comment","code":-32003,"data":{"code":"FORBIDDEN","httpStatus":403}}}"#,
        );
        match comments_update(&client(&base), "c-1", "x", None) {
            Err(ApiError::Http { status, message }) => {
                assert_eq!(status, 403);
                assert!(message.contains("Only the author"));
            }
            other => panic!("expected Http error, got {other:?}"),
        }
    }
}
