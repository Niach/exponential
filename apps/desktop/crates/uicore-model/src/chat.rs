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
    Message { id: String, author: String, body: String, name: Option<String>, time: String, queued: bool },
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
    }
}

/// `name · time`, a missing part dropped with its separator.
fn caption(parts: &[Option<&str>]) -> String {
    parts.iter().map(|part| part.unwrap_or("").trim()).filter(|part| !part.is_empty()).collect::<Vec<_>>().join(" · ")
}

fn row(input: &ChatRowInput, bubbles: bool) -> ChatRow {
    match input {
        ChatRowInput::Message { id, author, body, name, time, queued } => match author.as_str() {
            "user" => ChatRow {
                bubble: bubbles,
                body: body.clone(),
                caption: if bubbles { caption(&[Some(time)]) } else { caption(&[name.as_deref(), Some(time)]) },
                queued: *queued,
                ..blank(id, ChatRowKind::User)
            },
            "agent" => ChatRow {
                bubble: bubbles,
                body: body.clone(),
                caption: if bubbles { caption(&[Some(time)]) } else { String::new() },
                ..blank(id, ChatRowKind::Agent)
            },
            _ => ChatRow { icon: copy("systemIcon"), text: body.clone(), ..blank(id, ChatRowKind::Line) },
        },
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

pub fn chat_view(input: &ChatInput) -> ChatView {
    let home = input.session_id.is_none();
    let picked = input.agents.iter().find(|a| Some(&a.id) == input.agent_id.as_ref()).or(input.agents.first());
    let rows: Vec<ChatRow> = if home { Vec::new() } else { input.rows.iter().map(|r| row(r, input.bubbles)).collect() };
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
