//! Typed `teams.*` / `teamMembers.*` / `teamInvites.*` tRPC
//! helpers (masterplan-v3 §4.2 create-team dialog, Settings → General /
//! Members / Danger Zone, and the accept-invite surface). Verified against
//! `apps/web/src/lib/trpc/teams.ts`, `team-members.ts`,
//! `team-invites.ts`:
//!
//! - `teams.create({name, iconUrl?})` → `{team, txId}`
//! - `teams.update({id, name?, iconUrl?})` → `{team, txId}`
//! - `teams.delete({teamId})` → `{ok, txId}`
//! - `teams.ensureDefault()` → `{team, txId}` (txId 0 when reused)
//! - `teamMembers.updateRole({memberId, role})` → `{member}`
//! - `teamMembers.remove({memberId})` → `{ok}` (also "Leave team")
//! - `teamInvites.create({teamId, role})` → `{invite, token}`
//! - `teamInvites.accept({token})` → `{team, alreadyMember, txId?}`
//! - `teamInvites.list({teamId})` → `{invites}` (query)
//! - `teamInvites.revoke({id})` → `{ok}`
//! - `teamInvites.getByToken({token})` → `{invite}` (public query — the
//!   §4.2 invite preview card)

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::patch::Patch;
use crate::boards::OkTxOutput;
use crate::trpc::TrpcClient;

/// `team_member_role` — the only two roles (contract-locked).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TeamRole {
    Owner,
    Member,
}

/// Slim camelCase mirror of the team row a mutation returns.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamOut {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub slug: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamTxOutput {
    pub team: TeamOut,
    #[serde(default)]
    pub tx_id: Option<i64>,
}

/// `teams.create` — mutation. Plan-cap failures surface as
/// `ApiError::Http` (FORBIDDEN) → the §4.9 "Upgrade on the web" notification.
pub fn teams_create(
    trpc: &TrpcClient,
    name: &str,
    icon_url: Option<&str>,
) -> Result<TeamTxOutput, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        name: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        icon_url: Option<&'a str>,
    }
    trpc.mutation("teams.create", &Input { name, icon_url })
}

/// `teams.update` input (Settings → General; owner-only).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamsUpdateInput {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Patch::is_omit")]
    pub icon_url: Patch<String>,
}

impl TeamsUpdateInput {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: None,
            icon_url: Patch::Omit,
        }
    }
}

/// `teams.update` — mutation.
pub fn teams_update(
    trpc: &TrpcClient,
    input: &TeamsUpdateInput,
) -> Result<TeamTxOutput, ApiError> {
    trpc.mutation("teams.update", input)
}

/// `teams.delete` — mutation (Danger Zone; owner-gated server-side).
pub fn teams_delete(trpc: &TrpcClient, team_id: &str) -> Result<OkTxOutput, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        team_id: &'a str,
    }
    trpc.mutation("teams.delete", &Input { team_id })
}

/// `teams.ensureDefault` — input-less mutation (first-run/onboarding).
pub fn teams_ensure_default(trpc: &TrpcClient) -> Result<TeamTxOutput, ApiError> {
    trpc.mutation_no_input("teams.ensureDefault")
}

// ---------------------------------------------------------------------------
// teamMembers.* (Settings → Members)
// ---------------------------------------------------------------------------

/// Slim camelCase mirror of a team-member row.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamMemberOut {
    pub id: String,
    #[serde(default)]
    pub team_id: Option<String>,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
}

/// `teamMembers.updateRole` — mutation (Make owner / Make member).
pub fn team_members_update_role(
    trpc: &TrpcClient,
    member_id: &str,
    role: TeamRole,
) -> Result<TeamMemberOut, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        member_id: &'a str,
        role: TeamRole,
    }
    #[derive(Deserialize)]
    struct Output {
        member: TeamMemberOut,
    }
    let out: Output = trpc.mutation("teamMembers.updateRole", &Input { member_id, role })?;
    Ok(out.member)
}

/// `teamMembers.remove` — mutation (Remove member / Leave team —
/// self-removal is the leave path).
pub fn team_members_remove(trpc: &TrpcClient, member_id: &str) -> Result<(), ApiError> {
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
    let _: Output = trpc.mutation("teamMembers.remove", &Input { member_id })?;
    Ok(())
}

// ---------------------------------------------------------------------------
// teamInvites.* (Settings → Members invites + the accept surface)
// ---------------------------------------------------------------------------

/// camelCase mirror of an invite row (the synced shape is snake_case; this is
/// the tRPC return form).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamInviteOut {
    pub id: String,
    #[serde(default)]
    pub team_id: Option<String>,
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
    pub team_name: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InviteCreateOutput {
    pub invite: TeamInviteOut,
    /// The raw invite token — pair it with the instance URL for the
    /// copy-to-clipboard link (`{base}/invite/{token}`).
    pub token: String,
}

