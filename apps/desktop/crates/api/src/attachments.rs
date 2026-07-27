//! Typed `attachments.*` tRPC helpers (EXP-297).
//!
//! The attachment BYTES move over plain HTTP (multipart upload to
//! `POST /api/issues/{id}/files`, bearer GET on `/api/attachments/{id}` —
//! both in `ui::markdown::image_paste`); this module carries the tRPC
//! mutations the files rail and the settings Storage pane need.
//!
//! Wire shapes verified against `apps/web/src/lib/trpc/attachments.ts`:
//!
//! ```ts
//! // delete: input { id: string } → output { txId: number }
//! // listForTeam: input { teamId: string } → output {
//! //   attachments: [{ id, issueId, boardId, uploaderId, filename,
//! //                   contentType, sizeBytes, width, height, createdAt,
//! //                   isImage, referenced }],
//! //   totalBytes: number,
//! // }
//! // sweepUnreferencedImages: input { teamId: string } → output
//! //   { txId, deletedCount, freedBytes, skippedRecentCount }
//! ```
//!
//! `delete` is MEMBER-level (any team member may delete an attachment of
//! their team). The server rewrites every issue description / comment body
//! that still embeds the attachment, replacing the image with the plain-text
//! placeholder `*(deleted image: <label>)*`, drops the row, and reclaims the
//! blob after the transaction commits — so the Electric echo of both the
//! rewritten texts and the vanished attachment row is what updates the UI.
//!
//! `listForTeam` / `sweepUnreferencedImages` are OWNER-only (the settings
//! Storage manager): the list deliberately reads the raw `attachments` table
//! — trashed-board rows included — so owners can reclaim storage the synced
//! shape no longer shows, and the sweep bulk-deletes IMAGE rows no markdown
//! in the team references any more (a 24h upload grace window keeps images
//! that may still sit in an unsaved draft; plain files are never swept).

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::trpc::TrpcClient;

/// Output of `attachments.delete` — the Postgres txid for the §4.1
/// `awaitTxId` gate (the desktop reads through sync and ignores it).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentsDeleteOutput {
    #[serde(default)]
    pub tx_id: Option<i64>,
}

/// `attachments.delete` — mutation. Blocking; background executor only
/// (§3.5).
pub fn attachments_delete(
    trpc: &TrpcClient,
    attachment_id: &str,
) -> Result<AttachmentsDeleteOutput, ApiError> {
    #[derive(Serialize)]
    struct Input<'a> {
        id: &'a str,
    }
    trpc.mutation("attachments.delete", &Input { id: attachment_id })
}

/// One row of `attachments.listForTeam` — the raw attachment columns plus
/// the server-computed classification flags (`isImage`: the inline-image
/// content-type contract; `referenced`: some issue description / comment
/// body in the team still embeds it).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamAttachmentRow {
    pub id: String,
    pub issue_id: String,
    pub board_id: String,
    /// NULLABLE — widget screenshot uploads have no user uploader.
    #[serde(default)]
    pub uploader_id: Option<String>,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    /// Probed pixel dimensions — nullable for legacy/unmeasurable rows.
    #[serde(default)]
    pub width: Option<i64>,
    #[serde(default)]
    pub height: Option<i64>,
    /// ISO-8601 timestamp (plain-JSON tRPC serializes the Date).
    pub created_at: String,
    pub is_image: bool,
    pub referenced: bool,
}

/// Output of `attachments.listForTeam`: newest-first rows + the team's total
/// attachment bytes.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentsListForTeamOutput {
    pub attachments: Vec<TeamAttachmentRow>,
    pub total_bytes: i64,
}

/// `attachments.listForTeam` — owner-only query. Blocking; background
/// executor only (§3.5).
pub fn attachments_list_for_team(
    trpc: &TrpcClient,
    team_id: &str,
) -> Result<AttachmentsListForTeamOutput, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        team_id: &'a str,
    }
    trpc.query_with_input("attachments.listForTeam", &Input { team_id })
}

/// Output of `attachments.sweepUnreferencedImages` — what was reclaimed and
/// how many unreferenced images the 24h grace window kept.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentsSweepOutput {
    #[serde(default)]
    pub tx_id: Option<i64>,
    pub deleted_count: i64,
    pub freed_bytes: i64,
    pub skipped_recent_count: i64,
}

