//! EXP-792: resolve a launch's team MCP server picks into the wire the agent
//! adapters render ([`crate::argv::McpServerWire`]) plus the spawn-env pairs
//! carrying the device-held secrets.
//!
//! Phase-0 STUB: the shape every consumer codes against. The resolver
//! itself (secret-store reads, OAuth refresh, blocker text) lands with the
//! device lane; until then an empty pick resolves to nothing and a non-empty
//! one refuses with a named blocker, so no launch can ever proceed with a
//! server it cannot authenticate to.

use std::path::Path;

use api::trpc::TrpcClient;

use crate::argv::McpServerWire;

/// The per-server env var prefix: `EXP_MCP_TOKEN_<n>` carries an OAuth
/// bearer token, `EXP_MCP_ENV_<n>_<NAME>` a typed header/env value, where
/// `<n>` is the server's 1-based position in the launch's pick.
pub const MCP_SERVER_ENV_PREFIX: &str = "EXP_MCP";

/// A launch's resolved MCP servers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ResolvedMcp {
    /// In pick order; `exponential` is NOT among them.
    pub servers: Vec<McpServerWire>,
    /// Spawn-env pairs (`EXP_MCP_TOKEN_1` → the token …). Every value here
    /// is a SECRET: it goes into the child env and the steer redactor, and
    /// nowhere else (never argv, never a config file, never a log).
    pub env: Vec<(String, String)>,
}

impl ResolvedMcp {
    /// The secret values, for the steer redactor.
    pub fn secret_values(&self) -> Vec<String> {
        self.env.iter().map(|(_, v)| v.clone()).collect()
    }
}

/// Why a pick cannot launch here — rendered as a launch blocker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct McpBlocker {
    /// The server's display name (or id when the config is unknown).
    pub server: String,
    /// Human sentence: "not signed in on this machine", "no value for
    /// header X-Api-Key on this machine", "unknown server".
    pub reason: String,
}

impl std::fmt::Display for McpBlocker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MCP server {}: {}", self.server, self.reason)
    }
}

/// Resolve `ids` (the launch's `mcp_server_ids`) for this machine.
///
/// `data_dir` + `account_id` locate the secret store; `trpc` fetches the
/// non-secret config (`mcpServers.listForDevice`). Returns the wire +
/// env, or the FIRST blocker.
pub fn resolve(
    _data_dir: &Path,
    _account_id: &str,
    _trpc: &TrpcClient,
    ids: &[String],
) -> Result<ResolvedMcp, McpBlocker> {
    if ids.is_empty() {
        return Ok(ResolvedMcp::default());
    }
    Err(McpBlocker {
        server: ids[0].clone(),
        reason: "MCP servers are not resolvable on this build yet".to_string(),
    })
}
