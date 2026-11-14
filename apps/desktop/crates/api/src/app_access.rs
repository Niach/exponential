//! VAPP-183: typed `appAccess` tRPC helpers for an app's settings page (the
//! Accounts + MCP tools sections, `ui::settings::app_mcp`).
//!
//! Verified against `apps/web/src/lib/trpc/app-access.ts`:
//! - `appTools {appId}` (query, any member) → `{canToggle, tools:[{name,
//!   title, access, via, enabled}], upstreams:[{name, serverId|null,
//!   serverName|null, auth|null, state|null}], servers:[{id, name}] (#VAPP-254,
//!   name order, empty for a member), automations:[{name, on, tool, paused|null,
//!   lastRunAt, lastStatus|null, lastError, lastFailureAt}], hooks:[{name,
//!   url, hasSecret, rotatedAt|null}], shares:[{name, mode, kind, state,
//!   url|null, expiresAt|null}]}` (#VAPP-220, #VAPP-238, VAPP-241);
//! - #VAPP-254: `setUpstreamMapping {appId, upstream, serverId|null}`
//!   (mutation, owners) → `{ok: true}`;
//! - #VAPP-238: `rotateHook {appId, name}` (mutation, owners) → `{name,
//!   url, secret}` (the secret is shown once, never refetched);
//! - VAPP-247: `setShareLink {appId, name, action: expire, days}`
//! - VAPP-241: `setShareLink {appId, name, action: enable|rotate|revoke}`
//!   (mutation, owners) → `{ok: true}`;
//! - `setToolEnabled {appId, tool, enabled}` (mutation, owners) →
//!   `{ok: true}`;
//! - VAPP-185 (Settings › MCP › Access): `list {teamId}` (query) →
//!   `{canEdit, apps:[{id, slug, name, icon, color, toolCount}], rows:
//!   [{userId, name, email, image, role, levels:{[appId]: none|read|write}}]}`
//!   (each app also carries `tools:[{name, title, blurb}]`, Settings › MCP's
//!   open Team apps row)
//!   (owners: every member, a member: their own row); `set {appId, userId,
//!   level}` (mutation, owners) → `{ok: true}`.

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::trpc::TrpcClient;

/// One tool the app's package exposes on `/api/mcp/apps`.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppTool {
    pub name: String,
    #[serde(default)]
    pub title: String,
    /// `read` | `write`.
    #[serde(default)]
    pub access: String,
    #[serde(default)]
    pub via: Option<String>,
    #[serde(default)]
    pub enabled: bool,
}

/// One upstream the app's tools call (its DECLARED name), with the caller's
/// own connection to the team server filling it (#VAPP-254: the app's mapping,
/// else the server of exactly that name). `server_id: None` = nothing fills it.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppUpstream {
    pub name: String,
    #[serde(default)]
    pub server_id: Option<String>,
    /// #VAPP-254: the REAL server's name (absent on an older server).
    #[serde(default)]
    pub server_name: Option<String>,
    #[serde(default)]
    pub auth: Option<String>,
    /// The upstream state (`connected` | `reconnect` | `disconnected` | …).
    #[serde(default)]
    pub state: Option<String>,
}

/// #VAPP-254: a team MCP server an owner may pick for an upstream.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct AppServer {
    pub id: String,
    pub name: String,
}

/// #VAPP-220: one automation the PUBLISHED package declares, with its last
/// run (`apps/web/src/lib/apps/automations.ts` `AutomationRow`).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppAutomation {
    pub name: String,
    /// `{schedule:{cron,tz?}}` | `{record:{kind,op}}` | `{event}`; kept raw
    /// so a trigger kind this build does not know never fails the response
    /// (read it with [`AppAutomation::trigger`]).
    #[serde(default)]
    pub on: serde_json::Value,
    #[serde(default)]
    pub tool: String,
    /// Why it does not run (the sponsor is gone or read-only), else `None`.
    #[serde(default)]
    pub paused: Option<String>,
    /// #VAPP-243: the member a run acts as (name, else email); `None` when
    /// paused or unknown.
    #[serde(default)]
    pub sponsor_name: Option<String>,
    #[serde(default)]
    pub last_run_at: Option<String>,
    #[serde(default)]
    pub last_status: Option<String>,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub last_failure_at: Option<String>,
    /// #VAPP-255: runs the rate cap dropped in the last 24 h; 0 from an older server.
    #[serde(default)]
    pub dropped_runs: i64,
}

