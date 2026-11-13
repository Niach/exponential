//! #EXP-1815: where a notification shows, when it clears, who mutes it ×4.
//! Byte-locked by `packages/domain-contract/fixtures/notification-delivery.json`
//! (its `$comment` is the spec); mirrors TS `src/notification-delivery.ts`,
//! iOS `NotificationDelivery.swift`, Android `NotificationDelivery.kt`. The
//! test below replays EVERY `*Cases` array.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::contract::NOTIFICATION_TYPE_AGENT_SIGNED_OUT;

/// The fixture's `dataKeys`.
pub mod keys {
    pub const THREAD: &str = "thread";
    pub const SOURCE: &str = "source";
    pub const SOURCE_NAME: &str = "sourceName";
    pub const DISMISS_TYPE: &str = "dismiss";
    pub const IDS: &str = "ids";
    pub const THREADS: &str = "threads";
    pub const ALL: &str = "all";
    pub const TEAM_ID: &str = "teamId";
}

/// The fixture's `limits`.
pub const MAX_IDS: usize = 60;
pub const MAX_THREADS: usize = 30;

/// The fixture's `copy`.
pub mod copy {
    pub const TOAST_ACTION: &str = "Open";
    pub const SESSIONS_BAND: &str = "Sessions";
    pub const ASSISTANT: &str = "Personal Assistant";
    pub const TEMPORARY: &str = "Temporary sessions";
    pub const AGENTS_BAND: &str = "Team agents";
}

/// The fixture's `channels` (Android; mirrored for the ×4 replay).
pub mod channels {
    pub const ASSISTANT_ID: &str = "source-assistant";
    pub const ASSISTANT_NAME: &str = "Personal Assistant";
    pub const TEMPORARY_ID: &str = "source-temporary";
    pub const TEMPORARY_NAME: &str = "Temporary sessions";
    pub const AGENT_PREFIX: &str = "source-agent-";
    pub const AGENTS_GROUP_ID: &str = "team-agents";
    pub const AGENTS_GROUP_NAME: &str = "Team agents";
    pub const FALLBACK: &str = "issues_default";
}

const AGENT_PREFIX: &str = "agent:";

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.filter(|v| !v.is_empty())
}

// ── Thread ────────────────────────────────────────────────────────────────

/// The thread every screen of the Inbox covers.
pub const INBOX_THREAD: &str = "inbox";

/// The key a notification stacks and clears under. `session_root_id` = the
/// root of the row's session (a client without it synced: the session id).
pub fn notification_thread(
    kind: Option<&str>,
    issue_id: Option<&str>,
    session_root_id: Option<&str>,
    device_id: Option<&str>,
) -> String {
    if let Some(issue_id) = non_empty(issue_id) {
        return format!("issue:{issue_id}");
    }
    if let Some(root_id) = non_empty(session_root_id) {
        return format!("session:{root_id}");
    }
    if kind == Some(NOTIFICATION_TYPE_AGENT_SIGNED_OUT) {
        if let Some(device_id) = non_empty(device_id) {
            return format!("device:{device_id}");
        }
    }
    INBOX_THREAD.to_string()
}

// ── Presentation ──────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeliveryPlatform {
    Web,
    Desktop,
    Ios,
    Android,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Presentation {
    None,
    Toast,
    System,
}

/// `active` = web: the page; desktop: a focused window; phones: the
/// foreground. `viewing` = the current screen's thread (None = a screen
/// without one). `muted` = the source switches muted it (`pushed_at` NULL).
pub fn notification_presentation(
    platform: DeliveryPlatform,
    active: bool,
    viewing: Option<&str>,
    thread: &str,
    muted: bool,
) -> Presentation {
    if muted {
        return Presentation::None;
    }
    if !active {
        return if platform == DeliveryPlatform::Web {
            Presentation::Toast
        } else {
            Presentation::System
        };
    }
    if viewing == Some(INBOX_THREAD) || viewing == Some(thread) {
        return Presentation::None;
    }
    Presentation::Toast
}

// ── Clearing ──────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadRow {
    pub id: String,
    pub thread: String,
}

