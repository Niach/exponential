//! Thread 58cb32df (#EXP-1373): CHATS, the desktop twin of
//! `packages/domain-contract/src/chats.ts` (iOS ExpCore `Chats.swift`,
//! Android `domain/Chats.kt`), replaying EVERY case group (#EXP-1380: the
//! IDE's Chats page paints the same list): the list ([`chat_list`],
//! [`chat_preview`], [`chat_time_label`], [`chat_filter_label`],
//! [`unread_count`]), the bubbles ([`agent_bubbles`]), the context menus ×4
//! ([`thread_menu_entries`], [`agent_menu_entries`]), the transcript's
//! command dividers ([`command_divider`]) + its window ([`TranscriptSpec`]),
//! the surface card's reply rule ([`surface_reply`]) + its ENTER motion
//! spec, and [`plain_line`]. Byte-locked by `fixtures/chats.json`; pure.

use std::collections::{BTreeMap, HashMap};
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::rows::{MessageRow, SessionReadRow, SessionRow};
use crate::session_tree::{
    iso_ms, thread_row_lead_pr, thread_row_mark_state, AgentRow, PrLead, SessionState, ThreadRowWord, ThreadsListGroup,
    ThreadsListKey,
};

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
    pub chats: String,
    pub apps: String,
    pub team: String,
}

/// `copy.headers` (#EXP-1384: the phone page header's titles).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ChatsHeaderCopy {
    pub chats: String,
    pub apps: String,
    pub devices: String,
    pub agent: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatsBoardMenuCopy {
    pub new_board: String,
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

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatsTimeCopy {
    pub yesterday: String,
    pub weekdays_short: Vec<String>,
    pub months_short: Vec<String>,
}

/// `chats.json` `copy` (the parts the IDE and its styleguide twins paint).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatsCopy {
    pub title: String,
    pub tabs: ChatsTabCopy,
    pub headers: ChatsHeaderCopy,
    pub board_menu: ChatsBoardMenuCopy,
    pub filters: ChatsFilterCopy,
    pub preview: ChatsPreviewCopy,
    pub menu: ChatsMenuCopy,
    pub confirm_delete: ChatsConfirmCopy,
    pub toasts: ChatsToastCopy,
    pub dividers: ChatsDividerCopy,
    pub superseded: String,
    pub inbox: String,
    pub time: ChatsTimeCopy,
    pub load_earlier: String,
    pub working: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatsTestIds {
    pub chat_row: String,
    pub chat_filter: String,
    pub agent_bubble: String,
    pub surface_card: String,
    pub divider: String,
    pub load_earlier: String,
    pub jump_to_bottom: String,
}

/// `transcript` (#EXP-1380 WINDOW + FOLLOW): a chat paints its newest
/// `window` rows, `Load earlier` widens by `window_step`; the transcript
/// follows the tail within `follow_slack` px of the bottom.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptSpec {
    pub window: usize,
    pub window_step: usize,
    pub follow_slack: f32,
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
    /// The phone bar's tabs in order (#EXP-1384: board, chats, apps, team).
    pub tabs: Vec<String>,
    pub bar: ChatsBar,
    pub board_tab: ChatsBoardTab,
    pub board_menu: ChatsBoardMenu,
    pub header: ChatsHeader,
    pub lead_disc: f32,
    pub bubble: f32,
    pub bubble_ring: f32,
}

/// `shell.bar.drag`: the selection lens.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatsBarDrag {
    pub lens_scale: f32,
    pub cancel_distance: f32,
}

/// `shell.bar` (#EXP-1384): the phone tab capsule's geometry.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatsBar {
    pub height: f32,
    pub padding: f32,
    pub gap: f32,
    pub tab_min_width: f32,
    pub tab_padding_x: f32,
    pub drag: ChatsBarDrag,
}

/// `shell.boardTab`: the caret concept on the board view.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatsBoardTab {
    pub caret: String,
    pub caret_size: f32,
}

