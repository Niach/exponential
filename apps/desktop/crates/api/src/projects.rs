//! Typed `projects.*` tRPC helpers (masterplan-v3 §4.2 create-project dialog +
//! settings Projects pane). Verified against
//! `apps/web/src/lib/trpc/projects.ts`:
//!
//! - `projects.create({workspaceId, name, prefix, color?, repository})` →
//!   `{project, txId}` — `repository` is the `{repositoryId}` OR
//!   `{fullName, …}` union ([`ProjectRepositoryInput`]).
//!   (slug is server-derived — there is no slug field, §4.2; prefix is
//!   uppercased server-side).
//! - `projects.update({id, name?, color?, archivedAt?})` → `{project}` (no txId).
//! - `projects.delete({projectId})` → `{ok, txId}` (owner-only).
//!
//! (`projects.updatePreviewConfig` is the §7 run-target mirror — owned by the
//! run-configs surface, not wrapped here.)

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::patch::Patch;
use crate::trpc::TrpcClient;

/// Slim camelCase mirror of the project row a mutation returns.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectOut {
    pub id: String,
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    /// v4 §3.1: the project's one repository (`projects.repositoryId`).
    #[serde(default)]
    pub repository_id: Option<String>,
}

/// The backing repository for a new project (v4 §3.1 — `projects.repositoryId`
/// is NOT NULL, so `projects.create` requires one). Mirrors the server's
/// `repositoryInputSchema` union: either `Registry` (an existing registry repo,
/// `{repositoryId}`) or `Inline` (connect a GitHub-App repo in the same
/// transaction, `{fullName, defaultBranch?, private?}` — the installation id
/// is resolved server-side, never sent by clients). The
/// create-project dialog offers both — the registry picker and, when the App is
/// installed, an inline GitHub repo.
#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum ProjectRepositoryInput {
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
pub struct ProjectsCreateInput {
    pub workspace_id: String,
    pub name: String,
    /// ≤10 chars; server uppercases (web derives it from the name but keeps
    /// it editable — `derivePrefix`, §4.2).
    pub prefix: String,
    /// `dev` / `tasks` / `feedback` (v7). `type` is a Rust keyword so this
    /// renames onto the JSON field `type`; the server defaults to `dev`.
    #[serde(rename = "type")]
    pub project_type: String,
    /// `#rrggbb`; server defaults to `#6366f1` when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// The project's backing repository. Required for `dev` projects; omitted
    /// for repo-less `tasks`/`feedback` boards (v7 — nullable `repository_id`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<ProjectRepositoryInput>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectsCreateOutput {
    pub project: ProjectOut,
    #[serde(default)]
    pub tx_id: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectsUpdateInput {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// ISO datetime to archive; `Null` un-archives (owner-only server-side).
    #[serde(skip_serializing_if = "Patch::is_omit")]
    pub archived_at: Patch<String>,
}

impl ProjectsUpdateInput {
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
pub struct ProjectsUpdateOutput {
    pub project: ProjectOut,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OkTxOutput {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub tx_id: Option<i64>,
}

/// `projects.create` — mutation. Blocking; background executor only (§3.5).
pub fn projects_create(
    trpc: &TrpcClient,
    input: &ProjectsCreateInput,
) -> Result<ProjectsCreateOutput, ApiError> {
    trpc.mutation("projects.create", input)
}

/// `projects.update` — mutation.
pub fn projects_update(
    trpc: &TrpcClient,
    input: &ProjectsUpdateInput,
) -> Result<ProjectsUpdateOutput, ApiError> {
    trpc.mutation("projects.update", input)
}

/// `projects.setRepository({projectId, repositoryId})` → `{project, txId}`
/// (v4 §3.2 / §5.3): retarget a project at another registry repository. The
/// v4 replacement for the deleted `repositories.link/unlink/setPrimary` link
/// procs — a project has exactly one repository now. Owner/manage-repos gated
/// server-side; the repo must belong to the project's workspace.
pub fn projects_set_repository(
    trpc: &TrpcClient,
    project_id: &str,
    repository_id: &str,
) -> Result<ProjectsCreateOutput, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        project_id: &'a str,
        repository_id: &'a str,
    }
    trpc.mutation(
        "projects.setRepository",
        &Input { project_id, repository_id },
    )
}

/// `projects.delete` — mutation (owner-only; plan-cap/permission failures
/// surface as `ApiError::Http` with the server message — the §4.9 "Upgrade on
/// the web" notification is the UI's concern).
pub fn projects_delete(trpc: &TrpcClient, project_id: &str) -> Result<OkTxOutput, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        project_id: &'a str,
    }
    trpc.mutation("projects.delete", &Input { project_id })
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
    fn create_posts_camel_case_and_decodes_project() {
        let (base, captured) = one_shot_server(
            200,
            r##"{"result":{"data":{"project":{"id":"p-1","workspaceId":"w-1","name":"Gate","slug":"gate","prefix":"GATE","color":"#6366f1"},"txId":9}}}"##,
        );
        let out = projects_create(
            &client(&base),
            &ProjectsCreateInput {
                workspace_id: "w-1".to_string(),
                name: "Gate".to_string(),
                prefix: "gate".to_string(),
                project_type: "dev".to_string(),
                color: None,
                repository: Some(ProjectRepositoryInput::Registry {
                    repository_id: "repo-1".to_string(),
                }),
            },
        )
        .unwrap();
        assert_eq!(out.project.slug.as_deref(), Some("gate"));
        assert_eq!(out.tx_id, Some(9));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/projects.create HTTP/1.1"));
        // No slug in the input (server-derived, §4.2) and no null color noise;
        // the dev project sends its `type` and the required repository as the
        // `{repositoryId}` registry arm.
        assert!(request.ends_with(
            r#"{"workspaceId":"w-1","name":"Gate","prefix":"gate","type":"dev","repository":{"repositoryId":"repo-1"}}"#
        ));
    }

    #[test]
    fn inline_repository_serializes_as_camel_case_union_arm() {
        let inline = ProjectRepositoryInput::Inline {
            full_name: "acme/app".to_string(),
            default_branch: Some("main".to_string()),
            private: Some(true),
        };
        assert_eq!(
            serde_json::to_string(&inline).unwrap(),
            r#"{"fullName":"acme/app","defaultBranch":"main","private":true}"#
        );
        // Optional fields drop out when unknown (server fills defaults).
        let sparse = ProjectRepositoryInput::Inline {
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
        let mut input = ProjectsUpdateInput::new("p-1");
        input.name = Some("Renamed".to_string());
        let json = serde_json::to_string(&input).unwrap();
        assert_eq!(json, r#"{"id":"p-1","name":"Renamed"}"#);

        // Explicit un-archive is a null, not an omission.
        let mut input = ProjectsUpdateInput::new("p-1");
        input.archived_at = Patch::Null;
        let json = serde_json::to_string(&input).unwrap();
        assert_eq!(json, r#"{"id":"p-1","archivedAt":null}"#);
    }

    #[test]
    fn delete_decodes_ok_tx() {
        let (base, captured) =
            one_shot_server(200, r#"{"result":{"data":{"ok":true,"txId":3}}}"#);
        let out = projects_delete(&client(&base), "p-1").unwrap();
        assert!(out.ok);
        assert_eq!(out.tx_id, Some(3));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(r#"{"projectId":"p-1"}"#));
    }
}
