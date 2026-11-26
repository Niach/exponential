//! #EXP-1629: Settings → one agent's page (`AgentDetailPane`), its agent,
//! the team's machines and skills seeded into the headless store.
use super::*;
use crate::settings::agent_detail::AgentDetailPane;
use serde_json::json;
use sync::protocol::RowKey;

pub(super) const TEAM: &str = "team-shots";

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn accounts(email: &str) -> serde_json::Value {
    json!({"claude": {"signedIn": true, "email": email, "plan": "max", "checkedAt": now_iso(),
        "profiles": [{"id": "p1", "email": email, "plan": "max", "active": true, "signedIn": true, "health": "ok", "checkedAt": now_iso()}]}})
}

fn device(id: &str, label: &str, icon: &str, kind: &str, online: bool, logins: bool) -> domain::rows::DeviceRow {
    serde_json::from_value(json!({
        "id": format!("row-{id}"), "user_id": "someone-else", "device_id": id, "label": label, "kind": kind, "icon": icon,
        "agents": ["claude"], "agent_accounts": if logins { accounts("dennis@example.com") } else { json!(null) },
        "last_seen_at": if online { now_iso() } else { "2020-01-01T00:00:00Z".to_string() },
        "shared_team_ids": [TEAM],
    }))
    .expect("a device row")
}

fn skill(id: &str, name: &str, description: &str) -> domain::rows::SkillRow {
    serde_json::from_value(json!({"id": id, "team_id": TEAM, "name": name, "description": description})).expect("a skill row")
}

pub(super) fn agent(id: &str, role: &str, device: &str, triggers: serde_json::Value) -> domain::rows::AgentDefRow {
    serde_json::from_value(json!({
        "id": id, "team_id": TEAM, "name": if role == "assistant" { "Assistant" } else { "Triage" }, "icon": "bug", "color": "#a855f7",
        "triggers": triggers, "defaults": {"deviceId": device, "cli": "claude"}, "skill_ids": ["s-review", "s-triage"], "role": role,
    }))
    .expect("an agent row")
}

fn triggers() -> serde_json::Value {
    json!([
        // #EXP-1846: the Check in (the preset trigger) leads, outside the list.
        {"id": "t0", "kind": "schedule", "cron": "0 * * * *", "preset": "1h", "tz": "Europe/Vienna", "enabled": true},
        {"id": "t1", "kind": "schedule", "cron": "0 9 * * 1-5", "tz": "Europe/Vienna", "enabled": true},
        {"id": "t2", "kind": "schedule", "cron": "0 17 * * 5", "tz": "Europe/Vienna", "enabled": false},
    ])
}

/// Seeds the store: `agent`, three machines (mint online and signed in,
/// a server, an offline laptop) and two skills.
pub(super) fn seed(agent: domain::rows::AgentDefRow, cx: &mut gpui::App) {
    let collections = sync::Store::global(cx).collections().clone();
    collections.agents.update(cx, |c, _| c.seed(RowKey::Single(agent.id.clone()), agent));
    for d in [device("mint", "mint", "monitor", "desktop", true, true), device("build", "build-server", "server", "server", true, false), device("macbook", "macbook", "laptop", "desktop", false, false)] {
        collections.devices.update(cx, |c, _| c.seed(RowKey::Single(d.id.clone()), d));
    }
    for s in [skill("s-review", "review", "Run the checks, then summarise the diff."), skill("s-triage", "triage", "Label, prioritise and route new issues."), skill("s-release", "release-notes", "Draft the notes from the merged PRs.")] {
        collections.skills.update(cx, |c, _| c.seed(RowKey::Single(s.id.clone()), s));
    }
}

const INSTRUCTIONS: &str = "Triage every new issue on the board.\nLabel it, set its priority and ping the owner when it blocks a release.";

fn page(name: &str, agent: domain::rows::AgentDefRow, owner: bool, instructions: Option<Result<String, ()>>) {
    let id = agent.id.clone();
    shoot_pane_acted(name, move |window, cx| {
        seed(agent, cx);
        AgentDetailPane::for_shots(&id, window, cx)
    }, |_, _| {}, None, &[], move |pane, window, cx| pane.show(owner, instructions, window, cx));
}

