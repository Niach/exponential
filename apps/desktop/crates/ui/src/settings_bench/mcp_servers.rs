//! #EXP-1629: Settings → MCP › Servers in every state (owner, member, a
//! Connect waiting + a Reconnect in flight, reloading, loading, failed,
//! empty, no team, a row's ⋯ menu open).
use super::*;
use crate::settings::mcp_servers::{McpServersPane, McpShot};
use api::mcp_servers::{McpConnection, McpServerConfig, McpServerDevice, McpServerListEntry};

fn entry(id: &str, name: &str, url: Option<&str>, auth: &str, status: &str) -> McpServerListEntry {
    McpServerListEntry {
        config: McpServerConfig { id: id.into(), team_id: "t".into(), name: name.into(), url: url.map(str::to_string), auth: auth.into(), ..Default::default() },
        connection: McpConnection { status: status.into(), expires_at: None, error: None },
        ..Default::default()
    }
}

fn rows() -> Vec<McpServerListEntry> {
    let mut linear = entry("s1", "Linear", Some("https://mcp.linear.app/mcp"), "oauth", "connected");
    linear.config.enabled_by_default = true;
    linear.shared = true;
    linear.used_by = vec!["Triage".into()];
    let notion = entry("s2", "Notion", Some("https://mcp.notion.com/mcp"), "oauth", "expired");
    let docs = entry("s3", "Internal docs", Some("https://mcp.docs.acme.dev/sse"), "oauth", "not_connected");
    let mut keys = entry("s4", "Stripe", Some("https://mcp.stripe.com"), "secret", "not_connected");
    keys.config.header_names = vec!["Authorization".into()];
    let mut files = entry("s5", "Filesystem", None, "none", "not_needed");
    files.config.transport = "stdio".into();
    files.config.command = Some("npx".into());
    files.config.args = vec!["-y".into(), "@modelcontextprotocol/server-filesystem".into(), "/srv/shared".into()];
    files.device = Some(McpServerDevice { id: "d1".into(), label: "studio-mac".into(), owner_id: "u".into(), kind: "desktop".into(), icon: None });
    vec![linear, notion, docs, keys, files]
}

fn mcp(name: &str, team: bool, owner: bool, load: Option<Result<Vec<McpServerListEntry>, String>>, reloading: bool, pending: Option<&'static str>, connecting: Option<&'static str>, clicks: &'static [(f32, f32)]) {
    let shot = McpShot { team: team.then(|| "t".to_string()), owner };
    shoot_pane_clicked(name, McpServersPane::for_shots, move |pane, cx| pane.show(shot, load, reloading, pending, connecting, cx), None, clicks);
}

#[test]
#[ignore]
fn settings_mcp_servers_owner() {
    mcp("mcp-servers-owner", true, true, Some(Ok(rows())), false, None, None, &[]);
}

#[test]
#[ignore]
fn settings_mcp_servers_member() {
    mcp("mcp-servers-member", true, false, Some(Ok(rows())), false, None, None, &[]);
}

#[test]
#[ignore]
fn settings_mcp_servers_busy() {
    mcp("mcp-servers-busy", true, true, Some(Ok(rows())), false, Some("s2"), Some("s3"), &[]);
}

#[test]
#[ignore]
fn settings_mcp_servers_reloading() {
    mcp("mcp-servers-reloading", true, true, Some(Ok(rows())), true, None, None, &[]);
}

#[test]
#[ignore]
fn settings_mcp_servers_loading() {
    mcp("mcp-servers-loading", true, true, None, false, None, None, &[]);
}

#[test]
#[ignore]
fn settings_mcp_servers_failed() {
    mcp("mcp-servers-failed", true, true, Some(Err("offline".into())), false, None, None, &[]);
}

#[test]
#[ignore]
fn settings_mcp_servers_empty() {
    mcp("mcp-servers-empty", true, true, Some(Ok(vec![])), false, None, None, &[]);
}

#[test]
#[ignore]
fn settings_mcp_servers_no_team() {
    mcp("mcp-servers-no-team", false, true, None, false, None, None, &[]);
}

#[test]
#[ignore]
fn settings_mcp_servers_menu_open() {
    mcp("mcp-servers-menu-open", true, true, Some(Ok(rows())), false, None, None, &[(MENU_X, MENU_Y)]);
}

/// The first row's ⋯ (window points), read off `mcp-servers-owner`.
const MENU_X: f32 = 709.;
const MENU_Y: f32 = 97.5;
