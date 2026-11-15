//! EXP-1294: Team settings → Agents — the Rust twin of
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
    /// #EXP-1846 (`page`): the section headers of the one agent page.
    pub triggers: String,
    pub runs_on: String,
    pub check_in: String,
    /// #EXP-1850: an app agent's record trigger (`When a {record} is {op}`)
    /// and the op words.
    pub record: String,
    pub record_ops: BTreeMap<String, String>,
    /// #EXP-1850: `When webhook {name} is called`.
    pub webhook: String,
    /// #EXP-1850: a `run` trigger's second line (`Runs {tool}`), or a
    /// hand-off's (`Hands off to {agent}`).
    pub runs: String,
    pub hands_off: String,
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

/// #EXP-1793 (agent-settings.json `mcpRow`): the agent page's MCP servers
/// row, right under Skills.
pub const MCP_ROW_LABEL: &str = "MCP servers";
pub const MCP_ROW_NONE: &str = "None";
/// The most servers an agent may name (`mcpRow.max`).
pub const MCP_ROW_MAX: usize = 20;

/// The MCP servers row's value (web `agentMcpServersValue`): the picked
/// servers' names (team list order) through the shared picker summary
/// (`a` / `a, b` / `a, b +N`), `None` when empty.
pub fn agent_mcp_servers_value(names: &[&str]) -> String {
    match names.len() {
        0 => MCP_ROW_NONE.to_string(),
        1 | 2 => names.join(", "),
        n => format!("{} +{}", names[..2].join(", "), n - 2),
    }
}

/// One Triggers row's text for an `AgentTrigger` jsonb (NEW-2): a schedule
/// reads the schedule picker's [`schedule_summary`] (`Every day at 09:00`,
/// ` ({tz})` when zoned), a builtin event its copy, any
/// other event `On {name}`; #EXP-1850: a record trigger `copy.record`, a
/// webhook `copy.webhook`; a disabled trigger appends ` · Paused`.
pub fn trigger_summary(trigger: &Value) -> String {
    let copy = &AGENT_SETTINGS.copy;
    let text_of = |key: &str| trigger.get(key).and_then(Value::as_str).unwrap_or_default();
    let text = if text_of("kind") == "schedule" {
        schedule_summary(text_of("cron"), Some(text_of("tz")))
    } else if text_of("kind") == "record" {
        let op = copy.record_ops.get(text_of("op")).map(String::as_str).unwrap_or(text_of("op"));
        copy.record.replacen("{record}", text_of("record"), 1).replacen("{op}", op, 1)
    } else if text_of("kind") == "webhook" {
        copy.webhook.replacen("{name}", text_of("name"), 1)
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

/// #EXP-1850 (TS `runLabel`): an app binding in a few words:
/// `records.<op> <kind>`, the action's name, or `<upstream>/<tool>`.
pub fn run_label(run: &Value) -> String {
    let text = |value: &Value, key: &str| value.get(key).and_then(Value::as_str).unwrap_or_default().to_string();
    if let Some(records) = run.get("records").filter(|records| records.is_object()) {
        return format!("records.{} {}", text(records, "op"), text(records, "kind"));
    }
    if let Some(action) = run.get("action").and_then(Value::as_str) {
        return action.to_string();
    }
    if let Some(upstream) = run.get("upstream").and_then(Value::as_str) {
        return format!("{upstream}/{}", text(run, "tool"));
    }
    String::new()
}

/// #EXP-1850 (TS `triggerDetail`, `detailCases`): a Triggers row's second
/// line = what a `run` trigger runs (`copy.runs`), a hand-off's
/// `copy.handsOff`; `None` for a wake.
pub fn trigger_detail(trigger: &Value) -> Option<String> {
    let run = trigger.get("run").filter(|run| run.is_object())?;
    let copy = &AGENT_SETTINGS.copy;
    if let Some(agent) = run.get("agent").and_then(Value::as_str) {
        return Some(copy.hands_off.replacen("{agent}", agent, 1));
    }
    Some(copy.runs.replacen("{tool}", &run_label(run), 1))
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
// `agentRole`). Replays `scheduleCases`, `visibleCases`, `orderCases`,
// `teamAgentsCases`.
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
    /// #EXP-1439: `agents.update` refuses instructions for an Assistant.
    pub prompt_fixed: String,
    pub legacy_instructions: String,
}

/// #EXP-1439: the Assistant's built-in, versioned prompt (server-side; read-only here).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AssistantPrompt {
    pub version: u32,
    pub text: String,
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
    pub prompt: AssistantPrompt,
}

/// The Assistant page's copy, sections and Check in presets.
pub static ASSISTANT: LazyLock<AssistantSpec> = LazyLock::new(|| {
    let fixture: Value = serde_json::from_str(AGENT_SETTINGS_JSON).expect("agent-settings.json parses");
    serde_json::from_value(fixture["assistant"].clone()).expect("agent-settings.json assistant parses")
});

/// TS `assistantModelValue` (`modelValueCases`): the Assistant's Model row
/// value = the pinned model, else the device default for the login's CLI
/// (never `Last used`; #EXP-1734: no separate thread model).
pub fn assistant_model_value(model: Option<&str>, cli: Option<&str>) -> String {
    match model.filter(|model| !model.trim().is_empty()) {
        Some(model) => model.to_string(),
        None if cli == Some("codex") => crate::contract::DEVICE_AGENT_DEFAULTS_CODEX_MODEL.to_string(),
        None => crate::contract::DEVICE_AGENT_DEFAULTS_MODEL.to_string(),
    }
}

/// TS `runsOnDeviceIds` (NEW-99, `runsOnCases`): the Runs on device
/// picker's devices, as `(device_id, kind)` pairs in order. An Assistant's
/// device is the member's voice device, so it lists desktops only (a bound
/// non-desktop stays, naming itself); a team agent keeps all.
pub fn runs_on_device_ids<'a>(
    devices: impl IntoIterator<Item = (&'a str, Option<&'a str>)>,
    bound_id: Option<&str>,
    assistant: bool,
) -> Vec<&'a str> {
    devices
        .into_iter()
        .filter(|(device_id, kind)| !assistant || *kind == Some("desktop") || Some(*device_id) == bound_id)
        .map(|(device_id, _)| device_id)
        .collect()
}

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
    let mut trigger = serde_json::json!({ "kind": "schedule", "cron": cron, "preset": id, "enabled": true });
    if let Some(tz) = tz.filter(|tz| !tz.is_empty()) {
        trigger["tz"] = Value::String(tz.to_string());
    }
    vec![trigger]
}