/// The owner's page with one popup opened by a click at `at` (window
/// points; the same spot on today's pane and the document).
fn opened(name: &str, at: &'static [(f32, f32)], node: &'static str) {
    let agent = agent("a1", "agent", "mint", triggers());
    let id = agent.id.clone();
    shoot_pane_acted(name, move |window, cx| {
        seed(agent, cx);
        let mut pane = AgentDetailPane::for_shots(&id, window, cx);
        pane.show(true, Some(Ok(INSTRUCTIONS.into())), window, cx);
        pane
    }, |_, _| {}, Some(std::time::Duration::from_millis(1200)), at, move |pane, window, cx| pane.press_for_shots(node, window, cx));
}

#[test]
#[ignore]
fn settings_agent_icon_open() {
    opened("agent-icon-open", &[(54., 50.)], "icon_picker.trigger");
}

#[test]
#[ignore]
fn settings_agent_color_open() {
    opened("agent-color-open", &[(93., 50.)], "color_picker.trigger");
}

#[test]
#[ignore]
fn settings_agent_device_open() {
    opened("agent-device-open", &[(692., 98.)], "device.trigger");
}

#[test]
#[ignore]
fn settings_agent_skills_open() {
    opened("agent-skills-open", &[(680., 140.)], "skills.trigger");
}

#[test]
#[ignore]
fn settings_agent_account_open() {
    opened("agent-account-open", &[(640., 195.)], "account.trigger");
}

#[test]
#[ignore]
fn settings_agent_owner() {
    page("agent-owner", agent("a1", "agent", "mint", triggers()), true, Some(Ok(INSTRUCTIONS.into())));
}

#[test]
#[ignore]
fn settings_agent_member() {
    page("agent-member", agent("a1", "agent", "mint", triggers()), false, Some(Ok(INSTRUCTIONS.into())));
}

#[test]
#[ignore]
fn settings_agent_no_triggers() {
    page("agent-no-triggers", agent("a1", "agent", "mint", json!([])), true, Some(Ok(String::new())));
}

#[test]
#[ignore]
fn settings_agent_loading() {
    page("agent-loading", agent("a1", "agent", "mint", triggers()), true, None);
}

#[test]
#[ignore]
fn settings_agent_failed() {
    page("agent-failed", agent("a1", "agent", "mint", triggers()), true, Some(Err(())));
}

#[test]
#[ignore]
fn settings_agent_no_login() {
    page("agent-no-login", agent("a1", "agent", "macbook", triggers()), true, Some(Ok(INSTRUCTIONS.into())));
}

/// #EXP-1793 (agent-settings.json `mcpRow`): the MCP servers row under
/// Skills, the team's servers handed to the page (`mcpServers.list` is
/// server-only).
fn mcp_page(name: &str, agent: domain::rows::AgentDefRow, owner: bool) {
    let id = agent.id.clone();
    shoot_pane_acted(name, move |window, cx| {
        seed(agent, cx);
        AgentDetailPane::for_shots(&id, window, cx)
    }, |_, _| {}, None, &[], move |pane, window, cx| {
        let server = |id: &str, name: &str, url: &str, status: &str| -> api::mcp_servers::McpServerListEntry {
            serde_json::from_value(json!({"id": id, "teamId": TEAM, "name": name, "url": url, "auth": "oauth", "connection": {"status": status}})).expect("a server row")
        };
        let servers = vec![
            server("m-linear", "Linear", "https://mcp.linear.app/mcp", "connected"),
            server("m-sentry", "Sentry", "https://mcp.sentry.dev/mcp", "connected"),
            server("m-notion", "Notion", "https://mcp.notion.com/mcp", "not_connected"),
        ];
        pane.seed_mcp_for_shots(TEAM, servers, cx);
        pane.show(owner, Some(Ok(INSTRUCTIONS.into())), window, cx);
    });
}

