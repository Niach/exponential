//! Typed `releases.*` tRPC helpers (EXP-56 — the Releases surfaces). Shapes
//! verified against `apps/web/src/lib/trpc/releases.ts`:
//!
//! - `releases.create({workspaceId, name?, issueIds?})` → `{txId, release}`
//!   — name absent ⇒ the server auto-names sequentially ("Release N");
//!   issueIds (EXP-62, 1–200) attach in the same server transaction
//! - `releases.markShipped({id, shipped})` → `{txId}` — manual ship/unship
//!   (the GitHub webhook also auto-ships when the linked release PR merges)
//! - `releases.delete({id})` → `{txId}` — hard delete; `issues.release_id`
//!   is SET NULL server-side (issues survive, unbundled)
//! - `releases.setIssueRelease({issueId, releaseId|null})` → `{txId}` —
//!   moves ONE issue in/out of a release. NB: `releaseId` is `.nullable()`,
//!   not optional — the input always carries the key (`null` clears)
//! - `releases.addIssues({releaseId, issueIds[]})` → `{txId, added}` — bulk
//!   add (issues outside the release's workspace are silently skipped)
//!
//! Reads come from the synced `releases` collection, never a tRPC list call
//! (§4.1). All mutations are workspace-MEMBER gated server-side (releases are
//! team-manageable, no owner gate).

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::labels::TxOutput;
use crate::trpc::TrpcClient;

/// Slim camelCase mirror of the release row `releases.create` returns. Only
/// the fields the create flow consumes are typed strictly; the rest are
/// tolerant options so server-side additions never break the decode.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseOut {
    pub id: String,
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleasesCreateOutput {
    pub release: ReleaseOut,
    #[serde(default)]
    pub tx_id: Option<i64>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleasesAddIssuesOutput {
    #[serde(default)]
    pub added: Option<i64>,
    #[serde(default)]
    pub tx_id: Option<i64>,
}

/// `releases.create` — mutation. `None` name ⇒ the key is omitted and the
/// server auto-names the release ("Release N"). `issue_ids` is the
/// creation-time bundle (EXP-62): attached in the SAME server transaction
/// (addIssues semantics, incl. timeline events). The server caps it at 200
/// per call — callers chunk any overflow through [`add_issues`]. Empty ⇒
/// the key is omitted entirely (the pre-EXP-62 wire shape). Blocking;
/// background executor only (§3.5).
pub fn create(
    trpc: &TrpcClient,
    workspace_id: &str,
    name: Option<&str>,
    issue_ids: &[String],
) -> Result<ReleasesCreateOutput, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        workspace_id: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        issue_ids: Option<&'a [String]>,
    }
    trpc.mutation(
        "releases.create",
        &Input {
            workspace_id,
            name,
            issue_ids: (!issue_ids.is_empty()).then_some(issue_ids),
        },
    )
}

/// `releases.markShipped` — mutation (ship with `true`, unship with `false`).
pub fn mark_shipped(trpc: &TrpcClient, id: &str, shipped: bool) -> Result<TxOutput, ApiError> {
    #[derive(Serialize)]
    struct Input<'a> {
        id: &'a str,
        shipped: bool,
    }
    trpc.mutation("releases.markShipped", &Input { id, shipped })
}

/// `releases.delete` — mutation (hard delete; member issues are unbundled via
/// the server-side SET NULL, never deleted).
pub fn delete(trpc: &TrpcClient, id: &str) -> Result<TxOutput, ApiError> {
    #[derive(Serialize)]
    struct Input<'a> {
        id: &'a str,
    }
    trpc.mutation("releases.delete", &Input { id })
}

/// `releases.setIssueRelease` — mutation. `None` clears the issue's release
/// (the input's `releaseId` key is always present — the zod schema is
/// `.nullable()`, not `.optional()`).
pub fn set_issue_release(
    trpc: &TrpcClient,
    issue_id: &str,
    release_id: Option<&str>,
) -> Result<TxOutput, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        issue_id: &'a str,
        release_id: Option<&'a str>,
    }
    trpc.mutation(
        "releases.setIssueRelease",
        &Input {
            issue_id,
            release_id,
        },
    )
}

