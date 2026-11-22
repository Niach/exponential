//! Who sees and calls which app tool (`packages/emcp/fixtures/access.json`):
//! the `mcp_grants` model, the member × app policy (no row = `write`) and
//! the access rule.

use serde::Deserialize;

use super::exposes::ToolAccess;

/// `none` < `read` < `write`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    None,
    Read,
    Write,
}

/// The `mcp_grants` columns this contract reads. `kind` and `access` stay
/// strings: an unknown value is a malformed row, not a parse failure.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Grant {
    pub kind: String,
    pub all_teams: bool,
    pub team_ids: Vec<String>,
    pub board_ids: Vec<String>,
    pub all_apps: bool,
    pub app_ids: Vec<String>,
    pub access: String,
}

const GRANT_KINDS: &[&str] = &["builtin", "apps"];
const GRANT_ACCESS: &[&str] = &["read", "full"];

/// A grant's access as a level (`full` = `write`).
pub fn grant_level(access: &str) -> Level {
    if access == "full" {
        Level::Write
    } else {
        Level::Read
    }
}

/// Every reason a grant row is malformed (empty = valid).
pub fn grant_errors(grant: &Grant) -> Vec<&'static str> {
    let mut errors = Vec::new();
    if !GRANT_KINDS.contains(&grant.kind.as_str()) {
        errors.push("kind");
    }
    if !GRANT_ACCESS.contains(&grant.access.as_str()) {
        errors.push("access");
    }
    if grant.kind == "apps" {
        if grant.all_teams || grant.team_ids.len() != 1 {
            errors.push("team");
        }
        if !grant.board_ids.is_empty() {
            errors.push("boards");
        }
        if grant.all_apps == !grant.app_ids.is_empty() {
            errors.push("apps");
        }
    } else if grant.kind == "builtin" {
        if grant.all_apps || !grant.app_ids.is_empty() {
            errors.push("apps");
        }
        if grant.access != "full" {
            errors.push("access");
        }
    }
    errors
}

/// The member's policy for an app; no row = `write`.
pub fn member_policy(row: Option<Level>) -> Level {
    member_policy_with(row, None, false)
}

/// #FEED-242: the row, else `write` for an exempt member (the app's
/// creator or a team owner), else the app's default (`None` = `write`).
pub fn member_policy_with(row: Option<Level>, default: Option<Level>, exempt: bool) -> Level {
    match row {
        Some(level) => level,
        None if exempt => Level::Write,
        None => default.unwrap_or(Level::Write),
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessInput {
    pub grant: Grant,
    pub app_id: String,
    pub team_id: String,
    pub app_deleted: bool,
    /// The caller's role in the app's team, `None` = not a member.
    pub role: Option<String>,
    pub policy: Option<Level>,
}

/// grant ∩ policy; `none` outside the grant, for a trashed app, a
/// non-member or a malformed grant.
pub fn effective_access(input: &AccessInput) -> Level {
    let grant = &input.grant;
    if grant.kind != "apps" || !grant_errors(grant).is_empty() {
        return Level::None;
    }
    if input.role.is_none() || input.app_deleted {
        return Level::None;
    }
    if grant.team_ids.first() != Some(&input.team_id) {
        return Level::None;
    }
    if !grant.all_apps && !grant.app_ids.contains(&input.app_id) {
        return Level::None;
    }
    grant_level(&grant.access).min(member_policy(input.policy))
}

/// May a caller with `level` see (and call) a tool of `access`?
pub fn tool_visible(level: Level, access: ToolAccess) -> bool {
    let needed = match access {
        ToolAccess::Read => Level::Read,
        ToolAccess::Write => Level::Write,
    };
    level >= needed
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantScopeInput {
    /// The member's hidden personal team (the Assistant's own conversation).
    pub personal_team_id: String,
    /// The teams the member belongs to now, in their order.
    pub member_team_ids: Vec<String>,
    /// The composer's Teams pick on the conversation root; `None` = every team.
    pub picked_team_ids: Option<Vec<String>>,
}

/// #EXP-1720: the teams an Assistant tree may touch this turn: the personal
/// team, then the picked teams ∩ the member's (`None` = all).
pub fn assistant_scope(input: &AssistantScopeInput) -> Vec<String> {
    let mut teams = vec![input.personal_team_id.clone()];
    for team in &input.member_team_ids {
        let picked = input.picked_team_ids.as_ref().is_none_or(|picked| picked.contains(team));
        if picked && !teams.contains(team) {
            teams.push(team.clone());
        }
    }
    teams
}

/// #EXP-2027: what a tree's ROOT is: the Assistant's conversation, a team
/// agent's tree, or a plain chat (no acting agent, or the Assistant's).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TreeRootKind {
    Assistant,
    Agent,
    Chat,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeScopeInput {
    pub root: TreeRootKind,
    /// The root's own team (the Assistant's: the personal team).
    pub own_team_id: String,
    /// The teams the member belongs to now, in their order.
    pub member_team_ids: Vec<String>,
    /// The root's stored Teams pick (`sessions.scope_team_ids`).
    pub picked_team_ids: Option<Vec<String>>,
}

/// #EXP-2027 (access.json `treeScope`): the teams a tree may touch: the
/// Assistant's = [`assistant_scope`] (`None` = all); a team agent's = its own
/// team; a chat's = its own team, then its pick ∩ the member's (`None` =
/// none picked).
pub fn tree_scope(input: &TreeScopeInput) -> Vec<String> {
    let picked = match input.root {
        TreeRootKind::Agent => return vec![input.own_team_id.clone()],
        TreeRootKind::Chat => Some(input.picked_team_ids.clone().unwrap_or_default()),
        TreeRootKind::Assistant => input.picked_team_ids.clone(),
    };
    assistant_scope(&AssistantScopeInput {
        personal_team_id: input.own_team_id.clone(),
        member_team_ids: input.member_team_ids.clone(),
        picked_team_ids: picked,
    })
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActingSessionInput {
    /// The session the request's key is bound to; `None` = an unbound
    /// credential.
    pub key_session_id: Option<String>,
    /// The request's `X-Exp-Session-Id`; `None` = absent.
    pub header_session_id: Option<String>,
    /// Whether the session the request would speak for resolves as the
    /// caller's own.
    pub resolves: bool,
}

/// Why a request's session is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ActingRefusal {
    /// The header names another session than the key's.
    Mismatch,
    /// The key's session no longer resolves as the caller's.
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ActingSessionVerdict {
    SpeaksFor(Option<String>),
    Refused(ActingRefusal),
}

/// #EXP-2028 (access.json `actingSession`): the session a request speaks
/// for. A bound key speaks for its session with or without the header (one
/// naming another session is refused, a bound session that no longer
/// resolves too); an unbound credential speaks for the header's session
/// when it resolves, else for none.
pub fn acting_session(input: &ActingSessionInput) -> ActingSessionVerdict {
    if let Some(key) = &input.key_session_id {
        if input.header_session_id.as_ref().is_some_and(|header| header != key) {
            return ActingSessionVerdict::Refused(ActingRefusal::Mismatch);
        }
        return if input.resolves {
            ActingSessionVerdict::SpeaksFor(Some(key.clone()))
        } else {
            ActingSessionVerdict::Refused(ActingRefusal::Unknown)
        };
    }
    ActingSessionVerdict::SpeaksFor(input.header_session_id.clone().filter(|_| input.resolves))
}
