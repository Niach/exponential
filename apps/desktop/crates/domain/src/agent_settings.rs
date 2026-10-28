//! EXP-1294 (Threads v2): Team settings → Agents — the Rust twin of
//! `packages/domain-contract/src/agent-settings.ts` (iOS
//! `AgentSettings.swift`, Android `domain/AgentSettings.kt`), byte-locked by
//! `fixtures/agent-settings.json` (`summaryCases`, `validationCases`,
//! `accountValueCases`).
//!
//! An agent is managed exactly like a board: identity (icon + color + name),
//! Device, Account, Triggers (NEW-2: cron schedules + named events the
//! SERVER fires, one row each reading [`trigger_summary`]), Instructions,
//! Delete. The copy and the field list
//! come FROM the fixture ([`AGENT_SETTINGS`]); the caps from the contract
//! (`sessionsSend`).

use std::collections::BTreeMap;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::schedule_picker::schedule_summary;
use crate::contract::{
    SESSIONS_SEND_AGENT_INSTRUCTIONS_MAX_BYTES, SESSIONS_SEND_AGENT_NAME_MAX,
};

/// One field of the agent page, top to bottom.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSettingsField {
    /// `identity` | `device` | `account` | `triggers` | `instructions` |
    /// `delete`.
    pub key: String,
    pub label: String,
    /// `icon-color-name` | `device-picker` | `account-picker` |
    /// `trigger-list` | `markdown` | `danger`.
    pub control: String,
    pub required: bool,
    #[serde(default)]
    pub owner_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentErrorCopy {
    pub name_required: String,
    pub name_too_long: String,
    pub device_required: String,
    pub instructions_too_long: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSettingsCopy {
    /// The settings group / list title (`Agents`).
    pub title: String,
    /// `New agent`.
    pub new_agent: String,
    pub name_placeholder: String,
    pub instructions_placeholder: String,
    /// The Schedule row's `+ Add`.
    pub add_trigger: String,
    /// The Schedule row with no trigger.
    pub no_schedule: String,
    pub paused: String,
    pub delete_confirm: String,
    /// Event trigger text by builtin event name (contract
    /// `events.builtinNames`).
    pub events: BTreeMap<String, String>,
    /// Any other event: `On {name}`.
    pub event: String,
    pub errors: AgentErrorCopy,
    /// The Account row with no login reported (`Last used`, muted).
    pub account_last_used: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AgentSettingsTestIds {
    pub list: String,
    pub page: String,
}

/// `fixtures/agent-settings.json` minus its cases.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSettingsSpec {
    pub fields: Vec<AgentSettingsField>,
    pub copy: AgentSettingsCopy,
    pub test_ids: AgentSettingsTestIds,
}

const AGENT_SETTINGS_JSON: &str =
    include_str!("../../../../../packages/domain-contract/fixtures/agent-settings.json");

/// The agent page's fields, copy and test ids.
pub static AGENT_SETTINGS: LazyLock<AgentSettingsSpec> = LazyLock::new(|| {
    serde_json::from_str(AGENT_SETTINGS_JSON).expect("agent-settings.json parses")
});

/// One Triggers row's text for an `AgentTrigger` jsonb (NEW-2): a schedule
/// reads the schedule picker's [`schedule_summary`] (`Every day at 09:00`,
/// ` ({tz})` when zoned), a builtin event its copy, any
/// other event `On {name}`; a disabled trigger appends ` · Paused`.
pub fn trigger_summary(trigger: &Value) -> String {
    let copy = &AGENT_SETTINGS.copy;
    let text_of = |key: &str| trigger.get(key).and_then(Value::as_str).unwrap_or_default();
    let text = if text_of("kind") == "schedule" {
        schedule_summary(text_of("cron"), Some(text_of("tz")))
    } else {
        let name = text_of("name");
        copy.events
            .get(name)
            .cloned()
            .unwrap_or_else(|| copy.event.replacen("{name}", name, 1))
    };
    if trigger.get("enabled").and_then(Value::as_bool) == Some(false) {
        format!("{text} · {}", copy.paused)
    } else {
        text
    }
}

/// The agent form as the page edits it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AgentForm {
    pub name: String,
    pub device_id: Option<String>,
    pub instructions: String,
}

