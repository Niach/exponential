//! #EXP-1629: Settings › MCP (endpoint, servers, clients, access, the
//! built-in tools) and its Activity page, in every state.

use super::*;
use super::mcp_servers::rows as server_rows;
use crate::settings::mcp::{McpActivityPane, McpPane, McpShot};
use api::app_access::{AccessApp, AccessOut, AccessRow};
use api::mcp_clients::{McpActivityOut, McpCall, McpClient, McpClientsOut};

const TEAM: &str = "team-shots";

/// `minutes` ago: the rows' relative times stay the same across a run.
fn ago(minutes: i64) -> String {
    (chrono::Utc::now() - chrono::Duration::minutes(minutes)).to_rfc3339()
}

fn client(id: &str, name: &str, user: &str, kinds: &[&str], apps: Option<&str>, used: Option<i64>) -> McpClient {
    McpClient {
        client_id: id.into(),
        name: name.into(),
        user_id: format!("u-{user}"),
        user_name: user.into(),
        kinds: kinds.iter().map(|k| k.to_string()).collect(),
        apps_access: apps.map(str::to_string),
        last_used_at: used.map(ago),
        ..Default::default()
    }
}

fn clients() -> McpClientsOut {
    McpClientsOut {
        can_see_everyone: true,
        clients: vec![
            client("c1", "Claude Code", "Dennis", &["builtin", "apps"], Some("full"), Some(120)),
            client("c2", "Cursor", "Mia", &["apps"], Some("read"), None),
            client("c3", "A client whose registered name is long enough to need the ellipsis at the end of its row", "Robin", &["builtin"], None, Some(60 * 24 * 3)),
        ],
    }
}

fn app(id: &str, name: &str, icon: Option<&str>, color: Option<&str>, tools: u32) -> AccessApp {
    AccessApp { id: id.into(), slug: id.into(), name: name.into(), icon: icon.map(str::to_string), color: color.map(str::to_string), tool_count: tools }
}

fn member(id: &str, name: &str, email: &str, levels: &[(&str, &str)]) -> AccessRow {
    AccessRow { user_id: id.into(), name: name.into(), email: email.into(), image: None, role: "member".into(), levels: levels.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect() }
}

fn access(can_edit: bool) -> AccessOut {
    let rows = vec![
        member("u-dennis", "Dennis", "dennis@example.com", &[("a1", "read")]),
        member("u-mia", "Mia", "mia@example.com", &[("a1", "none"), ("a2", "read")]),
        member("u-x", "", "nameless@example.com", &[]),
    ];
    AccessOut {
        can_edit,
        apps: vec![app("a1", "Customer admin", Some("users"), Some("#22c55e"), 1), app("a2", "Working hours", None, None, 3)],
        rows: if can_edit { rows } else { rows.into_iter().take(1).collect() },
    }
}

fn base() -> McpShot {
    McpShot {
        team: Some(TEAM.into()),
        owner: true,
        endpoint: "https://app.exponential.at/api/mcp/apps".into(),
        scope: "mine",
        clients: Some(clients()),
        access: Some(access(true)),
        filter: "",
        revoking: false,
        servers: Some(Ok(server_rows())),
    }
}

/// No servers, clients or apps: the built-in tools come into the frame.
fn bare() -> McpShot {
    McpShot { clients: Some(McpClientsOut::default()), access: None, servers: Some(Ok(Vec::new())), ..base() }
}

fn page(name: &str, shot: McpShot) {
    shoot_pane(name, McpPane::for_shots, move |pane, cx| pane.show(shot, cx));
}

#[test]
#[ignore]
fn settings_mcp_owner() {
    page("mcp-owner", base());
}

#[test]
#[ignore]
fn settings_mcp_everyone() {
    page("mcp-everyone", McpShot { scope: "everyone", ..base() });
}

#[test]
#[ignore]
fn settings_mcp_member() {
    page("mcp-member", McpShot { owner: false, access: Some(access(false)), clients: Some(McpClientsOut { can_see_everyone: false, ..clients() }), ..base() });
}