fn mcp_agent() -> domain::rows::AgentDefRow {
    let mut agent = agent("a1", "agent", "mint", triggers());
    agent.mcp_server_ids = vec!["m-linear".into(), "m-sentry".into()];
    agent
}

#[test]
#[ignore]
fn settings_agent_mcp_owner() {
    mcp_page("agent-mcp-owner", mcp_agent(), true);
}

#[test]
#[ignore]
fn settings_agent_mcp_member() {
    mcp_page("agent-mcp-member", mcp_agent(), false);
}

#[test]
#[ignore]
fn settings_agent_assistant() {
    page("agent-assistant", agent("a0", "assistant", "mint", json!([])), true, Some(Ok(String::new())));
}

/// #EXP-1846: the Assistant's page with a Check in and its Voice loaded.
#[test]
#[ignore]
fn settings_agent_assistant_voice() {
    let check_in = json!([{"id": "t-ci", "kind": "schedule", "cron": "0 * * * *", "preset": "1h", "enabled": true, "tz": "Europe/Vienna"}]);
    let agent = agent("a0", "assistant", "mint", check_in);
    let id = agent.id.clone();
    shoot_pane_acted("agent-assistant-voice", move |window, cx| {
        seed(agent, cx);
        let mut pane = AgentDetailPane::for_shots(&id, window, cx);
        pane.show(true, Some(Ok(String::new())), window, cx);
        let settings = etalk::voice::VoiceSettings {
            voice_device_id: Some("mint".into()),
            agent_id: Some("a0".into()),
            ptt_shortcut: "ctrl+shift+space".into(),
            language: "de".into(),
            wake_word: true,
            ..Default::default()
        };
        // #EXP-1847: the page resolves the Assistant's voice against `voice.get`.
        pane.seed_member_voice_for_shots(settings.clone(), cx);
        pane.show_voice(crate::settings::voice_prefs::VoiceShot { settings, capture: None }, window, cx);
        pane
    }, |_, _| {}, Some(std::time::Duration::from_millis(1200)), &[], |_, _, _| {});
}

/// #EXP-1847 (agent-settings.json `voice`): a team agent with its own voice
/// (German, an uploaded clip, its own wake phrase).
#[test]
#[ignore]
fn settings_agent_voice() {
    let mut agent = agent("a1", "agent", "mint", triggers());
    agent.name = Some("Sloppy".into());
    agent.voice = domain::agent_settings::AgentVoice {
        language: Some("de".into()),
        clip_id: Some("c-1".into()),
        clip_name: Some("sloppy.wav".into()),
        transcript: Some("Hi, Sloppy here.".into()),
        wake_phrase: Some("Hey Sloppy".into()),
    };
    page("agent-voice", agent, true, Some(Ok(INSTRUCTIONS.into())));
}

// ── #EXP-2161: member spaces on Settings › Assistant › Runs on ───────────

/// The bench's member (headless has no account).
const ME: &str = "u-me";

