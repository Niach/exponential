//! Typed `attachments.*` tRPC helpers (EXP-297).
//!
//! The attachment BYTES move over plain HTTP (multipart upload to
//! `POST /api/issues/{id}/files`, bearer GET on `/api/attachments/{id}` —
//! both in `ui::markdown::image_paste`); this module carries the one tRPC
//! mutation the files rail needs.
//!
//! Wire shape verified against `apps/web/src/lib/trpc/attachments.ts`:
//!
//! ```ts
//! // input
//! { id: string /* uuid */ }
//! // output
//! { txId: number }
//! ```
//!
//! `delete` is MEMBER-level (any team member may delete an attachment of
//! their team). The server rewrites every issue description / comment body
//! that still embeds the attachment, replacing the image with the plain-text
//! placeholder `*(deleted image: <label>)*`, drops the row, and reclaims the
//! blob after the transaction commits — so the Electric echo of both the
//! rewritten texts and the vanished attachment row is what updates the UI.

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