impl AppAutomation {
    /// The typed trigger, `None` for a shape this build does not know.
    pub fn trigger(&self) -> Option<domain::apps::AutomationOn> {
        serde_json::from_value(self.on.clone()).ok()
    }
}

/// #VAPP-238: one webhook the PUBLISHED package's automations listen on
/// (`apps/web/src/lib/apps/webhooks.ts` `HookRow`).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppHook {
    pub name: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub has_secret: bool,
    #[serde(default)]
    pub rotated_at: Option<String>,
}

/// VAPP-241: one public link of the PUBLISHED package
/// (`apps/web/src/lib/apps/share-links.ts` `ShareRow`). `mode` and `state`
/// stay raw wire strings (read with `domain::app_share::{ShareMode,
/// ShareLinkState}::parse`) so a newer value never fails the response.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppShare {
    pub name: String,
    /// `submit` | `view`.
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub kind: String,
    /// `off` | `on` | `expired` | `orphaned`.
    #[serde(default)]
    pub state: String,
    /// Owners only, while the link has a live token.
    #[serde(default)]
    pub url: Option<String>,
    /// The expiry day (`YYYY-MM-DD`).
    #[serde(default)]
    pub expires_at: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppToolsOut {
    /// Owners switch tools.
    #[serde(default)]
    pub can_toggle: bool,
    #[serde(default)]
    pub tools: Vec<AppTool>,
    #[serde(default)]
    pub upstreams: Vec<AppUpstream>,
    /// #VAPP-254: what an owner may pick, in name order (empty for a member,
    /// absent on an older server).
    #[serde(default)]
    pub servers: Vec<AppServer>,
    /// #VAPP-220 (absent on an older server).
    #[serde(default)]
    pub automations: Vec<AppAutomation>,
    /// #VAPP-238 (absent on an older server), in package order.
    #[serde(default)]
    pub hooks: Vec<AppHook>,
    /// VAPP-241 (absent on an older server), in package order.
    #[serde(default)]
    pub shares: Vec<AppShare>,
}

/// `appAccess.appTools` — query.
pub fn app_tools(trpc: &TrpcClient, app_id: &str) -> Result<AppToolsOut, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        app_id: &'a str,
    }
    trpc.query_with_input("appAccess.appTools", &Input { app_id })
}

/// #VAPP-254: `appAccess.setUpstreamMapping` — mutation (owners; the server
/// checks). `server_id: None`, or the server named exactly like the upstream,
/// = the automatic default.
pub fn set_upstream_mapping(trpc: &TrpcClient, app_id: &str, upstream: &str, server_id: Option<&str>) -> Result<(), ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        app_id: &'a str,
        upstream: &'a str,
        server_id: Option<&'a str>,
    }
    #[derive(Deserialize)]
    struct Ok {
        #[serde(default)]
        #[allow(dead_code)]
        ok: bool,
    }
    let _: Ok = trpc.mutation("appAccess.setUpstreamMapping", &Input { app_id, upstream, server_id })?;
    Ok(())
}

/// #VAPP-238: a freshly minted (or rotated) webhook secret.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RotatedHook {
    pub name: String,
    #[serde(default)]
    pub url: String,
    pub secret: String,
}

/// `appAccess.rotateHook` — mutation (owners; the server checks).
pub fn rotate_hook(trpc: &TrpcClient, app_id: &str, name: &str) -> Result<RotatedHook, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        app_id: &'a str,
        name: &'a str,
    }
    trpc.mutation("appAccess.rotateHook", &Input { app_id, name })
}

/// VAPP-241: `appAccess.setShareLink` — mutation (owners; the server
/// checks). `action` = `enable` | `rotate` | `revoke`.
pub fn set_share_link(trpc: &TrpcClient, app_id: &str, name: &str, action: &str) -> Result<(), ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        app_id: &'a str,
        name: &'a str,
        action: &'a str,
    }
    #[derive(Deserialize)]
    struct Ok {
        #[serde(default)]
        #[allow(dead_code)]
        ok: bool,
    }
    let _: Ok = trpc.mutation("appAccess.setShareLink", &Input { app_id, name, action })?;
    Ok(())
}