/// `shell.boardMenu` (#EXP-1384): the strip morphed into the board menu.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatsBoardMenu {
    pub row_height: f32,
    pub max_visible_rows: usize,
    pub radius: f32,
}

/// `shell.header` (#EXP-1384): the phone page header.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ChatsHeader {
    pub pages: Vec<String>,
    pub buttons: Vec<String>,
    pub circle: f32,
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
    pub transcript: TranscriptSpec,
    pub agent_hint: AgentHintSpec,
}

/// #EXP-1383: the agent chat's "Instructions changed" Dock banner row
/// (dock.json kind `banner`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AgentHintBanner {
    pub id: String,
    /// An icon concept (`ui-info`).
    pub icon: String,
    pub label: String,
    pub text: String,
    pub tone: String,
    /// The action word; it sends [`AgentHintSpec::sends`].
    pub action: String,
}

/// #EXP-1383 `agentHint` minus its cases.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AgentHintSpec {
    pub banner: AgentHintBanner,
    /// `/new` (the agent menu's New conversation).
    pub sends: String,
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
/// delete; ended → Delete; nothing for a viewer who cannot steer it. The
/// delete entries need `can_delete` (the chat's starter or a team owner, the
/// server's `sessions.delete` rule).
pub fn thread_menu_entries(root_status: &str, can_steer: bool, can_delete: bool) -> Vec<ThreadMenuEntry> {
    if !can_steer {
        return Vec::new();
    }
    match (root_status == "ended", can_delete) {
        (true, true) => vec![ThreadMenuEntry::Delete],
        (true, false) => Vec::new(),
        (false, true) => vec![ThreadMenuEntry::Stop, ThreadMenuEntry::StopAndDelete],
        (false, false) => vec![ThreadMenuEntry::Stop],
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

/// #EXP-1383 (`agentHint.cases`, TS `agentInstructionsChanged`): the agent
/// chat's root session fetched its brief BEFORE the agent's instructions last
/// changed. Either stamp missing (or unparseable) = no hint; equal = current.
pub fn agent_instructions_changed(session_stamp: Option<&str>, agent_stamp: Option<&str>) -> bool {
    let (Some(session), Some(agent)) = (
        session_stamp.and_then(crate::activity_fold::parse_instant),
        agent_stamp.and_then(crate::activity_fold::parse_instant),
    ) else {
        return false;
    };
    session < agent
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

// ---------------------------------------------------------------------------
// LIST + BUBBLES (#EXP-1380: every form factor, the IDE included)
// ---------------------------------------------------------------------------

/// A filter chip (`filters`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatFilter {
    All,
    NeedsYou,
    Running,
    Unread,
}

impl ChatFilter {
    pub const ALL: [ChatFilter; 4] = [Self::All, Self::NeedsYou, Self::Running, Self::Unread];

    /// The fixture key (`all` | `needs_you` | `running` | `unread`).
    pub fn key(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::NeedsYou => "needs_you",
            Self::Running => "running",
            Self::Unread => "unread",
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|filter| filter.key() == key)
    }

    /// The chip's copy (no count).
    pub fn copy(self) -> &'static str {
        let filters = &CHATS.copy.filters;
        match self {
            Self::All => &filters.all,
            Self::NeedsYou => &filters.needs_you,
            Self::Running => &filters.running,
            Self::Unread => &filters.unread,
        }
    }
}

/// A row's lead disc content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatLead {
    NeedsYou,
    Working,
    Open,
    Merged,
    Waiting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatPreviewKind {
    Working,
    Question,
    Plan,
    You,
    Agent,
    Picture,
    Surface,
    New,
    Compact,
    Empty,
}

/// A row's preview line: the whole line as painted (prefix included).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChatPreview {
    pub kind: ChatPreviewKind,
    pub text: String,
}

