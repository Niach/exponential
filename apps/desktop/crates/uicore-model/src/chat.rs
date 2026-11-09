//! #EDE-65 lane I: what the shared chat document binds, a pure projection
//! of one conversation the host already resolved, locked by
//! `packages/uicore/fixtures/chat.json` (its `$comment` is the spec,
//! `packages/uicore/src/chat.ts` the TypeScript reference). The document
//! (`packages/uicore/docs/chat.json`) binds the result under `/chatting`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

const SOURCE: &str = include_str!("../../../../../packages/uicore/fixtures/chat.json");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatAgentInput {
    pub id: String,
    pub name: String,
    pub icon: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatOption {
    pub label: String,
    pub description: Option<String>,
}

/// An option as the document draws it: its row's id rides along (a
/// template cannot read its parent).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatRowOption {
    pub label: String,
    pub description: Option<String>,
    pub message_id: String,
}

/// One row as the host resolved it from the domain's chat rows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", rename_all_fields = "camelCase")]
pub enum ChatRowInput {
    Message {
        id: String,
        author: String,
        body: String,
        name: Option<String>,
        time: String,
        queued: bool,
        /// #EDE-65 lane O: the IDE transcript's grouping (#EXP-1736).
        #[serde(default)]
        group: Option<ChatGroup>,
    },
    /// #EDE-65 lane O: a row the host paints itself (the IDE's tool rows,
    /// the Work card): the document holds its place, an Embed the host
    /// claims by its painted id.
    Host { id: String },
    Pill { id: String, icon: String, label: String },
    Line { id: String, icon: String, text: String },
    /// `surface` = the genUI card the document draws in an Embed (Round 9
    /// §19 `{root, data, version?}`), absent = none.
    Card {
        id: String,
        icon: String,
        title: String,
        text: String,
        target: Option<String>,
        #[serde(default)]
        surface: Option<Value>,
    },
    Question {
        id: String,
        question: String,
        options: Vec<ChatOption>,
        custom: bool,
        multi: bool,
        waiting: bool,
        answer: Option<String>,
        #[serde(default)]
        surface: Option<Value>,
    },
    Picture { id: String, attachment_id: String, caption: Option<String> },
}

/// #EDE-65 lane O: how a message sits in a grouped transcript
/// (transcript-grouping.json, the host resolved it): `side` (`out` = the
/// viewer's own; absent = `out` for `user`, else `in`), the sender's
/// `avatar` icon beside a group's first `in` bubble, `indent` = an `in` row
/// right of the avatar column (the avatar or its spacer), `link` = the
/// session a press opens (the host's URL for it), `gap` = the space above
/// it in px (0, 3, 8, 12, 16). Without one a row sits 16 below the row
/// before it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ChatGroup {
    pub side: Option<String>,
    pub avatar: Option<String>,
    pub indent: bool,
    pub link: Option<String>,
    pub gap: u32,
}

/// What the host knows.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatInput {
    pub session_id: Option<String>,
    pub title: String,
    pub icon: String,
    pub bubbles: bool,
    pub agents: Vec<ChatAgentInput>,
    pub agent_id: Option<String>,
    pub draft: String,
    pub sending: bool,
    pub busy: bool,
    /// The host's composer has the `/` menu (the IDE); eDE until it does not.
    pub slash_commands: bool,
    pub rows: Vec<ChatRowInput>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRowKind {
    User,
    Agent,
    Line,
    Pill,
    Card,
    Question,
    Picture,
    Host,
}