/// VAPP-247: `appAccess.setShareLink {action: expire, days}` — a live link's
/// expiry, `days` from now, `None` = never (owners; the server checks).
pub fn set_share_expiry(trpc: &TrpcClient, app_id: &str, name: &str, days: Option<u32>) -> Result<(), ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        app_id: &'a str,
        name: &'a str,
        action: &'a str,
        days: Option<u32>,
    }
    #[derive(Deserialize)]
    struct Ok {
        #[serde(default)]
        #[allow(dead_code)]
        ok: bool,
    }
    let _: Ok = trpc.mutation("appAccess.setShareLink", &Input { app_id, name, action: "expire", days })?;
    Ok(())
}

/// eApps R3: the caller's own level for an app (`appAccess.mine`).
#[derive(Debug, Clone, Deserialize)]
pub struct MineOut {
    pub level: String,
}

/// `appAccess.mine` — query.
pub fn mine(trpc: &TrpcClient, app_id: &str) -> Result<MineOut, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        app_id: &'a str,
    }
    trpc.query_with_input("appAccess.mine", &Input { app_id })
}

/// VAPP-198: every member's non-Full level for an app (`appAccess.levels`, owners).
#[derive(Debug, Clone, Deserialize)]
pub struct LevelsOut {
    #[serde(default)]
    pub levels: std::collections::HashMap<String, String>,
}

/// `appAccess.levels` — query (owners; the server checks).
pub fn levels(trpc: &TrpcClient, app_id: &str) -> Result<LevelsOut, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        app_id: &'a str,
    }
    trpc.query_with_input("appAccess.levels", &Input { app_id })
}

/// `appAccess.setToolEnabled` — mutation (owners; the server checks).
pub fn set_tool_enabled(trpc: &TrpcClient, app_id: &str, tool: &str, enabled: bool) -> Result<(), ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        app_id: &'a str,
        tool: &'a str,
        enabled: bool,
    }
    #[derive(Deserialize)]
    struct Ok {
        #[serde(default)]
        #[allow(dead_code)]
        ok: bool,
    }
    let _: Ok = trpc.mutation("appAccess.setToolEnabled", &Input { app_id, tool, enabled })?;
    Ok(())
}

/// VAPP-185: an app that exposes tools (the Access group's header).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AccessApp {
    pub id: String,
    #[serde(default)]
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub tool_count: u32,
    /// Settings › MCP: the open Team apps row lists them (package order).
    #[serde(default)]
    pub tools: Vec<AccessAppTool>,
    /// #FEED-242: the level of a member without their own row (`none` |
    /// `read` | `write`; an older server sends none = `write`).
    #[serde(default = "default_access_write")]
    pub default_access: String,
}

fn default_access_write() -> String {
    "write".to_string()
}

/// One tool an app exposes (`appAccess.list` apps' `tools`).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AccessAppTool {
    pub name: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub blurb: String,
}

/// VAPP-185: one member's levels, keyed by app id (`none` | `read` |
/// `write`).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AccessRow {
    pub user_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub levels: std::collections::HashMap<String, String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AccessOut {
    /// Owners pick the levels.
    #[serde(default)]
    pub can_edit: bool,
    #[serde(default)]
    pub apps: Vec<AccessApp>,
    #[serde(default)]
    pub rows: Vec<AccessRow>,
}

/// `appAccess.list` — query.
pub fn list(trpc: &TrpcClient, team_id: &str) -> Result<AccessOut, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        team_id: &'a str,
    }
    trpc.query_with_input("appAccess.list", &Input { team_id })
}

/// `appAccess.set` — mutation (owners; the server checks).
pub fn set(trpc: &TrpcClient, app_id: &str, user_id: &str, level: &str) -> Result<(), ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        app_id: &'a str,
        user_id: &'a str,
        level: &'a str,
    }
    #[derive(Deserialize)]
    struct Ok {
        #[serde(default)]
        #[allow(dead_code)]
        ok: bool,
    }
    let _: Ok = trpc.mutation("appAccess.set", &Input { app_id, user_id, level })?;
    Ok(())
}

