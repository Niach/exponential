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
    row.unwrap_or(Level::Write)
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
