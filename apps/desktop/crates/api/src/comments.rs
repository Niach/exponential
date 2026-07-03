//! Typed `comments.*` tRPC helpers (masterplan-v3 §4.2 issue-detail timeline
//! composer + author-or-admin edit/delete). Verified against
//! `apps/web/src/lib/trpc/comments.ts`:
//!
//! - `comments.create({issueId, body})` → `{txId, comment, mentionedUserIds}`
//!   (`body` is GFM markdown; the SERVER resolves `@email` mentions and
//!   auto-subscribes — the desktop only produces the `@email` source text,
//!   §4.6)
//! - `comments.update({id, body})` → `{txId, comment}` (author-or-admin)
//! - `comments.delete({id})` → `{txId}` (author-or-admin)

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
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentsUpdateOutput {
    pub comment: CommentOut,
    #[serde(default)]
    pub tx_id: Option<i64>,
}

/// `comments.create` — mutation. Blocking; background executor only (§3.5).
pub fn comments_create(
    trpc: &TrpcClient,
    issue_id: &str,
    body: &str,
) -> Result<CommentsCreateOutput, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        issue_id: &'a str,
        body: &'a str,
    }
    trpc.mutation("comments.create", &Input { issue_id, body })
}

/// `comments.update` — mutation (author-or-admin; FORBIDDEN otherwise).
pub fn comments_update(
    trpc: &TrpcClient,
    id: &str,
    body: &str,
) -> Result<CommentsUpdateOutput, ApiError> {
    #[derive(Serialize)]
    struct Input<'a> {
        id: &'a str,
        body: &'a str,
    }
    trpc.mutation("comments.update", &Input { id, body })
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
        let out = comments_create(&client(&base), "i-1", "ping @a@b.com").unwrap();
        assert_eq!(out.comment.body.as_deref(), Some("ping @a@b.com"));
        assert_eq!(out.mentioned_user_ids, vec!["u-2".to_string()]);
        assert_eq!(out.tx_id, Some(21));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/comments.create HTTP/1.1"));
        assert!(request.ends_with(r#"{"issueId":"i-1","body":"ping @a@b.com"}"#));
    }

    #[test]
    fn update_and_delete_use_plain_id_inputs() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"txId":22,"comment":{"id":"c-1","body":"edited","editedAt":"2026-07-03T10:00:00Z"}}}}"#,
        );
        let out = comments_update(&client(&base), "c-1", "edited").unwrap();
        assert_eq!(out.comment.edited_at.as_deref(), Some("2026-07-03T10:00:00Z"));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
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
        match comments_update(&client(&base), "c-1", "x") {
            Err(ApiError::Http { status, message }) => {
                assert_eq!(status, 403);
                assert!(message.contains("Only the author"));
            }
            other => panic!("expected Http error, got {other:?}"),
        }
    }
}
