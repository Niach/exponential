//! #EXP-1629: Settings → one agent's page (`AgentDetailPane`), its agent,
//! the team's machines and skills seeded into the headless store.
use super::*;
use crate::settings::agent_detail::AgentDetailPane;
use serde_json::json;
use sync::protocol::RowKey;

const TEAM: &str = "team-shots";

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
        {"id": "t1", "kind": "schedule", "cron": "0 9 * * 1-5", "tz": "Europe/Vienna", "enabled": true},
        {"id": "t2", "kind": "schedule", "cron": "0 17 * * 5", "tz": "Europe/Vienna", "enabled": false},
    ])
}

/// Seeds the store: `agent`, three machines (mint online and signed in,
/// a server, an offline laptop) and two skills.
fn seed(agent: domain::rows::AgentDefRow, cx: &mut gpui::App) {
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

#[test]
#[ignore]
fn settings_agent_assistant() {
    page("agent-assistant", agent("a0", "assistant", "mint", json!([])), true, Some(Ok(String::new())));
}
