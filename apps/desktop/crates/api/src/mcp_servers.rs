//! EXP-792: typed `mcpServers.*` tRPC helpers — the device side of team MCP
//! servers. The SERVER holds each member's credentials (OAuth token sets and
//! typed secrets, encrypted at rest, one set per member per server): a member
//! connects once and it works on every device, remote start and trigger.
//! This machine holds none and runs no OAuth; it only asks for a launch's
//! values at spawn ([`resolve_for_launch`]).
//!
//! Shapes mirror `apps/web/src/lib/trpc/mcp-servers.ts`:
//!
//! - `mcpServers.list({teamId})` — **query** — one team's servers, each with
//!   the CALLER's [`McpConnection`] and the team's connected/member counts.
//! - `mcpServers.create` / `update` / `remove` — owner writes.
//! - `mcpServers.setSecret` / `disconnect` / `test` — the caller's own
//!   credential for one server. OAuth connects start in the BROWSER, never
//!   here: [`connect_page_url`] is the web settings deep link that runs
//!   `mcpServers.connect` in the signed-in browser session.
//! - `mcpServers.resolveForLaunch` — **mutation** — the caller's own values
//!   for a launch's pick (refreshed server-side when expiring); the ONLY
//!   response here that carries a secret ([`McpLaunchResolution`]).

use serde::{Deserialize, Serialize};
use std::fmt;

use crate::error::ApiError;
use crate::trpc::TrpcClient;

/// `mcp_servers` row (contract `mcpTransport` / `mcpAuth` vocabularies).
/// Every list field defaults so an older server's thinner row still parses.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    pub id: String,
    #[serde(default)]
    pub team_id: String,
    pub name: String,
    /// `http` | `stdio`.
    #[serde(default = "default_transport")]
    pub transport: String,
    #[serde(default)]
    pub url: Option<String>,
    /// Header NAMES a member's secret fills (`http`).
    #[serde(default)]
    pub header_names: Vec<String>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    /// Env NAMES a member's secret fills (`stdio`).
    #[serde(default)]
    pub env_names: Vec<String>,
    /// Advisory OAuth scopes to request.
    #[serde(default)]
    pub scopes: Vec<String>,
    /// `none` | `oauth` | `secret`.
    #[serde(default = "default_auth")]
    pub auth: String,
    #[serde(default)]
    pub enabled_by_default: bool,
}

fn default_transport() -> String {
    "http".to_string()
}

fn default_auth() -> String {
    "none".to_string()
}

impl McpServerConfig {
    pub fn is_http(&self) -> bool {
        self.transport != "stdio"
    }

    pub fn is_oauth(&self) -> bool {
        self.auth == "oauth"
    }

    /// The member-typed secret positions (`auth: secret`): every declared
    /// header name (http) or env name (stdio). An OAuth server has none.
    pub fn secret_names(&self) -> &[String] {
        if self.is_http() {
            &self.header_names
        } else {
            &self.env_names
        }
    }
}

/// The CALLER's credential for one server (`McpConnection`). A row that
/// carries none reads as `not_connected`.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpConnection {
    /// `not_needed` (auth `none`) | `connected` (usable or refreshable) |
    /// `not_connected` | `expired` (no refresh token left) | `error` (the
    /// last refresh failed; [`Self::error`] says why).
    #[serde(default = "default_status")]
    pub status: String,
    /// ISO expiry of the OAuth access token, when the provider named one.
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

fn default_status() -> String {
    "not_connected".to_string()
}

impl Default for McpConnection {
    fn default() -> Self {
        Self {
            status: default_status(),
            expires_at: None,
            error: None,
        }
    }
}

impl McpConnection {
    /// A launch picking this server gets its tools: connected, or nothing
    /// to connect.
    pub fn is_ready(&self) -> bool {
        matches!(self.status.as_str(), "connected" | "not_needed")
    }
}

