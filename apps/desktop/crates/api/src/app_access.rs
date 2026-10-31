//! VAPP-183: typed `appAccess` tRPC helpers for an app's settings page (the
//! Accounts + MCP tools sections, `ui::settings::app_mcp`).
//!
//! Verified against `apps/web/src/lib/trpc/app-access.ts`:
//! - `appTools {appId}` (query, any member) → `{canToggle, tools:[{name,
//!   title, access, via, enabled}], upstreams:[{name, serverId|null,
//!   auth|null, state|null}]}`;
//! - `setToolEnabled {appId, tool, enabled}` (mutation, owners) →
//!   `{ok: true}`.

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

/// One upstream (a team MCP server by name) the app's tools call, with the
/// caller's own connection. `server_id: None` = the team has no server of
/// that name.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppUpstream {
    pub name: String,
    #[serde(default)]
    pub server_id: Option<String>,
    #[serde(default)]
    pub auth: Option<String>,
    /// The upstream state (`connected` | `reconnect` | `disconnected` | …).
    #[serde(default)]
    pub state: Option<String>,
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
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.contains("GET /api/trpc/appAccess.appTools?input="), "query: {request}");
        assert!(request.contains("%22appId%22"), "encoded input: {request}");
    }

    #[test]
    fn set_tool_enabled_posts_the_switch() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"ok":true}}}"#);
        set_tool_enabled(&client(&base), "a-1", "log_hours", false).unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.contains("POST /api/trpc/appAccess.setToolEnabled"), "mutation: {request}");
        assert!(request.contains(r#""appId":"a-1""#) && request.contains(r#""tool":"log_hours""#) && request.contains(r#""enabled":false"#), "{request}");
    }
}
