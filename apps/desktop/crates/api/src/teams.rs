//! Typed `teams.*` / `teamMembers.*` / `teamInvites.*` tRPC
//! helpers (masterplan-v3 §4.2 create-team dialog, Settings → General /
//! Members / Danger Zone, and the accept-invite surface). Verified against
//! `apps/web/src/lib/trpc/teams.ts`, `team-members.ts`,
//! `team-invites.ts`:
//!
//! - `teams.create({name, iconUrl?})` → `{team, txId}`
//! - `teams.update({teamId, name?, iconUrl?, helpdeskEnabled?, estimationType?, agentPrompt?})` → `{team, txId}` (EXP-707, EXP-1025)
//! - `teams.getAgentPrompt({teamId})` → `{agentPrompt, agentPromptUpdatedAt, maxBytes}` (query, EXP-1025)
//! - `teams.delete({teamId})` → `{ok, txId}`
//! - `teams.inviteCapacity({teamId})` → `{remaining}` (query, EXP-725)
//! - `teamMembers.updateRole({memberId, role})` → `{member}`
//! - `teamMembers.remove({memberId})` → `{ok}` (also "Leave team")
//! - `teamInvites.create({teamId, role, email?, name?, placeholderUserId?})` →
//!   `{invite, token, emailDelivered, memberUserId}` (EXP-630)
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

/// `teams.update` input (Settings → General and, since EXP-771, → Helpdesk;
/// owner-only). Every field past `team_id` is a PATCH — an omitted one is not
/// sent and the server leaves that column alone.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamsUpdateInput {
    /// EXP-707: the wire name is `teamId` (renamed from `id`).
    pub team_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Patch::is_omit")]
    pub icon_url: Patch<String>,
    /// EXP-771: the team's shared support inbox. Turning it ON is gated
    /// server-side twice — a plan limit (PRECONDITION_FAILED with the "Your
    /// plan allows" prefix) and REV2-10(c)'s mail-transport check (a plain
    /// PRECONDITION_FAILED naming `AWS_SES_REGION`/`SMTP_HOST`) — so the
    /// caller must tell the two apart, not swallow both.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub helpdesk_enabled: Option<bool>,
    /// EXP-630: the estimate scale (contract `issueEstimation`; `none` = off).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimation_type: Option<String>,
    /// EXP-1025: the team prompt, raw markdown; `Some("")` clears it. The
    /// server refuses more than `domain::contract::TEAM_AGENT_PROMPT_MAX_BYTES`
    /// UTF-8 bytes (BAD_REQUEST), so the editor counts bytes, not chars.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_prompt: Option<String>,
}

impl TeamsUpdateInput {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            team_id: id.into(),
            name: None,
            icon_url: Patch::Omit,
            helpdesk_enabled: None,
            estimation_type: None,
            agent_prompt: None,
        }
    }
}

/// `teams.getAgentPrompt` output (EXP-1025).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TeamAgentPrompt {
    /// The raw markdown; empty = the team has no prompt.
    #[serde(default)]
    pub agent_prompt: String,
    /// ISO timestamp of the last write, `None` until the first one.
    #[serde(default)]
    pub agent_prompt_updated_at: Option<String>,
    /// The server's cap in UTF-8 bytes (the contract value, echoed so an
    /// older client never counts against a stale number).
    #[serde(default)]
    pub max_bytes: Option<usize>,
}

/// `teams.getAgentPrompt` — query, any member. The team prompt is NOT on the
/// teams shape (server-only like `actions.body`), so this is the one read
/// path: the Settings → General editor and the launcher at prepare/resume
/// time both take it from here.
pub fn teams_get_agent_prompt(
    trpc: &TrpcClient,
    team_id: &str,
) -> Result<TeamAgentPrompt, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        team_id: &'a str,
    }
    trpc.query_with_input("teams.getAgentPrompt", &Input { team_id })
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

/// `teams.inviteCapacity` — query (EXP-725, the onboarding invite step). How
/// many MORE people the team may invite on its plan, PENDING invites already
/// counted; `None` = unlimited (self-hosted, paid, comped). Any member may
/// ask — the wizard uses it to REMOVE the invite control on a full free tier
/// rather than let the mint fail.
pub fn teams_invite_capacity(
    trpc: &TrpcClient,
    team_id: &str,
) -> Result<Option<u32>, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        team_id: &'a str,
    }
    #[derive(Deserialize)]
    struct Output {
        /// An ABSENT key reads the same as an explicit `null`: unlimited.
        #[serde(default)]
        remaining: Option<u32>,
    }
    let out: Output = trpc.query_with_input("teams.inviteCapacity", &Input { team_id })?;
    Ok(out.remaining)
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
    /// Optional invitee email (EXP-188 invite-by-email; null on link-only
    /// invites).
    #[serde(default)]
    pub email: Option<String>,
    /// EXP-630: the placeholder member this invite is bound to (null on link
    /// invites and on invites to an existing account).
    #[serde(default)]
    pub placeholder_user_id: Option<String>,
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
    /// EXP-188: `null` = no email requested, `true` = invite link mailed,
    /// `false` = mail requested but delivery failed (show the link instead).
    #[serde(default)]
    pub email_delivered: Option<bool>,
    /// EXP-630: the PLACEHOLDER member the invite put on the roster (the
    /// re-invited one on a resend). `null` on a link invite and when the
    /// address already belongs to an account.
    #[serde(default)]
    pub member_user_id: Option<String>,
}