// ---------------------------------------------------------------------------
// #EXP-1846 (`checkIn`): THE CHECK IN = the one schedule trigger carrying
// `preset`, any agent's. Replays `checkInCases`, `withCheckInCases`,
// `listedCases`.
// ---------------------------------------------------------------------------

fn preset_cron(id: &str) -> Option<&'static str> {
    ASSISTANT
        .schedules
        .iter()
        .find(|schedule| schedule.id == id)
        .and_then(|schedule| schedule.cron.as_deref())
}

fn is_check_in(trigger: &Value) -> bool {
    trigger["kind"].as_str() == Some("schedule")
        && trigger["preset"].as_str().is_some_and(|preset| !preset.is_empty())
}

/// TS `checkInOf` (`checkInCases`): the Check in row's value = the preset
/// trigger's id, `off` when there is none or it is paused.
pub fn check_in_of(triggers: &[Value]) -> String {
    let Some(trigger) = triggers.iter().find(|trigger| is_check_in(trigger)) else {
        return "off".to_string();
    };
    let preset = trigger["preset"].as_str().unwrap_or_default();
    if trigger["enabled"].as_bool() == Some(false) || preset_cron(preset).is_none() {
        return "off".to_string();
    }
    preset.to_string()
}

/// TS `withCheckIn` (`withCheckInCases`): the triggers after a Check in pick
/// = the preset trigger replaced IN FRONT (keeping its `id` and stored `tz`;
/// a new one takes `tz`) or removed for `off`; every other trigger stays as
/// it is, in order.
pub fn with_check_in(triggers: &[Value], id: &str, tz: Option<&str>) -> Vec<Value> {
    let current = triggers.iter().position(is_check_in);
    let rest = triggers
        .iter()
        .enumerate()
        .filter(|(index, _)| Some(*index) != current)
        .map(|(_, trigger)| trigger.clone());
    let Some(cron) = preset_cron(id) else {
        return rest.collect();
    };
    let current = current.map(|index| &triggers[index]);
    let mut next = serde_json::Map::new();
    if let Some(trigger_id) = current.and_then(|trigger| trigger["id"].as_str()).filter(|id| !id.is_empty()) {
        next.insert("id".into(), Value::String(trigger_id.to_string()));
    }
    next.insert("kind".into(), Value::String("schedule".into()));
    next.insert("cron".into(), Value::String(cron.to_string()));
    next.insert("preset".into(), Value::String(id.to_string()));
    next.insert("enabled".into(), Value::Bool(true));
    let zone = match current {
        Some(trigger) => trigger["tz"].as_str(),
        None => tz,
    };
    if let Some(zone) = zone.filter(|zone| !zone.is_empty()) {
        next.insert("tz".into(), Value::String(zone.to_string()));
    }
    std::iter::once(Value::Object(next)).chain(rest).collect()
}