/// One row of the Chats list.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatRow {
    pub session_id: String,
    pub team_id: String,
    pub title: String,
    pub device_id: Option<String>,
    pub group: ThreadsListKey,
    pub lead: ChatLead,
    pub preview: ChatPreview,
    pub activity_at: String,
    pub time: String,
    /// Agent messages in the ROOT conversation newer than the root's read.
    pub unread_count: usize,
    pub needs_you: bool,
    pub busy: bool,
    pub word: Option<ThreadRowWord>,
}

/// The chip counts (All has none).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChatCounts {
    pub needs_you: usize,
    pub running: usize,
    pub unread: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatList {
    pub rows: Vec<ChatRow>,
    pub counts: ChatCounts,
}

const PREVIEW_CARDS: [&str; 5] = ["question", "plan", "ask", "picture", "surface"];

fn ms(iso: Option<&str>) -> Option<i64> {
    iso.filter(|value| !value.is_empty()).and_then(iso_ms)
}

fn card_kind(message: &MessageRow) -> Option<&str> {
    message.card.as_ref()?.get("kind")?.as_str()
}

/// JS `String(value ?? '')`.
fn card_text(message: &MessageRow, key: &str) -> String {
    match message.card.as_ref().and_then(|card| card.get(key)) {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Bool(flag)) => flag.to_string(),
        Some(Value::Number(number)) => number.to_string(),
        Some(other) => other.to_string(),
    }
}

fn is_author(message: &MessageRow, kind: &str) -> bool {
    message.author_kind.as_deref() == Some(kind)
}

