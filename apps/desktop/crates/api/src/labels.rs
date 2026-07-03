//! Typed `labels.*` + `issueLabels.*` tRPC helpers (masterplan-v3 §4.2
//! Settings → Labels pane; label toggles on rows/detail). Verified against
//! `apps/web/src/lib/trpc/labels.ts` and `issue-labels.ts`:
//!
//! - `labels.create({workspaceId, name, color?})` → `{txId, label}` (color
//!   defaults server-side to `#6366f1`)
//! - `labels.update({workspaceId, labelId, name?, color?})` → `{txId}`
//! - `labels.delete({workspaceId, labelId})` → `{txId}`
//! - `issueLabels.add({issueId, labelId})` → `{txId}` (idempotent —
//!   `onConflictDoNothing`)
//! - `issueLabels.remove({issueId, labelId})` → `{txId}`
//!
//! Reads come from the synced `labels`/`issue_labels` collections, never a
//! tRPC list call (§4.1).

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::trpc::TrpcClient;

/// Slim camelCase mirror of the label row `labels.create` returns.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelOut {
    pub id: String,
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelsCreateOutput {
    pub label: LabelOut,
    #[serde(default)]
    pub tx_id: Option<i64>,
}

/// Output of the tx-only mutations (`labels.update/delete`,
/// `issueLabels.add/remove`).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxOutput {
    #[serde(default)]
    pub tx_id: Option<i64>,
}

/// `labels.create` — mutation. Blocking; background executor only (§3.5).
pub fn labels_create(
    trpc: &TrpcClient,
    workspace_id: &str,
    name: &str,
    color: Option<&str>,
) -> Result<LabelsCreateOutput, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        workspace_id: &'a str,
        name: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        color: Option<&'a str>,
    }
    trpc.mutation(
        "labels.create",
        &Input {
            workspace_id,
            name,
            color,
        },
    )
}

/// `labels.update` — mutation (inline name/color edits in the Labels pane).
pub fn labels_update(
    trpc: &TrpcClient,
    workspace_id: &str,
    label_id: &str,
    name: Option<&str>,
    color: Option<&str>,
) -> Result<TxOutput, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        workspace_id: &'a str,
        label_id: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        color: Option<&'a str>,
    }
    trpc.mutation(
        "labels.update",
        &Input {
            workspace_id,
            label_id,
            name,
            color,
        },
    )
}

/// `labels.delete` — mutation.
pub fn labels_delete(
    trpc: &TrpcClient,
    workspace_id: &str,
    label_id: &str,
) -> Result<TxOutput, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        workspace_id: &'a str,
        label_id: &'a str,
    }
    trpc.mutation(
        "labels.delete",
        &Input {
            workspace_id,
            label_id,
        },
    )
}

/// `issueLabels.add` — mutation (label toggle ON; idempotent server-side).
pub fn issue_labels_add(
    trpc: &TrpcClient,
    issue_id: &str,
    label_id: &str,
) -> Result<TxOutput, ApiError> {
    trpc.mutation("issueLabels.add", &IssueLabelInput { issue_id, label_id })
}

/// `issueLabels.remove` — mutation (label toggle OFF).
pub fn issue_labels_remove(
    trpc: &TrpcClient,
    issue_id: &str,
    label_id: &str,
) -> Result<TxOutput, ApiError> {
    trpc.mutation("issueLabels.remove", &IssueLabelInput { issue_id, label_id })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IssueLabelInput<'a> {
    issue_id: &'a str,
    label_id: &'a str,
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
    fn create_decodes_label_and_skips_absent_color() {
        let (base, captured) = one_shot_server(
            200,
            r##"{"result":{"data":{"txId":5,"label":{"id":"l-1","workspaceId":"w-1","name":"bug","color":"#6366f1"}}}}"##,
        );
        let out = labels_create(&client(&base), "w-1", "bug", None).unwrap();
        assert_eq!(out.label.name.as_deref(), Some("bug"));
        assert_eq!(out.tx_id, Some(5));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(r#"{"workspaceId":"w-1","name":"bug"}"#));
    }

    #[test]
    fn issue_label_toggles_post_both_ids() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txId":8}}}"#);
        let out = issue_labels_add(&client(&base), "i-1", "l-1").unwrap();
        assert_eq!(out.tx_id, Some(8));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/issueLabels.add HTTP/1.1"));
        assert!(request.ends_with(r#"{"issueId":"i-1","labelId":"l-1"}"#));

        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"txId":9}}}"#);
        let _ = issue_labels_remove(&client(&base), "i-1", "l-1").unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/issueLabels.remove HTTP/1.1"));
    }
}
