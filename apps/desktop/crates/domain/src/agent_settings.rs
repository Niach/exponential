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

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        serde_json::from_str(AGENT_SETTINGS_JSON).unwrap()
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
}
