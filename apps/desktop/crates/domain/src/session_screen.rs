//! #EXP-1732 THE SESSION SCREEN ×4 (`packages/domain-contract/src/session-screen.ts`'s
//! twin, iOS ExpCore `SessionScreen.swift`, Android `domain/SessionScreen.kt`):
//! ONE screen for a chat, a team agent's conversation, the Assistant's and an
//! app's editor session: the header tabs per kind (with their muted counts),
//! THE gear, the opened item and the wide-screen `beside` rule. Byte-locked
//! by `fixtures/session-screen.json` (its `$comment` is the spec); pure.

use std::collections::HashMap;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

const SESSION_SCREEN_JSON: &str = include_str!("../../../../../packages/domain-contract/fixtures/session-screen.json");

/// The screen's kind (`sessionScreenKind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionScreenKind {
    Session,
    Agent,
    Assistant,
    App,
}

impl SessionScreenKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SessionScreenKind::Session => "session",
            SessionScreenKind::Agent => "agent",
            SessionScreenKind::Assistant => "assistant",
            SessionScreenKind::App => "app",
        }
    }
}

/// One header tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionScreenTab {
    Chat,
    Issues,
    Sessions,
    Results,
    Preview,
    Data,
    Versions,
}

impl SessionScreenTab {
    pub fn as_str(self) -> &'static str {
        match self {
            SessionScreenTab::Chat => "chat",
            SessionScreenTab::Issues => "issues",
            SessionScreenTab::Sessions => "sessions",
            SessionScreenTab::Results => "results",
            SessionScreenTab::Preview => "preview",
            SessionScreenTab::Data => "data",
            SessionScreenTab::Versions => "versions",
        }
    }

    pub fn parse(id: &str) -> Option<Self> {
        match id {
            "chat" => Some(SessionScreenTab::Chat),
            "issues" => Some(SessionScreenTab::Issues),
            "sessions" => Some(SessionScreenTab::Sessions),
            "results" => Some(SessionScreenTab::Results),
            "preview" => Some(SessionScreenTab::Preview),
            "data" => Some(SessionScreenTab::Data),
            "versions" => Some(SessionScreenTab::Versions),
            _ => None,
        }
    }

    /// The tab's label (`copy.tabs[id]`).
    pub fn label(self) -> String {
        SESSION_SCREEN.copy.tabs.get(self.as_str()).cloned().unwrap_or_else(|| self.as_str().to_string())
    }
}

/// The item opened over the tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionScreenOpen {
    Issue,
    Session,
    Plan,
    Settings,
}

impl SessionScreenOpen {
    pub fn parse(id: &str) -> Option<Self> {
        match id {
            "issue" => Some(SessionScreenOpen::Issue),
            "session" => Some(SessionScreenOpen::Session),
            "plan" => Some(SessionScreenOpen::Plan),
            "settings" => Some(SessionScreenOpen::Settings),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            SessionScreenOpen::Issue => "issue",
            SessionScreenOpen::Session => "session",
            SessionScreenOpen::Plan => "plan",
            SessionScreenOpen::Settings => "settings",
        }
    }
}

/// What the body lays out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionScreenLayout {
    Chat,
    Beside,
    Tab,
    Item,
}

/// One header tab with its muted count (`None` = no number, 0 included).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionScreenTabEntry {
    pub id: SessionScreenTab,
    pub label: String,
    pub count: Option<u64>,
}

/// The counted tabs' numbers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub struct SessionScreenCounts {
    pub issues: u64,
    pub sessions: u64,
    pub results: u64,
}

impl SessionScreenCounts {
    fn of(&self, id: &str) -> u64 {
        match id {
            "issues" => self.issues,
            "sessions" => self.sessions,
            "results" => self.results,
            _ => 0,
        }
    }
}

/// `sessionScreen`'s value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionScreen {
    pub tabs: Vec<SessionScreenTabEntry>,
    pub active: SessionScreenTab,
    pub gear: bool,
    pub open: Option<SessionScreenOpen>,
    pub layout: SessionScreenLayout,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionScreenCopy {
    pub tabs: HashMap<String, String>,
    pub settings: String,
    pub back: String,
    pub separator: String,
    pub link_issue: String,
    pub status: SessionScreenStatusCopy,
}

/// `copy.status`: THE tree's lowercase words.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionScreenStatusCopy {
    pub needs_input: String,
    pub working: String,
    pub queued: String,
    pub done: String,
    pub idle: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SessionScreenTestIds {
    pub tabs: String,
    pub gear: String,
    pub beside: String,
    pub pane: String,
    pub issues: String,
    pub sessions: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionScreenKindSpec {
    pub tabs: Vec<String>,
    pub gear: bool,
    pub wide_default: String,
}

/// The wide rule: the chat pane `chat` fr (≥ `chat_min_width` px) : the
/// tab pane `pane` fr; web switches at `breakpoint` px.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionScreenBeside {
    pub breakpoint: f32,
    pub chat: f32,
    pub pane: f32,
    pub chat_min_width: f32,
}