/// One field error (`field` = an [`AgentSettingsField::key`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentFormError {
    pub field: String,
    pub message: String,
}

/// Field errors in field order; empty = valid.
pub fn validate_agent_form(form: &AgentForm) -> Vec<AgentFormError> {
    let errors = &AGENT_SETTINGS.copy.errors;
    let mut out = Vec::new();
    let error = |field: &str, message: &str| AgentFormError {
        field: field.to_string(),
        message: message.to_string(),
    };
    let name = form.name.trim();
    if name.is_empty() {
        out.push(error("identity", &errors.name_required));
    } else if name.chars().count() > SESSIONS_SEND_AGENT_NAME_MAX {
        out.push(error("identity", &errors.name_too_long));
    }
    if form.device_id.as_deref().is_none_or(str::is_empty) {
        out.push(error("device", &errors.device_required));
    }
    if form.instructions.len() > SESSIONS_SEND_AGENT_INSTRUCTIONS_MAX_BYTES {
        out.push(error("instructions", &errors.instructions_too_long));
    }
    out
}

/// One login the device reports for the Account picker.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentAccountOption {
    pub id: String,
    /// The agent CLI (`claude` | `codex`).
    pub agent: String,
    pub email: String,
    pub is_last_used: bool,
}

/// What the Account row reads: the agent's brand mark + this email.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentAccountValue {
    pub agent: String,
    pub email: String,
}

/// EXP-1306: the Account row's value = the composer chip's (the agent mark +
/// the email): the pinned `account` (under `cli` when set), else `cli`'s
/// first login, else the last used one, else the first; `None` = no login
/// reported (the row reads `copy.account_last_used`, muted).
pub fn agent_account_value(
    options: &[AgentAccountOption],
    account: Option<&str>,
    cli: Option<&str>,
) -> Option<AgentAccountValue> {
    let pinned = account.and_then(|account| {
        options
            .iter()
            .find(|option| option.id == account && cli.is_none_or(|cli| option.agent == cli))
    });
    let picked = pinned
        .or_else(|| cli.and_then(|cli| options.iter().find(|option| option.agent == cli)))
        .or_else(|| options.iter().find(|option| option.is_last_used))
        .or_else(|| options.first())?;
    Some(AgentAccountValue {
        agent: picked.agent.clone(),
        email: picked.email.clone(),
    })
}

/// #EXP-1383: a synced agent row as an actor / assignee resolves it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AgentActorRow {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
}

impl AgentActorRow {
    /// The actor row of a synced `agents` definition (the Agents list's
    /// name rule).
    pub fn of(row: &crate::rows::AgentDefRow) -> Self {
        Self {
            id: row.id.clone(),
            name: row.name.clone().unwrap_or_default(),
            icon: row.icon.clone(),
            color: row.color.clone(),
        }
    }
}

/// #EXP-1383 (`actor.actorCases`): who an issue event / comment / issue
/// creation reads as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IssueActor {
    Agent(AgentActorRow),
    User(String),
}

/// #EXP-1383 (TS `issueActor`): a synced agent first, else the member, else
/// `None` (the server / a widget reporter).
pub fn issue_actor(
    actor_agent_id: Option<&str>,
    actor_user_id: Option<&str>,
    agents: &[AgentActorRow],
) -> Option<IssueActor> {
    if let Some(agent) = actor_agent_id
        .filter(|id| !id.is_empty())
        .and_then(|id| agents.iter().find(|row| row.id == id))
    {
        return Some(IssueActor::Agent(agent.clone()));
    }
    actor_user_id
        .filter(|id| !id.is_empty())
        .map(|id| IssueActor::User(id.to_string()))
}

/// The id prefix of an agent assignee option (`assignee.optionCases`).
pub const ASSIGNEE_AGENT_PREFIX: &str = "agent:";

