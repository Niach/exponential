//! #EXP-1736: ONE transcript ×4, every chat a group chat. Who sent each
//! row, which rows share an avatar, the gaps between them, and the device's
//! session-input lines (`Report from …` / `Message from …`) read back as
//! the session that sent them. Byte-locked by
//! `packages/domain-contract/fixtures/transcript-grouping.json` (its
//! `$comment` is the spec); mirrors TS `src/transcript-grouping.ts`, iOS
//! ExpCore `TranscriptGrouping.swift`, Android `domain/TranscriptGrouping.kt`.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;
use serde::Deserialize;

const TRANSCRIPT_GROUPING_JSON: &str =
    include_str!("../../../../../packages/domain-contract/fixtures/transcript-grouping.json");

/// `layout.maxWidth`: the bubble's width cap as a fraction of the column.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptMaxWidth {
    pub phone: f32,
    /// Anyone but the viewer's bubble on a phone.
    pub phone_in: f32,
    /// #FEED-239: the viewer's own bubble, every width (the column left of
    /// the own avatar).
    pub own: f32,
    pub wide: f32,
}

/// The fixture's `layout` (px unless noted).
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptLayout {
    /// THE Avatar's side beside a group.
    pub avatar: f32,
    /// The space between the avatar and the bubble column.
    pub avatar_gap: f32,
    /// The space above a row inside a group.
    pub gap_in_group: f32,
    /// The space above a row that starts a group (or a full row).
    pub gap_between: f32,
    pub max_width: TranscriptMaxWidth,
    /// The bubble's corner radius.
    pub radius: f32,
    /// The sender name's type size.
    pub name_size: f32,
}

/// The fixture's `copy`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptCopy {
    /// The name of a member whose row did not sync.
    pub former_member: String,
    /// The mark a cross-session sender's name ends with.
    pub link_chevron: String,
    /// #EXP-1772: the inline marker's label per system-input key.
    pub markers: std::collections::BTreeMap<String, String>,
    /// #FEED-238: THE fold's text button.
    pub show_more: String,
    pub show_less: String,
}

/// #FEED-238 THE FOLD (`fold`): a bubble over `chars` characters or
/// `lines` lines shows its first `lines` lines behind Show more.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct TranscriptFold {
    pub lines: usize,
    pub chars: usize,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    layout: TranscriptLayout,
    copy: TranscriptCopy,
    fold: TranscriptFold,
}

static FIXTURE: LazyLock<Fixture> = LazyLock::new(|| {
    serde_json::from_str(TRANSCRIPT_GROUPING_JSON).expect("transcript-grouping.json parses")
});

/// `layout` of transcript-grouping.json.
pub fn layout() -> &'static TranscriptLayout {
    &FIXTURE.layout
}

/// `copy` of transcript-grouping.json.
pub fn copy() -> &'static TranscriptCopy {
    &FIXTURE.copy
}

/// `fold` of transcript-grouping.json.
pub fn fold() -> &'static TranscriptFold {
    &FIXTURE.fold
}

/// #FEED-238 THE FOLD ×4 (`messageFolds`, `foldCases`): a message bubble
/// longer than `fold.chars` characters (UTF-16 units, the TS reference's
/// `length`) or `fold.lines` lines folds behind Show more.
pub fn message_folds(text: &str) -> bool {
    let fold = fold();
    text.encode_utf16().count() > fold.chars || text.split('\n').count() > fold.lines
}

/// What a transcript row is to the grouping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscriptItemKind {
    /// A card-less message row, a child's report, a pending input.
    Message,
    /// A tool, thought or subagent line of the agent's work.
    Trace,
    /// Every card (full width, never grouped).
    Full,
}

impl TranscriptItemKind {
    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "message" => Some(Self::Message),
            "trace" => Some(Self::Trace),
            "full" => Some(Self::Full),
            _ => None,
        }
    }
}