/// `attachments.sweepUnreferencedImages` — owner-only mutation. Blocking;
/// background executor only (§3.5).
pub fn attachments_sweep_unreferenced_images(
    trpc: &TrpcClient,
    team_id: &str,
) -> Result<AttachmentsSweepOutput, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        team_id: &'a str,
    }
    trpc.mutation("attachments.sweepUnreferencedImages", &Input { team_id })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trpc::tests::{has_header, one_shot_server};
    use crate::StaticToken;
    use std::sync::Arc;
    use std::time::Duration;

    fn client(base: &str) -> TrpcClient {
        TrpcClient::new(base, Arc::new(StaticToken("tok-1".to_string())))
    }

    #[test]
    fn delete_posts_the_id_and_decodes_tx() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txId":31}}}"#);
        let out = attachments_delete(&client(&base), "att-1").unwrap();
        assert_eq!(out.tx_id, Some(31));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/attachments.delete HTTP/1.1"));
        assert!(request.ends_with(r#"{"id":"att-1"}"#));
        assert!(has_header(&request, "Authorization: Bearer tok-1"));
    }

    #[test]
    fn list_for_team_encodes_input_and_decodes_rows() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"attachments":[{"id":"att-1","issueId":"iss-1","boardId":"b-1","uploaderId":null,"filename":"shot.png","contentType":"image/png","sizeBytes":2048,"width":800,"height":600,"createdAt":"2026-07-01T10:00:00.000Z","isImage":true,"referenced":false},{"id":"att-2","issueId":"iss-2","boardId":"b-1","uploaderId":"user-1","filename":"spec.pdf","contentType":"application/pdf","sizeBytes":4096,"width":null,"height":null,"createdAt":"2026-06-30T09:00:00.000Z","isImage":false,"referenced":false}],"totalBytes":6144}}}"#,
        );
        let out = attachments_list_for_team(&client(&base), "ws-1").unwrap();
        assert_eq!(out.total_bytes, 6144);
        assert_eq!(out.attachments.len(), 2);
        let image = &out.attachments[0];
        assert_eq!(image.id, "att-1");
        assert_eq!(image.issue_id, "iss-1");
        assert_eq!(image.uploader_id, None);
        assert_eq!(image.filename, "shot.png");
        assert_eq!(image.content_type, "image/png");
        assert_eq!(image.size_bytes, 2048);
        assert_eq!(image.width, Some(800));
        assert!(image.is_image);
        assert!(!image.referenced);
        let file = &out.attachments[1];
        assert_eq!(file.uploader_id.as_deref(), Some("user-1"));
        assert_eq!(file.width, None);
        assert!(!file.is_image);
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        // GET query with the raw-JSON input percent-encoded
        // (`{"teamId":"ws-1"}`).
        assert!(
            request.starts_with(
                "GET /api/trpc/attachments.listForTeam?input=%7B%22teamId%22%3A%22ws-1%22%7D HTTP/1.1"
            ),
            "unexpected request line: {}",
            request.lines().next().unwrap_or_default()
        );
        assert!(request.contains("Authorization: Bearer tok-1"));
    }

    #[test]
    fn list_for_team_surfaces_the_owner_gate() {
        let (base, _captured) = one_shot_server(
            403,
            r#"{"error":{"message":"Owner role required","code":-32003,"data":{"code":"FORBIDDEN","httpStatus":403}}}"#,
        );
        match attachments_list_for_team(&client(&base), "ws-1") {
            Err(ApiError::Http { status, message }) => {
                assert_eq!(status, 403);
                assert!(message.contains("Owner role required"));
            }
            other => panic!("expected Http error, got {other:?}"),
        }
    }

    #[test]
    fn sweep_posts_the_team_and_decodes_counts() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"txId":57,"deletedCount":3,"freedBytes":123456,"skippedRecentCount":1}}}"#,
        );
        let out = attachments_sweep_unreferenced_images(&client(&base), "ws-1").unwrap();
        assert_eq!(out.tx_id, Some(57));
        assert_eq!(out.deleted_count, 3);
        assert_eq!(out.freed_bytes, 123_456);
        assert_eq!(out.skipped_recent_count, 1);
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(
            request.starts_with("POST /api/trpc/attachments.sweepUnreferencedImages HTTP/1.1")
        );
        assert!(request.ends_with(r#"{"teamId":"ws-1"}"#));
        assert!(request.contains("Authorization: Bearer tok-1"));
    }

    #[test]
    fn delete_surfaces_the_server_message() {
        let (base, _captured) = one_shot_server(
            404,
            r#"{"error":{"message":"Attachment not found","code":-32603,"data":{"code":"NOT_FOUND","httpStatus":404}}}"#,
        );
        match attachments_delete(&client(&base), "att-gone") {
            Err(ApiError::Http { status, message }) => {
                assert_eq!(status, 404);
                assert!(message.contains("Attachment not found"));
            }
            other => panic!("expected Http error, got {other:?}"),
        }
    }
}