/// #EXP-1383 (TS `assigneeOptions`): the assignee picker's option ids ×4:
/// `unassigned`, the members in the given order, then the team's agents by
/// sortOrder then name.
pub fn assignee_options(member_ids: &[String], agents: &[(String, String, f64)]) -> Vec<String> {
    let mut sorted: Vec<&(String, String, f64)> = agents.iter().collect();
    sorted.sort_by(|a, b| {
        a.2.partial_cmp(&b.2)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.to_lowercase().cmp(&b.1.to_lowercase()))
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.0.cmp(&b.0))
    });
    std::iter::once("unassigned".to_string())
        .chain(member_ids.iter().map(|id| format!("user:{id}")))
        .chain(sorted.into_iter().map(|(id, _, _)| format!("{ASSIGNEE_AGENT_PREFIX}{id}")))
        .collect()
}

// ---------------------------------------------------------------------------
// S-3 (owner 2026-10-28): THE ASSISTANT (`assistant` in the fixture; contract
// `agentRole`). Replays `scheduleCases`, `visibleCases`, `orderCases`.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantCopy {
    pub title: String,
    pub name: String,
    pub runs_on: String,
    pub check_in: String,
    pub voice: String,
    pub no_device: String,
    pub delete_refused: String,
    pub schedule_refused: String,
}

/// One Check in preset (`off` = no trigger, `cron` None).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AssistantSchedule {
    pub id: String,
    pub label: String,
    pub cron: Option<String>,
}

/// `fixtures/agent-settings.json` `assistant` minus its cases.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantSpec {
    pub copy: AssistantCopy,
    /// The Assistant page's sections top to bottom.
    pub sections: Vec<String>,
    pub schedules: Vec<AssistantSchedule>,
    pub default_schedule: String,
    pub brief: String,
}

/// The Assistant page's copy, sections and Check in presets.
pub static ASSISTANT: LazyLock<AssistantSpec> = LazyLock::new(|| {
    let fixture: Value = serde_json::from_str(AGENT_SETTINGS_JSON).expect("agent-settings.json parses");
    serde_json::from_value(fixture["assistant"].clone()).expect("agent-settings.json assistant parses")
});

/// TS `assistantScheduleOf`: the preset id stored triggers stand for
/// (`off` = none or a paused preset); None = not an Assistant's (an event,
/// a custom cron, more than one) and `agents.update` refuses it.
pub fn assistant_schedule_of(triggers: &[Value]) -> Option<String> {
    match triggers {
        [] => Some("off".to_string()),
        [trigger] => {
            if trigger["kind"].as_str() != Some("schedule") {
                return None;
            }
            let cron = trigger["cron"].as_str()?;
            let preset = ASSISTANT
                .schedules
                .iter()
                .find(|schedule| schedule.cron.as_deref() == Some(cron))?;
            if trigger["enabled"].as_bool() == Some(false) {
                Some("off".to_string())
            } else {
                Some(preset.id.clone())
            }
        }
        _ => None,
    }
}

/// TS `assistantScheduleTriggers`: the triggers a preset saves as (`off` =
/// none).
pub fn assistant_schedule_triggers(id: &str, tz: Option<&str>) -> Vec<Value> {
    let Some(cron) = ASSISTANT
        .schedules
        .iter()
        .find(|schedule| schedule.id == id)
        .and_then(|schedule| schedule.cron.clone())
    else {
        return Vec::new();
    };
    let mut trigger = serde_json::json!({ "kind": "schedule", "cron": cron, "enabled": true });
    if let Some(tz) = tz.filter(|tz| !tz.is_empty()) {
        trigger["tz"] = Value::String(tz.to_string());
    }
    vec![trigger]
}

/// TS `isAssistant`.
pub fn is_assistant_role(role: Option<&str>) -> bool {
    role == Some(crate::contract::AGENT_ROLE_ASSISTANT)
}

/// TS `visibleAgent`: another member's Assistant is never listed
/// (settings, composer, chats).
pub fn visible_agent(role: Option<&str>, owner_id: Option<&str>, me: Option<&str>) -> bool {
    !is_assistant_role(role) || (owner_id.is_some() && owner_id == me)
}