/// One row of the transcript, as the grouping reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptItem {
    pub id: String,
    pub kind: TranscriptItemKind,
    /// `message` only: `user` | `agent` | `system`.
    pub author_kind: Option<String>,
    pub author_id: Option<String>,
    pub author_session_id: Option<String>,
}

impl TranscriptItem {
    /// A `trace` row.
    pub fn trace(id: impl Into<String>) -> Self {
        Self { id: id.into(), kind: TranscriptItemKind::Trace, author_kind: None, author_id: None, author_session_id: None }
    }

    /// A `full` row.
    pub fn full(id: impl Into<String>) -> Self {
        Self { id: id.into(), kind: TranscriptItemKind::Full, author_kind: None, author_id: None, author_session_id: None }
    }

    /// A `message` row.
    pub fn message(
        id: impl Into<String>,
        author_kind: &str,
        author_id: Option<&str>,
        author_session_id: Option<&str>,
    ) -> Self {
        Self {
            id: id.into(),
            kind: TranscriptItemKind::Message,
            author_kind: Some(author_kind.to_string()),
            author_id: author_id.map(str::to_string),
            author_session_id: author_session_id.map(str::to_string),
        }
    }
}

/// The chat whose transcript this is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptChat {
    /// The session whose conversation this is.
    pub session_id: String,
    pub title: String,
    pub agent_id: Option<String>,
    /// The session that started it (session-chat.json `origin`, from the
    /// synced rows), else `None`.
    pub origin_session_id: Option<String>,
    /// The items begin at the conversation's first message (no earlier rows
    /// to load). The origin applies only then.
    pub from_start: bool,
}

impl Default for TranscriptChat {
    fn default() -> Self {
        Self { session_id: String::new(), title: String::new(), agent_id: None, origin_session_id: None, from_start: true }
    }
}

/// A member the transcript can name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptMember {
    pub id: String,
    pub name: String,
}

/// An agent the transcript can name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptAgent {
    pub id: String,
    pub name: String,
}

/// A session the transcript knows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptSession {
    pub id: String,
    pub title: String,
    pub agent_id: Option<String>,
}

/// [`transcript_layout`]'s input.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TranscriptInput {
    pub viewer_id: Option<String>,
    pub chat: TranscriptChat,
    pub members: Vec<TranscriptMember>,
    pub agents: Vec<TranscriptAgent>,
    pub sessions: Vec<TranscriptSession>,
    pub items: Vec<TranscriptItem>,
}

/// Which avatar a sender wears.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SenderKind {
    /// A person: the Avatar's initials in the member's hue.
    Member,
    /// An agent: its own avatar (`id` = the agent).
    Agent,
    /// A session: the muted session avatar with its glyph (`id` = the session).
    Session,
}

impl SenderKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Member => "member",
            Self::Agent => "agent",
            Self::Session => "session",
        }
    }
}

/// Who sent a row.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TranscriptSender {
    pub kind: SenderKind,
    pub id: String,
    pub name: String,
    /// A cross-session sender: the session a press opens.
    pub link: Option<String>,
}

impl TranscriptSender {
    /// The name line as it reads: a cross-session sender's ends with ` ›`.
    pub fn display_name(&self) -> String {
        match self.link {
            Some(_) => format!("{} {}", self.name, copy().link_chevron),
            None => self.name.clone(),
        }
    }
}

/// Where a row sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TranscriptSide {
    /// The viewer's own: right, under the VIEWER's avatar (#FEED-239), no name.
    Own,
    /// Anyone else: left, under the sender's avatar.
    Other,
    /// A card: full width, never grouped.
    Full,
}

impl TranscriptSide {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Own => "own",
            Self::Other => "other",
            Self::Full => "full",
        }
    }
}