/// `mcpServers.list` entry: the config, the caller's connection and how many
/// of the team's members have connected.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpServerListEntry {
    #[serde(flatten)]
    pub config: McpServerConfig,
    #[serde(default)]
    pub connection: McpConnection,
    #[serde(default)]
    pub connected_count: u32,
    #[serde(default)]
    pub member_count: u32,
}

/// `mcpServers.list({teamId})`.
pub fn list(trpc: &TrpcClient, team_id: &str) -> Result<Vec<McpServerListEntry>, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        team_id: &'a str,
    }
    trpc.query_with_input("mcpServers.list", &Input { team_id })
}

/// The OWNER-write field set (`mcpServers.create` / `mcpServers.update`).
///
/// Every field is optional because `update` is a PATCH: the server merges it
/// onto the stored row and validates the MERGED result, so flipping `auth` to
/// `secret` alone still has to find exactly one declared header/env name.
/// `create` runs the same validator on a NON-partial input, so `name`,
/// `transport` and `auth` must be present there — the server answers
/// `BAD_REQUEST` otherwise, and this type does not pretend to know better.
///
/// The cross-field rules are the router's, not ours (`normalizeFields` in
/// `apps/web/src/lib/trpc/mcp-servers.ts`): an http server needs a `url` and
/// a stdio one a `command`; `oauth` is http-only; `secret` declares exactly
/// one name on the transport's side. A blank/`None` field is simply absent
/// from the wire, never `null`.
#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpServerFields {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// `http` | `stdio`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header_names: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env_names: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scopes: Option<Vec<String>>,
    /// `none` | `oauth` | `secret`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled_by_default: Option<bool>,
}

/// `mcpServers.create` — owner-only. Returns the stored row.
pub fn create(
    trpc: &TrpcClient,
    team_id: &str,
    fields: &McpServerFields,
) -> Result<McpServerConfig, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        team_id: &'a str,
        #[serde(flatten)]
        fields: &'a McpServerFields,
    }
    trpc.mutation("mcpServers.create", &Input { team_id, fields })
}

/// `mcpServers.update` — owner-only PATCH. Returns the stored row.
pub fn update(
    trpc: &TrpcClient,
    id: &str,
    fields: &McpServerFields,
) -> Result<McpServerConfig, ApiError> {
    #[derive(Serialize)]
    struct Input<'a> {
        id: &'a str,
        #[serde(flatten)]
        fields: &'a McpServerFields,
    }
    trpc.mutation("mcpServers.update", &Input { id, fields })
}

/// `mcpServers.remove` — owner-only. Every member's credential and OAuth
/// flow cascade with the server row.
pub fn remove(trpc: &TrpcClient, id: &str) -> Result<(), ApiError> {
    ok_mutation(trpc, "mcpServers.remove", &ServerIdInput { id })
}

#[derive(Serialize)]
struct ServerIdInput<'a> {
    id: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ServerInput<'a> {
    server_id: &'a str,
}

/// A `{ ok: true }` mutation.
fn ok_mutation<I: Serialize>(trpc: &TrpcClient, path: &str, input: &I) -> Result<(), ApiError> {
    #[derive(Deserialize)]
    struct Ok {
        #[serde(default)]
        #[allow(dead_code)]
        ok: bool,
    }
    let _: Ok = trpc.mutation(path, input)?;
    Ok(())
}

/// The web settings deep link that connects `server_id` for the signed-in
/// browser: `{instance}/t/{teamSlug}/settings/mcp-servers?connect={id}`.
///
/// The desktop and the CLI never call `mcpServers.connect` themselves: the
/// instance's OAuth callback only accepts a code when the BROWSER's
/// Exponential session is the member who started the flow, so the flow must
/// start in that browser. The page auto-starts the connect; the caller then
/// polls [`list`] until the row reads connected. `None` when the instance
/// URL is not an `http(s)` URL (never hand anything else to the OS opener)
/// or the slug / id is empty.
pub fn connect_page_url(instance_url: &str, team_slug: &str, server_id: &str) -> Option<String> {
    let base = instance_url.trim().trim_end_matches('/');
    if !crate::opener::is_web_url(base) || team_slug.is_empty() || server_id.is_empty() {
        return None;
    }
    Some(format!(
        "{base}/t/{}/settings/mcp-servers?connect={}",
        encode_component(team_slug),
        encode_component(server_id)
    ))
}