#[test]
#[ignore]
fn settings_mcp_revoking() {
    page("mcp-revoking", McpShot { revoking: true, ..base() });
}

#[test]
#[ignore]
fn settings_mcp_loading() {
    page("mcp-loading", McpShot { clients: None, access: None, servers: None, ..base() });
}

#[test]
#[ignore]
fn settings_mcp_empty() {
    page("mcp-empty", bare());
}

#[test]
#[ignore]
fn settings_mcp_empty_member() {
    page("mcp-empty-member", McpShot { owner: false, ..bare() });
}

#[test]
#[ignore]
fn settings_mcp_filter() {
    page("mcp-filter", McpShot { filter: "label", ..bare() });
}

#[test]
#[ignore]
fn settings_mcp_no_match() {
    page("mcp-no-match", McpShot { filter: "zzz-no-tool", ..bare() });
}

#[test]
#[ignore]
fn settings_mcp_servers_failed_page() {
    // Its own picture name: Settings → MCP servers' pane bench saves
    // `mcp-servers-failed` too, and the later run overwrote the other.
    page("mcp-page-servers-failed", McpShot { servers: Some(Err("connection refused".into())), clients: Some(McpClientsOut::default()), access: None, ..base() });
}

#[test]
#[ignore]
fn settings_mcp_no_team() {
    page("mcp-no-team", McpShot { team: None, ..base() });
}

fn call(id: &str, tool: &str, status: &str, app: Option<&str>, user: &str, client: Option<&str>, minutes: i64) -> McpCall {
    McpCall { id: id.into(), tool: tool.into(), status: status.into(), error: None, app_name: app.map(str::to_string), user_name: user.into(), client_name: client.map(str::to_string), created_at: ago(minutes) }
}

fn calls(owner: bool) -> McpActivityOut {
    McpActivityOut {
        owner,
        calls: vec![
            call("k1", "customers_list", "ok", Some("Customer admin"), "Mia", Some("Claude Code"), 5),
            call("k2", "exponential_issues_create", "denied", None, "Dennis", None, 60),
            call("k3", "hours_log", "invalid", Some("Working hours"), "Robin", Some("Cursor"), 60 * 5),
            call("k4", "customers_refund_with_a_long_tool_name_that_has_to_truncate_somewhere_in_the_row_before_the_pill", "failed", Some("Customer admin"), "Mia", Some("Claude Code"), 60 * 24 * 2),
            call("k5", "stripe_charges", "upstream", Some("Customer admin"), "Dennis", Some("Claude Code"), 60 * 24 * 6),
        ],
    }
}

fn activity(name: &str, team: Option<&str>, data: Option<McpActivityOut>) {
    let team = team.map(str::to_string);
    shoot_pane(name, McpActivityPane::for_shots, move |pane, cx| pane.show(team, data, cx));
}

#[test]
#[ignore]
fn settings_mcp_activity_owner() {
    activity("mcp-activity-owner", Some(TEAM), Some(calls(true)));
}

#[test]
#[ignore]
fn settings_mcp_activity_member() {
    activity("mcp-activity-member", Some(TEAM), Some(calls(false)));
}

#[test]
#[ignore]
fn settings_mcp_activity_loading() {
    activity("mcp-activity-loading", Some(TEAM), None);
}

#[test]
#[ignore]
fn settings_mcp_activity_empty() {
    activity("mcp-activity-empty", Some(TEAM), Some(McpActivityOut::default()));
}

#[test]
#[ignore]
fn settings_mcp_activity_no_team() {
    activity("mcp-activity-no-team", None, None);
}

/// An owner's level menu open on Dennis's Customer admin row (no BEFORE:
/// today's gpui picker does not open headless).
#[test]
#[ignore]
fn settings_after_mcp_level_open() {
    shoot_pane_acted("mcp-level-open", McpPane::for_shots, |pane, cx| pane.show(base(), cx), Some(std::time::Duration::from_millis(1200)), &[], |pane, window, cx| {
        pane.press_for_shots("level.trigger.a1:u-dennis|level.a1:u-dennis.trigger", window, cx)
    });
}