/// One row's place in the transcript.
#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptRowLayout {
    pub id: String,
    pub side: TranscriptSide,
    pub sender: Option<TranscriptSender>,
    /// The sender's avatar beside this row (a group's first row).
    pub avatar: bool,
    /// The sender's name above this row (a group's first message).
    pub name: bool,
    /// The space above the row, px.
    pub gap: f32,
}

/// THE grouping (the TS `transcriptLayout`).
pub fn transcript_layout(input: &TranscriptInput) -> Vec<TranscriptRowLayout> {
    let chat = &input.chat;
    let sessions: HashMap<&str, &TranscriptSession> =
        input.sessions.iter().map(|session| (session.id.as_str(), session)).collect();
    let members: HashMap<&str, &str> = input.members.iter().map(|m| (m.id.as_str(), m.name.as_str())).collect();
    let agents: HashMap<&str, &str> = input.agents.iter().map(|a| (a.id.as_str(), a.name.as_str())).collect();
    let from_session = |id: Option<&str>| -> Option<TranscriptSender> {
        let id = id.filter(|id| !id.is_empty() && *id != chat.session_id)?;
        let session = sessions.get(id)?;
        Some(match session.agent_id.as_deref().filter(|agent| !agent.is_empty()) {
            Some(agent) => TranscriptSender {
                kind: SenderKind::Agent,
                id: agent.to_string(),
                name: session.title.clone(),
                link: Some(session.id.clone()),
            },
            None => TranscriptSender {
                kind: SenderKind::Session,
                id: session.id.clone(),
                name: session.title.clone(),
                link: Some(session.id.clone()),
            },
        })
    };
    let own_agent = match chat.agent_id.as_deref().filter(|agent| !agent.is_empty()) {
        Some(agent) => TranscriptSender {
            kind: SenderKind::Agent,
            id: agent.to_string(),
            name: agents.get(agent).map_or_else(|| chat.title.clone(), |name| name.to_string()),
            link: None,
        },
        None => TranscriptSender {
            kind: SenderKind::Session,
            id: chat.session_id.clone(),
            name: chat.title.clone(),
            link: None,
        },
    };
    let viewer = input.viewer_id.as_deref().filter(|viewer| !viewer.is_empty());
    let mut origin_pending = chat.from_start && chat.origin_session_id.as_deref().is_some_and(|id| !id.is_empty());
    let layout = layout();

    let mut out: Vec<TranscriptRowLayout> = Vec::with_capacity(input.items.len());
    // A group = the same sender (kind + id + link; `own` = one sender).
    let mut prev_key: Option<(TranscriptSide, Option<(SenderKind, String, Option<String>)>)> = None;
    let mut group_named = false;
    for item in &input.items {
        let author_kind = item.author_kind.as_deref();
        let author_session = item.author_session_id.as_deref().filter(|id| !id.is_empty());
        let (side, sender) = if item.kind == TranscriptItemKind::Full
            || (item.kind == TranscriptItemKind::Message && author_kind == Some("system"))
        {
            (TranscriptSide::Full, None)
        } else if item.kind == TranscriptItemKind::Trace {
            (TranscriptSide::Other, Some(own_agent.clone()))
        } else {
            let own = author_kind == Some("user") && viewer.is_some() && item.author_id.as_deref() == viewer;
            let cross = from_session(author_session);
            // The origin: the conversation's first INPUT (a person's row or a
            // known session's), unless it names its own sender; the agent's
            // own output never takes it.
            let is_input = author_kind == Some("user") || cross.is_some();
            let origin = if origin_pending && is_input && cross.is_none() {
                from_session(chat.origin_session_id.as_deref())
            } else {
                None
            };
            if is_input {
                origin_pending = false;
            }
            if let Some(cross) = cross {
                (TranscriptSide::Other, Some(cross))
            } else if let Some(origin) = origin {
                (TranscriptSide::Other, Some(origin))
            } else if own {
                (TranscriptSide::Own, None)
            } else if author_kind == Some("user") {
                let id = item.author_id.clone().unwrap_or_default();
                let name =
                    members.get(id.as_str()).map_or_else(|| copy().former_member.clone(), |name| name.to_string());
                (TranscriptSide::Other, Some(TranscriptSender { kind: SenderKind::Member, id, name, link: None }))
            } else {
                (TranscriptSide::Other, Some(own_agent.clone()))
            }
        };
        let key = match side {
            TranscriptSide::Full => None,
            TranscriptSide::Own => Some((TranscriptSide::Own, None)),
            TranscriptSide::Other => Some((
                TranscriptSide::Other,
                sender.as_ref().map(|sender| (sender.kind, sender.id.clone(), sender.link.clone())),
            )),
        };
        let same_group = key.is_some() && key == prev_key;
        if !same_group {
            group_named = false;
        }
        let name = side == TranscriptSide::Other && item.kind == TranscriptItemKind::Message && !group_named;
        if name {
            group_named = true;
        }
        let gap = if out.is_empty() {
            0.
        } else if same_group {
            layout.gap_in_group
        } else {
            layout.gap_between
        };
        out.push(TranscriptRowLayout {
            id: item.id.clone(),
            side,
            sender,
            // #FEED-239: the viewer's own group wears THEIR avatar on the right.
            avatar: side != TranscriptSide::Full && !same_group,
            name,
            gap,
        });
        prev_key = key;
    }
    out
}

