//! EXP-792: resolve a launch's team MCP server picks into the wire the agent
//! adapters render ([`crate::argv::McpServerWire`]) plus the spawn-env pairs
//! carrying the member's credential values.
//!
//! The SERVER holds every credential, per member ([`api::mcp_servers`]):
//! this machine holds none and runs no OAuth. At spawn the launcher asks
//! `mcpServers.resolveForLaunch` for the pick; the server refreshes an
//! expiring OAuth token first and answers with the values. Nothing here logs
//! a value, and every value that reaches a child process does so through its
//! environment, never argv or a config file ([`ResolvedMcp::env`] → the spawn
//! env + the steer redactor).
//!
//! **A pick never blocks a launch.** A server the member has not connected
//! (or whose refresh failed, or that the team removed) is SKIPPED and named
//! in [`ResolvedMcp::warnings`]; so is the whole pick when the resolve call
//! itself fails. The run starts without those tools.
//!
//! **Mid-run expiry is ACCEPTED and SURFACED, never brokered (EXP-808 —
//! decided, do not reopen).** A token is resolved ONCE, at spawn, and handed
//! to the agent as an env value; nothing can rotate it while the run is
//! alive. A resume re-resolves, so it picks up a fresh token.

use api::mcp_servers::{resolve_for_launch, McpLaunchResolution, McpLaunchServer};
use api::trpc::TrpcClient;

use crate::argv::{McpServerWire, McpWireTransport};

/// The per-server env var prefix: `EXP_MCP_TOKEN_<n>` carries an OAuth
/// bearer token, `EXP_MCP_ENV_<n>_<NAME>` a typed header value, where `<n>`
/// is the server's 1-based position among the launch's resolved servers.
pub const MCP_SERVER_ENV_PREFIX: &str = "EXP_MCP";

/// The config key the launcher's own MCP entry owns (`mcpServers.exponential`
/// / `mcp_servers.exponential`); a team server may not fold to it.
pub const RESERVED_CONFIG_KEY: &str = "exponential";

/// Env names a stdio server's secret may NOT set (case-insensitive): the
/// ones that steer the agent, its toolchain, the shell or this launcher.
/// The name comes from a team row an owner wrote, and it lands in the spawn
/// env of the AGENT itself, so `PATH`/`LD_PRELOAD`/`ANTHROPIC_BASE_URL`
/// would hijack the run. Defense in depth over the server's own validation.
const RESERVED_ENV_NAMES: &[&str] = &["PATH", "HOME", "SHELL", "USER", "TMPDIR", "PWD"];
const RESERVED_ENV_PREFIXES: &[&str] = &[
    "LD_", "DYLD_", "NODE_", "ANTHROPIC_", "CLAUDE_", "CODEX_", "OPENAI_", "EXP_", "GIT_", "BUN_",
];

/// Whether a stdio server may carry its secret in env var `name`: not
/// reserved ([`RESERVED_ENV_NAMES`] / [`RESERVED_ENV_PREFIXES`]) and a
/// well-formed name (non-empty, no `=` or NUL).
pub fn is_allowed_env_name(name: &str) -> bool {
    if name.is_empty() || name.contains(['=', '\0']) {
        return false;
    }
    let upper = name.to_ascii_uppercase();
    !RESERVED_ENV_NAMES.contains(&upper.as_str())
        && !RESERVED_ENV_PREFIXES
            .iter()
            .any(|prefix| upper.starts_with(prefix))
}

/// A launch's resolved MCP servers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ResolvedMcp {
    /// In pick order; `exponential` is NOT among them.
    pub servers: Vec<McpServerWire>,
    /// Spawn-env pairs (`EXP_MCP_TOKEN_1` → the token …). Every value here
    /// is a SECRET: it goes into the child env and the steer redactor, and
    /// nowhere else (never argv, never a config file, never a log).
    pub env: Vec<(String, String)>,
    /// Launch-time notes: every picked server the run starts WITHOUT (not
    /// connected, refresh failed, removed, the resolve call failed) and the
    /// server's own notes (a token that will not outlive the run). Never a
    /// secret and never a blocker.
    pub warnings: Vec<String>,
}