/// TS `listedTriggers` (`listedCases`): the rows under Check in = every
/// trigger without `preset`, in order, with their index in `triggers`.
pub fn listed_triggers(triggers: &[Value]) -> Vec<(usize, &Value)> {
    triggers
        .iter()
        .enumerate()
        .filter(|(_, trigger)| trigger["preset"].as_str().is_none_or(str::is_empty))
        .collect()
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

/// #EXP-1837 (`assistant.fab`): THE Assistant FAB's copy and numbers.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantFabSpec {
    pub copy: AssistantFabCopy,
    pub widget: AssistantWidgetSpec,
    /// The FAB's distance from the content card's bottom-right corner.
    pub inset: f32,
    /// #EXP-1860: the card's bottom-right square the FAB owns (inset + 52 +
    /// inset): page scroll content ends this far clear of it, the Dock
    /// band keeps its right edge this far in.
    pub clearance: f32,
    /// How far the bottom-right toast stack rises while the FAB shows.
    pub toast_lift: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AssistantFabCopy {
    pub label: String,
    pub maximize: String,
    pub minimize: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct AssistantWidgetSize {
    pub width: f32,
    pub height: f32,
}

/// #EXP-1876 (`assistant.fab.widget`): the widget's responsive size and
/// drag numbers.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct AssistantWidgetSpec {
    pub min: AssistantWidgetSize,
    pub ratio: AssistantWidgetSize,
    pub max: AssistantWidgetSize,
    pub margin: AssistantWidgetSize,
    /// The invisible resize band along the top and left edges.
    pub handle: f32,
    pub radius: f32,
}

/// #EXP-1876 TS `assistantWidgetSize` (`widget.sizeCases`): the widget's
/// size in a content card of `card` px. No `stored` = the responsive default
/// (round(card × ratio) clamped to [min … max]); a dragged `stored` size
/// clamps to [min … card − margin]. Either never exceeds card − margin.
pub fn assistant_widget_size(card: AssistantWidgetSize, stored: Option<AssistantWidgetSize>) -> AssistantWidgetSize {
    let w = &ASSISTANT_FAB.widget;
    let axis = |card: f32, stored: Option<f32>, min: f32, ratio: f32, max: f32, margin: f32| {
        let room = (card - margin).max(0.);
        let wanted = match stored.filter(|v| v.is_finite()) {
            Some(stored) => stored.round().max(min),
            None => (card * ratio).round().clamp(min, max),
        };
        wanted.min(room)
    };
    AssistantWidgetSize {
        width: axis(card.width, stored.map(|s| s.width), w.min.width, w.ratio.width, w.max.width, w.margin.width),
        height: axis(card.height, stored.map(|s| s.height), w.min.height, w.ratio.height, w.max.height, w.margin.height),
    }
}

pub static ASSISTANT_FAB: LazyLock<AssistantFabSpec> = LazyLock::new(|| {
    let fixture: Value = serde_json::from_str(AGENT_SETTINGS_JSON).expect("agent-settings.json parses");
    serde_json::from_value(fixture["assistant"]["fab"].clone()).expect("assistant.fab parses")
});

/// #EXP-1837 TS `assistantFabVisible` (`assistant.fab.showCases`): the
/// Assistant FAB shows on wide surfaces (the IDE) while the viewer has an
/// Assistant, except on its own chat page.
pub fn assistant_fab_visible(wide: bool, assistant_id: Option<&str>, active_agent_id: Option<&str>) -> bool {
    wide && assistant_id.is_some() && active_agent_id != assistant_id
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

/// #EXP-1714 (TS `agentsInTeam`, `assistant.teamAgentsCases`): the agents a
/// team lists = its own rows (input order), plus the viewer's Assistant whose
/// team is none of `team_ids` (the synced teams; a personal team never syncs)
/// while the team holds no Assistant row of the viewer's.
pub fn agents_in_team<'a>(
    agents: impl IntoIterator<Item = &'a crate::rows::AgentDefRow>,
    team_id: &str,
    team_ids: &[&str],
    me: Option<&str>,
) -> Vec<&'a crate::rows::AgentDefRow> {
    let agents: Vec<&crate::rows::AgentDefRow> = agents.into_iter().collect();
    let mine = |agent: &&crate::rows::AgentDefRow| agent.is_assistant() && agent.owner_id.as_deref() == me;
    let own: Vec<&crate::rows::AgentDefRow> =
        agents.iter().copied().filter(|agent| agent.team_id.as_deref() == Some(team_id)).collect();
    if own.iter().any(mine) {
        return own;
    }
    let global = agents.iter().copied().find(|agent| {
        mine(agent) && agent.team_id.as_deref().is_none_or(|team| !team_ids.contains(&team))
    });
    match global {
        Some(global) => std::iter::once(global).chain(own).collect(),
        None => own,
    }
}

/// The viewer's own Assistant as `team_id` lists it (`agents_in_team`: the
/// team's own row, else the global one).
pub fn my_assistant<'a>(
    agents: impl IntoIterator<Item = &'a crate::rows::AgentDefRow>,
    team_id: &str,
    team_ids: &[&str],
    me: Option<&str>,
) -> Option<&'a crate::rows::AgentDefRow> {
    let me = me?;
    agents_in_team(agents, team_id, team_ids, Some(me))
        .into_iter()
        .find(|agent| agent.is_assistant() && agent.owner_id.as_deref() == Some(me))
}

// ---------------------------------------------------------------------------
// #EXP-1717: THE TEAM COORDINATOR (`coordinator` in the fixture; contract
// `agentRole`). Replays `optionCases`, `valueCases`.
// ---------------------------------------------------------------------------

