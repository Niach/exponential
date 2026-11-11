//! #EXP-1629: Settings → Agents, the index: the Lead Assistant row, the
//! team's agents, the team's Events.

use super::*;
use crate::settings::agents_index::{AgentsIndexPane, AgentsIndexShot};
use domain::session_tree::AgentRow;
use serde_json::json;

const TEAM: &str = "team-shots";

fn agent(id: &str, name: &str, icon: &str, color: &str, assistant: bool) -> AgentRow {
    AgentRow { agent_id: id.into(), team_id: TEAM.into(), name: name.into(), icon: icon.into(), color: color.into(), busy: false, needs_you: false, assistant }
}

fn agents() -> Vec<AgentRow> {
    vec![
        agent("a0", "Assistant", "bot", "#6366f1", true),
        agent("a1", "Triage", "bug", "#a855f7", false),
        agent("a2", "Release captain who reads every merged pull request and drafts the notes for the next train, then files what is missing", "rocket", "#22c55e", false),
    ]
}

/// `minutes` ago: the row's relative time stays the same across a run.
fn ago(minutes: i64) -> String {
    (chrono::Utc::now() - chrono::Duration::minutes(minutes)).to_rfc3339()
}

fn event(id: &str, name: &str, by: &str, session: Option<&str>, user: Option<&str>, minutes: i64) -> domain::rows::EventRow {
    serde_json::from_value(json!({"id": id, "team_id": TEAM, "name": name, "emitted_by": by, "session_id": session, "user_id": user, "created_at": ago(minutes)})).expect("an event row")
}

fn events() -> Vec<domain::rows::EventRow> {
    vec![
        event("e1", "issue.created", "server", None, None, 10),
        event("e2", "deploy.finished", "session", Some("s1"), None, 180),
        event("e3", "nightly.report", "user", None, Some("u1"), 60 * 24 * 2),
        event("e4", "orphaned.session", "session", Some("gone"), None, 60 * 24 * 3),
        event("e5", "a.very.long.event.name.that.an.agent.emitted.without.any.regard.for.the.width.of.this.row", "session", Some("s2"), None, 60 * 24 * 4),
    ]
}

fn map(rows: &[(&str, &str)]) -> std::collections::HashMap<String, String> {
    rows.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

fn base() -> AgentsIndexShot {
    AgentsIndexShot {
        team: Some(TEAM.into()),
        owner: true,
        lead: Some(None),
        members: vec![("u1".into(), "Dennis Example".into()), ("u2".into(), "Robin Example".into())],
        agents: agents(),
        agents_ready: true,
        events: events(),
        events_ready: true,
        titles: map(&[("s1", "Fix the build"), ("s2", "A session whose title is much longer than the emitter column could ever hope to show in full")]),
        names: map(&[("u1", "Dennis Example")]),
        open_lead: false,
    }
}

fn pane(name: &str, shot: AgentsIndexShot) {
    shoot_pane(name, AgentsIndexPane::for_shots, move |pane, cx| pane.show(shot, cx));
}

#[test]
#[ignore]
fn settings_agents_index_owner() {
    pane("agents-index-owner", base());
}

#[test]
#[ignore]
fn settings_agents_index_lead_set() {
    pane("agents-index-lead-set", AgentsIndexShot { lead: Some(Some("u1".into())), ..base() });
}

#[test]
#[ignore]
fn settings_agents_index_member() {
    pane("agents-index-member", AgentsIndexShot { owner: false, lead: Some(Some("u1".into())), ..base() });
}

/// `agents.lead` not read yet (or its read failed): the row is disabled.
#[test]
#[ignore]
fn settings_agents_index_lead_unread() {
    pane("agents-index-lead-unread", AgentsIndexShot { lead: None, ..base() });
}

#[test]
#[ignore]
fn settings_agents_index_empty() {
    pane("agents-index-empty", AgentsIndexShot { agents: Vec::new(), events: Vec::new(), ..base() });
}

#[test]
#[ignore]
fn settings_agents_index_empty_member() {
    pane("agents-index-empty-member", AgentsIndexShot { owner: false, agents: Vec::new(), events: Vec::new(), ..base() });
}

#[test]
#[ignore]
fn settings_agents_index_loading() {
    pane("agents-index-loading", AgentsIndexShot { agents: Vec::new(), agents_ready: false, events: Vec::new(), events_ready: false, ..base() });
}

/// Agents listed, the events still syncing.
#[test]
#[ignore]
fn settings_agents_index_events_loading() {
    pane("agents-index-events-loading", AgentsIndexShot { events: Vec::new(), events_ready: false, ..base() });
}

#[test]
#[ignore]
fn settings_agents_index_no_team() {
    pane("agents-index-no-team", AgentsIndexShot { team: None, ..base() });
}

/// The Lead Assistant menu open (no BEFORE: today's gpui picker does not
/// open headless).
#[test]
#[ignore]
fn settings_after_agents_index_lead_open() {
    pane("agents-index-lead-open", AgentsIndexShot { lead: Some(Some("u1".into())), open_lead: true, ..base() });
}