impl ResolvedMcp {
    /// The secret values, for the steer redactor.
    pub fn secret_values(&self) -> Vec<String> {
        self.env.iter().map(|(_, v)| v.clone()).collect()
    }
}

// ---------------------------------------------------------------------------
// Env var names
// ---------------------------------------------------------------------------

/// `EXP_MCP_TOKEN_<n>` — the n-th resolved server's OAuth bearer.
pub fn token_env_name(position: usize) -> String {
    format!("{MCP_SERVER_ENV_PREFIX}_TOKEN_{position}")
}

/// `EXP_MCP_ENV_<n>_<NAME>` — a typed http header value; `NAME` is the
/// header name upper-cased with everything non-alphanumeric folded to `_`.
pub fn value_env_name(position: usize, name: &str) -> String {
    format!("{MCP_SERVER_ENV_PREFIX}_ENV_{position}_{}", env_suffix(name))
}

/// `X-Api-Key` → `X_API_KEY`.
pub fn env_suffix(name: &str) -> String {
    let folded: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect();
    if folded.is_empty() {
        "VALUE".to_string()
    } else {
        folded
    }
}

// ---------------------------------------------------------------------------
// resolve
// ---------------------------------------------------------------------------

/// Resolve `ids` (the launch's `mcp_server_ids`) through
/// `mcpServers.resolveForLaunch` FOR the run `session_id` (EXP-1140: the
/// server hands out only that row's own persisted pick, so the row exists
/// first and the pick rode `codingSessions.start`). An empty pick resolves to
/// nothing without touching the network; a failed call degrades to a warning
/// and no servers.
pub fn resolve(trpc: &TrpcClient, ids: &[String], session_id: &str) -> ResolvedMcp {
    if ids.is_empty() {
        return ResolvedMcp::default();
    }
    match resolve_for_launch(trpc, ids, session_id) {
        Ok(resolution) => resolve_with(&resolution, ids),
        Err(error) => ResolvedMcp {
            warnings: vec![format!(
                "Could not load your MCP server credentials ({}); starting without the picked MCP servers.",
                error.user_message()
            )],
            ..ResolvedMcp::default()
        },
    }
}

