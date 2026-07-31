//! Typed `statuses.*` tRPC helpers (EXP-314 — Settings → Issue statuses).
//! Verified against `apps/web/src/lib/trpc/statuses.ts`:
//!
//! - `statuses.create({teamId, category, name, color})` → `{txId, status}`
//! - `statuses.update({teamId, statusId, name?, color?})` → `{txId}` —
//!   builtins are locked server-side (`BAD_REQUEST`); there is deliberately NO
//!   `category` input (a status's category is immutable)
//! - `statuses.move({teamId, statusId, direction})` → `{txId}` — swaps with
//!   the neighbor WITHIN the category; builtins ARE movable
//! - `statuses.delete({teamId, statusId, reassignToId?})` →
//!   `{txId, reassigned, reassignedToId}` — `reassignToId` is REQUIRED
//!   whenever issues still reference the status (`PRECONDITION_FAILED`
//!   otherwise; the server's count includes trashed-board issues, so the
//!   client's synced count can undershoot — always honor the error)
//! - `statuses.referencingCount({teamId, statusId})` → `{count}` — query
//!   (EXP-320): the server-authoritative referencing-issue count, trashed
//!   boards included — the delete dialog's copy source
//!
//! Writes are member-level (`mutate_resources`, like labels — no owner gate).
//! Reads come from the synced `issue_statuses` collection, never a tRPC list
//! call (§4.1). All calls are blocking; background executor only (§3.5).

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::labels::TxOutput;
use crate::trpc::TrpcClient;

/// Slim camelCase mirror of the row `statuses.create` returns.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusOut {
    pub id: String,
    #[serde(default)]
    pub team_id: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub builtin_key: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusesCreateOutput {
    pub status: StatusOut,
    #[serde(default)]
    pub tx_id: Option<i64>,
}

/// Output of `statuses.delete`: how many issues were moved, and where.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusesDeleteOutput {
    #[serde(default)]
    pub reassigned: Option<i64>,
    #[serde(default)]
    pub reassigned_to_id: Option<String>,
    #[serde(default)]
    pub tx_id: Option<i64>,
}

/// Which way [`statuses_move`] shifts a status within its category.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveDirection {
    Up,
    Down,
}

impl MoveDirection {
    fn as_wire(self) -> &'static str {
        match self {
            MoveDirection::Up => "up",
            MoveDirection::Down => "down",
        }
    }
}

/// `statuses.create` — mutation. `category` is a
/// `domain::contract::ISSUE_STATUS_CATEGORY_VALUES` wire value (never
/// `duplicate` — the server refuses); `color` is a required `#rrggbb`.
pub fn statuses_create(
    trpc: &TrpcClient,
    team_id: &str,
    category: &str,
    name: &str,
    color: &str,
) -> Result<StatusesCreateOutput, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        team_id: &'a str,
        category: &'a str,
        name: &'a str,
        color: &'a str,
    }
    trpc.mutation(
        "statuses.create",
        &Input {
            team_id,
            category,
            name,
            color,
        },
    )
}

/// `statuses.update` — mutation (inline name/color edits on CUSTOM rows).
pub fn statuses_update(
    trpc: &TrpcClient,
    team_id: &str,
    status_id: &str,
    name: Option<&str>,
    color: Option<&str>,
) -> Result<TxOutput, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        team_id: &'a str,
        status_id: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        color: Option<&'a str>,
    }
    trpc.mutation(
        "statuses.update",
        &Input {
            team_id,
            status_id,
            name,
            color,
        },
    )
}

/// `statuses.move` — mutation. A move past a category edge is an idempotent
/// server-side no-op (safe for double-clicks).
pub fn statuses_move(
    trpc: &TrpcClient,
    team_id: &str,
    status_id: &str,
    direction: MoveDirection,
) -> Result<TxOutput, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        team_id: &'a str,
        status_id: &'a str,
        direction: &'a str,
    }
    trpc.mutation(
        "statuses.move",
        &Input {
            team_id,
            status_id,
            direction: direction.as_wire(),
        },
    )
}

