//! On-disk MCP config the agent CLI reads — a port of the `claudeMcpServers` /
//! `codexOptionsForMcp` helpers from `drivers/{claude,codex}.ts`, but written as
//! files for the CLI (the desktop runs the CLI, not the SDK): claude's
//! `.mcp.json` and codex's `config.toml` fragment, both pointing the
//! `exponential` server at `/api/mcp` with the agent's bearer token.

use serde_json::json;

/// claude `--mcp-config` JSON: the exponential MCP server over HTTP + bearer.
pub fn claude_mcp_json(mcp_url: &str, token: &str) -> String {
    let v = json!({
        "mcpServers": {
            "exponential": {
                "type": "http",
                "url": mcp_url,
                "headers": { "Authorization": format!("Bearer {token}") }
            }
        }
    });
    serde_json::to_string_pretty(&v).unwrap_or_default()
}

/// Write `{cwd}/.mcp.json` (0600, since it holds the bearer token). Returns the path.
pub fn write_claude_mcp_json(cwd: &str, mcp_url: &str, token: &str) -> Result<String, String> {
    let path = format!("{cwd}/.mcp.json");
    std::fs::write(&path, claude_mcp_json(mcp_url, token)).map_err(|e| format!("write .mcp.json: {e}"))?;
    chmod_600(&path);
    Ok(path)
}

/// codex `config.toml` MCP fragment. The token is passed via the
/// `EXPONENTIAL_MCP_TOKEN` env var (set in the run's env) rather than inlined.
pub fn codex_config_toml(mcp_url: &str) -> String {
    format!(
        "[mcp_servers.exponential]\nurl = \"{mcp_url}\"\nbearer_token_env_var = \"EXPONENTIAL_MCP_TOKEN\"\n"
    )
}

fn chmod_600(path: &str) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_json_has_server_and_bearer() {
        let s = claude_mcp_json("https://x.at/api/mcp", "expk_abc");
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        let srv = &v["mcpServers"]["exponential"];
        assert_eq!(srv["url"], "https://x.at/api/mcp");
        assert_eq!(srv["type"], "http");
        assert_eq!(srv["headers"]["Authorization"], "Bearer expk_abc");
    }

    #[test]
    fn codex_toml_uses_env_var_for_token() {
        let t = codex_config_toml("https://x.at/api/mcp");
        assert!(t.contains("[mcp_servers.exponential]"));
        assert!(t.contains("url = \"https://x.at/api/mcp\""));
        assert!(t.contains("bearer_token_env_var = \"EXPONENTIAL_MCP_TOKEN\""));
        // The raw token is never written to the codex config.
        assert!(!t.contains("expk_"));
    }
}
