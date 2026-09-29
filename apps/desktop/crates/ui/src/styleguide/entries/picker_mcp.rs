//! EXP-792 — the `picker-mcp` styleguide entry: the MCP server picker — a
//! catalog URL wears its brand mark (a hosted entry by host, a self-managed
//! GitLab by its `/api/v4/mcp` path at any host), a command server the
//! shell, an unconnected one reads "Connect first" and is never preselected.
//!
//! Filled by EXP-792; never edits `entries/mod.rs` nor the index.

use gpui::{App, Div, Window};

use super::picker::{chip, column, demo, inert};

pub(crate) const ID: &str = "picker-mcp";
pub(crate) const OWNER: &str = "EXP-792";

pub(crate) fn render(_window: &mut Window, _cx: &mut App) -> Div {
    column(vec![demo(
        "mcp — brand mark by catalog URL, the host (or command) under the name; an unconnected server says so",
        |window, cx| {
            let servers = vec![
                crate::picker::mcp_server_picker::McpPickerServer {
                    id: "mcp-1".to_string(),
                    name: "Linear".to_string(),
                    url: Some("https://mcp.linear.app/mcp".to_string()),
                    ..Default::default()
                },
                crate::picker::mcp_server_picker::McpPickerServer {
                    id: "mcp-2".to_string(),
                    name: "Acme GitLab".to_string(),
                    url: Some("https://git.acme.dev/api/v4/mcp".to_string()),
                    ..Default::default()
                },
                crate::picker::mcp_server_picker::McpPickerServer {
                    id: "mcp-3".to_string(),
                    name: "Sentry".to_string(),
                    url: Some("https://mcp.sentry.dev/mcp".to_string()),
                    description: Some("Connect first".to_string()),
                    ..Default::default()
                },
                crate::picker::mcp_server_picker::McpPickerServer {
                    id: "mcp-4".to_string(),
                    name: "Local tools".to_string(),
                    command: Some("npx -y @acme/mcp".to_string()),
                    ..Default::default()
                },
            ];
            crate::picker::mcp_server_picker::mcp_server_picker(
                &servers,
                vec!["mcp-1".to_string()],
                chip("sg-picker-mcp", "Linear", cx),
                inert(),
            )
            .id("sg-picker-mcp-surface")
            .render(window, cx)
        },
    )])
}
