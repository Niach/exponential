//! Typed `workspaces.*` / `workspaceMembers.*` / `workspaceInvites.*` tRPC
//! helpers (masterplan-v3 §4.2 create-workspace dialog, Settings → General /
//! Members / Danger Zone, and the accept-invite surface). Verified against
//! `apps/web/src/lib/trpc/workspaces.ts`, `workspace-members.ts`,
//! `workspace-invites.ts`:
//!
//! - `workspaces.create({name, iconUrl?})` → `{workspace, txId}`
//! - `workspaces.update({id, name?, iconUrl?})` → `{workspace, txId}` (v6:
//!   isPublic/publicWritePolicy are rejected server-side — never sent)
//! - `workspaces.delete({workspaceId})` → `{ok, txId}`
//! - `workspaces.ensureDefault()` → `{workspace, txId}` (txId 0 when reused)
//! - `workspaces.getBySlug({slug})` → workspace metadata + caller membership
//!   (public query — the v6 join-gate lookup; NOT_FOUND for private
//!   workspaces the caller can't access)
//! - `workspaceMembers.join({workspaceId})` → `{member, txId}` (self-service,
//!   PUBLIC workspaces only — private ones answer FORBIDDEN; idempotent)
//! - `workspaceMembers.updateRole({memberId, role})` → `{member}`
//! - `workspaceMembers.remove({memberId})` → `{ok}` (also "Leave workspace")
//! - `workspaceInvites.create({workspaceId, role})` → `{invite, token}`
//! - `workspaceInvites.accept({token})` → `{workspace, alreadyMember, txId?}`
//! - `workspaceInvites.list({workspaceId})` → `{invites}` (query)
//! - `workspaceInvites.revoke({id})` → `{ok}`
//! - `workspaceInvites.getByToken({token})` → `{invite}` (public query — the
//!   §4.2 invite preview card)

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::patch::Patch;
use crate::projects::OkTxOutput;
use crate::trpc::TrpcClient;

/// `workspace_member_role` — the only two roles (contract-locked).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkspaceRole {
    Owner,
    Member,
}

/// Slim camelCase mirror of the workspace row a mutation returns.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceOut {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub is_public: Option<bool>,
    #[serde(default)]
    pub public_write_policy: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceTxOutput {
    pub workspace: WorkspaceOut,
    #[serde(default)]
    pub tx_id: Option<i64>,
}

/// `workspaces.create` — mutation. Plan-cap failures surface as
/// `ApiError::Http` (FORBIDDEN) → the §4.9 "Upgrade on the web" notification.
pub fn workspaces_create(
    trpc: &TrpcClient,
    name: &str,
    icon_url: Option<&str>,
) -> Result<WorkspaceTxOutput, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        name: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        icon_url: Option<&'a str>,
    }
    trpc.mutation("workspaces.create", &Input { name, icon_url })
}

/// `workspaces.update` input (Settings → General; owner-only).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacesUpdateInput {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Patch::is_omit")]
    pub icon_url: Patch<String>,
}

impl WorkspacesUpdateInput {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: None,
            icon_url: Patch::Omit,
        }
    }
}

/// `workspaces.update` — mutation.
pub fn workspaces_update(
    trpc: &TrpcClient,
    input: &WorkspacesUpdateInput,
) -> Result<WorkspaceTxOutput, ApiError> {
    trpc.mutation("workspaces.update", input)
}

/// `workspaces.delete` — mutation (Danger Zone; owner + non-public gated
/// server-side).
pub fn workspaces_delete(trpc: &TrpcClient, workspace_id: &str) -> Result<OkTxOutput, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        workspace_id: &'a str,
    }
    trpc.mutation("workspaces.delete", &Input { workspace_id })
}

/// `workspaces.ensureDefault` — input-less mutation (first-run/onboarding).
pub fn workspaces_ensure_default(trpc: &TrpcClient) -> Result<WorkspaceTxOutput, ApiError> {
    trpc.mutation_no_input("workspaces.ensureDefault")
}

/// `workspaces.getBySlug` output — the minimal metadata the web route guard /
/// join gate reads (`components/workspace/join-gate.tsx`). `membership` is
/// the caller's role (`owner`/`member`), `None` for non-members.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceBySlugOut {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub is_public: Option<bool>,
    #[serde(default)]
    pub public_write_policy: Option<String>,
    #[serde(default)]
    pub membership: Option<String>,
}

/// `workspaces.getBySlug` — public query. NOT_FOUND (`ApiError::Http` 404)
/// covers both unknown slugs and private workspaces the caller can't access
/// (existence is never leaked).
pub fn workspaces_get_by_slug(
    trpc: &TrpcClient,
    slug: &str,
) -> Result<WorkspaceBySlugOut, ApiError> {
    #[derive(Serialize)]
    struct Input<'a> {
        slug: &'a str,
    }
    trpc.query_with_input("workspaces.getBySlug", &Input { slug })
}

// ---------------------------------------------------------------------------
// workspaceMembers.* (Settings → Members)
// ---------------------------------------------------------------------------