/// The root conversation's preview line (`previewCases`). `messages` may
/// hold other sessions' rows: only `root_id`'s own count.
pub fn chat_preview(root_id: &str, messages: &[&MessageRow], needs_you: bool, busy: bool) -> ChatPreview {
    let copy = &CHATS.copy.preview;
    let mut own: Vec<&MessageRow> = messages
        .iter()
        .copied()
        .filter(|m| m.session_id == root_id && (is_author(m, "user") || is_author(m, "agent")))
        .filter(|m| match m.card.as_ref() {
            Some(_) => card_kind(m).is_some_and(|kind| PREVIEW_CARDS.contains(&kind)),
            None => !m.body.as_deref().unwrap_or_default().trim().is_empty(),
        })
        .collect();
    own.sort_by(|a, b| {
        ms(b.created_at.as_deref())
            .unwrap_or(0)
            .cmp(&ms(a.created_at.as_deref()).unwrap_or(0))
            .then_with(|| b.id.cmp(&a.id))
    });
    let fill = |template: &str, text: &str| template.replace("{text}", &plain_line(text));
    if needs_you {
        let waiting = own.iter().find(|m| {
            m.answered_at.is_none() && matches!(card_kind(m), Some("question" | "plan" | "ask"))
        });
        if let Some(waiting) = waiting {
            return if card_kind(waiting) == Some("plan") {
                ChatPreview { kind: ChatPreviewKind::Plan, text: fill(&copy.plan, &card_text(waiting, "plan")) }
            } else {
                ChatPreview {
                    kind: ChatPreviewKind::Question,
                    text: fill(&copy.question, &card_text(waiting, "question")),
                }
            };
        }
    }
    if busy {
        return ChatPreview { kind: ChatPreviewKind::Working, text: copy.working.clone() };
    }
    let Some(newest) = own.first() else {
        return ChatPreview { kind: ChatPreviewKind::Empty, text: copy.empty.clone() };
    };
    let body = newest.body.as_deref().unwrap_or_default();
    if is_author(newest, "user") {
        return match command_divider(body) {
            Some(divider) => ChatPreview {
                kind: match divider {
                    DividerKind::New => ChatPreviewKind::New,
                    DividerKind::Compact => ChatPreviewKind::Compact,
                },
                text: divider.label().to_string(),
            },
            None => ChatPreview { kind: ChatPreviewKind::You, text: fill(&copy.you, body) },
        };
    }
    match card_kind(newest) {
        Some("picture") => ChatPreview { kind: ChatPreviewKind::Picture, text: copy.picture.clone() },
        Some("surface") => {
            let title = newest
                .card
                .as_ref()
                .and_then(|card| card.get("title"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|title| !title.is_empty());
            ChatPreview {
                kind: ChatPreviewKind::Surface,
                text: title.map(str::to_string).unwrap_or_else(|| copy.surface.clone()),
            }
        }
        Some("question" | "ask") => ChatPreview {
            kind: ChatPreviewKind::Question,
            text: fill(&copy.question, &card_text(newest, "question")),
        },
        Some("plan") => ChatPreview { kind: ChatPreviewKind::Plan, text: fill(&copy.plan, &card_text(newest, "plan")) },
        _ => ChatPreview { kind: ChatPreviewKind::Agent, text: plain_line(body) },
    }
}

/// Days since 1970-01-01 → (year, month 1-12, day).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// The trailing time label (`timeCases`): the viewer's local day by
/// `utc_offset_minutes` — today `HH:mm`, yesterday, a short weekday under
/// 7 days, else `Mon D` (`, YYYY` from another year).
pub fn chat_time_label(at: &str, now: &str, utc_offset_minutes: i32) -> String {
    const DAY: i64 = 86_400_000;
    let copy = &CHATS.copy.time;
    let shift = i64::from(utc_offset_minutes) * 60_000;
    let local = ms(Some(at)).unwrap_or(0) + shift;
    let today = ms(Some(now)).unwrap_or(0) + shift;
    let (local_day, today_day) = (local.div_euclid(DAY), today.div_euclid(DAY));
    let diff = today_day - local_day;
    if diff <= 0 {
        let minutes = local.rem_euclid(DAY) / 60_000;
        return format!("{:02}:{:02}", minutes / 60, minutes % 60);
    }
    if diff == 1 {
        return copy.yesterday.clone();
    }
    if diff < 7 {
        // 1970-01-01 was a Thursday: Monday-first index.
        let monday_first = (local_day + 3).rem_euclid(7) as usize;
        return copy.weekdays_short[monday_first].clone();
    }
    let (year, month, day) = civil_from_days(local_day);
    let (now_year, _, _) = civil_from_days(today_day);
    let date = format!("{} {day}", copy.months_short[month as usize - 1]);
    if year == now_year {
        date
    } else {
        format!("{date}, {year}")
    }
}

/// Agent messages of `root_id`'s own conversation (event cards excluded)
/// newer than the viewer's read of it.
pub fn unread_count(root_id: &str, messages: &[&MessageRow], reads: &[&SessionReadRow]) -> usize {
    let read = reads
        .iter()
        .filter(|read| read.session_id == root_id)
        .filter_map(|read| ms(read.read_at.as_deref()))
        .max()
        .unwrap_or(-1);
    messages
        .iter()
        .filter(|m| {
            m.session_id == root_id
                && is_author(m, "agent")
                && card_kind(m) != Some("event")
                && ms(m.created_at.as_deref()).unwrap_or(0) > read
        })
        .count()
}

/// [`chat_list`]'s input.
#[derive(Debug, Clone, Copy)]
pub struct ChatListInput<'a> {
    pub groups: &'a [ThreadsListGroup],
    pub messages: &'a [&'a MessageRow],
    pub reads: &'a [&'a SessionReadRow],
    /// The ROOT's PR states by session id (absent = none).
    pub pr_states: &'a HashMap<String, Vec<String>>,
    /// `thread_row_word` by session id (absent = none).
    pub words: &'a HashMap<String, ThreadRowWord>,
    pub filter: ChatFilter,
    pub now: &'a str,
    pub utc_offset_minutes: i32,
}

fn keeps(row: &ChatRow, filter: ChatFilter) -> bool {
    filter.keeps(row)
}

impl ChatFilter {
    /// The chip keeps `row`: its group (needs you / running) or an unread
    /// count.
    pub fn keeps(self, row: &ChatRow) -> bool {
        match self {
            Self::All => true,
            Self::NeedsYou => row.group == ThreadsListKey::NeedsYou,
            Self::Running => row.group == ThreadsListKey::Running,
            Self::Unread => row.unread_count > 0,
        }
    }
}