/// `fixtures/session-screen.json` minus its cases.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionScreenSpec {
    pub copy: SessionScreenCopy,
    pub test_ids: SessionScreenTestIds,
    pub kinds: HashMap<String, SessionScreenKindSpec>,
    pub counted: Vec<String>,
    pub legacy_tabs: HashMap<String, String>,
    pub beside: SessionScreenBeside,
}

pub static SESSION_SCREEN: LazyLock<SessionScreenSpec> =
    LazyLock::new(|| serde_json::from_str(SESSION_SCREEN_JSON).expect("session-screen.json parses"));

fn kind_spec(kind: &str) -> &'static SessionScreenKindSpec {
    let kinds = &SESSION_SCREEN.kinds;
    kinds.get(kind).unwrap_or_else(|| kinds.get("session").expect("session-screen.json has the session kind"))
}

/// The screen's kind from its ROOT session (an app wins over its agent).
pub fn session_screen_kind(agent_id: Option<&str>, app_id: Option<&str>, assistant: bool) -> SessionScreenKind {
    let present = |id: Option<&str>| id.is_some_and(|id| !id.is_empty());
    if present(app_id) {
        SessionScreenKind::App
    } else if present(agent_id) {
        if assistant {
            SessionScreenKind::Assistant
        } else {
            SessionScreenKind::Agent
        }
    } else {
        SessionScreenKind::Session
    }
}

/// The header's subtitle: `status · device · trigger`, blanks dropped.
pub fn session_screen_subtitle(status: Option<&str>, device: Option<&str>, trigger: Option<&str>) -> String {
    [status, device, trigger]
        .into_iter()
        .map(|part| part.unwrap_or("").trim())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(SESSION_SCREEN.copy.separator.as_str())
}

/// The kind's tabs (no counts).
pub fn session_screen_tabs(kind: &str) -> Vec<SessionScreenTab> {
    kind_spec(kind).tabs.iter().filter_map(|id| SessionScreenTab::parse(id)).collect()
}

/// Whether the kind's header carries THE gear.
pub fn session_screen_gear(kind: &str) -> bool {
    kind_spec(kind).gear
}

/// THE algorithm (`sessionScreen`).
pub fn session_screen(
    kind: &str,
    wide: bool,
    requested: Option<&str>,
    open: Option<&str>,
    counts: SessionScreenCounts,
) -> SessionScreen {
    let spec = kind_spec(kind);
    let counted = &SESSION_SCREEN.counted;
    let tabs: Vec<SessionScreenTabEntry> = spec
        .tabs
        .iter()
        .filter_map(|id| {
            let tab = SessionScreenTab::parse(id)?;
            let value = if counted.iter().any(|c| c == id) { counts.of(id) } else { 0 };
            Some(SessionScreenTabEntry { id: tab, label: tab.label(), count: (value > 0).then_some(value) })
        })
        .collect();
    let req = requested.map(|requested| {
        SESSION_SCREEN.legacy_tabs.get(requested).map(String::as_str).unwrap_or(requested)
    });
    let mut open = open.and_then(SessionScreenOpen::parse);
    if open.is_none() && req == Some("settings") {
        open = Some(SessionScreenOpen::Settings);
    }
    if open == Some(SessionScreenOpen::Settings) && !spec.gear {
        open = None;
    }
    // ACTIVE FOLLOWS THE OPENED ITEM: a session = Sessions, an issue = Issues.
    let follows = match open {
        Some(SessionScreenOpen::Session) => Some("sessions"),
        Some(SessionScreenOpen::Issue) => Some("issues"),
        _ => None,
    };
    let active_id = match (follows, req) {
        (Some(follows), _) if spec.tabs.iter().any(|tab| tab == follows) => follows,
        (_, Some(req)) if spec.tabs.iter().any(|tab| tab == req) => req,
        _ if wide => spec.wide_default.as_str(),
        _ => "chat",
    };
    let active = SessionScreenTab::parse(active_id).unwrap_or(SessionScreenTab::Chat);
    let layout = if wide {
        if active != SessionScreenTab::Chat || open.is_some() {
            SessionScreenLayout::Beside
        } else {
            SessionScreenLayout::Chat
        }
    } else if open.is_some() {
        SessionScreenLayout::Item
    } else if active == SessionScreenTab::Chat {
        SessionScreenLayout::Chat
    } else {
        SessionScreenLayout::Tab
    };
    SessionScreen { tabs, active, gear: spec.gear, open, layout }
}