/// What a session input line says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrossSessionKind {
    Report,
    Message,
}

impl CrossSessionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Report => "report",
            Self::Message => "message",
        }
    }
}

/// A session input as the device words it (`coding::pickup::input_text`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrossSessionInput {
    pub kind: CrossSessionKind,
    pub name: String,
    pub session_id: String,
    pub body: String,
}

static INPUT_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(Report|Message) from (.+?) \(session ([^,)\s]+)(, ended)?\): ([\s\S]*)$")
        .expect("the session-input pattern compiles")
});

/// `Report from <name> (session <id>, ended): <summary>` /
/// `Message from <name> (session <id>): <body>` read back, else `None` (a
/// person's prompt).
pub fn cross_session_input(text: &str) -> Option<CrossSessionInput> {
    let captures = INPUT_PATTERN.captures(text)?;
    Some(CrossSessionInput {
        kind: if &captures[1] == "Report" { CrossSessionKind::Report } else { CrossSessionKind::Message },
        name: captures[2].to_string(),
        session_id: captures[3].to_string(),
        body: captures[5].to_string(),
    })
}

// #EXP-1772: only real messages are bubbles. A system input (a schedule or
// event firing, a PR or session status line) is THE inline marker, drawn
// only above the agent's reply to it.

/// A feed input read back (`feedInput`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeedInput {
    /// A device-tagged (or the server's own untagged firing) system line:
    /// `key` of `copy.markers`, `label` its copy.
    Marker { key: String, label: String },
    /// Another session's message or a child's report.
    Relay { name: String, session_id: String, body: String, report: bool },
}

static TAG_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\[([a-z_]+)\] ").expect("the tag pattern compiles"));
static SCHEDULE_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^Schedule `[^`\n]+`[^\n]* fired").expect("the schedule pattern compiles"));
static EVENT_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^Event `[^`\n]+` \(from ").expect("the event pattern compiles"));

fn marker(key: &str) -> Option<FeedInput> {
    let label = copy().markers.get(key)?;
    Some(FeedInput::Marker { key: key.to_string(), label: label.clone() })
}

/// A feed input: the device's tagged system line = a marker, another
/// session's input = a relay, else `None` (a person's prompt).
pub fn feed_input(text: &str) -> Option<FeedInput> {
    if let Some(found) = TAG_PATTERN.captures(text).and_then(|tag| marker(&tag[1])) {
        return Some(found);
    }
    if SCHEDULE_PATTERN.is_match(text) {
        return marker("schedule");
    }
    if EVENT_PATTERN.is_match(text) {
        return marker("event");
    }
    let cross = cross_session_input(text)?;
    Some(FeedInput::Relay {
        name: cross.name,
        session_id: cross.session_id,
        body: cross.body,
        report: cross.kind == CrossSessionKind::Report,
    })
}

/// What a feed row is to `markers_shown`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedRowKind {
    /// A system input (`feed_input` = a marker).
    Marker,
    /// Any other input (a person's prompt, a relay).
    Input,
    /// Agent prose with text.
    Reply,
    /// Work, cards, everything else.
    Other,
}

impl FeedRowKind {
    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "marker" => Some(Self::Marker),
            "input" => Some(Self::Input),
            "reply" => Some(Self::Reply),
            "other" => Some(Self::Other),
            _ => None,
        }
    }
}

