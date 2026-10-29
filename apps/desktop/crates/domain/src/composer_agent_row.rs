//! S-3 (owner 2026-10-28): the New-chat composer's AGENT ROW, the Rust twin
//! of `packages/domain-contract/src/composer-agent-row.ts`, locked by
//! `fixtures/composer-home.json` `agentRow.cases` (the `$comment` holds the
//! rules). The circles: the viewer's Assistant first (brand mark), the team
//! agents they see, then the dotted Temporary-session circle.

use std::sync::LazyLock;

use serde::Deserialize;
use serde_json::Value;

use crate::agent_settings::ordered_agents;
use crate::rows::AgentDefRow;

const COMPOSER_HOME_JSON: &str =
    include_str!("../../../../../packages/domain-contract/fixtures/composer-home.json");

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ComposerAgentRowCopy {
    /// `To {name}`.
    pub to: String,
    pub temporary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ComposerAgentRowTestIds {
    pub row: String,
    /// `composer-agent-{id}`.
    pub option: String,
    pub label: String,
}

/// `composer-home.json` `agentRow` minus its cases.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposerAgentRowSpec {
    pub size: f32,
    pub ring_width: f32,
    pub ring_gap: f32,
    pub gap: f32,
    pub label_to_row: f32,
    pub row_to_card: f32,
    /// The dotted circle's icon concept (`ui-icon-placeholder`).
    pub temporary_icon: String,
    /// The dotted circle's option id (`temporary`).
    pub temporary_id: String,
    pub copy: ComposerAgentRowCopy,
    pub test_ids: ComposerAgentRowTestIds,
}

pub static COMPOSER_AGENT_ROW: LazyLock<ComposerAgentRowSpec> = LazyLock::new(|| {
    let fixture: Value = serde_json::from_str(COMPOSER_HOME_JSON).expect("composer-home.json parses");
    serde_json::from_value(fixture["agentRow"].clone()).expect("composer-home.json agentRow parses")
});

/// The row's state for a viewer (TS `composerAgentRow`).
#[derive(Debug, Clone, PartialEq)]
pub struct ComposerAgentRowState<'a> {
    /// The agent circles in order (the dotted circle follows them).
    pub agents: Vec<&'a AgentDefRow>,
    /// Option ids in order, the temporary id last.
    pub options: Vec<String>,
    /// The selected option id (never empty).
    pub selected: String,
    /// The picked agent; None = Temporary session.
    pub agent: Option<&'a AgentDefRow>,
    /// The label line over the circles.
    pub label: String,
}

impl ComposerAgentRowState<'_> {
    /// Temporary session picked (the + menu's `temporary` condition).
    pub fn temporary(&self) -> bool {
        self.agent.is_none()
    }
}

/// `picked` = the last tap (None / unknown = the default: the Assistant,
/// else Temporary session).
pub fn composer_agent_row<'a>(
    agents: impl IntoIterator<Item = &'a AgentDefRow>,
    me: Option<&str>,
    picked: Option<&str>,
) -> ComposerAgentRowState<'a> {
    let spec = &*COMPOSER_AGENT_ROW;
    let agents = ordered_agents(agents, me);
    let mut options: Vec<String> = agents.iter().map(|agent| agent.id.clone()).collect();
    options.push(spec.temporary_id.clone());
    let fallback = agents
        .iter()
        .find(|agent| agent.is_assistant())
        .map(|agent| agent.id.clone())
        .unwrap_or_else(|| spec.temporary_id.clone());
    let selected = match picked {
        Some(picked) if options.iter().any(|option| option == picked) => picked.to_string(),
        _ => fallback,
    };
    let agent = agents.iter().copied().find(|agent| agent.id == selected);
    let label = match agent {
        Some(agent) => spec.copy.to.replace("{name}", agent.name.as_deref().unwrap_or("")),
        None => spec.copy.temporary.clone(),
    };
    ComposerAgentRowState {
        agents,
        options,
        selected,
        agent,
        label,
    }
}