/// One session of the tree for `session_screen_status`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionScreenStatusInput {
    pub status: String,
    pub busy: bool,
    pub needs_input: bool,
    /// The tree's root.
    pub root: bool,
}

/// THE tree's status word (`sessionScreenStatus`, `statusCases`): needs
/// input > working > queued (the root) > done (the root ended) > idle.
pub fn session_screen_status(sessions: &[SessionScreenStatusInput]) -> String {
    let words = &SESSION_SCREEN.copy.status;
    let live: Vec<&SessionScreenStatusInput> = sessions.iter().filter(|session| session.status != "ended").collect();
    let root = sessions.iter().find(|session| session.root);
    if live.iter().any(|session| session.needs_input) {
        return words.needs_input.clone();
    }
    if live.iter().any(|session| session.busy) {
        return words.working.clone();
    }
    match root.map(|root| root.status.as_str()) {
        Some("queued") => words.queued.clone(),
        Some("ended") => words.done.clone(),
        _ => words.idle.clone(),
    }
}

/// TONE ×4 (`sessionScreenTone`): the subtitle wears apps.json
/// `editorHeader`'s tones: attention (the needs-input amber) while the word
/// is needs input (an app also while its device part is `noDeviceOnline`),
/// active while working or queued, else muted.
pub fn session_screen_tone(status: &str, offline: bool) -> crate::apps::EditorHeaderTone {
    use crate::apps::EditorHeaderTone;
    let words = &SESSION_SCREEN.copy.status;
    if status == words.needs_input || offline {
        EditorHeaderTone::Attention
    } else if status == words.working || status == words.queued {
        EditorHeaderTone::Active
    } else {
        EditorHeaderTone::Muted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn fixture() -> Value {
        serde_json::from_str(SESSION_SCREEN_JSON).unwrap()
    }

    #[test]
    fn the_spec_parses() {
        assert_eq!(SESSION_SCREEN.test_ids.gear, "session-gear");
        assert_eq!(SESSION_SCREEN.copy.settings, "Settings");
        assert!(session_screen_gear("agent"));
        assert!(!session_screen_gear("session"));
        assert_eq!(session_screen_tabs("bogus"), session_screen_tabs("session"));
    }

    #[test]
    fn cases_match_the_fixture() {
        let cases = fixture()["cases"].as_array().unwrap().clone();
        assert!(!cases.is_empty());
        for case in cases {
            let input = &case["input"];
            let counts: SessionScreenCounts = serde_json::from_value(input["counts"].clone()).unwrap();
            let actual = session_screen(
                input["kind"].as_str().unwrap(),
                input["wide"].as_bool().unwrap(),
                input["requested"].as_str(),
                input["open"].as_str(),
                counts,
            );
            assert_eq!(serde_json::to_value(&actual).unwrap(), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn kind_cases_match_the_fixture() {
        let cases = fixture()["kindCases"].as_array().unwrap().clone();
        assert!(!cases.is_empty());
        for case in cases {
            let input = &case["input"];
            let actual = session_screen_kind(
                input["agentId"].as_str(),
                input["appId"].as_str(),
                input["assistant"].as_bool().unwrap(),
            );
            assert_eq!(serde_json::to_value(actual).unwrap(), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn subtitle_cases_match_the_fixture() {
        let cases = fixture()["subtitleCases"].as_array().unwrap().clone();
        assert!(!cases.is_empty());
        for case in cases {
            let input = &case["input"];
            let actual =
                session_screen_subtitle(input["status"].as_str(), input["device"].as_str(), input["trigger"].as_str());
            assert_eq!(actual, case["expected"].as_str().unwrap(), "{}", case["name"]);
        }
    }

    #[test]
    fn status_cases_match_the_fixture() {
        let cases = fixture()["statusCases"].as_array().unwrap().clone();
        assert!(!cases.is_empty());
        for case in cases {
            let input: Vec<SessionScreenStatusInput> = serde_json::from_value(case["input"].clone()).unwrap();
            assert_eq!(session_screen_status(&input), case["expected"].as_str().unwrap(), "{}", case["name"]);
        }
    }

    #[test]
    fn tone_follows_the_word() {
        use crate::apps::EditorHeaderTone::{Active, Attention, Muted};
        let words = &SESSION_SCREEN.copy.status;
        assert_eq!(session_screen_tone(&words.needs_input, false), Attention);
        assert_eq!(session_screen_tone(&words.working, false), Active);
        assert_eq!(session_screen_tone(&words.queued, false), Active);
        assert_eq!(session_screen_tone(&words.done, false), Muted);
        assert_eq!(session_screen_tone(&words.idle, false), Muted);
        assert_eq!(session_screen_tone(&words.idle, true), Attention);
    }
}