/// TS `orderedAgents` over synced rows: the viewer's Assistant first, then
/// the team agents by sortOrder, then name; others' Assistants dropped.
pub fn ordered_agents<'a>(
    agents: impl IntoIterator<Item = &'a crate::rows::AgentDefRow>,
    me: Option<&str>,
) -> Vec<&'a crate::rows::AgentDefRow> {
    let mut visible: Vec<&crate::rows::AgentDefRow> = agents
        .into_iter()
        .filter(|agent| visible_agent(Some(agent.role.as_str()), agent.owner_id.as_deref(), me))
        .collect();
    visible.sort_by(|a, b| {
        (!a.is_assistant())
            .cmp(&!b.is_assistant())
            .then_with(|| {
                a.sort_order
                    .unwrap_or(0.0)
                    .partial_cmp(&b.sort_order.unwrap_or(0.0))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                let (an, bn) = (a.name.as_deref().unwrap_or(""), b.name.as_deref().unwrap_or(""));
                an.to_lowercase().cmp(&bn.to_lowercase()).then_with(|| an.cmp(bn))
            })
    });
    visible
}

/// The viewer's own Assistant among synced rows (one per team).
pub fn my_assistant<'a>(
    agents: impl IntoIterator<Item = &'a crate::rows::AgentDefRow>,
    team_id: &str,
    me: Option<&str>,
) -> Option<&'a crate::rows::AgentDefRow> {
    let me = me?;
    agents.into_iter().find(|agent| {
        agent.is_assistant() && agent.team_id.as_deref() == Some(team_id) && agent.owner_id.as_deref() == Some(me)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        serde_json::from_str(AGENT_SETTINGS_JSON).unwrap()
    }


    #[test]
    fn actor_cases_match_the_fixture() {
        let fixture = fixture();
        let cases = fixture["actor"]["actorCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let agents: Vec<AgentActorRow> = serde_json::from_value(case["agents"].clone()).unwrap();
            let actual = issue_actor(
                case["input"]["actorAgentId"].as_str(),
                case["input"]["actorUserId"].as_str(),
                &agents,
            );
            let actual = match actual {
                None => Value::Null,
                Some(IssueActor::User(id)) => serde_json::json!({"kind": "user", "id": id}),
                Some(IssueActor::Agent(agent)) => serde_json::json!({
                    "kind": "agent", "id": agent.id, "name": agent.name, "icon": agent.icon, "color": agent.color,
                }),
            };
            assert_eq!(actual, case["expected"], "{name}");
        }
    }

    #[test]
    fn assignee_option_cases_match_the_fixture() {
        let fixture = fixture();
        let cases = fixture["assignee"]["optionCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        assert_eq!(fixture["assignee"]["copy"]["unassigned"].as_str(), Some("Unassigned"));
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let members: Vec<String> = case["input"]["members"]
                .as_array()
                .unwrap()
                .iter()
                .map(|member| member["id"].as_str().unwrap().to_string())
                .collect();
            let agents: Vec<(String, String, f64)> = case["input"]["agents"]
                .as_array()
                .unwrap()
                .iter()
                .map(|agent| {
                    (
                        agent["id"].as_str().unwrap().to_string(),
                        agent["name"].as_str().unwrap().to_string(),
                        agent["sortOrder"].as_f64().unwrap(),
                    )
                })
                .collect();
            let expected: Vec<String> = serde_json::from_value(case["expected"].clone()).unwrap();
            assert_eq!(assignee_options(&members, &agents), expected, "{name}");
        }
    }
    #[test]
    fn the_spec_parses_from_the_fixture() {
        let keys: Vec<&str> = AGENT_SETTINGS.fields.iter().map(|field| field.key.as_str()).collect();
        assert_eq!(keys, ["identity", "device", "account", "triggers", "instructions", "delete"]);
        assert!(AGENT_SETTINGS.fields.last().unwrap().owner_only);
        assert_eq!(AGENT_SETTINGS.copy.new_agent, "New agent");
        // NEW-2: every builtin event name has its copy.
        for name in crate::contract::EVENT_BUILTIN_NAMES {
            assert!(AGENT_SETTINGS.copy.events.contains_key(*name), "{name}");
        }
    }

    #[test]
    fn trigger_summary_matches_the_fixture() {
        let fixture = fixture();
        let cases = fixture["summaryCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            assert_eq!(
                trigger_summary(&case["trigger"]),
                case["expected"].as_str().unwrap(),
                "{}",
                case["trigger"]
            );
        }
    }

    #[test]
    fn validate_agent_form_matches_the_fixture() {
        let fixture = fixture();
        let cases = fixture["validationCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let form = &case["form"];
            let form = AgentForm {
                name: form["name"].as_str().unwrap_or_default().to_string(),
                device_id: form["deviceId"].as_str().map(str::to_string),
                instructions: form["instructions"].as_str().unwrap_or_default().to_string(),
            };
            let expected: Vec<AgentFormError> = serde_json::from_value(case["expected"].clone()).unwrap();
            assert_eq!(validate_agent_form(&form), expected, "{}", case["name"]);
        }
    }

    #[test]
    fn agent_account_value_matches_the_fixture() {
        assert_eq!(AGENT_SETTINGS.copy.account_last_used, "Last used");
        let fixture = fixture();
        let cases = fixture["accountValueCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let input = &case["input"];
            let options: Vec<AgentAccountOption> = serde_json::from_value(input["options"].clone()).unwrap();
            let expected: Option<AgentAccountValue> = serde_json::from_value(case["expected"].clone()).unwrap();
            assert_eq!(
                agent_account_value(&options, input["account"].as_str(), input["cli"].as_str()),
                expected,
                "{}",
                case["name"]
            );
        }
    }

    #[test]
    fn assistant_schedule_cases_match_the_fixture() {
        let fixture = fixture();
        let cases = fixture["assistant"]["scheduleCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let triggers = case["triggers"].as_array().unwrap();
            let expected = case["expected"].as_str().map(str::to_string);
            assert_eq!(assistant_schedule_of(triggers), expected, "{}", case["name"]);
        }
        assert_eq!(ASSISTANT.default_schedule, "1h");
        assert_eq!(ASSISTANT.copy.check_in, "Check in");
        assert_eq!(ASSISTANT.sections, ["identity", "runs-on", "check-in", "voice", "instructions"]);
    }

    #[test]
    fn assistant_schedule_triggers_round_trip() {
        assert!(assistant_schedule_triggers("off", None).is_empty());
        for schedule in &ASSISTANT.schedules {
            let triggers = assistant_schedule_triggers(&schedule.id, Some("Europe/Vienna"));
            assert_eq!(assistant_schedule_of(&triggers).as_deref(), Some(schedule.id.as_str()));
        }
    }

    #[test]
    fn assistant_visible_cases_match_the_fixture() {
        let fixture = fixture();
        let cases = fixture["assistant"]["visibleCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let agent = &case["agent"];
            let actual = visible_agent(agent["role"].as_str(), agent["ownerId"].as_str(), case["me"].as_str());
            assert_eq!(actual, case["expected"].as_bool().unwrap(), "{}", case["name"]);
        }
    }

    #[test]
    fn assistant_order_cases_match_the_fixture() {
        let fixture = fixture();
        let cases = fixture["assistant"]["orderCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let rows: Vec<crate::rows::AgentDefRow> = case["agents"]
                .as_array()
                .unwrap()
                .iter()
                .map(|agent| {
                    serde_json::from_value(serde_json::json!({
                        "id": agent["id"], "role": agent["role"], "owner_id": agent["ownerId"],
                        "name": agent["name"], "sort_order": agent["sortOrder"], "team_id": "t",
                    }))
                    .unwrap()
                })
                .collect();
            let ids: Vec<&str> = ordered_agents(&rows, case["me"].as_str()).iter().map(|row| row.id.as_str()).collect();
            let expected: Vec<&str> = case["expected"].as_array().unwrap().iter().map(|id| id.as_str().unwrap()).collect();
            assert_eq!(ids, expected, "{}", case["name"]);
            assert_eq!(my_assistant(&rows, "t", case["me"].as_str()).map(|row| row.id.as_str()), Some("me"));
        }
    }

    #[test]
    fn a_row_without_a_role_is_a_team_agent() {
        let row: crate::rows::AgentDefRow = serde_json::from_value(serde_json::json!({ "id": "a" })).unwrap();
        assert_eq!(row.role, "agent");
        assert!(!row.is_assistant());
        let row: crate::rows::AgentDefRow =
            serde_json::from_value(serde_json::json!({ "id": "a", "role": null })).unwrap();
        assert_eq!(row.role, "agent");
    }

}