/// The Coordinator row's copy (`coordinator.copy`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoordinatorCopy {
    pub label: String,
    pub none: String,
    pub refused_owner: String,
    pub refused_agent: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CoordinatorTestIds {
    pub row: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoordinatorSpec {
    pub copy: CoordinatorCopy,
    pub test_ids: CoordinatorTestIds,
}

/// `coordinator` off the fixture.
pub static COORDINATOR: LazyLock<CoordinatorSpec> = LazyLock::new(|| {
    let fixture: Value = serde_json::from_str(AGENT_SETTINGS_JSON).expect("agent-settings.json parses");
    serde_json::from_value(fixture["coordinator"].clone()).expect("coordinator block parses")
});

/// TS `COORDINATOR_NONE`: the option id of "no coordinator".
pub const COORDINATOR_NONE: &str = "none";

/// TS `isCoordinator`.
pub fn is_coordinator_role(role: Option<&str>) -> bool {
    role == Some(crate::contract::AGENT_ROLE_COORDINATOR)
}

/// TS `coordinatorOptions` (`optionCases`): the Coordinator row's options
/// after [`COORDINATOR_NONE`]: the team agents (never an Assistant; a
/// coordinator is one) by sortOrder, then name.
pub fn coordinator_options<'a>(
    agents: impl IntoIterator<Item = &'a crate::rows::AgentDefRow>,
) -> Vec<&'a crate::rows::AgentDefRow> {
    let mut team: Vec<&crate::rows::AgentDefRow> = agents.into_iter().filter(|agent| !agent.is_assistant()).collect();
    team.sort_by(|a, b| {
        a.sort_order
            .unwrap_or(0.0)
            .partial_cmp(&b.sort_order.unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                let (an, bn) = (a.name.as_deref().unwrap_or(""), b.name.as_deref().unwrap_or(""));
                an.to_lowercase().cmp(&bn.to_lowercase()).then_with(|| an.cmp(bn))
            })
    });
    team
}

/// TS `coordinatorOf` (`valueCases`): the team's coordinator id, or `None`.
pub fn coordinator_of<'a>(
    agents: impl IntoIterator<Item = &'a crate::rows::AgentDefRow>,
    team_id: &str,
) -> Option<&'a str> {
    agents
        .into_iter()
        .find(|agent| agent.team_id.as_deref() == Some(team_id) && is_coordinator_role(Some(agent.role.as_str())))
        .map(|agent| agent.id.as_str())
}

// ---------------------------------------------------------------------------
// #EXP-1735: THE TEAM'S LEAD ASSISTANT (`lead` in the fixture). Replays
// `optionCases`. Supersedes the Coordinator row (its helpers stay for the
// wire until #EXP-1730 phase B).
// ---------------------------------------------------------------------------

/// The Lead Assistant row's copy (`lead.copy`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LeadCopy {
    pub label: String,
    pub none: String,
    pub refused_owner: String,
    pub refused_member: String,
    pub no_assistant: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LeadTestIds {
    pub row: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LeadSpec {
    pub copy: LeadCopy,
    pub test_ids: LeadTestIds,
}

/// `lead` off the fixture.
pub static LEAD: LazyLock<LeadSpec> = LazyLock::new(|| {
    let fixture: Value = serde_json::from_str(AGENT_SETTINGS_JSON).expect("agent-settings.json parses");
    serde_json::from_value(fixture["lead"].clone()).expect("lead block parses")
});

/// TS `LEAD_NONE`: the option id of "no lead Assistant".
pub const LEAD_NONE: &str = "none";

/// TS `leadOptions` (`optionCases`): the Lead Assistant row's options after
/// [`LEAD_NONE`]: the team's members by display name, then user id.
/// `member` maps an item to its `(user_id, display_name)`.
pub fn lead_options<T>(members: impl IntoIterator<Item = T>, member: impl Fn(&T) -> (&str, &str)) -> Vec<T> {
    let mut sorted: Vec<T> = members.into_iter().collect();
    sorted.sort_by(|a, b| {
        let ((a_id, a_name), (b_id, b_name)) = (member(a), member(b));
        a_name
            .to_lowercase()
            .cmp(&b_name.to_lowercase())
            .then_with(|| a_name.cmp(b_name))
            .then_with(|| a_id.cmp(b_id))
    });
    sorted
}

// ---------------------------------------------------------------------------
// #EXP-1847: EVERY agent's own voice (`voice` in the fixture; TS
// `agentVoiceOf`). Replays `voiceOfCases`. The agent's `voice` jsonb
// (`{language?, clipId?, clipName?, transcript?, wakePhrase?}`, `{}` = the
// member's) resolved against the member's voice settings.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentVoiceErrors {
    pub clip_type: String,
    pub clip_size: String,
}

/// The Voice section's copy (`voice.copy`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentVoiceCopy {
    pub label: String,
    pub language: String,
    pub clip: String,
    pub clip_built_in: String,
    pub clip_upload: String,
    pub clip_remove: String,
    pub transcript: String,
    pub transcript_placeholder: String,
    pub wake_phrase: String,
    /// `Hey {agent}`.
    pub wake_phrase_placeholder: String,
    pub errors: AgentVoiceErrors,
}