/// Percent-encode everything but RFC 3986 unreserved characters (slugs and
/// uuids pass through untouched; anything else cannot break out of its
/// path segment or query value).
fn encode_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// `mcpServers.setSecret` — store the caller's typed value for an `auth:
/// secret` server (server-side, encrypted). The value is never echoed back.
pub fn set_secret(trpc: &TrpcClient, server_id: &str, value: &str) -> Result<(), ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        server_id: &'a str,
        value: &'a str,
    }
    ok_mutation(trpc, "mcpServers.setSecret", &Input { server_id, value })
}

/// `mcpServers.disconnect` — delete the caller's credential for one server.
pub fn disconnect(trpc: &TrpcClient, server_id: &str) -> Result<(), ApiError> {
    ok_mutation(trpc, "mcpServers.disconnect", &ServerInput { server_id })
}

/// `mcpServers.test` output: an MCP `initialize` (+ `tools/list`) against an
/// http server with the caller's credential.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpTestResult {
    pub ok: bool,
    #[serde(default)]
    pub tools: Option<u32>,
    #[serde(default)]
    pub error: Option<String>,
}

/// `mcpServers.test`.
pub fn test(trpc: &TrpcClient, server_id: &str) -> Result<McpTestResult, ApiError> {
    trpc.mutation("mcpServers.test", &ServerInput { server_id })
}

/// One `name` → `value` pair of a launch's resolved server (a header or an
/// env var). The value is a SECRET: `Debug` redacts it.
#[derive(Clone, Deserialize, PartialEq, Eq)]
pub struct McpNamedValue {
    pub name: String,
    pub value: String,
}

impl fmt::Debug for McpNamedValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("McpNamedValue")
            .field("name", &self.name)
            .field("value", &"***")
            .finish()
    }
}

/// One server `resolveForLaunch` resolved: its wire config plus the caller's
/// values. `Debug` is safe — the values print as `***`.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpLaunchServer {
    pub id: String,
    pub name: String,
    /// `http` | `stdio`.
    #[serde(default = "default_transport")]
    pub transport: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    /// OAuth: `Authorization: Bearer …`; secret http: the declared header.
    #[serde(default)]
    pub headers: Vec<McpNamedValue>,
    /// Secret stdio: the declared env var.
    #[serde(default)]
    pub env: Vec<McpNamedValue>,
}

impl McpLaunchServer {
    pub fn is_http(&self) -> bool {
        self.transport != "stdio"
    }
}

/// A pick the server did not resolve (not connected, refresh failed, unknown
/// id). The launch goes ahead without it.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpSkippedServer {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub reason: String,
}

/// `mcpServers.resolveForLaunch` output.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct McpLaunchResolution {
    #[serde(default)]
    pub servers: Vec<McpLaunchServer>,
    #[serde(default)]
    pub skipped: Vec<McpSkippedServer>,
    /// Launch-time notes ("linear: access token expires in 40 min and
    /// cannot be refreshed"). Never a secret.
    #[serde(default)]
    pub warnings: Vec<String>,
}