/// EXP-630: the placeholder-member extras of an invite by email. All-`None`
/// (`Default`) keeps the wire shape byte-identical to the EXP-188 invite.
#[derive(Clone, Copy, Debug, Default)]
pub struct InviteExtras<'a> {
    /// The placeholder's display name; empty/absent = the mailbox local part.
    pub name: Option<&'a str>,
    /// Re-invite THIS unclaimed placeholder member ("Resend invite"),
    /// optionally at a corrected address — the roster row keeps its
    /// attributions.
    pub placeholder_user_id: Option<&'a str>,
}

/// `teamInvites.create` — mutation (owner-only; plan-cap gated). `email`
/// is optional (EXP-188): when set, the server mails the invite link, puts the
/// invitee on the roster as a placeholder member (EXP-630, see
/// [`InviteExtras`]) and reports the outcome via `email_delivered`.
pub fn team_invites_create(
    trpc: &TrpcClient,
    team_id: &str,
    role: TeamRole,
    email: Option<&str>,
    extras: InviteExtras<'_>,
) -> Result<InviteCreateOutput, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        team_id: &'a str,
        role: TeamRole,
        #[serde(skip_serializing_if = "Option::is_none")]
        email: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        placeholder_user_id: Option<&'a str>,
    }
    trpc.mutation(
        "teamInvites.create",
        &Input {
            team_id,
            role,
            email,
            name: extras.name.filter(|name| !name.is_empty()),
            placeholder_user_id: extras.placeholder_user_id,
        },
    )
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
        team_invites_create(
            &client(&base),
            "w-1",
            TeamRole::Member,
            None,
            InviteExtras::default(),
        )
        .unwrap();
        assert_eq!(out.token, "rawtoken123");
        assert_eq!(out.invite.id, "inv-1");
        assert_eq!(out.email_delivered, None);
        // No email ⇒ the wire shape stays byte-identical to pre-EXP-188.
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
    fn invite_create_with_email_serializes_and_decodes_delivery() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"invite":{"id":"inv-2","teamId":"w-1","role":"member","email":"jo@example.com"},"token":"rawtoken456","emailDelivered":true}}}"#,
        );
        let out = team_invites_create(
            &client(&base),
            "w-1",
            TeamRole::Member,
            Some("jo@example.com"),
            InviteExtras::default(),
        )
        .unwrap();
        assert_eq!(out.email_delivered, Some(true));
        assert_eq!(out.invite.email.as_deref(), Some("jo@example.com"));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request
            .ends_with(r#"{"teamId":"w-1","role":"member","email":"jo@example.com"}"#));
    }

    /// EXP-630: a "Resend invite" carries the placeholder's id and the (possibly
    /// corrected) name beside the address, and the response names the roster row
    /// the invite belongs to.
    #[test]
    fn invite_create_sends_the_placeholder_extras_and_decodes_the_member() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"invite":{"id":"inv-3","teamId":"w-1","role":"member","email":"jo@example.com","placeholderUserId":"u-9"},"token":"rawtoken789","emailDelivered":true,"memberUserId":"u-9"}}}"#,
        );
        let out = team_invites_create(
            &client(&base),
            "w-1",
            TeamRole::Member,
            Some("jo@example.com"),
            InviteExtras {
                name: Some("Jo Miller"),
                placeholder_user_id: Some("u-9"),
            },
        )
        .unwrap();
        assert_eq!(out.member_user_id.as_deref(), Some("u-9"));
        assert_eq!(out.invite.placeholder_user_id.as_deref(), Some("u-9"));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(
            r#"{"teamId":"w-1","role":"member","email":"jo@example.com","name":"Jo Miller","placeholderUserId":"u-9"}"#
        ));

        // An EMPTY name is not a name — it must not reach the wire (the
        // server would take it as "call them ''" instead of defaulting to the
        // mailbox local part).
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"invite":{"id":"inv-4","teamId":"w-1"},"token":"t"}}}"#,
        );
        team_invites_create(
            &client(&base),
            "w-1",
            TeamRole::Member,
            Some("jo@example.com"),
            InviteExtras {
                name: Some(""),
                placeholder_user_id: None,
            },
        )
        .unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.ends_with(r#"{"teamId":"w-1","role":"member","email":"jo@example.com"}"#));
    }

    /// EXP-725: the wizard's invite step asks BEFORE offering the control —
    /// `null` (or a missing key) is unlimited, a number is what is left after
    /// pending invites, and the call is a plain GET query.
    #[test]
    fn invite_capacity_decodes_unlimited_and_a_remainder() {
        let (base, captured) =
            one_shot_server(200, r#"{"result":{"data":{"remaining":null}}}"#);
        assert_eq!(teams_invite_capacity(&client(&base), "w-1").unwrap(), None);
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(
            request.starts_with(
                "GET /api/trpc/teams.inviteCapacity?input=%7B%22teamId%22%3A%22w-1%22%7D HTTP/1.1"
            ),
            "unexpected request line: {}",
            request.lines().next().unwrap_or_default()
        );

        let (base, _captured) =
            one_shot_server(200, r#"{"result":{"data":{"remaining":2}}}"#);
        assert_eq!(
            teams_invite_capacity(&client(&base), "w-1").unwrap(),
            Some(2)
        );

        // A server that omits the key entirely still means unlimited.
        let (base, _captured) = one_shot_server(200, r#"{"result":{"data":{}}}"#);
        assert_eq!(teams_invite_capacity(&client(&base), "w-1").unwrap(), None);
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