/// `voice.clipUpload`: the clip's route (`{agentId}`), the accepted types
/// and the byte cap.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentVoiceClipUpload {
    pub route: String,
    pub accept: Vec<String>,
    pub max_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentVoiceLimits {
    pub transcript: usize,
    pub wake_phrase: usize,
    pub wake_phrase_min_letters: usize,
}

/// `fixtures/agent-settings.json` `voice` minus its cases.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentVoiceSpec {
    /// `language`, `clip`, `transcript`, `wake-phrase`.
    pub rows: Vec<String>,
    pub copy: AgentVoiceCopy,
    pub clip_upload: AgentVoiceClipUpload,
    pub limits: AgentVoiceLimits,
}

/// `voice` off the fixture.
pub static AGENT_VOICE: LazyLock<AgentVoiceSpec> = LazyLock::new(|| {
    let fixture: Value = serde_json::from_str(AGENT_SETTINGS_JSON).expect("agent-settings.json parses");
    serde_json::from_value(fixture["voice"].clone()).expect("agent-settings.json voice parses")
});

/// An agent's stored voice (`agents.voice`, SYNCED jsonb). Every key is
/// optional; a key that is absent, null, blank-less empty or not a string
/// reads `None` (tolerant: a garbage cell is the member's voice).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentVoice {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clip_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clip_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcript: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wake_phrase: Option<String>,
}

impl AgentVoice {
    /// The jsonb cell (already JSON-decoded), tolerantly.
    pub fn from_value(value: Option<&Value>) -> Self {
        let key = |name: &str| -> Option<String> {
            value
                .and_then(|value| value.get(name))
                .and_then(Value::as_str)
                .filter(|text| !text.is_empty())
                .map(str::to_string)
        };
        Self {
            language: key("language"),
            clip_id: key("clipId"),
            clip_name: key("clipName"),
            transcript: key("transcript"),
            wake_phrase: key("wakePhrase"),
        }
    }
}

/// The member's voice settings `agent_voice_of` falls back to (voice.get).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MemberVoice<'a> {
    pub language: &'a str,
    pub voice_file: &'a str,
    pub voice_transcript: &'a str,
    pub wake_phrase: &'a str,
}

/// The clip an agent speaks with (None = the built-in voice).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentVoiceClip {
    /// The agent's uploaded clip (`/api/attachments/{id}`).
    Attachment(String),
    /// The member's local `voiceFile` (the Assistant only).
    Path(String),
}

/// TS `ResolvedAgentVoice`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAgentVoice {
    pub language: String,
    pub clip: Option<AgentVoiceClip>,
    /// The clip row's value (`copy.clipBuiltIn` when none).
    pub clip_label: String,
    pub transcript: String,
    /// Empty = the detector and the placeholder read `Hey {name}`.
    pub wake_phrase: String,
}

/// TS `agentVoiceOf` (`voiceOfCases`): the voice agent `role`/`voice`
/// speaks with, against the member's settings. `_name` names the agent for
/// the placeholder (`Hey {name}`), which reads it, not this.
pub fn agent_voice_of(role: Option<&str>, _name: &str, voice: &AgentVoice, member: &MemberVoice<'_>) -> ResolvedAgentVoice {
    let assistant = is_assistant_role(role);
    let language = voice.language.clone().filter(|language| !language.is_empty()).unwrap_or_else(|| member.language.to_string());
    let own_phrase = voice.wake_phrase.clone().filter(|phrase| !phrase.trim().is_empty());
    let wake_phrase = own_phrase.unwrap_or_else(|| if assistant { member.wake_phrase.to_string() } else { String::new() });
    if let Some(clip_id) = voice.clip_id.clone().filter(|id| !id.is_empty()) {
        return ResolvedAgentVoice {
            language,
            clip: Some(AgentVoiceClip::Attachment(clip_id)),
            clip_label: voice.clip_name.clone().filter(|name| !name.is_empty()).unwrap_or_else(|| AGENT_VOICE.copy.clip_built_in.clone()),
            transcript: voice.transcript.clone().unwrap_or_default(),
            wake_phrase,
        };
    }
    if assistant && !member.voice_file.is_empty() {
        let last = member.voice_file.rsplit(['/', '\\']).next().filter(|segment| !segment.is_empty()).unwrap_or(member.voice_file);
        return ResolvedAgentVoice {
            language,
            clip: Some(AgentVoiceClip::Path(member.voice_file.to_string())),
            clip_label: last.to_string(),
            transcript: voice.transcript.clone().unwrap_or_else(|| member.voice_transcript.to_string()),
            wake_phrase,
        };
    }
    // The agent's own transcript reads back even before its clip is uploaded.
    ResolvedAgentVoice { language, clip: None, clip_label: AGENT_VOICE.copy.clip_built_in.clone(), transcript: voice.transcript.clone().unwrap_or_default(), wake_phrase }
}

/// The clip route of `agent_id` (`clipUpload.route`).
pub fn agent_voice_clip_route(agent_id: &str) -> String {
    AGENT_VOICE.clip_upload.route.replace("{agentId}", agent_id)
}

/// The Transcript / Wake phrase placeholders (`{agent}` = the name).
pub fn agent_wake_phrase_placeholder(name: &str) -> String {
    AGENT_VOICE.copy.wake_phrase_placeholder.replace("{agent}", name)
}