/// Slim camelCase mirror of a workspace-member row.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceMemberOut {
    pub id: String,
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemberJoinOutput {
    #[serde(default)]
    pub member: Option<WorkspaceMemberOut>,
    #[serde(default)]
    pub tx_id: Option<i64>,
}

/// `workspaceMembers.join` — self-service join, restricted to PUBLIC
/// workspaces server-side (private workspaces answer FORBIDDEN — invites are
/// the only path in). Idempotent: re-joining returns the existing member row,
/// so callers can retry safely.
pub fn workspace_members_join(
    trpc: &TrpcClient,
    workspace_id: &str,
) -> Result<MemberJoinOutput, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        workspace_id: &'a str,
    }
    trpc.mutation("workspaceMembers.join", &Input { workspace_id })
}

/// `workspaceMembers.updateRole` — mutation (Make owner / Make member).
pub fn workspace_members_update_role(
    trpc: &TrpcClient,
    member_id: &str,
    role: WorkspaceRole,
) -> Result<WorkspaceMemberOut, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        member_id: &'a str,
        role: WorkspaceRole,
    }
    #[derive(Deserialize)]
    struct Output {
        member: WorkspaceMemberOut,
    }
    let out: Output = trpc.mutation("workspaceMembers.updateRole", &Input { member_id, role })?;
    Ok(out.member)
}

/// `workspaceMembers.remove` — mutation (Remove member / Leave workspace —
/// self-removal is the leave path).
pub fn workspace_members_remove(trpc: &TrpcClient, member_id: &str) -> Result<(), ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        member_id: &'a str,
    }
    #[derive(Deserialize)]
    struct Output {
        #[allow(dead_code)]
        #[serde(default)]
        ok: bool,
    }
    let _: Output = trpc.mutation("workspaceMembers.remove", &Input { member_id })?;
    Ok(())
}

// ---------------------------------------------------------------------------
// workspaceInvites.* (Settings → Members invites + the accept surface)
// ---------------------------------------------------------------------------

/// camelCase mirror of an invite row (the synced shape is snake_case; this is
/// the tRPC return form).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceInviteOut {
    pub id: String,
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub token: Option<String>,
    #[serde(default)]
    pub accepted_at: Option<String>,
    #[serde(default)]
    pub expires_at: Option<String>,
    /// Only on `getByToken` (joined for the preview card).
    #[serde(default)]
    pub workspace_name: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InviteCreateOutput {
    pub invite: WorkspaceInviteOut,
    /// The raw invite token — pair it with the instance URL for the
    /// copy-to-clipboard link (`{base}/invite/{token}`).
    pub token: String,
}

/// `workspaceInvites.create` — mutation (owner-only; plan-cap gated).
pub fn workspace_invites_create(
    trpc: &TrpcClient,
    workspace_id: &str,
    role: WorkspaceRole,
) -> Result<InviteCreateOutput, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        workspace_id: &'a str,
        role: WorkspaceRole,
    }
    trpc.mutation("workspaceInvites.create", &Input { workspace_id, role })
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InviteAcceptOutput {
    #[serde(default)]
    pub workspace: Option<WorkspaceOut>,
    #[serde(default)]
    pub already_member: Option<bool>,
    #[serde(default)]
    pub tx_id: Option<i64>,
}

/// `workspaceInvites.accept` — mutation. Expired/used tokens surface as
/// `ApiError::Http` with the server's message (mirror the web card states).
pub fn workspace_invites_accept(
    trpc: &TrpcClient,
    token: &str,
) -> Result<InviteAcceptOutput, ApiError> {
    #[derive(Serialize)]
    struct Input<'a> {
        token: &'a str,
    }
    trpc.mutation("workspaceInvites.accept", &Input { token })
}

/// `workspaceInvites.list` — query (pending invites for the Members pane).
pub fn workspace_invites_list(
    trpc: &TrpcClient,
    workspace_id: &str,
) -> Result<Vec<WorkspaceInviteOut>, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        workspace_id: &'a str,
    }
    #[derive(Deserialize)]
    struct Output {
        invites: Vec<WorkspaceInviteOut>,
    }
    let out: Output = trpc.query_with_input("workspaceInvites.list", &Input { workspace_id })?;
    Ok(out.invites)
}

/// `workspaceInvites.revoke` — mutation.
pub fn workspace_invites_revoke(trpc: &TrpcClient, id: &str) -> Result<(), ApiError> {
    #[derive(Serialize)]
    struct Input<'a> {
        id: &'a str,
    }
    #[derive(Deserialize)]
    struct Output {
        #[allow(dead_code)]
        #[serde(default)]
        ok: bool,
    }
    let _: Output = trpc.mutation("workspaceInvites.revoke", &Input { id })?;
    Ok(())
}