/// [`resolve`] against an already-fetched resolution: servers in pick order
/// (`ids`), every value moved into [`ResolvedMcp::env`] behind a `${VAR}`
/// reference, `skipped` + `warnings` folded into [`ResolvedMcp::warnings`].
pub fn resolve_with(resolution: &McpLaunchResolution, ids: &[String]) -> ResolvedMcp {
    let mut resolved = ResolvedMcp::default();
    let mut servers: Vec<&McpLaunchServer> = resolution.servers.iter().collect();
    // Pick order; anything the server returned outside the pick sorts last.
    servers.sort_by_key(|server| ids.iter().position(|id| id == &server.id).unwrap_or(usize::MAX));
    for server in servers {
        // `exponential` is the launcher's own entry in every rendered
        // config; a team server folding to that key would be dropped
        // silently by the renderers, so skip it up front.
        if McpServerWire::config_key(&server.name) == RESERVED_CONFIG_KEY {
            resolved.warnings.push(format!(
                "MCP server {}: the name is reserved; starting without it.",
                server.name
            ));
            continue;
        }
        // A secret env name that would override the agent's own
        // environment skips the WHOLE server (never a partial one).
        if let Some(bad) = server.env.iter().find(|pair| !is_allowed_env_name(&pair.name)) {
            log::warn!(
                "[mcp] skipping MCP server {}: env name {:?} is reserved",
                server.name,
                bad.name
            );
            resolved.warnings.push(format!(
                "MCP server {}: its key's env name {} is reserved; starting without it.",
                server.name, bad.name
            ));
            continue;
        }
        let position = resolved.servers.len() + 1;
        let transport = if server.is_http() {
            McpWireTransport::Http {
                url: server.url.clone().unwrap_or_default(),
            }
        } else {
            McpWireTransport::Stdio {
                command: server.command.clone().unwrap_or_default(),
                args: server.args.clone(),
            }
        };
        let mut wire = McpServerWire {
            id: server.id.clone(),
            name: McpServerWire::config_key(&server.name),
            transport,
            headers: Vec::new(),
            token_env: None,
            env: Vec::new(),
        };
        for header in &server.headers {
            let bearer = header
                .value
                .strip_prefix("Bearer ")
                .filter(|_| header.name.eq_ignore_ascii_case("authorization"));
            match bearer {
                // An OAuth bearer: codex reads it via `bearer_token_env_var`.
                Some(token) if wire.token_env.is_none() => {
                    let var = token_env_name(position);
                    wire.headers
                        .push((header.name.clone(), format!("Bearer ${{{var}}}")));
                    wire.token_env = Some(var.clone());
                    resolved.env.push((var, token.to_string()));
                }
                _ => {
                    let var = value_env_name(position, &header.name);
                    wire.headers.push((header.name.clone(), format!("${{{var}}}")));
                    resolved.env.push((var, header.value.clone()));
                }
            }
        }
        for pair in &server.env {
            // A stdio server's env NAME is the var itself: the launcher sets
            // `<NAME>=value` and the config says `${NAME}` (codex forwards
            // named vars verbatim).
            wire.env.push((pair.name.clone(), format!("${{{}}}", pair.name)));
            resolved.env.push((pair.name.clone(), pair.value.clone()));
        }
        resolved.servers.push(wire);
    }
    for skipped in &resolution.skipped {
        let name = if skipped.name.is_empty() {
            &skipped.id
        } else {
            &skipped.name
        };
        let reason = if skipped.reason.is_empty() {
            "not available"
        } else {
            skipped.reason.as_str()
        };
        resolved.warnings.push(format!(
            "MCP server {name}: {reason}; starting without it. Connect it in Settings → MCP servers."
        ));
    }
    resolved
        .warnings
        .extend(resolution.warnings.iter().cloned());
    resolved
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::canned_server_recording;
    use api::mcp_servers::{McpNamedValue, McpSkippedServer};
    use api::StaticToken;
    use std::sync::Arc;

    fn named(name: &str, value: &str) -> McpNamedValue {
        McpNamedValue {
            name: name.into(),
            value: value.into(),
        }
    }

    fn http(id: &str, name: &str, headers: Vec<McpNamedValue>) -> McpLaunchServer {
        McpLaunchServer {
            id: id.into(),
            name: name.into(),
            transport: "http".into(),
            url: Some("https://mcp.example.com/mcp".into()),
            command: None,
            args: Vec::new(),
            headers,
            env: Vec::new(),
        }
    }

    #[test]
    fn reserved_env_names_are_refused_case_insensitively() {
        for name in [
            "PATH", "path", "HOME", "Shell", "USER", "TMPDIR", "PWD", "LD_PRELOAD",
            "dyld_insert_libraries", "NODE_OPTIONS", "ANTHROPIC_BASE_URL", "ANTHROPIC_API_KEY",
            "CLAUDE_CONFIG_DIR", "CODEX_HOME", "OPENAI_API_KEY", "EXP_MCP_TOKEN_1",
            "GIT_ASKPASS", "git_ssh_command", "BUN_OPTIONS", "", "A=B", "A\0B",
        ] {
            assert!(!is_allowed_env_name(name), "{name:?} must be refused");
        }
        for name in ["GITHUB_TOKEN", "SENTRY_AUTH_TOKEN", "LINEAR_API_KEY", "PATHFINDER_KEY", "USERNAME"] {
            assert!(is_allowed_env_name(name), "{name:?} must pass");
        }
    }

    #[test]
    fn a_stdio_server_with_a_reserved_env_name_is_skipped_whole() {
        let hijack = McpLaunchServer {
            id: "s-bad".into(),
            name: "Evil".into(),
            transport: "stdio".into(),
            url: None,
            command: Some("npx".into()),
            args: Vec::new(),
            headers: Vec::new(),
            env: vec![named("GITHUB_TOKEN", "ok"), named("ld_preload", "/tmp/x.so")],
        };
        let fine = McpLaunchServer {
            id: "s-ok".into(),
            name: "GitHub".into(),
            transport: "stdio".into(),
            url: None,
            command: Some("npx".into()),
            args: Vec::new(),
            headers: Vec::new(),
            env: vec![named("GITHUB_TOKEN", "ghp_x")],
        };
        let resolution = McpLaunchResolution {
            servers: vec![hijack, fine],
            ..Default::default()
        };
        let resolved = resolve_with(&resolution, &["s-bad".into(), "s-ok".into()]);
        assert_eq!(resolved.servers.len(), 1);
        assert_eq!(resolved.servers[0].id, "s-ok");
        assert_eq!(resolved.env, vec![("GITHUB_TOKEN".to_string(), "ghp_x".to_string())]);
        assert!(!resolved.env.iter().any(|(name, _)| name.eq_ignore_ascii_case("LD_PRELOAD")));
        assert_eq!(resolved.warnings.len(), 1);
        assert!(resolved.warnings[0].contains("Evil"), "{:?}", resolved.warnings);
        assert!(resolved.warnings[0].contains("reserved"), "{:?}", resolved.warnings);
        assert!(!resolved.warnings[0].contains("/tmp/x.so"));
    }

    #[test]
    fn env_var_names_follow_the_contract() {
        assert_eq!(token_env_name(1), "EXP_MCP_TOKEN_1");
        assert_eq!(value_env_name(2, "X-Api-Key"), "EXP_MCP_ENV_2_X_API_KEY");
        assert_eq!(env_suffix("  "), "VALUE");
    }

    #[test]
    fn empty_pick_resolves_to_nothing_without_a_fetch() {
        // An unreachable base proves nothing is fetched (a fetch would warn).
        let trpc = TrpcClient::new("http://127.0.0.1:1", Arc::new(StaticToken("t".into())));
        assert_eq!(resolve(&trpc, &[], "sess-1"), ResolvedMcp::default());
    }

    #[test]
    fn resolve_builds_wires_and_env_in_pick_order() {
        let stdio = McpLaunchServer {
            id: "s-stdio".into(),
            name: "GitHub".into(),
            transport: "stdio".into(),
            url: None,
            command: Some("npx".into()),
            args: vec!["-y".into(), "server".into()],
            headers: Vec::new(),
            env: vec![named("GITHUB_TOKEN", "ghp_x")],
        };
        // The server's order is NOT the pick's: the pick wins.
        let resolution = McpLaunchResolution {
            servers: vec![
                http("s-oauth", "Linear", vec![named("Authorization", "Bearer at-1")]),
                stdio,
                http("s-none", "Docs", Vec::new()),
                http("s-http", "Sentry Bridge", vec![named("X-Api-Key", "key-1")]),
            ],
            ..Default::default()
        };
        let ids: Vec<String> = ["s-http", "s-oauth", "s-none", "s-stdio"]
            .iter()
            .map(|id| id.to_string())
            .collect();
        let resolved = resolve_with(&resolution, &ids);
        assert_eq!(resolved.servers.len(), 4);
        let http = &resolved.servers[0];
        assert_eq!(http.name, "sentry_bridge");
        assert_eq!(
            http.headers,
            vec![("X-Api-Key".to_string(), "${EXP_MCP_ENV_1_X_API_KEY}".to_string())]
        );
        assert_eq!(http.token_env, None);
        let oauth = &resolved.servers[1];
        assert_eq!(oauth.name, "linear");
        assert_eq!(
            oauth.headers,
            vec![("Authorization".to_string(), "Bearer ${EXP_MCP_TOKEN_2}".to_string())]
        );
        assert_eq!(oauth.token_env.as_deref(), Some("EXP_MCP_TOKEN_2"));
        let docs = &resolved.servers[2];
        assert!(docs.headers.is_empty() && docs.env.is_empty() && docs.token_env.is_none());
        let stdio = &resolved.servers[3];
        assert_eq!(
            stdio.transport,
            McpWireTransport::Stdio {
                command: "npx".into(),
                args: vec!["-y".into(), "server".into()]
            }
        );
        assert_eq!(
            stdio.env,
            vec![("GITHUB_TOKEN".to_string(), "${GITHUB_TOKEN}".to_string())]
        );
        assert_eq!(
            resolved.env,
            vec![
                ("EXP_MCP_ENV_1_X_API_KEY".to_string(), "key-1".to_string()),
                ("EXP_MCP_TOKEN_2".to_string(), "at-1".to_string()),
                ("GITHUB_TOKEN".to_string(), "ghp_x".to_string()),
            ]
        );
        assert_eq!(resolved.secret_values(), vec!["key-1", "at-1", "ghp_x"]);
        assert!(resolved.warnings.is_empty());
        // No secret ever lands in the wire.
        let wire = serde_json::to_string(&resolved.servers).unwrap();
        for secret in ["key-1", "at-1", "ghp_x"] {
            assert!(!wire.contains(secret), "{wire}");
        }
    }

    /// Skipped picks, the server's own notes and a reserved name all become
    /// warnings — never a blocker, never a secret.
    #[test]
    fn skips_and_server_notes_become_warnings() {
        let resolution = McpLaunchResolution {
            servers: vec![
                http("s-x", "Exponential", vec![named("X-Api-Key", "k")]),
                http("s-ok", "Docs", Vec::new()),
            ],
            skipped: vec![
                McpSkippedServer {
                    id: "s-1".into(),
                    name: "Linear".into(),
                    reason: "not connected".into(),
                },
                McpSkippedServer {
                    id: "s-gone".into(),
                    name: String::new(),
                    reason: String::new(),
                },
            ],
            warnings: vec!["sentry: access token expires in 40 min".into()],
        };
        let ids: Vec<String> = ["s-x", "s-ok", "s-1", "s-gone"].iter().map(|id| id.to_string()).collect();
        let resolved = resolve_with(&resolution, &ids);
        assert_eq!(resolved.servers.len(), 1);
        assert_eq!(resolved.servers[0].name, "docs");
        assert!(resolved.env.is_empty(), "the reserved server's value never lands");
        assert_eq!(
            resolved.warnings,
            vec![
                "MCP server Exponential: the name is reserved; starting without it.".to_string(),
                "MCP server Linear: not connected; starting without it. Connect it in Settings → MCP servers.".to_string(),
                "MCP server s-gone: not available; starting without it. Connect it in Settings → MCP servers.".to_string(),
                "sentry: access token expires in 40 min".to_string(),
            ]
        );
    }

    #[test]
    fn resolve_posts_the_pick_and_degrades_a_failed_call_to_a_warning() {
        let body = serde_json::json!({"result":{"data":{
            "servers":[{"id":"s","name":"Docs","transport":"http","url":"https://docs.example.com/mcp","args":[],"headers":[],"env":[]}],
            "skipped":[],"warnings":[]
        }}})
        .to_string();
        let (base, captured) = canned_server_recording(vec![(200, body)]);
        let trpc = TrpcClient::new(&base, Arc::new(StaticToken("t".into())));
        let resolved = resolve(&trpc, &["s".to_string()], "sess-1");
        assert_eq!(resolved.servers[0].name, "docs");
        assert!(resolved.env.is_empty() && resolved.warnings.is_empty());
        let requests = captured.lock().unwrap();
        assert!(requests[0].contains("POST /api/trpc/mcpServers.resolveForLaunch"));
        assert!(requests[0].contains(r#"{"serverIds":["s"]}"#));
        // EXP-1140: the run it is for rides as the session header.
        assert!(
            requests[0].to_ascii_lowercase().contains("x-exp-session-id: sess-1"),
            "{}",
            requests[0]
        );

        let dead = TrpcClient::new("http://127.0.0.1:1", Arc::new(StaticToken("t".into())));
        let degraded = resolve(&dead, &["s".to_string()], "sess-1");
        assert!(degraded.servers.is_empty() && degraded.env.is_empty());
        assert_eq!(degraded.warnings.len(), 1);
        assert!(degraded.warnings[0].starts_with("Could not load your MCP server credentials"));
    }
}