/// `teamInvites.create` — mutation (owner-only; plan-cap gated).
pub fn team_invites_create(
    trpc: &TrpcClient,
    team_id: &str,
    role: TeamRole,
) -> Result<InviteCreateOutput, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        team_id: &'a str,
        role: TeamRole,
    }
    trpc.mutation("teamInvites.create", &Input { team_id, role })
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InviteAcceptOutput {
    #[serde(default)]
    pub team: Option<TeamOut>,
    #[serde(default)]
    pub already_member: Option<bool>,
    #[serde(default)]
    pub tx_id: Option<i64>,
}

/// `teamInvites.accept` — mutation. Expired/used tokens surface as
/// `ApiError::Http` with the server's message (mirror the web card states).
pub fn team_invites_accept(
    trpc: &TrpcClient,
    token: &str,
) -> Result<InviteAcceptOutput, ApiError> {
    #[derive(Serialize)]
    struct Input<'a> {
        token: &'a str,
    }
    trpc.mutation("teamInvites.accept", &Input { token })
}

/// `teamInvites.list` — query (pending invites for the Members pane).
pub fn team_invites_list(
    trpc: &TrpcClient,
    team_id: &str,
) -> Result<Vec<TeamInviteOut>, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        team_id: &'a str,
    }
    #[derive(Deserialize)]
    struct Output {
        invites: Vec<TeamInviteOut>,
    }
    let out: Output = trpc.query_with_input("teamInvites.list", &Input { team_id })?;
    Ok(out.invites)
}

/// `teamInvites.revoke` — mutation.
pub fn team_invites_revoke(trpc: &TrpcClient, id: &str) -> Result<(), ApiError> {
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
    let _: Output = trpc.mutation("teamInvites.revoke", &Input { id })?;
    Ok(())
}

/// `teamInvites.getByToken` — public query (the §4.2 invite preview:
/// team name + role + expiry/used state).
pub fn team_invites_get_by_token(
    trpc: &TrpcClient,
    token: &str,
) -> Result<TeamInviteOut, ApiError> {
    #[derive(Serialize)]
    struct Input<'a> {
        token: &'a str,
    }
    #[derive(Deserialize)]
    struct Output {
        invite: TeamInviteOut,
    }
    let out: Output = trpc.query_with_input("teamInvites.getByToken", &Input { token })?;
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
    fn create_posts_name_only_and_decodes_team() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"team":{"id":"w-1","name":"Acme","slug":"acme"},"txId":11}}}"#,
        );
        let out = teams_create(&client(&base), "Acme", None).unwrap();
        assert_eq!(out.team.slug.as_deref(), Some("acme"));
        assert_eq!(out.tx_id, Some(11));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/teams.create HTTP/1.1"));
        assert!(request.ends_with(r#"{"name":"Acme"}"#));
    }

    #[test]
    fn ensure_default_posts_empty_body() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"team":{"id":"w-1","name":"My Team"},"txId":0}}}"#,
        );
        let out = teams_ensure_default(&client(&base)).unwrap();
        assert_eq!(out.tx_id, Some(0));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("POST /api/trpc/teams.ensureDefault HTTP/1.1"));
    }

    #[test]
    fn update_role_serializes_lowercase_and_unwraps_member() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"member":{"id":"m-1","teamId":"w-1","userId":"u-1","role":"owner"}}}}"#,
        );
        let member =
            team_members_update_role(&client(&base), "m-1", TeamRole::Owner).unwrap();
        assert_eq!(member.role.as_deref(), Some("owner"));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(r#"{"memberId":"m-1","role":"owner"}"#));
    }

    #[test]
    fn invites_round_trip_the_web_shapes() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"invite":{"id":"inv-1","teamId":"w-1","role":"member","expiresAt":"2026-07-10T00:00:00Z"},"token":"rawtoken123"}}}"#,
        );
        let out =
            team_invites_create(&client(&base), "w-1", TeamRole::Member).unwrap();
        assert_eq!(out.token, "rawtoken123");
        assert_eq!(out.invite.id, "inv-1");
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(r#"{"teamId":"w-1","role":"member"}"#));

        // list is a GET query.
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"invites":[{"id":"inv-1","teamId":"w-1","role":"member"}]}}}"#,
        );
        let invites = team_invites_list(&client(&base), "w-1").unwrap();
        assert_eq!(invites.len(), 1);
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.starts_with("GET /api/trpc/teamInvites.list?input="));

        // getByToken carries the joined teamName for the preview card.
        let (base, _captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"invite":{"id":"inv-1","teamId":"w-1","role":"member","acceptedAt":null,"expiresAt":"2026-07-10T00:00:00Z","teamName":"Acme"}}}}"#,
        );
        let invite = team_invites_get_by_token(&client(&base), "t").unwrap();
        assert_eq!(invite.team_name.as_deref(), Some("Acme"));
    }

    #[test]
    fn accept_decodes_already_member_variant_without_tx() {
        let (base, _captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"team":{"id":"w-1","name":"Acme"},"alreadyMember":true}}}"#,
        );
        let out = team_invites_accept(&client(&base), "t").unwrap();
        assert_eq!(out.already_member, Some(true));
        assert_eq!(out.tx_id, None);
        assert_eq!(out.team.unwrap().id, "w-1");
    }
}