/// The Chats list (`listCases`): the ROOT rows of `partition_threads`
/// (active team) flattened and sorted by activity desc (ties by id), each
/// with its lead, preview, time and unread count, then filtered by the chip.
pub fn chat_list(input: ChatListInput<'_>) -> ChatList {
    // One pass over the messages: each root reads only its own.
    let mut by_session: HashMap<&str, Vec<&MessageRow>> = HashMap::new();
    for message in input.messages {
        by_session.entry(message.session_id.as_str()).or_default().push(message);
    }
    let none: Vec<&MessageRow> = Vec::new();
    let mut all: Vec<ChatRow> = input
        .groups
        .iter()
        .flat_map(|group| group.rows.iter().filter(|row| row.depth == 0).map(move |row| (row, group.key)))
        .map(|(row, group)| {
            let own = by_session.get(row.session_id.as_str()).unwrap_or(&none);
            let lead = match thread_row_mark_state(row.needs_you, row.busy) {
                SessionState::NeedsYou => ChatLead::NeedsYou,
                SessionState::Working => ChatLead::Working,
                _ => match thread_row_lead_pr(
                    input.pr_states.get(&row.session_id).into_iter().flatten().map(String::as_str),
                ) {
                    Some(PrLead::Open) => ChatLead::Open,
                    Some(PrLead::Merged) => ChatLead::Merged,
                    None => ChatLead::Waiting,
                },
            };
            ChatRow {
                session_id: row.session_id.clone(),
                team_id: row.team_id.clone(),
                title: row.title.clone(),
                device_id: row.device_id.clone(),
                group,
                lead,
                preview: chat_preview(&row.session_id, own, row.needs_you, row.busy),
                activity_at: row.activity_at.clone(),
                time: chat_time_label(&row.activity_at, input.now, input.utc_offset_minutes),
                unread_count: unread_count(&row.session_id, own, input.reads),
                needs_you: row.needs_you,
                busy: row.busy,
                word: input.words.get(&row.session_id).copied(),
            }
        })
        .collect();
    all.sort_by(|a, b| {
        ms(Some(&b.activity_at))
            .unwrap_or(0)
            .cmp(&ms(Some(&a.activity_at)).unwrap_or(0))
            .then_with(|| a.session_id.cmp(&b.session_id))
    });
    let count = |filter| all.iter().filter(|row| keeps(row, filter)).count();
    let counts = ChatCounts {
        needs_you: count(ChatFilter::NeedsYou),
        running: count(ChatFilter::Running),
        unread: count(ChatFilter::Unread),
    };
    ChatList { rows: all.into_iter().filter(|row| keeps(row, input.filter)).collect(), counts }
}

/// A chip's label: its copy, then its count when there is one.
pub fn chat_filter_label(filter: ChatFilter, counts: &ChatCounts) -> String {
    let label = filter.copy();
    let n = match filter {
        ChatFilter::All => return label.to_string(),
        ChatFilter::NeedsYou => counts.needs_you,
        ChatFilter::Running => counts.running,
        ChatFilter::Unread => counts.unread,
    };
    if n > 0 {
        format!("{label} {n}")
    } else {
        label.to_string()
    }
}

/// A bubble's ring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRing {
    NeedsYou,
    Unread,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentBubble {
    #[serde(flatten)]
    pub agent: AgentRow,
    pub ring: Option<AgentRing>,
}