/// A dismiss push's data (string keys → string values).
pub type DismissPayload = BTreeMap<String, String>;

fn distinct<'a>(values: impl Iterator<Item = &'a str>) -> Vec<&'a str> {
    let mut seen = HashSet::new();
    values.filter(|value| seen.insert(*value)).collect()
}

/// The silent push for the rows one write turned read; None = no push.
pub fn dismiss_payload(rows: &[ReadRow], all: bool, team_id: Option<&str>) -> Option<DismissPayload> {
    if rows.is_empty() {
        return None;
    }
    let mut payload = DismissPayload::new();
    payload.insert("type".to_string(), keys::DISMISS_TYPE.to_string());
    let everything = |mut payload: DismissPayload| {
        payload.insert(keys::ALL.to_string(), "1".to_string());
        if let Some(team_id) = non_empty(team_id) {
            payload.insert(keys::TEAM_ID.to_string(), team_id.to_string());
        }
        payload
    };
    if all {
        return Some(everything(payload));
    }
    let ids = distinct(rows.iter().map(|row| row.id.as_str()));
    if ids.len() <= MAX_IDS {
        payload.insert(keys::IDS.to_string(), ids.join(","));
        return Some(payload);
    }
    let threads = distinct(rows.iter().map(|row| row.thread.as_str()));
    if threads.len() <= MAX_THREADS {
        payload.insert(keys::THREADS.to_string(), threads.join(","));
        return Some(payload);
    }
    Some(everything(payload))
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeliveredNotification {
    pub notification_id: Option<String>,
    pub thread: Option<String>,
    pub team_id: Option<String>,
}

fn split_list(value: Option<&String>) -> impl Iterator<Item = &str> {
    value.map(String::as_str).unwrap_or("").split(',').filter(|part| !part.is_empty())
}

/// Does a dismiss push remove this delivered notification?
pub fn dismiss_matches(payload: &BTreeMap<String, String>, delivered: &DeliveredNotification) -> bool {
    if payload.get("type").map(String::as_str) != Some(keys::DISMISS_TYPE) {
        return false;
    }
    if payload.get(keys::ALL).map(String::as_str) == Some("1") {
        return match non_empty(payload.get(keys::TEAM_ID).map(String::as_str)) {
            None => true,
            Some(team_id) => delivered.team_id.as_deref() == Some(team_id),
        };
    }
    if let Some(id) = non_empty(delivered.notification_id.as_deref()) {
        if split_list(payload.get(keys::IDS)).any(|part| part == id) {
            return true;
        }
    }
    match delivered.thread.as_deref() {
        Some(thread) => split_list(payload.get(keys::THREADS)).any(|part| part == thread),
        None => false,
    }
}

// ── Source switches ───────────────────────────────────────────────────────

/// The session ROOT a notification row's source is read off.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceSession {
    pub agent_id: Option<String>,
    pub agent_role: Option<String>,
    pub started_by_agent_id: Option<String>,
    pub started_by_agent_role: Option<String>,
}

/// `assistant` | `temporary` | `agent:<agentId>`.
pub fn notification_source(root: &SourceSession) -> String {
    let own = non_empty(root.agent_id.as_deref());
    let (agent_id, role) = match own {
        Some(id) => (Some(id), root.agent_role.as_deref()),
        None => (non_empty(root.started_by_agent_id.as_deref()), root.started_by_agent_role.as_deref()),
    };
    match agent_id {
        None => "temporary".to_string(),
        Some(_) if role == Some("assistant") => "assistant".to_string(),
        Some(id) => format!("{AGENT_PREFIX}{id}"),
    }
}

/// `user_notification_prefs.source_prefs`: absent = on.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourcePrefs {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assistant: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temporary: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agents: Option<BTreeMap<String, bool>>,
}

pub fn source_allowed(prefs: Option<&SourcePrefs>, source: Option<&str>) -> bool {
    let (Some(prefs), Some(source)) = (prefs, non_empty(source)) else {
        return true;
    };
    match source {
        "assistant" => prefs.assistant != Some(false),
        "temporary" => prefs.temporary != Some(false),
        _ => match source.strip_prefix(AGENT_PREFIX) {
            Some(id) => prefs.agents.as_ref().and_then(|agents| agents.get(id)) != Some(&false),
            None => true,
        },
    }
}