/// Two teams, the member's laptop, two of Mira's servers opted in (unraid
/// online in Acme, build offline in both) and Sam's closed one; `space` =
/// the member's space on unraid (picked through Acme).
fn space_seed(space: bool, cx: &mut gpui::App) {
    crate::settings::agent_detail::SHOT_VIEWER.with(|viewer| *viewer.borrow_mut() = Some(ME.into()));
    let collections = sync::Store::global(cx).collections().clone();
    for team in [json!({"id": "t-acme", "name": "Acme"}), json!({"id": "t-side", "name": "Side project"})] {
        let team: domain::rows::Team = serde_json::from_value(team).expect("a team row");
        collections.teams.update(cx, |c, _| c.seed(RowKey::Single(team.id.clone()), team));
    }
    for user in [json!({"id": ME, "name": "Dennis"}), json!({"id": "u-mira", "name": "Mira Jensen"}), json!({"id": "u-sam", "name": "Sam"})] {
        let user: domain::rows::User = serde_json::from_value(user).expect("a user row");
        collections.users.update(cx, |c, _| c.seed(RowKey::Single(user.id.clone()), user));
    }
    let now = now_iso();
    let mut rows = vec![
        json!({"id": "row-mint", "user_id": ME, "device_id": "mint", "label": "mint", "kind": "desktop", "icon": "monitor", "agents": ["claude"], "agent_accounts": accounts("dennis@example.com"), "last_seen_at": now}),
        json!({"id": "row-unraid", "user_id": "u-mira", "device_id": "srv-unraid", "label": "unraid", "kind": "server", "icon": "server", "agents": ["claude"], "agent_accounts": accounts("mira@example.com"), "last_seen_at": now, "shared_team_ids": "{t-acme}", "assistant_team_ids": "{t-acme}"}),
        json!({"id": "row-build", "user_id": "u-mira", "device_id": "srv-build", "label": "build", "kind": "server", "icon": "server", "agents": ["claude"], "last_seen_at": "2020-01-01T00:00:00Z", "shared_team_ids": "{t-acme,t-side}", "assistant_team_ids": "{t-acme,t-side}"}),
        json!({"id": "row-closed", "user_id": "u-sam", "device_id": "srv-closed", "label": "closed", "kind": "server", "agents": ["claude"], "last_seen_at": now, "shared_team_ids": "{t-acme}"}),
    ];
    if space {
        rows.push(json!({"id": "row-space", "user_id": ME, "device_id": "space-1", "label": "unraid", "kind": "server", "agents": ["claude"], "last_seen_at": now, "space_host_id": "row-unraid", "space_team_id": "t-acme"}));
    }
    for row in rows {
        let row: domain::rows::DeviceRow = serde_json::from_value(row).expect("a device row");
        collections.devices.update(cx, |c, _| c.seed(RowKey::Single(row.id.clone()), row));
    }
}

/// The member's Assistant bound to `device` ("" = none).
fn space_assistant(device: &str) -> domain::rows::AgentDefRow {
    let mut agent = agent("a0", "assistant", device, json!([]));
    agent.owner_id = Some(ME.into());
    agent
}

fn space_page(name: &str, device: &'static str, space: bool, waiting: bool, press: Option<&'static str>) {
    let agent = space_assistant(device);
    let id = agent.id.clone();
    shoot_pane_acted(name, move |window, cx| {
        space_seed(space, cx);
        let collections = sync::Store::global(cx).collections().clone();
        collections.agents.update(cx, |c, _| c.seed(RowKey::Single(agent.id.clone()), agent));
        let mut pane = AgentDetailPane::for_shots(&id, window, cx);
        pane.show(true, Some(Ok(String::new())), window, cx);
        if waiting {
            pane.seed_space_waiting_for_shots("row-space", cx);
        }
        pane
    }, |_, _| {}, Some(std::time::Duration::from_millis(800)), &[], move |pane, window, cx| {
        if let Some(node) = press {
            pane.press_for_shots(node, window, cx);
        }
    });
}

/// Runs on open: the member's laptop, then Acme's and Side project's team
/// devices under their names (build offline, disabled).
#[test]
#[ignore]
fn settings_assistant_spaces_open() {
    space_page("desktop-assistant-runs-on-open", "mint", false, false, Some("assistant_device.trigger"));
}

/// Bound to the space on unraid, picked: its host's row reads selected.
#[test]
#[ignore]
fn settings_assistant_spaces_bound_open() {
    space_page("desktop-assistant-space-bound-open", "space-1", true, false, Some("assistant_device.trigger"));
}

/// The space waits for the member's approval: `copy.waiting` under the row.
#[test]
#[ignore]
fn settings_assistant_spaces_waiting() {
    space_page("desktop-assistant-space-waiting", "space-1", true, true, None);
}

/// The Assistant has no device (its space ended): `copy.noDevice`.
#[test]
#[ignore]
fn settings_assistant_spaces_no_device() {
    space_page("desktop-assistant-no-device", "", false, false, None);
}
