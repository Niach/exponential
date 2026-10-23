//! Thread 58cb32df (#EXP-1373): CHATS, the desktop twin of
//! `packages/domain-contract/src/chats.ts` (iOS ExpCore `Chats.swift`,
//! Android `domain/Chats.kt`). The IDE has no phone list, so this mirror
//! keeps the parts the IDE paints: the copy, the context menus ×4
//! ([`thread_menu_entries`], [`agent_menu_entries`]), the transcript's
//! command dividers ([`command_divider`]), the surface card's reply rule
//! ([`surface_reply`]) + its ENTER motion spec, and [`plain_line`].
//! Byte-locked by `fixtures/chats.json`; pure.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;
use serde::Deserialize;
use serde_json::Value;

const CHATS_JSON: &str = include_str!("../../../../../packages/domain-contract/fixtures/chats.json");

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatsMenuCopy {
    pub stop: String,
    pub stop_and_delete: String,
    pub delete: String,
    pub compact: String,
    pub new_conversation: String,
    pub settings: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ChatsConfirmCopy {
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatsToastCopy {
    pub stop_failed: String,
    pub delete_failed: String,
    pub stop_not_delivered: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ChatsDividerCopy {
    pub new: String,
    pub compact: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ChatsFilterCopy {
    pub all: String,
    pub needs_you: String,
    pub running: String,
    pub unread: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ChatsTabCopy {
    pub board: String,
    pub devices: String,
    pub chats: String,
    pub team: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ChatsPreviewCopy {
    pub working: String,
    pub question: String,
    pub plan: String,
    pub you: String,
    pub picture: String,
    pub surface: String,
    pub empty: String,
}

/// `chats.json` `copy` (the parts the IDE and its styleguide twins paint).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatsCopy {
    pub title: String,
    pub tabs: ChatsTabCopy,
    pub filters: ChatsFilterCopy,
    pub preview: ChatsPreviewCopy,
    pub menu: ChatsMenuCopy,
    pub confirm_delete: ChatsConfirmCopy,
    pub toasts: ChatsToastCopy,
    pub dividers: ChatsDividerCopy,
    pub superseded: String,
    pub inbox: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatsTestIds {
    pub chat_row: String,
    pub chat_filter: String,
    pub agent_bubble: String,
    pub surface_card: String,
    pub divider: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceEnterFrame {
    pub from_opacity: f32,
    pub from_translate_y: f32,
    pub from_scale: f32,
    /// A motion duration token (`slow`).
    pub duration: String,
    /// A motion ease token (`decelerate`).
    pub ease: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceEnterChildren {
    pub from_opacity: f32,
    pub from_translate_y: f32,
    pub duration: String,
    pub ease: String,
    pub stagger_ms: u64,
    pub max_staggered: usize,
    pub start_after_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceEnterGlow {
    pub from_opacity: f32,
    pub duration: String,
    pub ease: String,
}

/// `surfaceCard.enter`: the live-arrival animation (motion tokens only).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SurfaceEnter {
    pub frame: SurfaceEnterFrame,
    pub children: SurfaceEnterChildren,
    pub glow: SurfaceEnterGlow,
}

impl SurfaceEnter {
    /// The delay before the `index`th first-level child enters: the start,
    /// then `staggerMs` apart; children past `maxStaggered` share the last.
    pub fn child_delay_ms(&self, index: usize) -> u64 {
        let slot = index.min(self.children.max_staggered.saturating_sub(1));
        self.children.start_after_ms + self.children.stagger_ms * slot as u64
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceCardSpec {
    pub reply_event: String,
    pub surface_max_bytes: usize,
    pub surface_max_components: usize,
    pub enter: SurfaceEnter,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AgentMenuSpec {
    pub sends: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatsShell {
    pub lead_disc: f32,
    pub bubble: f32,
    pub bubble_ring: f32,
}

/// `fixtures/chats.json` minus its cases.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatsSpec {
    pub copy: ChatsCopy,
    pub test_ids: ChatsTestIds,
    pub agent_menu: AgentMenuSpec,
    pub surface_card: SurfaceCardSpec,
    pub shell: ChatsShell,
}

pub static CHATS: LazyLock<ChatsSpec> = LazyLock::new(|| serde_json::from_str(CHATS_JSON).expect("chats.json parses"));

/// A thread row's context-menu entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadMenuEntry {
    Stop,
    StopAndDelete,
    Delete,
}

impl ThreadMenuEntry {
    /// The fixture key (`stop` | `stopAndDelete` | `delete`).
    pub fn key(self) -> &'static str {
        match self {
            Self::Stop => "stop",
            Self::StopAndDelete => "stopAndDelete",
            Self::Delete => "delete",
        }
    }

    pub fn label(self) -> &'static str {
        let menu = &CHATS.copy.menu;
        match self {
            Self::Stop => &menu.stop,
            Self::StopAndDelete => &menu.stop_and_delete,
            Self::Delete => &menu.delete,
        }
    }

    /// The icon concept (`coding-stop` | `ui-delete`).
    pub fn icon(self) -> &'static str {
        match self {
            Self::Stop => "coding-stop",
            Self::StopAndDelete | Self::Delete => "ui-delete",
        }
    }

    pub fn destructive(self) -> bool {
        !matches!(self, Self::Stop)
    }

    /// Asks `copy.confirmDelete` first.
    pub fn deletes(self) -> bool {
        self.destructive()
    }
}

/// A thread row's context menu ×4 (`menuCases`): live → Stop, Stop and
/// delete; ended → Delete; nothing for a viewer who cannot steer it.
pub fn thread_menu_entries(root_status: &str, can_steer: bool) -> Vec<ThreadMenuEntry> {
    if !can_steer {
        return Vec::new();
    }
    if root_status == "ended" {
        vec![ThreadMenuEntry::Delete]
    } else {
        vec![ThreadMenuEntry::Stop, ThreadMenuEntry::StopAndDelete]
    }
}

/// The agent chat's menu entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentMenuEntry {
    Compact,
    NewConversation,
    Stop,
    Settings,
}

impl AgentMenuEntry {
    pub fn key(self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::NewConversation => "newConversation",
            Self::Stop => "stop",
            Self::Settings => "settings",
        }
    }

    pub fn label(self) -> &'static str {
        let menu = &CHATS.copy.menu;
        match self {
            Self::Compact => &menu.compact,
            Self::NewConversation => &menu.new_conversation,
            Self::Stop => &menu.stop,
            Self::Settings => &menu.settings,
        }
    }

    /// The icon concept.
    pub fn icon(self) -> &'static str {
        match self {
            Self::Compact => "coding-compact",
            Self::NewConversation => "ui-clean",
            Self::Stop => "coding-stop",
            Self::Settings => "nav-settings",
        }
    }

    /// The builtin line a pick sends (`agentMenu.sends`): Compact and New
    /// conversation, one path with the typed command.
    pub fn sends(self) -> Option<&'static str> {
        CHATS.agent_menu.sends.get(self.key()).map(String::as_str)
    }
}

/// The agent chat's header menu (and its rail row's right-click) ×4
/// (`agentMenuCases`).
pub fn agent_menu_entries(busy: bool, is_owner: bool) -> Vec<AgentMenuEntry> {
    let mut entries = vec![AgentMenuEntry::Compact, AgentMenuEntry::NewConversation];
    if busy {
        entries.push(AgentMenuEntry::Stop);
    }
    if is_owner {
        entries.push(AgentMenuEntry::Settings);
    }
    entries
}

/// A builtin command line's divider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DividerKind {
    New,
    Compact,
}

impl DividerKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::New => &CHATS.copy.dividers.new,
            Self::Compact => &CHATS.copy.dividers.compact,
        }
    }

    /// The icon concept (`ui-clean` | `coding-compact`).
    pub fn icon(self) -> &'static str {
        match self {
            Self::New => "ui-clean",
            Self::Compact => "coding-compact",
        }
    }
}