/// The prefs after one switch flips (`key` = a settings row's key).
pub fn toggle_source_pref(prefs: &SourcePrefs, key: &str, on: bool) -> SourcePrefs {
    let mut next = prefs.clone();
    match key {
        "assistant" => next.assistant = Some(on),
        "temporary" => next.temporary = Some(on),
        _ => {
            if let Some(id) = key.strip_prefix(AGENT_PREFIX) {
                next.agents.get_or_insert_with(BTreeMap::new).insert(id.to_string(), on);
            }
        }
    }
    next
}

// ── Settings ──────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsTeam {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsAgent {
    pub id: String,
    pub team_id: String,
    pub name: String,
    pub role: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsRow {
    pub key: String,
    pub label: String,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsGroup {
    pub band: String,
    pub rows: Vec<SettingsRow>,
}

pub fn settings_rows(teams: &[SettingsTeam], agents: &[SettingsAgent]) -> Vec<SettingsGroup> {
    let mut groups = vec![SettingsGroup {
        band: copy::SESSIONS_BAND.to_string(),
        rows: vec![
            SettingsRow { key: "assistant".to_string(), label: copy::ASSISTANT.to_string(), detail: None },
            SettingsRow { key: "temporary".to_string(), label: copy::TEMPORARY.to_string(), detail: None },
        ],
    }];
    let team_name = |id: &str| teams.iter().find(|team| team.id == id).map(|team| team.name.as_str());
    let mut listed: Vec<(&SettingsAgent, &str)> = agents
        .iter()
        .filter(|agent| agent.role != "assistant")
        .filter_map(|agent| team_name(&agent.team_id).map(|team| (agent, team)))
        .collect();
    listed.sort_by(|(a, a_team), (b, b_team)| {
        a_team.cmp(b_team).then_with(|| a.name.cmp(&b.name)).then_with(|| a.id.cmp(&b.id))
    });
    if !listed.is_empty() {
        let many = teams.len() > 1;
        groups.push(SettingsGroup {
            band: copy::AGENTS_BAND.to_string(),
            rows: listed
                .into_iter()
                .map(|(agent, team)| SettingsRow {
                    key: format!("{AGENT_PREFIX}{}", agent.id),
                    label: agent.name.clone(),
                    detail: many.then(|| team.to_string()),
                })
                .collect(),
        });
    }
    groups
}

// ── Android channels ──────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AndroidChannel {
    pub id: String,
    pub name: Option<String>,
    pub group: Option<String>,
}