/// The Chats header's agent bubbles (`bubbleCases`), in `list_agents`
/// order: amber ring = the viewer's conversation needs them, white ring =
/// an agent message newer than their read of it, else none.
pub fn agent_bubbles(
    agents: &[AgentRow],
    sessions: &[&SessionRow],
    messages: &[&MessageRow],
    reads: &[&SessionReadRow],
    me: Option<&str>,
) -> Vec<AgentBubble> {
    agents
        .iter()
        .map(|agent| {
            if agent.needs_you {
                return AgentBubble { agent: agent.clone(), ring: Some(AgentRing::NeedsYou) };
            }
            let conversation = sessions.iter().find(|session| {
                session.is_root()
                    && session.agent_id.as_deref() == Some(agent.agent_id.as_str())
                    && session.user_id.as_deref() == me
            });
            let unread = conversation.is_some_and(|session| unread_count(&session.id, messages, reads) > 0);
            AgentBubble { agent: agent.clone(), ring: unread.then_some(AgentRing::Unread) }
        })
        .collect()
}

/// WINDOW (#EXP-1380): the first index painted of `total` rows under a
/// window of `window` rows (the newest `window`; 0 = everything).
pub fn transcript_window_start(total: usize, window: usize) -> usize {
    total.saturating_sub(window)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        serde_json::from_str(CHATS_JSON).unwrap()
    }

    #[test]
    fn agent_hint_cases_match_the_fixture() {
        let fixture = fixture();
        let hint = &fixture["agentHint"];
        let banner = &CHATS.agent_hint.banner;
        assert_eq!(hint["banner"]["id"].as_str(), Some(banner.id.as_str()));
        assert_eq!(banner.icon, "ui-info");
        assert_eq!(CHATS.agent_hint.sends, "/new");
        assert_eq!(
            Some(CHATS.agent_hint.sends.as_str()),
            AgentMenuEntry::NewConversation.sends(),
            "the banner sends exactly what New conversation sends"
        );
        let cases = hint["cases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let actual = agent_instructions_changed(
                case["input"]["sessionStamp"].as_str(),
                case["input"]["agentStamp"].as_str(),
            );
            assert_eq!(Some(actual), case["expected"].as_bool(), "{name}");
        }
        // Electric's timestamptz text form compares the same way.
        assert!(agent_instructions_changed(
            Some("2026-10-24 10:00:00.123+00"),
            Some("2026-10-24 10:00:00.456+00")
        ));
    }

    #[test]
    fn spec_parses() {
        assert_eq!(CHATS.copy.title, "Chats");
        assert_eq!(CHATS.surface_card.reply_event, "reply");
        assert_eq!(CHATS.shell.tabs, ["board", "chats", "apps", "team"]);
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
                input["canDelete"].as_bool().unwrap(),
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

    /// A camelCase TS row → the snake_case domain row (top-level keys).
    fn row<T: serde::de::DeserializeOwned>(value: &Value, fallback_id: &str) -> T {
        let mut out = serde_json::Map::new();
        for (key, cell) in value.as_object().expect("row object") {
            let mut snake = String::new();
            for ch in key.chars() {
                if ch.is_ascii_uppercase() {
                    snake.push('_');
                    snake.push(ch.to_ascii_lowercase());
                } else {
                    snake.push(ch);
                }
            }
            out.insert(snake, cell.clone());
        }
        out.entry("id").or_insert_with(|| serde_json::json!(fallback_id));
        serde_json::from_value(Value::Object(out)).expect("fixture row hydrates")
    }

    fn rows<T: serde::de::DeserializeOwned>(value: &Value) -> Vec<T> {
        value
            .as_array()
            .map(|items| items.iter().enumerate().map(|(i, item)| row(item, &format!("row-{i}"))).collect())
            .unwrap_or_default()
    }

    /// A fixture `TreeMessage` is already keyed (its `sessionId`).
    fn messages(value: &Value) -> Vec<MessageRow> {
        value
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .enumerate()
                    .map(|(i, item)| {
                        let mut item = item.clone();
                        let session_id = item["sessionId"].as_str().unwrap_or_default().to_string();
                        item.as_object_mut()
                            .unwrap()
                            .entry("conversationId")
                            .or_insert_with(|| serde_json::json!(format!("c-{session_id}")));
                        let mut message: MessageRow = row(&item, &format!("m-{i}"));
                        message.session_id = session_id;
                        message
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn preview_cases_match_the_fixture() {
        let fixture = fixture();
        let cases = fixture["previewCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let input = &case["input"];
            let all = messages(&input["messages"]);
            let refs: Vec<&MessageRow> = all.iter().collect();
            let got = chat_preview(
                input["rootId"].as_str().unwrap(),
                &refs,
                input["needsYou"].as_bool().unwrap(),
                input["busy"].as_bool().unwrap(),
            );
            assert_eq!(serde_json::to_value(got).unwrap(), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn time_cases_match_the_fixture() {
        let fixture = fixture();
        let cases = fixture["timeCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let got = chat_time_label(
                case["at"].as_str().unwrap(),
                case["now"].as_str().unwrap(),
                case["utcOffsetMinutes"].as_i64().unwrap() as i32,
            );
            assert_eq!(got, case["expected"].as_str().unwrap(), "{}", case["name"]);
        }
    }

    #[test]
    fn list_cases_match_the_fixture() {
        let fixture = fixture();
        let cases = fixture["listCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let input = &case["input"];
            let groups: Vec<ThreadsListGroup> = serde_json::from_value(input["groups"].clone()).unwrap();
            let all = messages(&input["messages"]);
            let refs: Vec<&MessageRow> = all.iter().collect();
            let reads: Vec<SessionReadRow> = rows(&input["reads"]);
            let read_refs: Vec<&SessionReadRow> = reads.iter().collect();
            let pr_states: HashMap<String, Vec<String>> =
                serde_json::from_value(input["prStates"].clone()).unwrap_or_default();
            let words: HashMap<String, ThreadRowWord> =
                serde_json::from_value(input["words"].clone()).unwrap_or_default();
            let filter = ChatFilter::parse(input["filter"].as_str().unwrap()).unwrap();
            let got = chat_list(ChatListInput {
                groups: &groups,
                messages: &refs,
                reads: &read_refs,
                pr_states: &pr_states,
                words: &words,
                filter,
                now: input["now"].as_str().unwrap(),
                utc_offset_minutes: input["utcOffsetMinutes"].as_i64().unwrap() as i32,
            });
            assert_eq!(serde_json::to_value(&got).unwrap(), case["expected"], "{}", case["name"]);
            let labels: Vec<String> = ChatFilter::ALL.iter().map(|f| chat_filter_label(*f, &got.counts)).collect();
            assert_eq!(labels, ["All", "Needs you 1", "Running 1", "Unread 1"]);
        }
        assert_eq!(chat_filter_label(ChatFilter::Unread, &ChatCounts::default()), "Unread");
    }

    #[test]
    fn bubble_cases_match_the_fixture() {
        let fixture = fixture();
        let cases = fixture["bubbleCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let input = &case["input"];
            let agents: Vec<AgentRow> = serde_json::from_value(input["agents"].clone()).unwrap();
            let sessions: Vec<SessionRow> = rows(&input["sessions"]);
            let session_refs: Vec<&SessionRow> = sessions.iter().collect();
            let all = messages(&input["messages"]);
            let refs: Vec<&MessageRow> = all.iter().collect();
            let reads: Vec<SessionReadRow> = rows(&input["reads"]);
            let read_refs: Vec<&SessionReadRow> = reads.iter().collect();
            let got = agent_bubbles(&agents, &session_refs, &refs, &read_refs, input["me"].as_str());
            assert_eq!(serde_json::to_value(&got).unwrap(), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn the_transcript_window_reads_the_fixture() {
        let spec = &CHATS.transcript;
        assert_eq!((spec.window, spec.window_step), (100, 100));
        assert_eq!(spec.follow_slack, 32.);
        assert_eq!(transcript_window_start(1500, 100), 1400);
        assert_eq!(transcript_window_start(40, 100), 0);
        assert_eq!(CHATS.copy.load_earlier, "Load earlier");
        assert_eq!(CHATS.test_ids.jump_to_bottom, "jump-to-bottom");
    }

}