/// One row the document's template draws; every field present.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatRow {
    pub id: String,
    pub kind: ChatRowKind,
    pub bubble: bool,
    pub body: String,
    pub caption: String,
    pub queued: bool,
    pub icon: String,
    pub title: String,
    pub text: String,
    pub target: Option<String>,
    pub options: Vec<ChatRowOption>,
    pub custom: bool,
    pub multi: bool,
    pub waiting: bool,
    pub answer: Option<String>,
    pub attachment_id: Option<String>,
    /// The card's genUI surface (cards and questions), else `None`.
    pub surface: Option<Value>,
    /// #EDE-65 lane O: a bubble's MessageBubble props (bubbles only): `in`
    /// or `out`, the sender's name over an `in` bubble, its time.
    pub side: String,
    pub author: String,
    pub time: String,
    /// [`ChatGroup`]'s, passed through (every message).
    pub avatar: String,
    pub indent: bool,
    pub link: Option<String>,
    pub gap: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatFace {
    Home,
    Conversation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatHeader {
    pub title: String,
    pub icon: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatAgentPick {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatAgentRow {
    pub label: String,
    pub agents: Vec<ChatAgentPick>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatComposer {
    pub placeholder: String,
    pub can_send: bool,
    pub busy: bool,
}

/// What the document binds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatView {
    pub face: ChatFace,
    pub header: ChatHeader,
    pub agent_row: ChatAgentRow,
    pub rows: Vec<ChatRow>,
    pub empty: bool,
    pub composer: ChatComposer,
}

fn copy(id: &str) -> String {
    static COPY: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    let copy = COPY.get_or_init(|| serde_json::from_str::<serde_json::Value>(SOURCE).expect("chat.json")["copy"].clone());
    copy[id].as_str().unwrap_or_default().to_string()
}

fn blank(id: &str, kind: ChatRowKind) -> ChatRow {
    ChatRow {
        id: id.into(),
        kind,
        bubble: false,
        body: String::new(),
        caption: String::new(),
        queued: false,
        icon: String::new(),
        title: String::new(),
        text: String::new(),
        target: None,
        options: Vec::new(),
        custom: false,
        multi: false,
        waiting: false,
        answer: None,
        attachment_id: None,
        surface: None,
        side: String::new(),
        author: String::new(),
        time: String::new(),
        avatar: String::new(),
        indent: false,
        link: None,
        gap: 0,
    }
}

/// `name · time`, a missing part dropped with its separator.
fn caption(parts: &[Option<&str>]) -> String {
    parts.iter().map(|part| part.unwrap_or("").trim()).filter(|part| !part.is_empty()).collect::<Vec<_>>().join(" · ")
}

fn row(input: &ChatRowInput, bubbles: bool) -> ChatRow {
    match input {
        ChatRowInput::Message { id, author, body, name, time, queued, group } => {
            let group = group.clone().unwrap_or_default();
            let grouped = |kind| {
                let side = group.side.clone().filter(|s| s == "in" || s == "out").unwrap_or_else(|| if author == "user" { "out" } else { "in" }.to_string());
                ChatRow {
                    author: if bubbles && side == "in" { name.clone().unwrap_or_default() } else { String::new() },
                    time: if bubbles { time.trim().to_string() } else { String::new() },
                    side: if bubbles { side } else { String::new() },
                    avatar: group.avatar.clone().unwrap_or_default(),
                    indent: group.indent,
                    link: group.link.clone(),
                    gap: group.gap,
                    ..blank(id, kind)
                }
            };
            match author.as_str() {
                "user" => ChatRow {
                    bubble: bubbles,
                    body: body.clone(),
                    caption: if bubbles { caption(&[Some(time)]) } else { caption(&[name.as_deref(), Some(time)]) },
                    queued: *queued,
                    ..grouped(ChatRowKind::User)
                },
                "agent" => ChatRow {
                    bubble: bubbles,
                    body: body.clone(),
                    caption: if bubbles { caption(&[Some(time)]) } else { String::new() },
                    ..grouped(ChatRowKind::Agent)
                },
                _ => ChatRow { icon: copy("systemIcon"), text: body.clone(), ..blank(id, ChatRowKind::Line) },
            }
        }
        ChatRowInput::Host { id } => blank(id, ChatRowKind::Host),
        ChatRowInput::Pill { id, icon, label } => ChatRow { icon: icon.clone(), text: label.clone(), ..blank(id, ChatRowKind::Pill) },
        ChatRowInput::Line { id, icon, text } => ChatRow { icon: icon.clone(), text: text.clone(), ..blank(id, ChatRowKind::Line) },
        ChatRowInput::Card { id, icon, title, text, target, surface } => ChatRow {
            icon: icon.clone(),
            title: title.clone(),
            text: text.clone(),
            target: target.clone(),
            surface: surface.clone(),
            ..blank(id, ChatRowKind::Card)
        },
        ChatRowInput::Question { id, question, options, custom, multi, waiting, answer, surface } => ChatRow {
            icon: copy("questionIcon"),
            title: question.clone(),
            options: options.iter().map(|o| ChatRowOption { label: o.label.clone(), description: o.description.clone(), message_id: id.clone() }).collect(),
            custom: *custom,
            multi: *multi,
            waiting: *waiting,
            answer: answer.clone(),
            surface: surface.clone(),
            ..blank(id, ChatRowKind::Question)
        },
        ChatRowInput::Picture { id, attachment_id, caption } => ChatRow {
            attachment_id: Some(attachment_id.clone()),
            text: caption.clone().unwrap_or_default(),
            ..blank(id, ChatRowKind::Picture)
        },
    }
}

/// The space above row `index`: a grouped message's own, a host row none
/// (it paints its own), else 16 below the row before it.
fn gap(input: &ChatRowInput, index: usize) -> u32 {
    match input {
        ChatRowInput::Message { group: Some(group), author, .. } if author == "user" || author == "agent" => group.gap,
        ChatRowInput::Host { .. } => 0,
        _ if index == 0 => 0,
        _ => DEFAULT_GAP,
    }
}

/// A row without a group's gap: the document's old `lg` list gap.
const DEFAULT_GAP: u32 = 16;

pub fn chat_view(input: &ChatInput) -> ChatView {
    let home = input.session_id.is_none();
    let picked = input.agents.iter().find(|a| Some(&a.id) == input.agent_id.as_ref()).or(input.agents.first());
    let rows: Vec<ChatRow> = if home { Vec::new() } else { input.rows.iter().enumerate().map(|(i, r)| ChatRow { gap: gap(r, i), ..row(r, input.bubbles) }).collect() };
    ChatView {
        face: if home { ChatFace::Home } else { ChatFace::Conversation },
        header: ChatHeader { title: input.title.clone(), icon: input.icon.clone() },
        agent_row: ChatAgentRow {
            label: picked.map(|a| copy("to").replace("{name}", &a.name)).unwrap_or_default(),
            agents: input
                .agents
                .iter()
                .map(|a| ChatAgentPick { id: a.id.clone(), name: a.name.clone(), icon: a.icon.clone(), selected: picked.is_some_and(|p| p.id == a.id) })
                .collect(),
        },
        empty: rows.is_empty(),
        rows,
        composer: ChatComposer {
            placeholder: if home {
                copy("homePlaceholder")
            } else if input.slash_commands {
                copy("placeholder")
            } else {
                copy("messagePlaceholder").replace("{name}", &input.title)
            },
            can_send: !input.draft.trim().is_empty() && !input.sending && (!home || picked.is_some()),
            busy: input.busy,
        },
    }
}
