//! #EXP-1629: Settings → New agent (`NewAgentPane`), the shots team's
//! machines seeded into the headless store.
use super::*;
use crate::settings::new_agent::{NewAgentPane, NewAgentShot};
use serde_json::json;
use sync::protocol::RowKey;

const TEAM: &str = "team-shots";

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn device(id: &str, label: &str, icon: &str, kind: &str, online: bool, logins: bool) -> domain::rows::DeviceRow {
    let email = "dennis@example.com";
    let accounts = json!({"claude": {"signedIn": true, "email": email, "plan": "max", "checkedAt": now_iso(),
        "profiles": [{"id": "p1", "email": email, "plan": "max", "active": true, "signedIn": true, "health": "ok", "checkedAt": now_iso()}]}});
    serde_json::from_value(json!({
        "id": format!("row-{id}"), "user_id": "someone-else", "device_id": id, "label": label, "kind": kind, "icon": icon,
        "agents": ["claude"], "agent_accounts": if logins { accounts } else { json!(null) },
        "last_seen_at": if online { now_iso() } else { "2020-01-01T00:00:00Z".to_string() },
        "shared_team_ids": [TEAM],
    }))
    .expect("a device row")
}

/// Three machines: mint (online, signed in), a server, an offline laptop.
fn seed(cx: &mut gpui::App) {
    let collections = sync::Store::global(cx).collections().clone();
    for d in [device("mint", "mint", "monitor", "desktop", true, true), device("build", "build-server", "server", "server", true, false), device("macbook", "macbook", "laptop", "desktop", false, false)] {
        collections.devices.update(cx, |c, _| c.seed(RowKey::Single(d.id.clone()), d));
    }
}

fn triggers() -> Vec<serde_json::Value> {
    vec![
        json!({"id": "t1", "kind": "schedule", "cron": "0 9 * * 1-5", "tz": "Europe/Vienna", "enabled": true}),
        json!({"id": "t2", "kind": "schedule", "cron": "0 17 * * 5", "tz": "Europe/Vienna", "enabled": false}),
    ]
}

fn blank() -> NewAgentShot {
    NewAgentShot { name: "", instructions: "", device: None, triggers: Vec::new(), validate: false, pending: false }
}

fn filled() -> NewAgentShot {
    NewAgentShot { name: "Triage", instructions: "Triage every new issue on the board.\nLabel it, set its priority and ping the owner when it blocks a release.", device: Some("mint"), triggers: triggers(), ..blank() }
}

fn page(name: &str, shot: NewAgentShot, clicks: &'static [(f32, f32)], node: Option<&'static str>) {
    let settle = (!clicks.is_empty()).then_some(std::time::Duration::from_millis(1200));
    shoot_pane_acted(name, move |window, cx| {
        seed(cx);
        let mut pane = NewAgentPane::for_shots(window, cx);
        pane.show(shot, window, cx);
        pane
    }, |_, _| {}, settle, clicks, move |pane, window, cx| {
        if let Some(node) = node {
            pane.press_for_shots(node, window, cx);
        }
    });
}

#[test]
#[ignore]
fn settings_new_agent_blank() {
    page("new-agent-blank", blank(), &[], None);
}

#[test]
#[ignore]
fn settings_new_agent_filled() {
    page("new-agent-filled", filled(), &[], None);
}

#[test]
#[ignore]
fn settings_new_agent_invalid() {
    page("new-agent-invalid", NewAgentShot { validate: true, ..blank() }, &[], None);
}

#[test]
#[ignore]
fn settings_new_agent_no_login() {
    page("new-agent-no-login", NewAgentShot { device: Some("macbook"), ..filled() }, &[], None);
}

#[test]
#[ignore]
fn settings_new_agent_pending() {
    page("new-agent-pending", NewAgentShot { pending: true, ..filled() }, &[], None);
}

#[test]
#[ignore]
fn settings_new_agent_icon_open() {
    page("new-agent-icon-open", filled(), &[(54., 50.)], Some("icon_picker.trigger"));
}

#[test]
#[ignore]
fn settings_new_agent_color_open() {
    page("new-agent-color-open", filled(), &[(93., 50.)], Some("color_picker.trigger"));
}