/// Output of `statuses.referencingCount`: the server-authoritative number of
/// issues still referencing a status (trashed-board issues included).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusesReferencingCountOutput {
    pub count: i64,
}

/// `statuses.referencingCount` — query (EXP-320). The synced issue count can
/// undershoot the server's (trashed-board issues never sync); the delete
/// dialog shows this count instead.
pub fn statuses_referencing_count(
    trpc: &TrpcClient,
    team_id: &str,
    status_id: &str,
) -> Result<StatusesReferencingCountOutput, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        team_id: &'a str,
        status_id: &'a str,
    }
    trpc.query_with_input(
        "statuses.referencingCount",
        &Input { team_id, status_id },
    )
}

/// `statuses.delete` — mutation. Pass `reassign_to_id` whenever any issue
/// still sits in the status; omitting it there fails with a
/// `PRECONDITION_FAILED` naming the count.
pub fn statuses_delete(
    trpc: &TrpcClient,
    team_id: &str,
    status_id: &str,
    reassign_to_id: Option<&str>,
) -> Result<StatusesDeleteOutput, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        team_id: &'a str,
        status_id: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        reassign_to_id: Option<&'a str>,
    }
    trpc.mutation(
        "statuses.delete",
        &Input {
            team_id,
            status_id,
            reassign_to_id,
        },
    )
}

/// Which PR event [`statuses_set_pr_automation`] configures (EXP-319).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrAutomationEvent {
    Opened,
    Merged,
}

impl PrAutomationEvent {
    fn as_wire(self) -> &'static str {
        match self {
            PrAutomationEvent::Opened => "pr_opened",
            PrAutomationEvent::Merged => "pr_merged",
        }
    }
}

/// The target [`statuses_set_pr_automation`] writes: a status row uuid,
/// `Default` (reset to NULL — the builtin In Review/Done fallback), or
/// `DoNothing` (disable the automation).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrAutomationTarget {
    Status(String),
    Default,
    DoNothing,
}

impl PrAutomationTarget {
    fn as_wire(&self) -> &str {
        match self {
            PrAutomationTarget::Status(id) => id,
            PrAutomationTarget::Default => "default",
            PrAutomationTarget::DoNothing => "none",
        }
    }
}