/// Why a picked clip is refused before the upload (`copy.errors`): not a
/// WAV (`accept`, or a `.wav` name when the type is unknown), or over
/// `maxBytes`. `None` = it goes.
pub fn agent_voice_clip_refusal(filename: &str, content_type: &str, len: u64) -> Option<String> {
    let typed = AGENT_VOICE.clip_upload.accept.iter().any(|accept| accept.eq_ignore_ascii_case(content_type));
    let named = filename.to_ascii_lowercase().ends_with(".wav");
    if !typed && !named {
        return Some(AGENT_VOICE.copy.errors.clip_type.clone());
    }
    if len > AGENT_VOICE.clip_upload.max_bytes {
        return Some(AGENT_VOICE.copy.errors.clip_size.clone());
    }
    None
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
    fn trigger_detail_matches_the_fixture() {
        let fixture = fixture();
        let cases = fixture["detailCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            assert_eq!(trigger_detail(&case["trigger"]).as_deref(), case["expected"].as_str(), "{}", case["trigger"]);
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

    /// #EXP-1793: `mcpRow` + `mcpValueCases` (the skills `agentValueCases`
    /// rule, skills.rs).
    #[test]
    fn mcp_row_replays_the_fixture() {
        let fixture = fixture();
        let row = &fixture["mcpRow"];
        assert_eq!(row["label"], MCP_ROW_LABEL);
        assert_eq!(row["none"], MCP_ROW_NONE);
        assert_eq!(row["after"], "skills");
        assert_eq!(row["max"].as_u64(), Some(MCP_ROW_MAX as u64));
        let cases = fixture["mcpValueCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let names: Vec<&str> = case["names"].as_array().unwrap().iter().map(|n| n.as_str().unwrap()).collect();
            assert_eq!(agent_mcp_servers_value(&names), case["expected"].as_str().unwrap(), "{}", case["name"]);
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
    fn assistant_widget_size_cases_match_the_fixture() {
        let fixture: Value = serde_json::from_str(AGENT_SETTINGS_JSON).unwrap();
        let size = |v: &Value| AssistantWidgetSize { width: v["width"].as_f64().unwrap() as f32, height: v["height"].as_f64().unwrap() as f32 };
        let cases = fixture["assistant"]["fab"]["widget"]["sizeCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let stored = (!case["stored"].is_null()).then(|| size(&case["stored"]));
            assert_eq!(assistant_widget_size(size(&case["card"]), stored), size(&case["expected"]), "{}", case["name"]);
        }
    }

    #[test]
    fn assistant_model_value_cases_match_the_fixture() {
        let fixture = fixture();
        let cases = fixture["assistant"]["modelValueCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let input = &case["input"];
            assert_eq!(
                assistant_model_value(input["model"].as_str(), input["cli"].as_str()),
                case["expected"].as_str().unwrap(),
                "{}",
                case["name"]
            );
        }
    }

    #[test]
    fn assistant_runs_on_cases_match_the_fixture() {
        let fixture = fixture();
        let cases = fixture["assistant"]["runsOnCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let devices = case["devices"]
                .as_array()
                .unwrap()
                .iter()
                .map(|device| (device["deviceId"].as_str().unwrap(), device["kind"].as_str()));
            let expected: Vec<&str> =
                case["expected"].as_array().unwrap().iter().map(|id| id.as_str().unwrap()).collect();
            assert_eq!(
                runs_on_device_ids(devices, case["boundId"].as_str(), case["assistant"].as_bool().unwrap()),
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
        assert_eq!(ASSISTANT.sections, ["identity", "runs-on", "check-in", "voice"]);
        assert_eq!(ASSISTANT.prompt.version, 3);
        assert!(!ASSISTANT.prompt.text.is_empty());
        assert_eq!(ASSISTANT.copy.prompt_fixed, "The Assistant's prompt is built in");
    }

    #[test]
    fn check_in_cases_match_the_fixture() {
        let fixture = fixture();
        let check_in = &fixture["checkIn"];
        let cases = check_in["checkInCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let triggers = case["triggers"].as_array().unwrap();
            assert_eq!(check_in_of(triggers), case["expected"].as_str().unwrap(), "{}", case["name"]);
        }
        let cases = check_in["withCheckInCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let triggers = case["triggers"].as_array().unwrap();
            let next = with_check_in(triggers, case["preset"].as_str().unwrap(), case["tz"].as_str());
            assert_eq!(&next, case["expected"].as_array().unwrap(), "{}", case["name"]);
        }
        let cases = check_in["listedCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let triggers = case["triggers"].as_array().unwrap();
            let ids: Vec<&str> = listed_triggers(triggers)
                .into_iter()
                .map(|(_, trigger)| trigger["id"].as_str().unwrap())
                .collect();
            let expected: Vec<&str> =
                case["expected"].as_array().unwrap().iter().map(|id| id.as_str().unwrap()).collect();
            assert_eq!(ids, expected, "{}", case["name"]);
        }
        assert_eq!(AGENT_SETTINGS.copy.triggers, "Triggers");
        assert_eq!(AGENT_SETTINGS.copy.runs_on, "Runs on");
        assert_eq!(AGENT_SETTINGS.copy.check_in, "Check in");
    }

    #[test]
    fn assistant_schedule_triggers_round_trip() {
        assert!(assistant_schedule_triggers("off", None).is_empty());
        for schedule in &ASSISTANT.schedules {
            let triggers = assistant_schedule_triggers(&schedule.id, Some("Europe/Vienna"));
            assert_eq!(assistant_schedule_of(&triggers).as_deref(), Some(schedule.id.as_str()));
            assert_eq!(check_in_of(&triggers), schedule.id);
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
    fn assistant_fab_spec_reads_the_fixture() {
        let fab = &*ASSISTANT_FAB;
        assert_eq!(fab.copy.maximize, "Open full chat");
        assert_eq!(fab.copy.minimize, "Minimize");
        assert_eq!((fab.widget.min.width, fab.widget.min.height, fab.widget.radius), (480., 560., 16.));
        assert_eq!((fab.inset, fab.toast_lift), (16., 76.));
        // #EXP-1860: the corner the FAB owns = inset + the 52px circle + inset.
        assert_eq!(fab.clearance, fab.inset * 2. + 52.);
    }

    #[test]
    fn assistant_fab_show_cases_match_the_fixture() {
        let fixture = fixture();
        let cases = fixture["assistant"]["fab"]["showCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let actual = assistant_fab_visible(
                case["wide"].as_bool().unwrap(),
                case["assistantId"].as_str(),
                case["activeAgentId"].as_str(),
            );
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
            assert_eq!(my_assistant(&rows, "t", &["t"], case["me"].as_str()).map(|row| row.id.as_str()), Some("me"));
        }
    }

    #[test]
    fn assistant_team_agents_cases_match_the_fixture() {
        let fixture = fixture();
        let cases = fixture["assistant"]["teamAgentsCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let rows: Vec<crate::rows::AgentDefRow> = case["agents"]
                .as_array()
                .unwrap()
                .iter()
                .map(|agent| {
                    serde_json::from_value(serde_json::json!({
                        "id": agent["id"], "role": agent["role"], "owner_id": agent["ownerId"],
                        "team_id": agent["teamId"],
                    }))
                    .unwrap()
                })
                .collect();
            let team_ids: Vec<&str> = case["teamIds"].as_array().unwrap().iter().map(|id| id.as_str().unwrap()).collect();
            let ids: Vec<&str> = agents_in_team(&rows, case["teamId"].as_str().unwrap(), &team_ids, case["me"].as_str())
                .iter()
                .map(|row| row.id.as_str())
                .collect();
            let expected: Vec<&str> = case["expected"].as_array().unwrap().iter().map(|id| id.as_str().unwrap()).collect();
            assert_eq!(ids, expected, "{}", case["name"]);
        }
    }

    #[test]
    fn coordinator_cases_match_the_fixture() {
        let fixture = fixture();
        let block = &fixture["coordinator"];
        assert_eq!(COORDINATOR.copy.none, block["copy"]["none"].as_str().unwrap());
        assert_eq!(COORDINATOR.test_ids.row, block["testIds"]["row"].as_str().unwrap());
        let row_of = |agent: &Value| -> crate::rows::AgentDefRow {
            serde_json::from_value(serde_json::json!({
                "id": agent["id"], "role": agent["role"], "owner_id": agent["ownerId"],
                "name": agent["name"], "sort_order": agent["sortOrder"], "team_id": agent["teamId"],
            }))
            .unwrap()
        };
        let option_cases = block["optionCases"].as_array().unwrap();
        assert!(!option_cases.is_empty());
        for case in option_cases {
            let rows: Vec<crate::rows::AgentDefRow> = case["agents"].as_array().unwrap().iter().map(row_of).collect();
            let ids: Vec<&str> = std::iter::once(COORDINATOR_NONE)
                .chain(coordinator_options(&rows).into_iter().map(|row| row.id.as_str()))
                .collect();
            let expected: Vec<&str> = case["expected"].as_array().unwrap().iter().map(|id| id.as_str().unwrap()).collect();
            assert_eq!(ids, expected, "{}", case["name"]);
        }
        let value_cases = block["valueCases"].as_array().unwrap();
        assert!(!value_cases.is_empty());
        for case in value_cases {
            let rows: Vec<crate::rows::AgentDefRow> = case["agents"].as_array().unwrap().iter().map(row_of).collect();
            assert_eq!(
                coordinator_of(&rows, case["teamId"].as_str().unwrap()),
                case["expected"].as_str(),
                "{}",
                case["name"]
            );
        }
    }

    #[test]
    fn lead_cases_match_the_fixture() {
        let fixture = fixture();
        let block = &fixture["lead"];
        assert_eq!(LEAD.copy.label, block["copy"]["label"].as_str().unwrap());
        assert_eq!(LEAD.copy.none, block["copy"]["none"].as_str().unwrap());
        assert_eq!(LEAD.test_ids.row, block["testIds"]["row"].as_str().unwrap());
        let option_cases = block["optionCases"].as_array().unwrap();
        assert!(!option_cases.is_empty());
        for case in option_cases {
            let members: Vec<(&str, &str)> = case["members"]
                .as_array()
                .unwrap()
                .iter()
                .map(|member| (member["userId"].as_str().unwrap(), member["name"].as_str().unwrap()))
                .collect();
            let ids: Vec<&str> = std::iter::once(LEAD_NONE)
                .chain(lead_options(members, |member| (member.0, member.1)).into_iter().map(|member| member.0))
                .collect();
            let expected: Vec<&str> = case["expected"].as_array().unwrap().iter().map(|id| id.as_str().unwrap()).collect();
            assert_eq!(ids, expected, "{}", case["name"]);
        }
    }

    #[test]
    fn voice_of_cases_match_the_fixture() {
        let fixture = fixture();
        let block = &fixture["voice"];
        assert_eq!(AGENT_VOICE.rows, vec!["language", "clip", "transcript", "wake-phrase"]);
        assert_eq!(AGENT_VOICE.copy.label, block["copy"]["label"].as_str().unwrap());
        assert_eq!(AGENT_VOICE.limits.wake_phrase, 64);
        assert_eq!(AGENT_VOICE.clip_upload.max_bytes, 5 * 1024 * 1024);
        let cases = block["voiceOfCases"].as_array().unwrap();
        assert!(cases.len() >= 6, "the typed-transcript case included");
        for case in cases {
            let agent = &case["agent"];
            let member = &case["member"];
            let text = |key: &str| member[key].as_str().unwrap();
            let member = MemberVoice { language: text("language"), voice_file: text("voiceFile"), voice_transcript: text("voiceTranscript"), wake_phrase: text("wakePhrase") };
            let voice = AgentVoice::from_value(Some(&agent["voice"]));
            let got = agent_voice_of(agent["role"].as_str(), agent["name"].as_str().unwrap(), &voice, &member);
            let expected = &case["expected"];
            let clip = match &got.clip {
                None => Value::Null,
                Some(AgentVoiceClip::Attachment(id)) => serde_json::json!({"attachmentId": id}),
                Some(AgentVoiceClip::Path(path)) => serde_json::json!({"path": path}),
            };
            assert_eq!(got.language, expected["language"].as_str().unwrap(), "{}", case["name"]);
            assert_eq!(clip, expected["clip"], "{}", case["name"]);
            assert_eq!(got.clip_label, expected["clipLabel"].as_str().unwrap(), "{}", case["name"]);
            assert_eq!(got.transcript, expected["transcript"].as_str().unwrap(), "{}", case["name"]);
            assert_eq!(got.wake_phrase, expected["wakePhrase"].as_str().unwrap(), "{}", case["name"]);
        }
    }

    #[test]
    fn the_voice_helpers_read_the_fixture() {
        assert_eq!(agent_voice_clip_route("a-1"), "/api/agents/a-1/voice-clip");
        assert_eq!(agent_wake_phrase_placeholder("Sloppy"), "Hey Sloppy");
        assert_eq!(agent_voice_clip_refusal("me.wav", "audio/wav", 10), None);
        assert_eq!(agent_voice_clip_refusal("me.WAV", "application/octet-stream", 10), None);
        assert_eq!(agent_voice_clip_refusal("me.mp3", "audio/mpeg", 10).as_deref(), Some("Voice clips must be WAV files"));
        assert_eq!(agent_voice_clip_refusal("me.wav", "audio/wav", 6 * 1024 * 1024).as_deref(), Some("Voice clips must be 5 MB or smaller"));
    }

    /// #EXP-1847: the synced `voice` cell decodes tolerantly (the store's
    /// TEXT cell, null, garbage).
    #[test]
    fn an_agent_row_reads_its_voice_tolerantly() {
        let row: crate::rows::AgentDefRow = serde_json::from_value(serde_json::json!({ "id": "a", "voice": "{\"language\":\"de\",\"clipId\":\"c-1\",\"clipName\":\"sloppy.wav\"}" })).unwrap();
        assert_eq!(row.voice.language.as_deref(), Some("de"));
        assert_eq!(row.voice.clip_id.as_deref(), Some("c-1"));
        let row: crate::rows::AgentDefRow = serde_json::from_value(serde_json::json!({ "id": "a", "voice": {"wakePhrase": "Hey Sloppy", "language": 3} })).unwrap();
        assert_eq!(row.voice.wake_phrase.as_deref(), Some("Hey Sloppy"));
        assert_eq!(row.voice.language, None);
        for garbage in [serde_json::json!(null), serde_json::json!("nope"), serde_json::json!([1])] {
            let row: crate::rows::AgentDefRow = serde_json::from_value(serde_json::json!({ "id": "a", "voice": garbage })).unwrap();
            assert_eq!(row.voice, AgentVoice::default());
        }
        let row: crate::rows::AgentDefRow = serde_json::from_value(serde_json::json!({ "id": "a" })).unwrap();
        assert_eq!(row.voice, AgentVoice::default());
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