/// `workspaceInvites.getByToken` — public query (the §4.2 invite preview:
/// workspace name + role + expiry/used state).
pub fn workspace_invites_get_by_token(
    trpc: &TrpcClient,
    token: &str,
) -> Result<WorkspaceInviteOut, ApiError> {
    #[derive(Serialize)]
    struct Input<'a> {
        token: &'a str,
    }
    #[derive(Deserialize)]
    struct Output {
        invite: WorkspaceInviteOut,
    }
    let out: Output = trpc.query_with_input("workspaceInvites.getByToken", &Input { token })?;
    Ok(out.invite)
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
    fn create_posts_name_only_and_decodes_workspace() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"workspace":{"id":"w-1","name":"Acme","slug":"acme"},"txId":11}}}"#,
        );
        let out = workspaces_create(&client(&base), "Acme", None).unwrap();
        assert_eq!(out.workspace.slug.as_deref(), Some("acme"));
        assert_eq!(out.tx_id, Some(11));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/workspaces.create HTTP/1.1"));
        assert!(request.ends_with(r#"{"name":"Acme"}"#));
    }

    #[test]
    fn ensure_default_posts_empty_body() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"workspace":{"id":"w-1","name":"My Workspace"},"txId":0}}}"#,
        );
        let out = workspaces_ensure_default(&client(&base)).unwrap();
        assert_eq!(out.tx_id, Some(0));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/workspaces.ensureDefault HTTP/1.1"));
    }

    #[test]
    fn get_by_slug_is_a_get_query_and_decodes_membership() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"id":"w-pub","name":"Feedback","slug":"feedback","iconUrl":null,"isPublic":true,"publicWritePolicy":"authenticated","membership":null}}}"#,
        );
        let workspace = workspaces_get_by_slug(&client(&base), "feedback").unwrap();
        assert_eq!(workspace.id, "w-pub");
        assert_eq!(workspace.is_public, Some(true));
        assert_eq!(workspace.membership, None);
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("GET /api/trpc/workspaces.getBySlug?input="));

        // A member's role rides along for the already-joined fast path.
        let (base, _captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"id":"w-pub","name":"Feedback","slug":"feedback","isPublic":true,"membership":"member"}}}"#,
        );
        let workspace = workspaces_get_by_slug(&client(&base), "feedback").unwrap();
        assert_eq!(workspace.membership.as_deref(), Some("member"));
    }

    #[test]
    fn join_posts_workspace_id_and_decodes_member() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"member":{"id":"m-1","workspaceId":"w-pub","userId":"u-1","role":"member"},"txId":7}}}"#,
        );
        let out = workspace_members_join(&client(&base), "w-pub").unwrap();
        assert_eq!(out.tx_id, Some(7));
        assert_eq!(out.member.unwrap().role.as_deref(), Some("member"));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/workspaceMembers.join HTTP/1.1"));
        assert!(request.ends_with(r#"{"workspaceId":"w-pub"}"#));
    }

    #[test]
    fn update_role_serializes_lowercase_and_unwraps_member() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"member":{"id":"m-1","workspaceId":"w-1","userId":"u-1","role":"owner"}}}}"#,
        );
        let member =
            workspace_members_update_role(&client(&base), "m-1", WorkspaceRole::Owner).unwrap();
        assert_eq!(member.role.as_deref(), Some("owner"));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(r#"{"memberId":"m-1","role":"owner"}"#));
    }

    #[test]
    fn invites_round_trip_the_web_shapes() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"invite":{"id":"inv-1","workspaceId":"w-1","role":"member","expiresAt":"2026-07-10T00:00:00Z"},"token":"rawtoken123"}}}"#,
        );
        let out =
            workspace_invites_create(&client(&base), "w-1", WorkspaceRole::Member).unwrap();
        assert_eq!(out.token, "rawtoken123");
        assert_eq!(out.invite.id, "inv-1");
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(r#"{"workspaceId":"w-1","role":"member"}"#));

        // list is a GET query.
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"invites":[{"id":"inv-1","workspaceId":"w-1","role":"member"}]}}}"#,
        );
        let invites = workspace_invites_list(&client(&base), "w-1").unwrap();
        assert_eq!(invites.len(), 1);
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("GET /api/trpc/workspaceInvites.list?input="));

        // getByToken carries the joined workspaceName for the preview card.
        let (base, _captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"invite":{"id":"inv-1","workspaceId":"w-1","role":"member","acceptedAt":null,"expiresAt":"2026-07-10T00:00:00Z","workspaceName":"Acme"}}}}"#,
        );
        let invite = workspace_invites_get_by_token(&client(&base), "t").unwrap();
        assert_eq!(invite.workspace_name.as_deref(), Some("Acme"));
    }

    #[test]
    fn accept_decodes_already_member_variant_without_tx() {
        let (base, _captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"workspace":{"id":"w-1","name":"Acme"},"alreadyMember":true}}}"#,
        );
        let out = workspace_invites_accept(&client(&base), "t").unwrap();
        assert_eq!(out.already_member, Some(true));
        assert_eq!(out.tx_id, None);
        assert_eq!(out.workspace.unwrap().id, "w-1");
    }
}