/// #FEED-242: `appAccess.setDefault` — mutation (owners; the server
/// checks): the app's level for members without their own row.
pub fn set_default(trpc: &TrpcClient, app_id: &str, level: &str) -> Result<(), ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        app_id: &'a str,
        level: &'a str,
    }
    #[derive(Deserialize)]
    struct Ok {
        #[serde(default)]
        #[allow(dead_code)]
        ok: bool,
    }
    let _: Ok = trpc.mutation("appAccess.setDefault", &Input { app_id, level })?;
    Ok(())
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
    fn app_tools_gets_input_and_decodes_null_servers() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"canToggle":true,"tools":[{"name":"log_hours","title":"Log hours","access":"write","via":"records","enabled":false}],"upstreams":[{"name":"github","serverId":"s-1","auth":"oauth","state":"reconnect"},{"name":"linear","serverId":null,"auth":null,"state":null}]}}}"#,
        );
        let out = app_tools(&client(&base), "a-1").unwrap();
        assert!(out.can_toggle);
        assert_eq!(out.tools[0].name, "log_hours");
        assert_eq!(out.tools[0].access, "write");
        assert!(!out.tools[0].enabled);
        assert_eq!(out.upstreams[0].server_id.as_deref(), Some("s-1"));
        assert_eq!(out.upstreams[0].state.as_deref(), Some("reconnect"));
        assert_eq!(out.upstreams[1].server_id, None);
        // An older server sends no automations.
        assert!(out.automations.is_empty());
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.contains("GET /api/trpc/appAccess.appTools?input="), "query: {request}");
        assert!(request.contains("%22appId%22"), "encoded input: {request}");
    }

    #[test]
    fn app_tools_decodes_the_mapping() {
        let out: AppToolsOut = serde_json::from_str(
            r#"{"canToggle":true,"tools":[],"upstreams":[{"name":"crm","serverId":"s-2","serverName":"hubspot","auth":"oauth","state":"connected"},{"name":"linear","serverId":null,"serverName":null,"auth":null,"state":null}],"servers":[{"id":"s-1","name":"crm"},{"id":"s-2","name":"hubspot"}]}"#,
        )
        .unwrap();
        assert_eq!(out.upstreams[0].server_name.as_deref(), Some("hubspot"));
        assert_eq!(out.upstreams[1].server_name, None);
        assert_eq!(out.servers, vec![AppServer { id: "s-1".into(), name: "crm".into() }, AppServer { id: "s-2".into(), name: "hubspot".into() }]);
        // An older server sends neither.
        let old: AppToolsOut = serde_json::from_str(r#"{"canToggle":true,"tools":[],"upstreams":[{"name":"crm","serverId":"s-1"}]}"#).unwrap();
        assert!(old.servers.is_empty());
        assert_eq!(old.upstreams[0].server_name, None);
    }

    #[test]
    fn set_upstream_mapping_posts_the_server() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"ok":true}}}"#);
        set_upstream_mapping(&client(&base), "a-1", "crm", Some("s-2")).unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.contains("POST /api/trpc/appAccess.setUpstreamMapping"), "mutation: {request}");
        assert!(request.contains(r#""appId":"a-1""#) && request.contains(r#""upstream":"crm""#) && request.contains(r#""serverId":"s-2""#), "{request}");
    }

    #[test]
    fn set_upstream_mapping_clears_with_null() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"ok":true}}}"#);
        set_upstream_mapping(&client(&base), "a-1", "crm", None).unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.contains(r#""serverId":null"#), "{request}");
    }

    #[test]
    fn app_tools_decodes_automations() {
        let out: AppToolsOut = serde_json::from_str(
            r#"{"canToggle":false,"tools":[],"upstreams":[],"automations":[{"name":"tick","on":{"schedule":{"cron":"0 9 * * *","tz":"Europe/Vienna"}},"tool":"log","paused":null,"lastRunAt":"2026-11-03T08:00:00.000Z","lastStatus":"ok","lastError":null,"lastFailureAt":null},{"name":"odd","on":{"webhook":"x"},"tool":"log","paused":"sponsorGone","lastRunAt":null,"lastStatus":null,"lastError":null,"lastFailureAt":null}]}"#,
        )
        .unwrap();
        assert_eq!(out.automations.len(), 2);
        assert_eq!(out.automations[0].last_status.as_deref(), Some("ok"));
        // #VAPP-255: an older server sends no count.
        assert_eq!(out.automations[0].dropped_runs, 0);
        assert!(matches!(out.automations[0].trigger(), Some(domain::apps::AutomationOn::Schedule(_))));
        assert_eq!(out.automations[1].trigger(), None);
        assert_eq!(out.automations[1].paused.as_deref(), Some("sponsorGone"));
    }

    #[test]
    fn app_tools_decodes_hooks() {
        let out: AppToolsOut = serde_json::from_str(
            r#"{"canToggle":true,"tools":[],"upstreams":[],"automations":[],"hooks":[{"name":"stripe","url":"https://x.at/api/apps/a-1/hooks/stripe","hasSecret":true,"rotatedAt":"2026-11-03T08:00:00.000Z"},{"name":"github","url":"https://x.at/api/apps/a-1/hooks/github","hasSecret":false,"rotatedAt":null}]}"#,
        )
        .unwrap();
        assert_eq!(out.hooks.len(), 2);
        assert!(out.hooks[0].has_secret && !out.hooks[1].has_secret);
        assert_eq!(out.hooks[1].rotated_at, None);
        let old: AppToolsOut = serde_json::from_str(r#"{"canToggle":false,"tools":[],"upstreams":[]}"#).unwrap();
        assert!(old.hooks.is_empty());
    }

    #[test]
    fn app_tools_decodes_shares() {
        let out: AppToolsOut = serde_json::from_str(
            r#"{"canToggle":true,"tools":[],"upstreams":[],"automations":[],"hooks":[],"shares":[{"name":"signup","mode":"submit","kind":"lead","state":"on","url":"https://x.at/s/tok","expiresAt":"2026-12-01"},{"name":"old","mode":"submit","kind":"","state":"off","url":null,"expiresAt":null}]}"#,
        )
        .unwrap();
        assert_eq!(out.shares.len(), 2);
        assert_eq!(out.shares[0].url.as_deref(), Some("https://x.at/s/tok"));
        assert_eq!(out.shares[0].expires_at.as_deref(), Some("2026-12-01"));
        assert_eq!(out.shares[1].url, None);
        let old: AppToolsOut = serde_json::from_str(r#"{"canToggle":false,"tools":[],"upstreams":[]}"#).unwrap();
        assert!(old.shares.is_empty());
    }

    #[test]
    fn set_share_link_posts_the_action() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"ok":true}}}"#);
        set_share_link(&client(&base), "a-1", "signup", "rotate").unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.contains("POST /api/trpc/appAccess.setShareLink"), "mutation: {request}");
        assert!(request.contains(r#""appId":"a-1""#) && request.contains(r#""name":"signup""#) && request.contains(r#""action":"rotate""#), "{request}");
    }

    #[test]
    fn set_share_expiry_posts_the_days_or_null() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"ok":true}}}"#);
        set_share_expiry(&client(&base), "a-1", "signup", Some(30)).unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.contains(r#""action":"expire""#) && request.contains(r#""days":30"#), "{request}");
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"ok":true}}}"#);
        set_share_expiry(&client(&base), "a-1", "signup", None).unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.contains(r#""days":null"#), "{request}");
    }

    #[test]
    fn rotate_hook_posts_and_decodes_the_secret() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"name":"stripe","url":"https://x.at/api/apps/a-1/hooks/stripe","secret":"whsec_1"}}}"#,
        );
        let out = rotate_hook(&client(&base), "a-1", "stripe").unwrap();
        assert_eq!(out.secret, "whsec_1");
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.contains("POST /api/trpc/appAccess.rotateHook"), "mutation: {request}");
        assert!(request.contains(r#""appId":"a-1""#) && request.contains(r#""name":"stripe""#), "{request}");
    }

    #[test]
    fn set_tool_enabled_posts_the_switch() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"ok":true}}}"#);
        set_tool_enabled(&client(&base), "a-1", "log_hours", false).unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.contains("POST /api/trpc/appAccess.setToolEnabled"), "mutation: {request}");
        assert!(request.contains(r#""appId":"a-1""#) && request.contains(r#""tool":"log_hours""#) && request.contains(r#""enabled":false"#), "{request}");
    }

    #[test]
    fn list_decodes_apps_and_levels() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"canEdit":true,"apps":[{"id":"a-1","slug":"customer-admin","name":"Customer admin","icon":null,"color":"blue","toolCount":3}],"rows":[{"userId":"u-1","name":"Mia","email":"mia@x.at","image":null,"role":"member","levels":{"a-1":"read"}}]}}}"#,
        );
        let out = list(&client(&base), "t-1").unwrap();
        assert!(out.can_edit);
        assert_eq!(out.apps[0].tool_count, 3);
        assert_eq!(out.rows[0].levels.get("a-1").map(String::as_str), Some("read"));
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.contains("GET /api/trpc/appAccess.list?input="), "query: {request}");
    }

    #[test]
    fn set_posts_the_level() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"ok":true}}}"#);
        set(&client(&base), "a-1", "u-1", "none").unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.contains("POST /api/trpc/appAccess.set"), "mutation: {request}");
        assert!(request.contains(r#""userId":"u-1""#) && request.contains(r#""level":"none""#), "{request}");
    }
}