pub fn android_channel(source: Option<&str>, source_name: Option<&str>) -> AndroidChannel {
    match source {
        Some("assistant") => AndroidChannel {
            id: channels::ASSISTANT_ID.to_string(),
            name: Some(channels::ASSISTANT_NAME.to_string()),
            group: None,
        },
        Some("temporary") => AndroidChannel {
            id: channels::TEMPORARY_ID.to_string(),
            name: Some(channels::TEMPORARY_NAME.to_string()),
            group: None,
        },
        Some(source) if source.starts_with(AGENT_PREFIX) => AndroidChannel {
            id: format!("{}{}", channels::AGENT_PREFIX, &source[AGENT_PREFIX.len()..]),
            name: Some(non_empty(source_name).unwrap_or(channels::AGENTS_GROUP_NAME).to_string()),
            group: Some(channels::AGENTS_GROUP_ID.to_string()),
        },
        _ => AndroidChannel { id: channels::FALLBACK.to_string(), name: None, group: None },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/notification-delivery.json");

    fn fixture() -> Value {
        serde_json::from_str(FIXTURE).unwrap()
    }

    fn cases(key: &str) -> Vec<Value> {
        let cases = fixture()[key].as_array().unwrap().clone();
        assert!(!cases.is_empty(), "{key} is empty");
        cases
    }

    fn opt(value: &Value) -> Option<&str> {
        value.as_str()
    }

    fn string_map(value: &Value) -> BTreeMap<String, String> {
        value
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string()))
            .collect()
    }

    #[test]
    fn every_case_array_is_replayed() {
        let replayed = [
            "threadCases",
            "presentCases",
            "dismissCases",
            "matchCases",
            "sourceCases",
            "allowCases",
            "settingsCases",
            "toggleCases",
            "channelCases",
        ];
        let fixture = fixture();
        for key in fixture.as_object().unwrap().keys().filter(|k| k.ends_with("Cases")) {
            assert!(replayed.contains(&key.as_str()), "{key} is not replayed");
        }
    }

    #[test]
    fn the_constants_match_the_fixture() {
        let f = fixture();
        let k = &f["dataKeys"];
        for (key, value) in [
            ("thread", keys::THREAD),
            ("source", keys::SOURCE),
            ("sourceName", keys::SOURCE_NAME),
            ("dismissType", keys::DISMISS_TYPE),
            ("ids", keys::IDS),
            ("threads", keys::THREADS),
            ("all", keys::ALL),
            ("teamId", keys::TEAM_ID),
        ] {
            assert_eq!(k[key], json!(value), "dataKeys.{key}");
        }
        assert_eq!(f["limits"]["maxIds"], json!(MAX_IDS));
        assert_eq!(f["limits"]["maxThreads"], json!(MAX_THREADS));
        let c = &f["copy"];
        assert_eq!(c["toastAction"], json!(copy::TOAST_ACTION));
        assert_eq!(c["sessionsBand"], json!(copy::SESSIONS_BAND));
        assert_eq!(c["assistant"], json!(copy::ASSISTANT));
        assert_eq!(c["temporary"], json!(copy::TEMPORARY));
        assert_eq!(c["agentsBand"], json!(copy::AGENTS_BAND));
        let ch = &f["channels"];
        assert_eq!(ch["assistant"], json!({"id": channels::ASSISTANT_ID, "name": channels::ASSISTANT_NAME}));
        assert_eq!(ch["temporary"], json!({"id": channels::TEMPORARY_ID, "name": channels::TEMPORARY_NAME}));
        assert_eq!(ch["agentPrefix"], json!(channels::AGENT_PREFIX));
        assert_eq!(ch["agentsGroup"], json!({"id": channels::AGENTS_GROUP_ID, "name": channels::AGENTS_GROUP_NAME}));
        assert_eq!(ch["fallback"], json!(channels::FALLBACK));
    }

    #[test]
    fn thread_cases() {
        for case in cases("threadCases") {
            let i = &case["input"];
            let got = notification_thread(opt(&i["type"]), opt(&i["issueId"]), opt(&i["sessionRootId"]), opt(&i["deviceId"]));
            assert_eq!(got, case["expected"].as_str().unwrap(), "{}", case["name"]);
        }
    }

    #[test]
    fn present_cases() {
        for case in cases("presentCases") {
            let i = &case["input"];
            let platform = match i["platform"].as_str().unwrap() {
                "web" => DeliveryPlatform::Web,
                "desktop" => DeliveryPlatform::Desktop,
                "ios" => DeliveryPlatform::Ios,
                "android" => DeliveryPlatform::Android,
                other => panic!("unknown platform {other}"),
            };
            let got = notification_presentation(
                platform,
                i["active"].as_bool().unwrap(),
                opt(&i["viewing"]),
                i["thread"].as_str().unwrap(),
                i["muted"].as_bool().unwrap(),
            );
            let expected = match case["expected"].as_str().unwrap() {
                "none" => Presentation::None,
                "toast" => Presentation::Toast,
                "system" => Presentation::System,
                other => panic!("unknown presentation {other}"),
            };
            assert_eq!(got, expected, "{}", case["name"]);
        }
    }

    /// The fixture's `rowsOfRule`.
    fn rows_of(input: &Value) -> Vec<ReadRow> {
        if let Some(rows) = input["rows"].as_array() {
            return rows
                .iter()
                .map(|row| ReadRow {
                    id: row["id"].as_str().unwrap().to_string(),
                    thread: row["thread"].as_str().unwrap().to_string(),
                })
                .collect();
        }
        let spec = &input["rowsOf"];
        let count = spec["count"].as_u64().unwrap() as usize;
        (0..count)
            .map(|i| ReadRow {
                id: format!("n{i}"),
                thread: match spec["threads"].as_array() {
                    Some(threads) => threads[i % threads.len()].as_str().unwrap().to_string(),
                    None => format!("t{}", i % spec["threadCount"].as_u64().unwrap() as usize),
                },
            })
            .collect()
    }

    #[test]
    fn dismiss_cases() {
        for case in cases("dismissCases") {
            let i = &case["input"];
            let got = dismiss_payload(&rows_of(i), i["all"].as_bool().unwrap(), opt(&i["teamId"]));
            let expected = (!case["expected"].is_null()).then(|| string_map(&case["expected"]));
            assert_eq!(got, expected, "{}", case["name"]);
        }
    }

    #[test]
    fn match_cases() {
        for case in cases("matchCases") {
            let i = &case["input"];
            let d = &i["delivered"];
            let delivered = DeliveredNotification {
                notification_id: opt(&d["notificationId"]).map(str::to_string),
                thread: opt(&d["thread"]).map(str::to_string),
                team_id: opt(&d["teamId"]).map(str::to_string),
            };
            let got = dismiss_matches(&string_map(&i["payload"]), &delivered);
            assert_eq!(got, case["expected"].as_bool().unwrap(), "{}", case["name"]);
        }
    }

    #[test]
    fn source_cases() {
        for case in cases("sourceCases") {
            let i = &case["input"];
            let s = |key: &str| opt(&i[key]).map(str::to_string);
            let root = SourceSession {
                agent_id: s("agentId"),
                agent_role: s("agentRole"),
                started_by_agent_id: s("startedByAgentId"),
                started_by_agent_role: s("startedByAgentRole"),
            };
            assert_eq!(notification_source(&root), case["expected"].as_str().unwrap(), "{}", case["name"]);
        }
    }

    #[test]
    fn allow_cases() {
        for case in cases("allowCases") {
            let i = &case["input"];
            let prefs: Option<SourcePrefs> = serde_json::from_value(i["prefs"].clone()).unwrap();
            let got = source_allowed(prefs.as_ref(), opt(&i["source"]));
            assert_eq!(got, case["expected"].as_bool().unwrap(), "{}", case["name"]);
        }
    }

    #[test]
    fn toggle_cases() {
        for case in cases("toggleCases") {
            let i = &case["input"];
            let prefs: SourcePrefs = serde_json::from_value(i["prefs"].clone()).unwrap();
            let got = toggle_source_pref(&prefs, i["key"].as_str().unwrap(), i["on"].as_bool().unwrap());
            assert_eq!(serde_json::to_value(&got).unwrap(), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn settings_cases() {
        for case in cases("settingsCases") {
            let i = &case["input"];
            let s = |v: &Value, key: &str| v[key].as_str().unwrap().to_string();
            let teams: Vec<SettingsTeam> = i["teams"]
                .as_array()
                .unwrap()
                .iter()
                .map(|t| SettingsTeam { id: s(t, "id"), name: s(t, "name") })
                .collect();
            let agents: Vec<SettingsAgent> = i["agents"]
                .as_array()
                .unwrap()
                .iter()
                .map(|a| SettingsAgent { id: s(a, "id"), team_id: s(a, "teamId"), name: s(a, "name"), role: s(a, "role") })
                .collect();
            let got: Vec<Value> = settings_rows(&teams, &agents)
                .into_iter()
                .map(|group| {
                    json!({
                        "band": group.band,
                        "rows": group.rows.into_iter().map(|row| json!({"key": row.key, "label": row.label, "detail": row.detail})).collect::<Vec<_>>(),
                    })
                })
                .collect();
            assert_eq!(Value::Array(got), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn channel_cases() {
        for case in cases("channelCases") {
            let i = &case["input"];
            let got = android_channel(opt(&i["source"]), opt(&i["sourceName"]));
            let got = json!({"id": got.id, "name": got.name, "group": got.group});
            assert_eq!(got, case["expected"], "{}", case["name"]);
        }
    }
}