/// A person's builtin command line (`/new`, `/compact …`): the transcript
/// paints it as a divider, never a bubble (`dividerCases`).
pub fn command_divider(body: &str) -> Option<DividerKind> {
    let text = body.trim();
    if text == "/new" {
        Some(DividerKind::New)
    } else if text == "/compact" || text.starts_with("/compact ") {
        Some(DividerKind::Compact)
    } else {
        None
    }
}

/// A press inside an inline preview (`replyCases`): an event named `reply`
/// with a non-empty `context.text` posts that text as the person's
/// message; anything else does nothing.
pub fn surface_reply(name: Option<&str>, context: Option<&Value>) -> Option<String> {
    if name != Some(CHATS.surface_card.reply_event.as_str()) {
        return None;
    }
    let text = context?.get("text")?.as_str()?.trim();
    (!text.is_empty()).then(|| text.to_string())
}

static IMAGE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"!\[[^\]]*\]\([^)]*\)").unwrap());
static LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[([^\]]*)\]\([^)]*\)").unwrap());
static LINE_PREFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(?:#{1,6}\s+|>\s?|[-*+]\s+|\d+[.)]\s+)").unwrap());
static MARKS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\*\*|__|~~|`").unwrap());
static EM_OPEN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(^|\s)[*_](\S)").unwrap());
static EM_CLOSE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\S)[*_](\s|$)").unwrap());
static SPACE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());

/// Markdown → one plain line (`plainCases`).
pub fn plain_line(markdown: &str) -> String {
    let text = IMAGE.replace_all(markdown, " ");
    let text = LINK.replace_all(&text, "$1");
    let text = text
        .split('\n')
        .map(|line| LINE_PREFIX.replace(line, "").into_owned())
        .collect::<Vec<_>>()
        .join(" ");
    let text = MARKS.replace_all(&text, "");
    let text = EM_OPEN.replace_all(&text, "$1$2");
    let text = EM_CLOSE.replace_all(&text, "$1$2");
    SPACE.replace_all(&text, " ").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        serde_json::from_str(CHATS_JSON).unwrap()
    }

    #[test]
    fn spec_parses() {
        assert_eq!(CHATS.copy.title, "Chats");
        assert_eq!(CHATS.surface_card.reply_event, "reply");
    }

    #[test]
    fn menu_cases_match_the_fixture() {
        let fixture = fixture();
        let cases = fixture["menuCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let input = &case["input"];
            let got: Vec<&str> = thread_menu_entries(
                input["rootStatus"].as_str().unwrap(),
                input["canSteer"].as_bool().unwrap(),
            )
            .into_iter()
            .map(ThreadMenuEntry::key)
            .collect();
            let want: Vec<&str> = case["expected"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
            assert_eq!(got, want, "{}", case["name"]);
        }
    }

    #[test]
    fn agent_menu_cases_match_the_fixture() {
        let fixture = fixture();
        let cases = fixture["agentMenuCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let input = &case["input"];
            let got: Vec<&str> =
                agent_menu_entries(input["busy"].as_bool().unwrap(), input["isOwner"].as_bool().unwrap())
                    .into_iter()
                    .map(AgentMenuEntry::key)
                    .collect();
            let want: Vec<&str> = case["expected"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
            assert_eq!(got, want, "{}", case["name"]);
        }
        assert_eq!(AgentMenuEntry::Compact.sends(), Some("/compact"));
        assert_eq!(AgentMenuEntry::NewConversation.sends(), Some("/new"));
        assert_eq!(AgentMenuEntry::Stop.sends(), None);
    }

    #[test]
    fn reply_cases_match_the_fixture() {
        let fixture = fixture();
        let cases = fixture["replyCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let event = &case["event"];
            let got = surface_reply(event["name"].as_str(), event.get("context"));
            let want = case["expected"]["text"].as_str().map(str::to_string);
            assert_eq!(got, want, "{}", case["name"]);
        }
    }

    #[test]
    fn divider_cases_match_the_fixture() {
        let fixture = fixture();
        let cases = fixture["dividerCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let body = case["body"].as_str().unwrap();
            let got = command_divider(body).map(|kind| match kind {
                DividerKind::New => "new",
                DividerKind::Compact => "compact",
            });
            assert_eq!(got, case["expected"].as_str(), "{body:?}");
        }
    }

    #[test]
    fn plain_cases_match_the_fixture() {
        let fixture = fixture();
        let cases = fixture["plainCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let text = case["text"].as_str().unwrap();
            assert_eq!(plain_line(text), case["expected"].as_str().unwrap(), "{text:?}");
        }
    }

    #[test]
    fn child_delays_share_the_last_slot() {
        let enter = &CHATS.surface_card.enter;
        assert_eq!(enter.child_delay_ms(0), enter.children.start_after_ms);
        let last = enter.child_delay_ms(enter.children.max_staggered - 1);
        assert_eq!(enter.child_delay_ms(enter.children.max_staggered + 3), last);
    }
}