/// `releases.addIssues` — mutation (bulk add from a picker; already-member
/// issues are a server-side no-op).
pub fn add_issues(
    trpc: &TrpcClient,
    release_id: &str,
    issue_ids: &[String],
) -> Result<ReleasesAddIssuesOutput, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        release_id: &'a str,
        issue_ids: &'a [String],
    }
    trpc.mutation(
        "releases.addIssues",
        &Input {
            release_id,
            issue_ids,
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
    fn create_decodes_release_and_posts_explicit_name() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"txId":41,"release":{"id":"rel-1","workspaceId":"ws-1","name":"v1.0","description":null,"targetDate":null,"shippedAt":null}}}}"#,
        );
        let output = create(&client(&base), "ws-1", Some("v1.0"), &[]).unwrap();
        assert_eq!(output.release.id, "rel-1");
        assert_eq!(output.release.name.as_deref(), Some("v1.0"));
        assert_eq!(output.tx_id, Some(41));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/releases.create HTTP/1.1"));
        // Empty issue_ids ⇒ no issueIds key (the pre-EXP-62 wire shape).
        assert!(request.ends_with(r#"{"workspaceId":"ws-1","name":"v1.0"}"#));
    }

    #[test]
    fn create_carries_the_creation_time_issue_ids() {
        // EXP-62: the creation-time bundle rides the create call itself and
        // the server attaches it in the same transaction.
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"txId":42,"release":{"id":"rel-2","workspaceId":"ws-1","name":"v2.0"}}}}"#,
        );
        let ids = vec!["issue-1".to_string(), "issue-2".to_string()];
        let output = create(&client(&base), "ws-1", Some("v2.0"), &ids).unwrap();
        assert_eq!(output.release.id, "rel-2");
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(
            r#"{"workspaceId":"ws-1","name":"v2.0","issueIds":["issue-1","issue-2"]}"#
        ));
    }

    #[test]
    fn create_omits_absent_name_for_server_auto_naming() {
        // The instant-create path: no name key at all — the server picks
        // "Release N" (an explicit null would fail the zod min(1)).
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"txId":1,"release":{"id":"rel-1","workspaceId":"ws-1","name":"Release 4"}}}}"#,
        );
        let output = create(&client(&base), "ws-1", None, &[]).unwrap();
        assert_eq!(output.release.name.as_deref(), Some("Release 4"));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(r#"{"workspaceId":"ws-1"}"#));
    }

    #[test]
    fn mark_shipped_and_delete_post_expected_bodies() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txId":7}}}"#);
        let output = mark_shipped(&client(&base), "rel-1", true).unwrap();
        assert_eq!(output.tx_id, Some(7));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/releases.markShipped HTTP/1.1"));
        assert!(request.ends_with(r#"{"id":"rel-1","shipped":true}"#));

        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txId":8}}}"#);
        let _ = delete(&client(&base), "rel-1").unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/releases.delete HTTP/1.1"));
        assert!(request.ends_with(r#"{"id":"rel-1"}"#));
    }

    #[test]
    fn set_issue_release_always_carries_the_release_id_key() {
        // Set — a real id.
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txId":9}}}"#);
        let _ = set_issue_release(&client(&base), "issue-1", Some("rel-1")).unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/releases.setIssueRelease HTTP/1.1"));
        assert!(request.ends_with(r#"{"issueId":"issue-1","releaseId":"rel-1"}"#));

        // Clear — the zod schema is `.nullable()`: the key must be an
        // explicit null, never omitted.
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txId":10}}}"#);
        let _ = set_issue_release(&client(&base), "issue-1", None).unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(r#"{"issueId":"issue-1","releaseId":null}"#));
    }

    #[test]
    fn add_issues_posts_the_id_array_and_decodes_added() {
        let (base, captured) =
            one_shot_server(200, r#"{"result":{"data":{"txId":11,"added":2}}}"#);
        let output = add_issues(
            &client(&base),
            "rel-1",
            &["issue-1".to_string(), "issue-2".to_string()],
        )
        .unwrap();
        assert_eq!(output.added, Some(2));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/releases.addIssues HTTP/1.1"));
        assert!(
            request.ends_with(r#"{"releaseId":"rel-1","issueIds":["issue-1","issue-2"]}"#)
        );
    }
}