/// `mcpServers.resolveForLaunch` — the caller's OWN credentials for
/// `server_ids`, OAuth tokens refreshed server-side when expiring. Handle the
/// result like a password.
///
/// EXP-1140: the call names the run it is for (`session_id` = the
/// `coding_sessions` row `codingSessions.start` just created, sent as
/// `X-Exp-Session-Id`); the server hands out values ONLY for that row's own
/// persisted pick, to its owner or host, and never to the agent's own key.
/// So the row must exist BEFORE this is called, and a resolve after a
/// server-side pick trim comes back `skipped`, never an error.
pub fn resolve_for_launch(
    trpc: &TrpcClient,
    server_ids: &[String],
    session_id: &str,
) -> Result<McpLaunchResolution, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        server_ids: &'a [String],
    }
    trpc.mutation_in_session("mcpServers.resolveForLaunch", &Input { server_ids }, session_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trpc::tests::one_shot_server;
    use crate::StaticToken;
    use std::sync::Arc;

    fn client(base: &str) -> TrpcClient {
        TrpcClient::new(base, Arc::new(StaticToken("tok".to_string())))
    }

    #[test]
    fn list_parses_the_callers_connection_and_counts() {
        let (base, rx) = one_shot_server(
            200,
            r#"{"result":{"data":[{"id":"s1","teamId":"t","name":"Linear","auth":"oauth","createdById":"u","connection":{"status":"expired","expiresAt":"2026-09-09T10:00:00.000Z","error":null},"connectedCount":2,"memberCount":5},{"id":"s2","name":"Docs"}]}}"#,
        );
        let rows = list(&client(&base), "t").expect("ok");
        assert!(rx.recv().unwrap().contains("GET /api/trpc/mcpServers.list?input="));
        assert_eq!(rows[0].config.name, "Linear");
        assert_eq!(rows[0].connection.status, "expired");
        assert!(!rows[0].connection.is_ready());
        assert_eq!(rows[0].connected_count, 2);
        assert_eq!(rows[0].member_count, 5);
        // A thinner row defaults to "not connected".
        assert_eq!(rows[1].connection.status, "not_connected");
    }

    #[test]
    fn connect_page_url_is_the_web_settings_deep_link() {
        assert_eq!(
            connect_page_url("https://app.exponential.at/", "acme", "s1").as_deref(),
            Some("https://app.exponential.at/t/acme/settings/mcp-servers?connect=s1")
        );
        assert_eq!(
            connect_page_url("http://localhost:3000", "a b", "x&y=1").as_deref(),
            Some("http://localhost:3000/t/a%20b/settings/mcp-servers?connect=x%26y%3D1")
        );
        assert_eq!(connect_page_url("file:///tmp", "acme", "s1"), None);
        assert_eq!(connect_page_url("javascript:alert(1)", "acme", "s1"), None);
        assert_eq!(connect_page_url("https://app.exponential.at", "", "s1"), None);
        assert_eq!(connect_page_url("https://app.exponential.at", "acme", ""), None);
    }

    #[test]
    fn set_secret_disconnect_and_test_post_the_server_id() {
        let (base, rx) = one_shot_server(200, r#"{"result":{"data":{"ok":true}}}"#);
        set_secret(&client(&base), "s1", "v-1").expect("ok");
        let request = rx.recv().unwrap();
        assert!(request.contains("POST /api/trpc/mcpServers.setSecret"));
        assert!(request.contains(r#"{"serverId":"s1","value":"v-1"}"#));

        let (base, rx) = one_shot_server(200, r#"{"result":{"data":{"ok":true}}}"#);
        disconnect(&client(&base), "s1").expect("ok");
        assert!(rx.recv().unwrap().contains("POST /api/trpc/mcpServers.disconnect"));

        let (base, _rx) =
            one_shot_server(200, r#"{"result":{"data":{"ok":false,"tools":null,"error":"401"}}}"#);
        let result = test(&client(&base), "s1").expect("ok");
        assert_eq!(
            result,
            McpTestResult {
                ok: false,
                tools: None,
                error: Some("401".into())
            }
        );
    }

    #[test]
    fn resolve_for_launch_parses_and_never_prints_a_value() {
        let (base, rx) = one_shot_server(
            200,
            r#"{"result":{"data":{"servers":[{"id":"s1","name":"Linear","transport":"http","url":"https://mcp.linear.app/mcp","command":null,"args":[],"headers":[{"name":"Authorization","value":"Bearer at-secret"}],"env":[]}],"skipped":[{"id":"s2","name":"Sentry","reason":"not connected"}],"warnings":["w"]}}}"#,
        );
        let ids = vec!["s1".to_string(), "s2".to_string()];
        let resolved = resolve_for_launch(&client(&base), &ids, "sess-1").expect("ok");
        let request = rx.recv().unwrap();
        assert!(request.contains("POST /api/trpc/mcpServers.resolveForLaunch"));
        assert!(request.contains(r#"{"serverIds":["s1","s2"]}"#));
        // EXP-1140: the call names its run — the server refuses it otherwise.
        assert!(
            request.to_ascii_lowercase().contains("x-exp-session-id: sess-1"),
            "{request}"
        );
        assert_eq!(resolved.servers[0].headers[0].value, "Bearer at-secret");
        assert_eq!(resolved.skipped[0].reason, "not connected");
        assert_eq!(resolved.warnings, vec!["w".to_string()]);
        let printed = format!("{resolved:?}");
        assert!(!printed.contains("at-secret"), "{printed}");
        assert!(printed.contains("Authorization"), "{printed}");
    }

    /// The create wire is `{teamId, ...fields}` — a flattened field set, so
    /// the router's `fieldsSchema.extend({teamId})` sees ONE flat object.
    #[test]
    fn create_flattens_the_fields_beside_the_team_id() {
        let (base, rx) = one_shot_server(
            200,
            r#"{"result":{"data":{"id":"s1","name":"Linear","transport":"http","auth":"oauth"}}}"#,
        );
        let row = create(
            &client(&base),
            "team-1",
            &McpServerFields {
                name: Some("Linear".into()),
                transport: Some("http".into()),
                url: Some("https://mcp.linear.app/mcp".into()),
                auth: Some("oauth".into()),
                enabled_by_default: Some(true),
                ..Default::default()
            },
        )
        .expect("ok");
        assert_eq!(row.id, "s1");
        let request = rx.recv().unwrap();
        assert!(request.contains("POST /api/trpc/mcpServers.create"));
        assert!(request.contains(
            r#"{"teamId":"team-1","name":"Linear","transport":"http","url":"https://mcp.linear.app/mcp","auth":"oauth","enabledByDefault":true}"#
        ));
    }

    /// An update is a PATCH: absent fields never reach the wire as `null`,
    /// which is what lets the server validate the MERGED row.
    #[test]
    fn update_sends_only_the_named_fields() {
        let (base, rx) = one_shot_server(
            200,
            r#"{"result":{"data":{"id":"s1","name":"Linear"}}}"#,
        );
        update(
            &client(&base),
            "s1",
            &McpServerFields {
                enabled_by_default: Some(false),
                ..Default::default()
            },
        )
        .expect("ok");
        let request = rx.recv().unwrap();
        assert!(request.contains("POST /api/trpc/mcpServers.update"));
        assert!(request.contains(r#"{"id":"s1","enabledByDefault":false}"#));
    }

    #[test]
    fn remove_posts_the_id() {
        let (base, rx) = one_shot_server(200, r#"{"result":{"data":{"ok":true}}}"#);
        remove(&client(&base), "s1").expect("ok");
        let request = rx.recv().unwrap();
        assert!(request.contains("POST /api/trpc/mcpServers.remove"));
        assert!(request.contains(r#"{"id":"s1"}"#));
    }

    #[test]
    fn config_parses_a_thin_row_with_defaults() {
        let row: McpServerConfig =
            serde_json::from_str(r#"{"id":"s1","name":"Linear"}"#).expect("parse");
        assert_eq!(row.transport, "http");
        assert_eq!(row.auth, "none");
        assert!(row.header_names.is_empty());
        assert!(row.is_http());
        assert!(!row.is_oauth());
    }

    #[test]
    fn secret_names_follow_the_transport() {
        let http = McpServerConfig {
            header_names: vec!["X-Api-Key".into()],
            env_names: vec!["IGNORED".into()],
            ..Default::default()
        };
        assert_eq!(http.secret_names(), ["X-Api-Key".to_string()]);
        let stdio = McpServerConfig {
            transport: "stdio".into(),
            header_names: vec!["IGNORED".into()],
            env_names: vec!["GITHUB_TOKEN".into()],
            ..Default::default()
        };
        assert_eq!(stdio.secret_names(), ["GITHUB_TOKEN".to_string()]);
    }
}
