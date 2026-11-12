//! Typed `composerDrafts.*` tRPC helpers (#FEED-219, composer-draft.json).
//! Verified against `apps/web/src/lib/trpc/composer-drafts.ts`:
//!
//! - `composerDrafts.upsert({teamId, key, text})` → `{draft, txId}`; refuses
//!   an empty text (that is a delete: [`domain::composer_draft::write_for`]).
//! - `composerDrafts.delete({key})` → `{txId, deleted}`; deleting nothing is fine.
//!
//! The row re-streams over the per-user `composer_drafts` shape, so callers
//! fire-and-forget; a failed write is silent (the writer retries).

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::trpc::TrpcClient;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposerDraftWriteOutput {
    #[serde(default)]
    pub tx_id: Option<i64>,
}

/// `composerDrafts.upsert` — mutation.
pub fn composer_drafts_upsert(
    trpc: &TrpcClient,
    team_id: &str,
    key: &str,
    text: &str,
) -> Result<ComposerDraftWriteOutput, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        team_id: &'a str,
        key: &'a str,
        text: &'a str,
    }
    trpc.mutation("composerDrafts.upsert", &Input { team_id, key, text })
}

/// `composerDrafts.delete` — mutation.
pub fn composer_drafts_delete(trpc: &TrpcClient, key: &str) -> Result<ComposerDraftWriteOutput, ApiError> {
    #[derive(Serialize)]
    struct Input<'a> {
        key: &'a str,
    }
    trpc.mutation("composerDrafts.delete", &Input { key })
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
    fn upsert_posts_team_key_and_text() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txId":7,"draft":{"id":"d-1"}}}}"#);
        let out = composer_drafts_upsert(&client(&base), "t-1", "comment:i-1", " hi\n").unwrap();
        assert_eq!(out.tx_id, Some(7));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/composerDrafts.upsert HTTP/1.1"));
        assert!(request.ends_with(r#"{"teamId":"t-1","key":"comment:i-1","text":" hi\n"}"#));
    }

    #[test]
    fn delete_posts_the_key() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txId":9,"deleted":false}}}"#);
        let out = composer_drafts_delete(&client(&base), "newChat:t-1").unwrap();
        assert_eq!(out.tx_id, Some(9));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/composerDrafts.delete HTTP/1.1"));
        assert!(request.ends_with(r#"{"key":"newChat:t-1"}"#));
    }
}