/// The pick a page mount starts on (TS `seedPick`, `agentRow.seedCases`):
/// a seed with a subject (issues, a prompt, a device) is a LAUNCH, so the
/// row starts on Temporary session; anything else = None (the default).
pub fn seed_pick(has_issues: bool, has_prompt: bool, has_device: bool) -> Option<String> {
    (has_issues || has_prompt || has_device).then(|| COMPOSER_AGENT_ROW.temporary_id.clone())
}

/// The kind of subject the composer holds, for [`pick_for_subject`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposerSubjectKind {
    Issues,
    Prompt,
    Other,
}

/// The pick after the composer's subject changed (TS `pickForSubject`,
/// `agentRow.subjectCases`): an issue or prompt subject is a launch, so the
/// row follows to Temporary session; any other subject keeps `current`.
pub fn pick_for_subject(subject: ComposerSubjectKind, current: Option<&str>) -> Option<String> {
    match subject {
        ComposerSubjectKind::Issues | ComposerSubjectKind::Prompt => Some(COMPOSER_AGENT_ROW.temporary_id.clone()),
        ComposerSubjectKind::Other => current.map(str::to_string),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_spec_parses_from_the_fixture() {
        let spec = &*COMPOSER_AGENT_ROW;
        assert_eq!(spec.size, 40.0);
        assert_eq!(spec.temporary_icon, "ui-icon-placeholder");
        assert_eq!(spec.temporary_id, "temporary");
    }

    #[test]
    fn agent_row_cases_match_the_fixture() {
        let fixture: Value = serde_json::from_str(COMPOSER_HOME_JSON).unwrap();
        let cases = fixture["agentRow"]["cases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let rows: Vec<AgentDefRow> = case["agents"]
                .as_array()
                .unwrap()
                .iter()
                .map(|agent| {
                    serde_json::from_value(serde_json::json!({
                        "id": agent["id"], "role": agent["role"], "owner_id": agent["ownerId"],
                        "name": agent["name"], "sort_order": agent["sortOrder"],
                    }))
                    .unwrap()
                })
                .collect();
            let state = composer_agent_row(&rows, case["me"].as_str(), case["picked"].as_str());
            let expected = &case["expected"];
            let options: Vec<&str> = expected["options"].as_array().unwrap().iter().map(|o| o.as_str().unwrap()).collect();
            assert_eq!(state.options, options, "{}", case["name"]);
            assert_eq!(state.selected, expected["selected"].as_str().unwrap(), "{}", case["name"]);
            assert_eq!(state.label, expected["label"].as_str().unwrap(), "{}", case["name"]);
            assert_eq!(state.temporary(), state.selected == "temporary");
        }
    }

    #[test]
    fn agent_row_seed_cases_match_the_fixture() {
        let fixture: Value = serde_json::from_str(COMPOSER_HOME_JSON).unwrap();
        let cases = fixture["agentRow"]["seedCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let seed = &case["seed"];
            let has_issues = seed["issueIds"].as_array().is_some_and(|ids| !ids.is_empty());
            let pick = seed_pick(has_issues, seed["promptId"].is_string(), seed["deviceId"].is_string());
            assert_eq!(pick.as_deref(), case["expected"].as_str(), "{}", case["name"]);
        }
    }

    #[test]
    fn agent_row_subject_cases_match_the_fixture() {
        let fixture: Value = serde_json::from_str(COMPOSER_HOME_JSON).unwrap();
        let cases = fixture["agentRow"]["subjectCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let subject = match case["subject"].as_str() {
                Some("issues") => ComposerSubjectKind::Issues,
                Some("prompt") => ComposerSubjectKind::Prompt,
                _ => ComposerSubjectKind::Other,
            };
            let pick = pick_for_subject(subject, case["current"].as_str());
            assert_eq!(pick.as_deref(), case["expected"].as_str(), "{}", case["name"]);
        }
    }
}