/// Per row: is it drawn? A marker only when a reply follows it before the
/// next input or marker; every other row always.
pub fn markers_shown(kinds: &[FeedRowKind]) -> Vec<bool> {
    let mut shown = vec![true; kinds.len()];
    let mut pending: Option<usize> = None;
    for (index, kind) in kinds.iter().enumerate() {
        match kind {
            FeedRowKind::Marker | FeedRowKind::Input => {
                if let Some(at) = pending {
                    shown[at] = false;
                }
                pending = (*kind == FeedRowKind::Marker).then_some(index);
            }
            FeedRowKind::Reply => pending = None,
            FeedRowKind::Other => {}
        }
    }
    if let Some(at) = pending {
        shown[at] = false;
    }
    shown
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn fixture() -> Value {
        serde_json::from_str(TRANSCRIPT_GROUPING_JSON).unwrap()
    }

    fn opt(value: &Value) -> Option<String> {
        value.as_str().map(str::to_string)
    }

    fn input_of(value: &Value) -> TranscriptInput {
        let chat = &value["chat"];
        TranscriptInput {
            viewer_id: opt(&value["viewerId"]),
            chat: TranscriptChat {
                session_id: chat["sessionId"].as_str().unwrap().to_string(),
                title: chat["title"].as_str().unwrap().to_string(),
                agent_id: opt(&chat["agentId"]),
                origin_session_id: opt(&chat["originSessionId"]),
                from_start: chat["fromStart"].as_bool().unwrap_or(true),
            },
            members: value["members"]
                .as_array()
                .unwrap()
                .iter()
                .map(|m| TranscriptMember { id: opt(&m["id"]).unwrap(), name: opt(&m["name"]).unwrap() })
                .collect(),
            agents: value["agents"]
                .as_array()
                .unwrap()
                .iter()
                .map(|a| TranscriptAgent { id: opt(&a["id"]).unwrap(), name: opt(&a["name"]).unwrap() })
                .collect(),
            sessions: value["sessions"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| TranscriptSession {
                    id: opt(&s["id"]).unwrap(),
                    title: opt(&s["title"]).unwrap(),
                    agent_id: opt(&s["agentId"]),
                })
                .collect(),
            items: value["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|item| TranscriptItem {
                    id: opt(&item["id"]).unwrap(),
                    kind: TranscriptItemKind::parse(item["kind"].as_str().unwrap()).unwrap(),
                    author_kind: opt(&item["authorKind"]),
                    author_id: opt(&item["authorId"]),
                    author_session_id: opt(&item["authorSessionId"]),
                })
                .collect(),
        }
    }

    fn row_json(row: &TranscriptRowLayout) -> Value {
        json!({
            "id": row.id,
            "side": row.side.as_str(),
            "sender": row.sender.as_ref().map(|sender| json!({
                "kind": sender.kind.as_str(),
                "id": sender.id,
                "name": sender.name,
                "link": sender.link,
            })),
            "avatar": row.avatar,
            "name": row.name,
            "gap": row.gap,
        })
    }

    fn same_number(a: &Value, b: &Value) -> bool {
        match (a.as_f64(), b.as_f64()) {
            (Some(a), Some(b)) => (a - b).abs() < 1e-9,
            _ => a == b,
        }
    }

    #[test]
    fn every_case_matches_the_whole_expected_array() {
        let fixture = fixture();
        let cases = fixture["cases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let got: Vec<Value> = transcript_layout(&input_of(&case["input"])).iter().map(row_json).collect();
            let expected = case["expected"].as_array().unwrap();
            assert_eq!(got.len(), expected.len(), "{name}: row count");
            for (got, expected) in got.iter().zip(expected) {
                for key in ["id", "side", "sender", "avatar", "name"] {
                    assert_eq!(got[key], expected[key], "{name}: {key} of {}", expected["id"]);
                }
                assert!(same_number(&got["gap"], &expected["gap"]), "{name}: gap of {}", expected["id"]);
            }
        }
    }

    #[test]
    fn every_input_case_parses() {
        let fixture = fixture();
        let cases = fixture["inputCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let got = cross_session_input(case["text"].as_str().unwrap()).map(|input| {
                json!({
                    "kind": input.kind.as_str(),
                    "name": input.name,
                    "sessionId": input.session_id,
                    "body": input.body,
                })
            });
            assert_eq!(got.unwrap_or(Value::Null), case["expected"], "{name}");
        }
    }

    #[test]
    fn every_feed_input_case_reads() {
        let fixture = fixture();
        let cases = fixture["feedInputCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let got = feed_input(case["text"].as_str().unwrap()).map(|input| match input {
                FeedInput::Marker { key, label } => json!({"kind": "marker", "key": key, "label": label}),
                FeedInput::Relay { name, session_id, body, report } => json!({
                    "kind": "relay",
                    "name": name,
                    "sessionId": session_id,
                    "body": body,
                    "report": report,
                }),
            });
            assert_eq!(got.unwrap_or(Value::Null), case["expected"], "{name}");
        }
    }

    #[test]
    fn every_marker_case_matches() {
        let fixture = fixture();
        let cases = fixture["markerCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let kinds: Vec<FeedRowKind> = case["kinds"]
                .as_array()
                .unwrap()
                .iter()
                .map(|kind| FeedRowKind::parse(kind.as_str().unwrap()).unwrap())
                .collect();
            let expected: Vec<bool> =
                case["expected"].as_array().unwrap().iter().map(|v| v.as_bool().unwrap()).collect();
            assert_eq!(markers_shown(&kinds), expected, "{name}");
        }
    }

    #[test]
    fn the_layout_reads_the_fixture() {
        let layout = layout();
        assert_eq!(layout.avatar, 32.);
        assert_eq!(layout.avatar_gap, 8.);
        assert_eq!(layout.gap_in_group, 3.);
        assert_eq!(layout.gap_between, 12.);
        assert_eq!(layout.max_width.wide, 0.62);
        assert_eq!(layout.max_width.phone, 0.78);
        assert_eq!(layout.max_width.phone_in, 1.0);
        assert_eq!(layout.max_width.own, 1.0);
        assert_eq!(copy().link_chevron, "›");
        assert_eq!(copy().show_more, "Show more");
        assert_eq!(copy().show_less, "Show less");
    }

    #[test]
    fn every_fold_case_matches() {
        let fixture = fixture();
        let cases = fixture["foldCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let name = case["name"].as_str().unwrap();
            assert_eq!(message_folds(case["text"].as_str().unwrap()), case["expected"].as_bool().unwrap(), "{name}");
        }
    }

    #[test]
    fn a_cross_session_name_ends_with_the_chevron() {
        let sender = TranscriptSender {
            kind: SenderKind::Session,
            id: "c1".into(),
            name: "Fix it".into(),
            link: Some("c1".into()),
        };
        assert_eq!(sender.display_name(), "Fix it ›");
    }
}
