//! VAPP-185: typed `mcpClients` + `mcpActivity` tRPC helpers for Settings ›
//! MCP (`ui::settings::mcp`).
//!
//! Verified against `apps/web/src/lib/trpc/mcp-clients.ts`:
//! - `mcpClients.list {teamId, scope: mine|everyone}` (query; Everyone is the
//!   owners') → `{canSeeEveryone, clients:[{clientId, name, icon, userId,
//!   userName, userEmail, kinds, appsAccess, connectedAt, lastUsedAt}]}`;
//! - `mcpClients.revoke {clientId, teamId, userId?}` (mutation; `userId` only
//!   in Everyone) → `{ok: true}`;
//! - `mcpActivity.list {teamId}` (query; owners the team's calls, a member
//!   their own) → `{owner, calls:[{id, tool, status, error, appName,
//!   userName, clientName, createdAt}]}`.

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::trpc::TrpcClient;

/// One OAuth client a member authorized.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct McpClient {
    pub client_id: String,
    pub name: String,
    pub icon: Option<String>,
    pub user_id: String,
    pub user_name: String,
    pub user_email: String,
    /// `builtin` | `apps`, in that order.
    pub kinds: Vec<String>,
    /// The apps grant's `read` | `full`; `None` without one.
    pub apps_access: Option<String>,
    pub connected_at: String,
    pub last_used_at: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct McpClientsOut {
    pub can_see_everyone: bool,
    pub clients: Vec<McpClient>,
}

/// One `mcp_calls` row.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct McpCall {
    pub id: String,
    pub tool: String,
    pub status: String,
    pub error: Option<String>,
    pub app_name: Option<String>,
    pub user_name: String,
    pub client_name: Option<String>,
    pub created_at: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct McpActivityOut {
    pub owner: bool,
    pub calls: Vec<McpCall>,
}

/// `mcpClients.list` — query; `scope` = `mine` | `everyone`.
pub fn list(trpc: &TrpcClient, team_id: &str, scope: &str) -> Result<McpClientsOut, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        team_id: &'a str,
        scope: &'a str,
    }
    trpc.query_with_input("mcpClients.list", &Input { team_id, scope })
}

/// `mcpClients.revoke` — mutation; `user_id` only when an owner revokes a
/// member's client from Everyone.
pub fn revoke(trpc: &TrpcClient, client_id: &str, team_id: &str, user_id: Option<&str>) -> Result<(), ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        client_id: &'a str,
        team_id: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        user_id: Option<&'a str>,
    }
    #[derive(Deserialize)]
    struct Ok {
        #[serde(default)]
        #[allow(dead_code)]
        ok: bool,
    }
    let _: Ok = trpc.mutation("mcpClients.revoke", &Input { client_id, team_id, user_id })?;
    Ok(())
}

/// `mcpActivity.list` — query.
pub fn activity(trpc: &TrpcClient, team_id: &str) -> Result<McpActivityOut, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        team_id: &'a str,
    }
    trpc.query_with_input("mcpActivity.list", &Input { team_id })
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
    fn list_sends_the_scope_and_decodes_null_fields() {
        let (base, captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"canSeeEveryone":true,"clients":[{"clientId":"c-1","name":"Claude Code","icon":null,"userId":"u-1","userName":"Mia","userEmail":"mia@x.at","kinds":["apps","builtin"],"appsAccess":"full","connectedAt":"2026-10-31T10:00:00.000Z","lastUsedAt":null}]}}}"#,
        );
        let out = list(&client(&base), "t-1", "everyone").unwrap();
        assert!(out.can_see_everyone);
        assert_eq!(out.clients[0].apps_access.as_deref(), Some("full"));
        assert_eq!(out.clients[0].last_used_at, None);
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.contains("GET /api/trpc/mcpClients.list?input="), "query: {request}");
        assert!(request.contains("%22everyone%22"), "encoded input: {request}");
    }

    #[test]
    fn revoke_sends_the_user_only_when_given() {
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"ok":true}}}"#);
        revoke(&client(&base), "c-1", "t-1", None).unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.contains("POST /api/trpc/mcpClients.revoke"), "mutation: {request}");
        assert!(request.contains(r#""clientId":"c-1""#) && !request.contains("userId"), "{request}");
        let (base, captured) = one_shot_server(200, r#"{"result":{"data":{"ok":true}}}"#);
        revoke(&client(&base), "c-1", "t-1", Some("u-2")).unwrap();
        let request = captured.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(request.contains(r#""userId":"u-2""#), "{request}");
    }

    #[test]
    fn activity_decodes_the_calls() {
        let (base, _captured) = one_shot_server(
            200,
            r#"{"result":{"data":{"owner":false,"calls":[{"id":"1","tool":"log_hours","status":"ok","error":null,"appName":null,"userName":"Danny","clientName":null,"createdAt":"2026-10-31T10:00:00.000Z"}]}}}"#,
        );
        let out = activity(&client(&base), "t-1").unwrap();
        assert!(!out.owner);
        assert_eq!(out.calls[0].tool, "log_hours");
        assert_eq!(out.calls[0].app_name, None);
    }
}
