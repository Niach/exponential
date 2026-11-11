//! #EXP-1629: Settings → Coding CLIs (the agent tabs, autosaving controls).

use super::*;
use crate::settings::agents::{AgentsPane, AgentsShot};

fn agents_shot() -> AgentsShot {
    AgentsShot { tab: coding::CodingAgent::Claude, claude_path: "claude", claude_model: "opus", claude_effort: "", codex_model: "gpt-5.6-sol", codex_effort: "", ultracode: false, typed: None }
}

fn agents(name: &str, shot: AgentsShot) {
    shoot_pane_acted(name, AgentsPane::for_shots, |_, _| {}, None, &[], move |pane, window, cx| pane.show(shot, window, cx));
}

#[test]
#[ignore]
fn settings_agents_claude() {
    agents("agents-claude", agents_shot());
}

#[test]
#[ignore]
fn settings_agents_claude_set() {
    agents("agents-claude-set", AgentsShot { claude_path: "/opt/homebrew/bin/claude", claude_model: "fable", claude_effort: "high", ..agents_shot() });
}

#[test]
#[ignore]
fn settings_agents_ultracode() {
    agents("agents-ultracode", AgentsShot { ultracode: true, claude_effort: "max", ..agents_shot() });
}

#[test]
#[ignore]
fn settings_agents_typed() {
    agents("agents-typed", AgentsShot { typed: Some("~/.local/bin/claude-nightly"), ..agents_shot() });
}

#[test]
#[ignore]
fn settings_agents_codex() {
    agents("agents-codex", AgentsShot { tab: coding::CodingAgent::Codex, codex_model: "gpt-5.6-terra", codex_effort: "high", ..agents_shot() });
}
