//! Typed `boards.*` tRPC helpers (masterplan-v3 §4.2 create-board dialog +
//! settings Boards pane). Verified against
//! `apps/web/src/lib/trpc/boards.ts`:
//!
//! - `boards.create({teamId, name, prefix, icon?, color?,
//!   repository?})` → `{board, txId}` — `repository` (optional) is the
//!   `{repositoryId}` OR `{fullName, …}` union
//!   ([`BoardRepositoryInput`]). (slug is server-derived — there is no slug
//!   field, §4.2; prefix is uppercased server-side.)
//! - `boards.update({id, name?, color?, archivedAt?})` → `{board}` (no txId).
//! - `boards.delete({boardId})` → `{ok, txId}` (owner-only).
//!
//! (`boards.updatePreviewConfig` is the §7 run-target mirror — owned by the
//! run-configs surface, not wrapped here.)

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::patch::Patch;
use crate::trpc::TrpcClient;

/// Slim camelCase mirror of the board row a mutation returns.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardOut {
    pub id: String,
    #[serde(default)]
    pub team_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    /// v4 §3.1: the board's one repository (`boards.repositoryId`).
    #[serde(default)]
    pub repository_id: Option<String>,
}

/// The backing repository for a new board (v4 §3.1 — `boards.repositoryId`
/// is NOT NULL, so `boards.create` requires one). Mirrors the server's
/// `repositoryInputSchema` union: either `Registry` (an existing registry repo,
/// `{repositoryId}`) or `Inline` (connect a GitHub-App repo in the same
/// transaction, `{fullName, defaultBranch?, private?}` — the installation id
/// is resolved server-side, never sent by clients). The
/// create-board dialog offers both — the registry picker and, when the App is
/// installed, an inline GitHub repo.
#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum BoardRepositoryInput {
    #[serde(rename_all = "camelCase")]
    Registry {
        repository_id: String,
    },
    #[serde(rename_all = "camelCase")]
    Inline {
        full_name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        default_branch: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        private: Option<bool>,
    },
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardsCreateInput {
    pub team_id: String,
    pub name: String,
    /// ≤10 chars; server uppercases (web derives it from the name but keeps
    /// it editable — `derivePrefix`, §4.2).
    pub prefix: String,
    /// Curated icon name (`domain::contract::BOARD_ICON_VALUES`); omitted when
    /// unset so the server picks a default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// `#rrggbb`; server defaults to `#6366f1` when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// The board's backing repository. Optional (nullable
    /// `repository_id`) — omitted for a repo-less board.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<BoardRepositoryInput>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardsCreateOutput {
    pub board: BoardOut,
    #[serde(default)]
    pub tx_id: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardsUpdateInput {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// ISO datetime to archive; `Null` un-archives (owner-only server-side).
    #[serde(skip_serializing_if = "Patch::is_omit")]
    pub archived_at: Patch<String>,
}

impl BoardsUpdateInput {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: None,
            color: None,
            archived_at: Patch::Omit,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardsUpdateOutput {
    pub board: BoardOut,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OkTxOutput {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub tx_id: Option<i64>,
}

/// `boards.create` — mutation. Blocking; background executor only (§3.5).
pub fn boards_create(
    trpc: &TrpcClient,
    input: &BoardsCreateInput,
) -> Result<BoardsCreateOutput, ApiError> {
    trpc.mutation("boards.create", input)
}

/// `boards.update` — mutation.
pub fn boards_update(
    trpc: &TrpcClient,
    input: &BoardsUpdateInput,
) -> Result<BoardsUpdateOutput, ApiError> {
    trpc.mutation("boards.update", input)
}

/// `boards.setRepository({boardId, repositoryId})` → `{board, txId}`
/// (v4 §3.2 / §5.3): retarget a board at another registry repository. The
/// v4 replacement for the deleted `repositories.link/unlink/setPrimary` link
/// procs — a board has exactly one repository now. Owner/manage-repos gated
/// server-side; the repo must belong to the board's team.
pub fn boards_set_repository(
    trpc: &TrpcClient,
    board_id: &str,
    repository_id: &str,
) -> Result<BoardsCreateOutput, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        board_id: &'a str,
        repository_id: &'a str,
    }
    trpc.mutation(
        "boards.setRepository",
        &Input { board_id, repository_id },
    )
}

/// `boards.delete` — mutation (owner-only; plan-cap/permission failures
/// surface as `ApiError::Http` with the server message — the §4.9 "Upgrade on
/// the web" notification is the UI's concern).
pub fn boards_delete(trpc: &TrpcClient, board_id: &str) -> Result<OkTxOutput, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        board_id: &'a str,
    }
    trpc.mutation("boards.delete", &Input { board_id })
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
    fn create_posts_camel_case_and_decodes_board() {
        let (base, captured) = one_shot_server(
            200,
            r##"{"result":{"data":{"board":{"id":"p-1","teamId":"w-1","name":"Gate","slug":"gate","prefix":"GATE","color":"#6366f1"},"txId":9}}}"##,
        );
        let out = boards_create(
            &client(&base),
            &BoardsCreateInput {
                team_id: "w-1".to_string(),
                name: "Gate".to_string(),
                prefix: "gate".to_string(),
                icon: Some("code".to_string()),
                color: None,
                repository: Some(BoardRepositoryInput::Registry {
                    repository_id: "repo-1".to_string(),
                }),
            },
        )
        .unwrap();
        assert_eq!(out.board.slug.as_deref(), Some("gate"));
        assert_eq!(out.tx_id, Some(9));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/boards.create HTTP/1.1"));
        // No slug in the input (server-derived, §4.2) and no null color noise;
        // the board sends `icon` and the repository as the `{repositoryId}`
        // registry arm.
        assert!(request.ends_with(
            r#"{"teamId":"w-1","name":"Gate","prefix":"gate","icon":"code","repository":{"repositoryId":"repo-1"}}"#
        ));
    }

    #[test]
    fn inline_repository_serializes_as_camel_case_union_arm() {
        let inline = BoardRepositoryInput::Inline {
            full_name: "acme/app".to_string(),
            default_branch: Some("main".to_string()),
            private: Some(true),
        };
        assert_eq!(
            serde_json::to_string(&inline).unwrap(),
            r#"{"fullName":"acme/app","defaultBranch":"main","private":true}"#
        );
        // Optional fields drop out when unknown (server fills defaults).
        let sparse = BoardRepositoryInput::Inline {
            full_name: "acme/app".to_string(),
            default_branch: None,
            private: None,
        };
        assert_eq!(
            serde_json::to_string(&sparse).unwrap(),
            r#"{"fullName":"acme/app"}"#
        );
    }

    #[test]
    fn update_omits_untouched_archived_at() {
        let mut input = BoardsUpdateInput::new("p-1");
        input.name = Some("Renamed".to_string());
        let json = serde_json::to_string(&input).unwrap();
        assert_eq!(json, r#"{"id":"p-1","name":"Renamed"}"#);

        // Explicit un-archive is a null, not an omission.
        let mut input = BoardsUpdateInput::new("p-1");
        input.archived_at = Patch::Null;
        let json = serde_json::to_string(&input).unwrap();
        assert_eq!(json, r#"{"id":"p-1","archivedAt":null}"#);
    }

    #[test]
    fn delete_decodes_ok_tx() {
        let (base, captured) =
            one_shot_server(200, r#"{"result":{"data":{"ok":true,"txId":3}}}"#);
        let out = boards_delete(&client(&base), "p-1").unwrap();
        assert!(out.ok);
        assert_eq!(out.tx_id, Some(3));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(r#"{"boardId":"p-1"}"#));
    }
}
