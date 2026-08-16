//! Typed `attachments.*` tRPC helpers (EXP-297).
//!
//! The attachment BYTES move over plain HTTP (multipart upload to
//! `POST /api/issues/{id}/files`, bearer GET on `/api/attachments/{id}` —
//! both in `ui::markdown::image_paste`); this module carries the tRPC
//! mutations the files rail and the settings Storage pane need, plus the ONE
//! byte path the gpui-free hosts share ([`download_image`], EXP-511 — the CLI
//! daemon cannot reach into `ui`).
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

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::trpc::TrpcClient;

/// EXP-511: the image content types a steered message may embed, mapped to the
/// extension the local copy is written with. Anything else is REFUSED — the
/// localizer exists so the agent can read an image, and pointing it at an
/// arbitrary blob is worse than leaving the URL in the message.
const IMAGE_EXTENSIONS: &[(&str, &str)] = &[
    ("image/png", "png"),
    ("image/jpeg", "jpg"),
    ("image/webp", "webp"),
    ("image/gif", "gif"),
    ("image/avif", "avif"),
];

/// The local file extension for an attachment `Content-Type` (parameters like
/// `; charset=` are ignored), or `None` for a non-image type.
pub fn image_extension(content_type: &str) -> Option<&'static str> {
    let base = content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    IMAGE_EXTENSIONS
        .iter()
        .find(|(mime, _)| *mime == base)
        .map(|(_, extension)| *extension)
}

/// EXP-511 steer-image localization: download attachment `attachment_id` into
/// `dest_dir` as `{id}.{ext}` and return its path, so the publisher can hand
/// the agent a file to read instead of a URL to fetch. Auth is the device's
/// own bearer, so the server's membership ACL applies unchanged.
///
/// Attachment blobs are immutable, so an already-present `{id}.{ext}` short
/// circuits the round trip. Blocking; background executor only (§3.5).
pub fn download_image(
    trpc: &TrpcClient,
    attachment_id: &str,
    dest_dir: &Path,
) -> Result<PathBuf, ApiError> {
    // The id becomes a FILENAME — anything but the server's id alphabet could
    // escape `dest_dir` (the publisher only ever passes matched UUIDs, but
    // this function is public).
    if attachment_id.is_empty()
        || !attachment_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err(ApiError::InvalidUrl(format!(
            "not an attachment id: {attachment_id:?}"
        )));
    }
    for (_, extension) in IMAGE_EXTENSIONS {
        let existing = dest_dir.join(format!("{attachment_id}.{extension}"));
        if existing.is_file() {
            return Ok(existing);
        }
    }
    let (bytes, content_type) = trpc.get_bytes(&format!("/api/attachments/{attachment_id}"))?;
    let content_type = content_type.unwrap_or_default();
    let Some(extension) = image_extension(&content_type) else {
        return Err(ApiError::Decode(format!(
            "attachment {attachment_id} is not an image ({content_type})"
        )));
    };
    std::fs::create_dir_all(dest_dir)
        .map_err(|err| ApiError::Io(format!("{}: {err}", dest_dir.display())))?;
    let path = dest_dir.join(format!("{attachment_id}.{extension}"));
    std::fs::write(&path, &bytes)
        .map_err(|err| ApiError::Io(format!("{}: {err}", path.display())))?;
    Ok(path)
}

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
    use crate::trpc::tests::{has_header, one_shot_server, one_shot_server_typed};
    use crate::StaticToken;
    use std::sync::Arc;
    use std::time::Duration;

    /// A scratch directory that removes itself — the EXP-511 download writes
    /// real files.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "exp-attachments-{tag}-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&path);
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

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
        assert!(has_header(&request, "Authorization: Bearer tok-1"));
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
        assert!(has_header(&request, "Authorization: Bearer tok-1"));
    }

    // ── EXP-511: steer-image localization ───────────────────────────────────

    #[test]
    fn image_extension_maps_the_accepted_types_only() {
        assert_eq!(image_extension("image/png"), Some("png"));
        assert_eq!(image_extension("image/jpeg"), Some("jpg"));
        assert_eq!(image_extension("image/webp"), Some("webp"));
        assert_eq!(image_extension("image/gif"), Some("gif"));
        assert_eq!(image_extension("image/avif"), Some("avif"));
        // Servers append parameters and vary the case.
        assert_eq!(image_extension("IMAGE/PNG; charset=binary"), Some("png"));
        // Everything else is refused — the agent gets the URL instead.
        assert_eq!(image_extension("application/pdf"), None);
        assert_eq!(image_extension(""), None);
    }

    #[test]
    fn download_image_writes_the_blob_under_its_id() {
        let dir = TempDir::new("download");
        let (base, captured) = one_shot_server_typed(200, "image/png", "PNGBYTES");
        let path = download_image(&client(&base), "att-1", &dir.0).unwrap();
        assert_eq!(path, dir.0.join("att-1.png"));
        assert_eq!(std::fs::read(&path).unwrap(), b"PNGBYTES");
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("GET /api/attachments/att-1 HTTP/1.1"));
        assert!(has_header(&request, "Authorization: Bearer tok-1"));
    }

    #[test]
    fn download_image_skips_the_round_trip_when_the_file_is_there() {
        // Blobs are immutable, so a present `{id}.{ext}` is authoritative —
        // asserted against a base URL nothing is listening on.
        let dir = TempDir::new("cached");
        std::fs::create_dir_all(&dir.0).unwrap();
        std::fs::write(dir.0.join("att-2.webp"), b"cached").unwrap();
        let path = download_image(&client("http://127.0.0.1:1"), "att-2", &dir.0).unwrap();
        assert_eq!(path, dir.0.join("att-2.webp"));
    }

    #[test]
    fn download_image_refuses_non_images_and_non_ids() {
        let dir = TempDir::new("refused");
        let (base, _captured) = one_shot_server_typed(200, "application/pdf", "%PDF");
        match download_image(&client(&base), "att-3", &dir.0) {
            Err(ApiError::Decode(message)) => assert!(message.contains("not an image")),
            other => panic!("expected Decode, got {other:?}"),
        }
        assert!(!dir.0.join("att-3.png").exists());
        // A traversal-shaped id never reaches the network or the disk.
        match download_image(&client("http://127.0.0.1:1"), "../etc/passwd", &dir.0) {
            Err(ApiError::InvalidUrl(_)) => {}
            other => panic!("expected InvalidUrl, got {other:?}"),
        }
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
