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
    /// #EXP-1772: THE inline marker (an event line, a system note): a
    /// centred muted label between two hairlines.
    Marker { id: String, label: String },
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
/// session the relay's name line opens (#EXP-1772: set = a relay), `gap` = the space above
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
    /// #EDE-65 lane O: the "+" menu's rows the host serves
    /// (composer-menu.json, the host resolved them), in order.
    #[serde(default)]
    pub plus: Vec<ChatPlusRow>,
    /// The quick replies over the composer (the newest message's).
    #[serde(default)]
    pub replies: Option<ChatRepliesInput>,
    /// The host paints the context ring beside Send (an Embed it claims).
    #[serde(default)]
    pub ring: bool,
}

/// #EDE-65 lane O: one row of the composer's "+" menu as the host resolved
/// it: `id` (composer-menu.json's; `separator` = a separator), its label and
/// concept icon; `items` = a submenu's entries (absent = an action row).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatPlusRow {
    pub id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub items: Option<Vec<ChatPlusItem>>,
    /// The submenu filters its entries in a search field.
    #[serde(default)]
    pub searchable: bool,
    /// #EDE-65 lane T: a submenu parent's current value (`Model › Opus`).
    #[serde(default)]
    pub detail: String,
}

/// A submenu entry: `checked` = a check entry (the picked model, a team),
/// absent = an action.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatPlusItem {
    pub label: String,
    pub value: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub checked: Option<bool>,
    #[serde(default)]
    pub disabled: bool,
    /// #EDE-65 lane T: the muted text after the label (an Account's health).
    #[serde(default)]
    pub detail: Option<String>,
    /// The mono id before the label (`EXP-12`, an Issue row).
    #[serde(default)]
    pub identifier: Option<String>,
    /// The glyph's own tint (an Issue row's status colour).
    #[serde(default)]
    pub icon_color: Option<String>,
    /// The nested surface beside the row (an Account's usage card).
    #[serde(default)]
    pub preview: Option<Value>,
}

/// The quick replies of message `message_id`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatRepliesInput {
    pub message_id: String,
    pub items: Vec<ChatReplyInput>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatReplyInput {
    pub id: String,
    pub text: String,
}

/// One quick reply as the document's chip draws it (its message rides along).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatReply {
    pub key: String,
    pub label: String,
    pub message_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRowKind {
    User,
    Agent,
    Line,
    Marker,
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
    /// #EXP-1772: a relay's name line ABOVE the bubble (the name +
    /// `copy.linkChevron`), the press that opens `link`; blank otherwise.
    pub relay: String,
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
    /// #EDE-65 lane T: a send in flight (the IDE's `sending`): the "+" is
    /// inert until it lands.
    pub sending: bool,
    /// #EDE-65 lane O: the "+" Menu's `items` (genUI menuItems; the host
    /// sets them as the Menu's literal prop), `plus` = any.
    pub plus: bool,
    pub menu: Vec<Value>,
    /// The quick replies' chips and their message (`""` = none).
    pub replies: Vec<ChatReply>,
    pub replies_message_id: String,
    pub ring: bool,
}

/// The "+" rows as genUI menuItems: a separator; an action row = its id as
/// the value; a submenu row = its entries, each `<row id>:<value>` (a check
/// entry a checkbox), `searchable` when it filters.
pub fn plus_menu(rows: &[ChatPlusRow]) -> Vec<Value> {
    use serde_json::{json, Map};
    rows.iter()
        .map(|row| {
            if row.id == "separator" {
                return json!({"kind": "separator"});
            }
            let mut item = Map::new();
            item.insert("label".into(), json!(row.label));
            item.insert("value".into(), json!(row.id));
            if !row.icon.is_empty() {
                item.insert("icon".into(), json!(row.icon));
            }
            if !row.detail.is_empty() {
                item.insert("detail".into(), json!(row.detail));
            }
            if let Some(entries) = &row.items {
                item.insert("kind".into(), json!("submenu"));
                if row.searchable {
                    item.insert("searchable".into(), json!(true));
                }
                let entries: Vec<Value> = entries
                    .iter()
                    .map(|entry| {
                        let mut e = Map::new();
                        e.insert("label".into(), json!(entry.label));
                        e.insert("value".into(), json!(format!("{}:{}", row.id, entry.value)));
                        if let Some(icon) = entry.icon.as_deref().filter(|i| !i.is_empty()) {
                            e.insert("icon".into(), json!(icon));
                        }
                        if let Some(checked) = entry.checked {
                            e.insert("kind".into(), json!("checkbox"));
                            e.insert("checked".into(), json!(checked));
                        }
                        if entry.disabled {
                            e.insert("disabled".into(), json!(true));
                        }
                        if let Some(detail) = entry.detail.as_deref().filter(|d| !d.is_empty()) {
                            e.insert("detail".into(), json!(detail));
                        }
                        if let Some(identifier) = entry.identifier.as_deref().filter(|i| !i.is_empty()) {
                            e.insert("identifier".into(), json!(identifier));
                        }
                        if let Some(color) = entry.icon_color.as_deref().filter(|c| !c.is_empty()) {
                            e.insert("iconColor".into(), json!(color));
                        }
                        if let Some(preview) = &entry.preview {
                            e.insert("preview".into(), preview.clone());
                        }
                        Value::Object(e)
                    })
                    .collect();
                item.insert("items".into(), Value::Array(entries));
            }
            Value::Object(item)
        })
        .collect()
}

/// A "+" pick (`select` {value}): the row's id and, for a submenu entry,
/// its own value (the first `:` splits them).
pub fn plus_pick(value: &str) -> (&str, Option<&str>) {
    match value.split_once(':') {
        Some((row, entry)) => (row, Some(entry)),
        None => (value, None),
    }
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
        relay: String::new(),
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
            let relay = group.link.as_deref().is_some_and(|link| !link.is_empty());
            let grouped = |kind| {
                let side = group.side.clone().filter(|s| s == "in" || s == "out").unwrap_or_else(|| if author == "user" { "out" } else { "in" }.to_string());
                let name = if bubbles && side == "in" { name.clone().unwrap_or_default() } else { String::new() };
                ChatRow {
                    relay: if relay && !name.trim().is_empty() { format!("{} {}", name.trim(), copy("linkChevron")) } else { String::new() },
                    author: if relay { String::new() } else { name },
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
                _ => ChatRow { text: body.clone(), ..blank(id, ChatRowKind::Marker) },
            }
        }
        ChatRowInput::Host { id } => blank(id, ChatRowKind::Host),
        ChatRowInput::Pill { id, icon, label } => ChatRow { icon: icon.clone(), text: label.clone(), ..blank(id, ChatRowKind::Pill) },
        ChatRowInput::Line { id, icon, text } => ChatRow { icon: icon.clone(), text: text.clone(), ..blank(id, ChatRowKind::Line) },
        ChatRowInput::Marker { id, label } => ChatRow { text: label.clone(), ..blank(id, ChatRowKind::Marker) },
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
            sending: input.sending,
            plus: !input.plus.is_empty(),
            menu: plus_menu(&input.plus),
            replies: input
                .replies
                .iter()
                .flat_map(|r| r.items.iter().map(|item| ChatReply { key: item.id.clone(), label: item.text.clone(), message_id: r.message_id.clone() }))
                .collect(),
            replies_message_id: input.replies.as_ref().filter(|r| !r.items.is_empty()).map(|r| r.message_id.clone()).unwrap_or_default(),
            ring: input.ring && !home,
        },
    }
}
