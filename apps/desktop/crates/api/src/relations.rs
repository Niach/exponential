//! Typed `relations.*` tRPC helpers (EXP-736). Verified against
//! `apps/web/src/lib/trpc/relations.ts`:
//!
//! - `relations.create({issueId, relatedIssueId, type, inverse?})` → `{txId}`
//!   — `inverse` flips the pair before the server stores the canonical row
//!   (`blocks` = issue blocks related, `parent` = issue is the parent,
//!   `duplicate` = issue is the duplicate, `related` = symmetric); a
//!   `duplicate` create delegates server-side to `issues.update`, so the
//!   desktop's "Duplicate of" pick goes through the existing duplicate
//!   picker instead of this call.
//! - `relations.delete({id})` → `{txId}`
//!
//! Reads come from the synced `issue_relations` collection, never a tRPC
//! list call (§4.1).

use serde::Serialize;

use crate::error::ApiError;
use crate::labels::TxOutput;
use crate::trpc::TrpcClient;

/// `relations.create` — mutation. Blocking; background executor only (§3.5).
pub fn relations_create(
    trpc: &TrpcClient,
    issue_id: &str,
    related_issue_id: &str,
    kind: &str,
    inverse: bool,
) -> Result<TxOutput, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        issue_id: &'a str,
        related_issue_id: &'a str,
        // The wire field is `type` — Rust's keyword, so it is renamed.
        #[serde(rename = "type")]
        kind: &'a str,
        inverse: bool,
    }
    trpc.mutation(
        "relations.create",
        &Input {
            issue_id,
            related_issue_id,
            kind,
            inverse,
        },
    )
}

/// `relations.delete` — mutation (the row's hover remove).
pub fn relations_delete(trpc: &TrpcClient, id: &str) -> Result<TxOutput, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        id: &'a str,
    }
    trpc.mutation("relations.delete", &Input { id })
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
    fn create_posts_the_canonical_pair_and_the_inverse_flag() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txId":7}}}"#);
        let out = relations_create(&client(&base), "i-1", "i-2", "blocks", true).unwrap();
        assert_eq!(out.tx_id, Some(7));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/relations.create HTTP/1.1"));
        assert!(request.ends_with(
            r#"{"issueId":"i-1","relatedIssueId":"i-2","type":"blocks","inverse":true}"#
        ));
    }

    #[test]
    fn delete_posts_the_row_id() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txId":8}}}"#);
        let out = relations_delete(&client(&base), "r-1").unwrap();
        assert_eq!(out.tx_id, Some(8));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/relations.delete HTTP/1.1"));
        assert!(request.ends_with(r#"{"id":"r-1"}"#));
    }
}