/// `statuses.setPrAutomation` — mutation (EXP-319, member-level like every
/// other write in this file). Sets where issues move when a PR opens/merges;
/// convergence is the teams-shape Electric echo.
pub fn statuses_set_pr_automation(
    trpc: &TrpcClient,
    team_id: &str,
    event: PrAutomationEvent,
    target: &PrAutomationTarget,
) -> Result<TxOutput, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        team_id: &'a str,
        event: &'a str,
        target: &'a str,
    }
    trpc.mutation(
        "statuses.setPrAutomation",
        &Input {
            team_id,
            event: event.as_wire(),
            target: target.as_wire(),
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
    fn create_posts_the_full_camel_case_body() {
        let (base, captured) = one_shot_server(
            200,
            r##"{"result":{"data":{"txId":5,"status":{"id":"s-1","teamId":"t-1","category":"started","name":"QA","color":"#22c55e","builtinKey":null}}}}"##,
        );
        let out = statuses_create(&client(&base), "t-1", "started", "QA", "#22c55e").unwrap();
        assert_eq!(out.status.name.as_deref(), Some("QA"));
        assert_eq!(out.status.builtin_key, None);
        assert_eq!(out.tx_id, Some(5));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/statuses.create HTTP/1.1"));
        assert!(request
            .ends_with(r##"{"teamId":"t-1","category":"started","name":"QA","color":"#22c55e"}"##));
    }

    #[test]
    fn update_omits_untouched_fields() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txId":6}}}"#);
        let out = statuses_update(&client(&base), "t-1", "s-1", Some("Testing"), None).unwrap();
        assert_eq!(out.tx_id, Some(6));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(r#"{"teamId":"t-1","statusId":"s-1","name":"Testing"}"#));

        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txId":7}}}"#);
        let _ = statuses_update(&client(&base), "t-1", "s-1", None, Some("#ef4444")).unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(r##"{"teamId":"t-1","statusId":"s-1","color":"#ef4444"}"##));
    }

    #[test]
    fn move_sends_the_direction_wire_value() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txId":8}}}"#);
        let _ = statuses_move(&client(&base), "t-1", "s-1", MoveDirection::Up).unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/statuses.move HTTP/1.1"));
        assert!(request.ends_with(r#"{"teamId":"t-1","statusId":"s-1","direction":"up"}"#));

        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txId":9}}}"#);
        let _ = statuses_move(&client(&base), "t-1", "s-1", MoveDirection::Down).unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(r#"{"teamId":"t-1","statusId":"s-1","direction":"down"}"#));
    }

    #[test]
    fn set_pr_automation_sends_all_three_target_forms() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txId":12}}}"#);
        let out = statuses_set_pr_automation(
            &client(&base),
            "t-1",
            PrAutomationEvent::Opened,
            &PrAutomationTarget::Status("s-1".to_string()),
        )
        .unwrap();
        assert_eq!(out.tx_id, Some(12));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/statuses.setPrAutomation HTTP/1.1"));
        assert!(request.ends_with(r#"{"teamId":"t-1","event":"pr_opened","target":"s-1"}"#));

        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txId":13}}}"#);
        let _ = statuses_set_pr_automation(
            &client(&base),
            "t-1",
            PrAutomationEvent::Merged,
            &PrAutomationTarget::DoNothing,
        )
        .unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(r#"{"teamId":"t-1","event":"pr_merged","target":"none"}"#));

        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txId":14}}}"#);
        let _ = statuses_set_pr_automation(
            &client(&base),
            "t-1",
            PrAutomationEvent::Merged,
            &PrAutomationTarget::Default,
        )
        .unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(r#"{"teamId":"t-1","event":"pr_merged","target":"default"}"#));
    }

    #[test]
    fn delete_omits_the_reassign_target_when_none() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"txId":10,"reassigned":0,"reassignedToId":null}}}"#,
        );
        let out = statuses_delete(&client(&base), "t-1", "s-1", None).unwrap();
        assert_eq!(out.reassigned, Some(0));
        assert_eq!(out.reassigned_to_id, None);
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(r#"{"teamId":"t-1","statusId":"s-1"}"#));

        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"txId":11,"reassigned":3,"reassignedToId":"s-2"}}}"#,
        );
        let out = statuses_delete(&client(&base), "t-1", "s-1", Some("s-2")).unwrap();
        assert_eq!(out.reassigned, Some(3));
        assert_eq!(out.reassigned_to_id.as_deref(), Some("s-2"));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(
            request.ends_with(r#"{"teamId":"t-1","statusId":"s-1","reassignToId":"s-2"}"#)
        );
    }

    #[test]
    fn referencing_count_is_a_get_query() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"count":3}}}"#);
        let out = statuses_referencing_count(&client(&base), "t-1", "s-1").unwrap();
        assert_eq!(out.count, 3);
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("GET /api/trpc/statuses.referencingCount?input="));
    }

    #[test]
    fn precondition_failures_surface_their_message() {
        // The reassign-required refusal the pane renders inline.
        let (base, _captured) = one_shot_server(
            412,
            r#"{"error":{"json":{"message":"3 issues use this status. Pick a replacement first.","code":-32003}}}"#,
        );
        let err = statuses_delete(&client(&base), "t-1", "s-1", None).unwrap_err();
        match err {
            ApiError::Http { message, .. } => {
                assert!(message.contains("Pick a replacement first"), "{message}");
            }
            other => panic!("expected an HTTP error, got {other:?}"),
        }
    }
}
