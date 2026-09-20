//! EXP-891: typed `deviceMcpServers.*` tRPC helpers — the device side of
//! PER-DEVICE MCP servers (beside the team registry in
//! [`crate::mcp_servers`]). The machine is the writer: `sync` replaces the
//! server's copy of THIS device's whole set with what the local store holds
//! ([`coding::device_mcp_servers`]). Config only — a url or a command +
//! args — never a header value, a token or an env value.
//!
//! Shapes mirror `apps/web/src/lib/trpc/device-mcp-servers.ts`.

use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::trpc::TrpcClient;

/// One row of the machine's set as `deviceMcpServers.sync` takes it.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceMcpServerInput {
    pub name: String,
    /// `http` | `stdio`.
    pub transport: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// `detected` | `manual`.
    pub source: String,
    /// `claude` | `codex` for a detected row; absent for a typed one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    pub enabled: bool,
}

/// `deviceMcpServers.sync` — replace this device's set. Returns how many rows
/// the server now holds for it.
pub fn sync(
    trpc: &TrpcClient,
    device_id: &str,
    servers: &[DeviceMcpServerInput],
) -> Result<usize, ApiError> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Input<'a> {
        device_id: &'a str,
        servers: &'a [DeviceMcpServerInput],
    }
    #[derive(Deserialize)]
    struct Ok {
        #[serde(default)]
        count: usize,
    }
    let ok: Ok = trpc.mutation("deviceMcpServers.sync", &Input { device_id, servers })?;
    Ok(ok.count)
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

    /// The wire is `{deviceId, servers: [...]}` with absent optionals left
    /// off — the router's zod schema takes `nullish` there, and an empty
    /// `args` is the stdio default.
    #[test]
    fn sync_posts_the_set_under_the_device_id() {
        let (base, rx) = one_shot_server(200, r#"{"result":{"data":{"ok":true,"count":2}}}"#);
        let count = sync(
            &client(&base),
            "dev-1",
            &[
                DeviceMcpServerInput {
                    name: "linear".into(),
                    transport: "http".into(),
                    url: Some("https://mcp.linear.app/mcp".into()),
                    source: "detected".into(),
                    agent: Some("claude".into()),
                    enabled: true,
                    ..Default::default()
                },
                DeviceMcpServerInput {
                    name: "acme".into(),
                    transport: "stdio".into(),
                    command: Some("npx".into()),
                    args: vec!["-y".into(), "@acme/mcp".into()],
                    source: "manual".into(),
                    enabled: false,
                    ..Default::default()
                },
            ],
        )
        .expect("ok");
        assert_eq!(count, 2);
        let request = rx.recv().unwrap();
        assert!(request.contains("POST /api/trpc/deviceMcpServers.sync"));
        assert!(request.contains(
            r#"{"deviceId":"dev-1","servers":[{"name":"linear","transport":"http","url":"https://mcp.linear.app/mcp","source":"detected","agent":"claude","enabled":true},{"name":"acme","transport":"stdio","command":"npx","args":["-y","@acme/mcp"],"source":"manual","enabled":false}]}"#
        ), "{request}");
    }

    #[test]
    fn sync_of_an_empty_set_clears_the_device() {
        let (base, rx) = one_shot_server(200, r#"{"result":{"data":{"ok":true,"count":0}}}"#);
        assert_eq!(sync(&client(&base), "dev-1", &[]).expect("ok"), 0);
        let request = rx.recv().unwrap();
        assert!(request.contains(r#"{"deviceId":"dev-1","servers":[]}"#));
    }
}
