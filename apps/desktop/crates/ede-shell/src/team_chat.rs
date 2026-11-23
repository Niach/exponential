//! The agent chat on a team (#EDE-65 lane I): the account's synced rows
//! (sessions, agents, messages, users: the IDE's shapes) as
//! the chat model's input, and the sends through `sessions.send` like the
//! IDE's (`chat_actions::send_to_agent`, `chat_launch`): a message into an
//! open session, or to the picked agent (the server lands it in the
//! person's conversation with that agent). A sealed tree's text is sealed
//! to its public key first (`coding::sealing::seal_body`'s envelope);
//! #EDE-131: plaintext only into a clear tree, never otherwise
//! ([`wire_text`]).

use std::collections::HashMap;

use domain::rows::{AgentDefRow, MessageRow, SessionRow, User};
use uicore_model::chat::{ChatAgentInput, ChatInput, ChatMark};

use crate::inbox::AccountRows;

/// The chat's place on a team: the account, the team, the open session
/// (`None` = the home).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamChat {
    pub account_id: String,
    pub team_id: String,
    pub session_id: Option<String>,
}

/// What the chat reads off the account's store (#EDE-141: held by
/// [`crate::chat_rows::ChatRows`], moved by the sync's deltas).
#[derive(Clone)]
pub struct TeamRows {
    pub account: AccountRows,
    pub messages: Vec<MessageRow>,
    pub users: Vec<User>,
    /// The account's prompts (the `/` menu's second group).
    pub prompts: Vec<domain::rows::PromptRow>,
    /// The account's memberships (who `@` can name, with `users`).
    pub members: Vec<domain::rows::TeamMember>,
}

/// The team's people as `@` lists them: its members' users (a user without
/// an email cannot be mentioned: the interchange form is `@<email>`).
pub fn members(rows: &TeamRows, team_id: &str) -> Vec<crate::chat::Member> {
    let users: HashMap<&str, &User> = rows.users.iter().map(|u| (u.id.as_str(), u)).collect();
    rows.members
        .iter()
        .filter(|m| m.team_id == team_id)
        .filter_map(|m| users.get(m.user_id.as_str()))
        .filter_map(|u| {
            let email = u.email.clone().filter(|e| !e.is_empty())?;
            Some(crate::chat::Member { name: u.name.clone().filter(|n| !n.is_empty()).unwrap_or_else(|| email.clone()), email, image: u.image.clone() })
        })
        .collect()
}

/// The shapes the chat reads: a delta on one repaints an open team chat.
pub const SHAPES: &[&str] = &["sessions", "messages", "agents", "users", "session_issues", "pull_requests", "prompts"];

/// The brand mark: the viewer's Assistant (the IDE's agent row draws it).
pub const ASSISTANT_ICON: &str = "brand-exponential";

/// An agent's icon as the documents name it: the IDE rail's name
/// (`ide:agent:<id>`, #EXP-1725 `shell_regions`), painted by the host from
/// its glyph map ([`glyphs`]), never a glyph name of its own.
pub fn agent_icon(agent_id: &str) -> String {
    format!("{AGENT_ICON_PREFIX}{agent_id}")
}

pub const AGENT_ICON_PREFIX: &str = "ide:agent:";

/// What the host paints for an agent: its icon in its colour (`0xrrggbb`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentGlyph {
    pub icon: String,
    pub color: u32,
}

/// An agent's glyph: its stored icon and colour, else `chat-list.json`
/// `agentDefaults` (the IDE's `agent_glyph` / `agent_tint`).
pub fn agent_glyph(icon: Option<&str>, color: Option<&str>) -> AgentGlyph {
    let defaults = &domain::session_tree::CHATS_LIST.agent_defaults;
    AgentGlyph {
        icon: icon.filter(|i| !i.is_empty()).unwrap_or(&defaults.icon).to_string(),
        color: color.and_then(hex).or_else(|| hex(&defaults.color)).unwrap_or(0x737373),
    }
}

/// The team's agents' glyphs by their icon name (the Assistant is the
/// brand mark, no glyph).
pub fn glyphs(rows: &AccountRows, team_id: &str) -> HashMap<String, AgentGlyph> {
    let team_ids: Vec<&str> = rows.teams.iter().map(|t| t.id.as_str()).collect();
    agent_glyphs(&rows.agents, team_id, &team_ids, &rows.user_id)
}

/// [`glyphs`] over any agent rows (a team's, a Local desktop's).
pub fn agent_glyphs(agents: &[AgentDefRow], team_id: &str, team_ids: &[&str], me: &str) -> HashMap<String, AgentGlyph> {
    domain::agent_settings::agents_in_team(agents.iter(), team_id, team_ids, Some(me))
        .into_iter()
        .filter(|a| !a.is_assistant())
        .map(|a| (agent_icon(&a.id), agent_glyph(a.icon.as_deref(), a.color.as_deref())))
        .collect()
}

fn hex(color: &str) -> Option<u32> {
    let digits = color.strip_prefix('#')?;
    (digits.len() == 6).then(|| u32::from_str_radix(digits, 16).ok()).flatten()
}

/// The team's agents as the IDE's agent row lists them (`agents_in_team`,
/// `orderedAgents`: the viewer's Assistant first).
pub fn agents(rows: &AccountRows, team_id: &str) -> Vec<ChatAgentInput> {
    let team_ids: Vec<&str> = rows.teams.iter().map(|t| t.id.as_str()).collect();
    agent_inputs(&rows.agents, team_id, &team_ids, &rows.user_id)
}

/// [`agents`] over any agent rows (a team's, a Local desktop's).
pub fn agent_inputs(agents: &[AgentDefRow], team_id: &str, team_ids: &[&str], me: &str) -> Vec<ChatAgentInput> {
    let me = Some(me);
    let listed = domain::agent_settings::agents_in_team(agents.iter(), team_id, team_ids, me);
    domain::agent_settings::ordered_agents(listed, me)
        .into_iter()
        .map(|a| ChatAgentInput {
            id: a.id.clone(),
            name: a.name.clone().filter(|n| !n.is_empty()).unwrap_or_else(|| "Agent".into()),
            icon: if a.is_assistant() { ASSISTANT_ICON.into() } else { agent_icon(&a.id) },
        })
        .collect()
}

/// The chat model's input: the home with the team's agents, or the open
/// session's chat as the domain builds it (session-chat.json).
pub fn input(rows: &TeamRows, chat: &TeamChat, now_ms: i64) -> ChatInput {
    let agents = agents(&rows.account, &chat.team_id);
    let mut input = ChatInput { agent_id: agents.first().map(|a| a.id.clone()), agents, ..Default::default() };
    let Some((root, view, keyed)) = session_chat(rows, chat, now_ms) else {
        return input;
    };
    let by_id: HashMap<&str, &MessageRow> = keyed.iter().map(|m| (m.id.as_str(), m)).collect();
    let users: HashMap<&str, &User> = rows.users.iter().map(|u| (u.id.as_str(), u)).collect();
    let agent = root.agent_id.as_deref().and_then(|id| input.agents.iter().find(|a| a.id == id));
    input.icon = agent.map(|a| a.icon.clone()).unwrap_or_default();
    input.title = if root.agent_id.is_some() { agent.map(|a| a.name.clone()).unwrap_or_else(|| view.title.clone()) } else { view.title.clone() };
    input.bubbles = domain::session_tree::chat_mode(root.agent_id.as_deref()) == domain::session_tree::ChatMode::Bubbles;
    input.busy = view.busy;
    input.session_id = Some(root.id.clone());
    input.marks = marks(&rows.account, root);
    input.rows = crate::chat::rows(&view.rows, &by_id, |id| Some(author_label(id.and_then(|id| users.get(id).copied()))), Some(rows.account.user_id.as_str()));
    // #EDE-65 lane O: the IDE transcript's grouping (avatars, names, gaps).
    let transcript = domain::transcript_grouping::TranscriptInput {
        viewer_id: Some(rows.account.user_id.clone()),
        chat: domain::transcript_grouping::TranscriptChat { session_id: root.id.clone(), title: view.title.clone(), agent_id: root.agent_id.clone(), ..Default::default() },
        members: rows.users.iter().map(|u| domain::transcript_grouping::TranscriptMember { id: u.id.clone(), name: author_label(Some(u)) }).collect(),
        agents: input.agents.iter().map(|a| domain::transcript_grouping::TranscriptAgent { id: a.id.clone(), name: a.name.clone() }).collect(),
        ..Default::default()
    };
    let icons: HashMap<String, String> = input.agents.iter().map(|a| (a.id.clone(), a.icon.clone())).collect();
    crate::chat::group(&mut input.rows, &view.rows, transcript, |sender| Some(avatar_icon(sender, &icons)));
    // #EXP-2077: an agent's row with Express blocks = its segments.
    input.rows = crate::chat::express(std::mem::take(&mut input.rows), &view.rows, &mut crate::chat_express::split);
    input
}

/// #EDE-98: the chat's marks after the title (chats.json chatMarks), read
/// off the tree's root: `ui-team` when shared, `ui-unencrypted` without a key.
fn marks(rows: &AccountRows, session: &SessionRow) -> Vec<ChatMark> {
    chat_marks(rows.sessions.iter().find(|s| s.id == session.root_id_or_self()).unwrap_or(session))
}

/// #EDE-98: a root's marks as the chat model takes them.
pub fn chat_marks(root: &SessionRow) -> Vec<ChatMark> {
    domain::chats::chat_marks(root.shared, root.seal_epoch)
        .into_iter()
        .map(|mark| ChatMark { icon: mark.concept().into(), label: mark.label().into(), test_id: mark.test_id().into() })
        .collect()
}

/// #EDE-65 lane O: a sender's avatar as the chat document names it (the
/// host paints `avatar:<icon>` as the IDE's disc): an agent's own glyph, a
/// session's, a person's.
pub fn avatar_icon(sender: &domain::transcript_grouping::TranscriptSender, agent_icons: &HashMap<String, String>) -> String {
    use domain::transcript_grouping::SenderKind;
    let icon = match sender.kind {
        SenderKind::Agent => agent_icons.get(&sender.id).cloned().unwrap_or_else(|| agent_icon(&sender.id)),
        SenderKind::Session => SESSION_AVATAR_ICON.into(),
        SenderKind::Member => MEMBER_AVATAR_ICON.into(),
    };
    format!("{}{icon}", crate::chat::AVATAR_ICON_PREFIX)
}

/// The muted session avatar's glyph (the IDE's `session_avatar`).
const SESSION_AVATAR_ICON: &str = "coding-assistant";
/// A person without a picture.
const MEMBER_AVATAR_ICON: &str = "user";

/// The open session's chat as the domain builds it (session-chat.json),
/// with the tree's messages it read.
fn session_chat<'a>(rows: &'a TeamRows, chat: &TeamChat, now_ms: i64) -> Option<(&'a SessionRow, domain::session_tree::SessionChatView, Vec<MessageRow>)> {
    let root = chat.session_id.as_deref().and_then(|id| rows.account.sessions.iter().find(|s| s.id == id))?;
    let tree: Vec<&SessionRow> = rows.account.sessions.iter().filter(|s| s.root_id_or_self() == root.root_id_or_self()).collect();
    let in_tree = |m: &&MessageRow| m.root_id.as_deref().is_none_or(|r| r == root.id);
    // #EXP-1730: a message names its own session (`session_id`).
    let keyed = domain::message_session::key_messages_by_session(rows.messages.iter().filter(in_tree));
    let message_refs: Vec<&MessageRow> = keyed.iter().collect();
    let session_issues: Vec<_> = rows.account.session_issues.iter().collect();
    let issues: Vec<domain::session_tree::TreeIssue> = rows.account.issues.iter().map(domain::session_tree::TreeIssue::from_issue).collect();
    let prs: Vec<_> = rows.account.pull_requests.iter().collect();
    let view = domain::session_tree::build_session_chat(
        domain::session_tree::SessionChatInput { root, sessions: &tree, session_issues: &session_issues, issues: &issues, messages: &message_refs, pull_requests: &prs, now_ms },
        None,
    );
    Some((root, view, keyed))
}

/// Lane Q, #EXP-2077: what an Embed's inner event on chat row `message_id`
/// asks (`domain::chat_document::card_reply` over the open session's rows:
/// a show card, an Express block's surface).
pub fn card_reply(rows: &TeamRows, chat: &TeamChat, message_id: &str, name: &str, context: &serde_json::Value, now_ms: i64) -> Option<crate::chat::CardReply> {
    let (root, view, _) = session_chat(rows, chat, now_ms)?;
    crate::chat::card_reply(&view.rows, &root.id, message_id, Some(name), Some(context))
}

/// A card reply's send (chat-express.json ACTIONS): the body (blank =
/// none) into the reply's session, and THE `action` card it carries, still
/// clear ([`Wire::action_card`] is its wire). `None` = too large: nothing
/// is sent.
pub fn reply_input(chat: &TeamChat, reply: &crate::chat::CardReply) -> Option<(api::sessions::SendInput, serde_json::Value)> {
    let crate::chat::CardReply::Send { session_id, text, card } = reply else { return None };
    let card = serde_json::to_value(card).ok()?;
    let text = Some(text.clone()).filter(|text| !text.trim().is_empty());
    // The clear card rides the input until the wire is known (`send_tree`
    // reads it: a card alone is content to seal too).
    Some((api::sessions::SendInput { team_id: Some(chat.team_id.clone()), session_id: Some(session_id.clone()), text, action_card: Some(card.clone()), ..Default::default() }, card))
}

/// A person's label (the IDE's `author_label`): the name, else the email.
fn author_label(user: Option<&User>) -> String {
    user.and_then(|u| u.name.clone().filter(|n| !n.is_empty()).or_else(|| u.email.clone().filter(|e| !e.is_empty()))).unwrap_or_else(|| "Someone".into())
}

/// The send: into the open session, else to the picked agent on the team;
/// `attachments` = files ride it, so it may go without text.
pub fn send_input(chat: &TeamChat, agent_id: Option<&str>, text: &str, attachments: bool) -> Option<api::sessions::SendInput> {
    let text = text.trim();
    if text.is_empty() && !attachments {
        return None;
    }
    let text = (!text.is_empty()).then(|| text.to_string());
    let mut input = api::sessions::SendInput { team_id: Some(chat.team_id.clone()), text, ..Default::default() };
    match (&chat.session_id, agent_id) {
        (Some(session), _) => input.session_id = Some(session.clone()),
        (None, Some(agent)) => input.agent_id = Some(agent.to_string()),
        (None, None) => return None,
    }
    Some(input)
}

/// The tree a send lands in (the open session's root, else THE agent's chat
/// for the person: #EXP-1990 `agent_chat_of`, its group chat, else their own
/// conversation), when it is synced.
pub fn target_root<'a>(rows: &'a AccountRows, input: &api::sessions::SendInput) -> Option<&'a SessionRow> {
    if let Some(session) = &input.session_id {
        let row = rows.sessions.iter().find(|s| &s.id == session)?;
        return rows.sessions.iter().find(|s| s.id == row.root_id_or_self());
    }
    let agent = input.agent_id.as_deref()?;
    let group_chat_id = rows.agents.iter().find(|a| a.id == agent).and_then(|a| a.group_chat_id.as_deref());
    domain::agent_settings::agent_chat_of(agent, group_chat_id, &rows.sessions, Some(rows.user_id.as_str()))
}

/// #EDE-97 `noKeyRefusal` (`sealed-content.json`): what a send that cannot
/// be sealed says instead of going out in clear.
pub const NO_KEY_REFUSAL: &str = "This chat is end-to-end encrypted and this device holds no key for it yet. Try again in a moment.";

/// What an answer to a card the shell cannot (re)seal says (the IDE's
/// `LOCKED_ANSWER`).
pub const LOCKED_ANSWER: &str = "This card can't be answered on this device yet.";

/// #EDE-131: where a send's text goes, as far as the synced rows know (the
/// IDE's `plan_send`); [`wire_text`] asks the server the rest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SendTree {
    /// No text: nothing to seal (an attachment-only send).
    None,
    /// A `clear_tree` root: plaintext is its wire.
    Clear,
    /// A synced root's current key.
    Keyed { root_id: String, epoch: u32, public: [u8; 32] },
    /// A synced root without a usable key (pre-#EDE-97, a bad key or
    /// epoch): the server names its current one.
    Keyless(String),
    /// A session the rows lack (a deep link, a push before sync): the
    /// server names its tree.
    Unsynced(String),
    /// An agent send without a synced conversation: the server names the
    /// conversation (`sessions.sendTarget`).
    Agent(String),
}

/// The send's [`SendTree`] off the synced rows.
pub fn send_tree(rows: &AccountRows, input: &api::sessions::SendInput) -> SendTree {
    if input.text.is_none() && input.action_card.is_none() {
        return SendTree::None;
    }
    if let Some(session) = &input.session_id {
        return match target_root(rows, input) {
            Some(root) => root_tree(root),
            None => SendTree::Unsynced(session.clone()),
        };
    }
    match (target_root(rows, input), &input.agent_id) {
        (Some(root), _) => root_tree(root),
        (None, Some(agent)) => SendTree::Agent(agent.clone()),
        (None, None) => SendTree::None,
    }
}

fn root_tree(root: &SessionRow) -> SendTree {
    if root.clear_tree {
        return SendTree::Clear;
    }
    let key = root.seal_public_key.as_deref().and_then(|key| enet_core::sealed::public_key_from_b64(key).ok());
    let epoch = root.seal_epoch.and_then(|epoch| u32::try_from(epoch).ok()).filter(|epoch| *epoch > 0);
    match (key, epoch) {
        (Some(public), Some(epoch)) => SendTree::Keyed { root_id: root.id.clone(), epoch, public },
        _ => SendTree::Keyless(root.id.clone()),
    }
}

/// The server's word on a tree the rows cannot settle (the IDE's
/// `Writer::tree_of` after #EXP-1960).
pub trait TreeServer {
    /// `sessions get` (any member id resolves to its root): the root id
    /// and its clear flag.
    fn tree_of(&self, session_id: &str) -> Result<(String, bool), String>;
    /// `sessionKeys.publicKeys` by the ROOT id (`None` = keyless).
    fn key_of(&self, root_id: &str) -> Result<Option<(u32, [u8; 32])>, String>;
    /// `sessions.sendTarget`: the person's conversation with `agent_id`.
    fn send_target(&self, agent_id: &str) -> Result<api::sessions::SendTarget, String>;
}

/// The live [`TreeServer`]: the account's tRPC.
pub struct Server<'a>(pub &'a api::TrpcClient);

impl TreeServer for Server<'_> {
    fn tree_of(&self, session_id: &str) -> Result<(String, bool), String> {
        let tree = api::mcp_tools::sessions_get(self.0, session_id).map_err(|err| format!("sessions get: {err}"))?;
        if tree.root.id.is_empty() {
            return Err(format!("{session_id}: the server named no root"));
        }
        Ok((tree.root.id, tree.root.clear_tree))
    }
    fn key_of(&self, root_id: &str) -> Result<Option<(u32, [u8; 32])>, String> {
        let keys = api::session_keys::public_keys(self.0, &[root_id.to_string()]).map_err(|err| format!("sessionKeys.publicKeys: {err}"))?;
        keys.into_iter()
            .find(|key| key.root_id == root_id)
            .map(|key| enet_core::sealed::public_key_from_b64(&key.public_key).map(|public| (key.epoch, public)))
            .transpose()
            .map_err(|err| format!("{root_id}: {err}"))
    }
    fn send_target(&self, agent_id: &str) -> Result<api::sessions::SendTarget, String> {
        api::sessions::send_target(self.0, Some(agent_id), None).map_err(|err| format!("sessions.sendTarget: {err}"))
    }
}

/// #EDE-131 (`clearTree.writerCases`): the text that leaves for `tree`:
/// SEALED to the tree's current key, plaintext ONLY into a clear tree. A
/// tree the rows lack is asked of the server first; a keyless one (the
/// shell holds no install, so it never keys one) or one the server cannot
/// name is an `Err` and NOTHING is sent. A new conversation (the server
/// names none) goes as the server's own start, keyed and re-sealed at its
/// device's claim (`startCases`). BLOCKING (off the UI thread).
pub fn wire_text(server: &dyn TreeServer, tree: &SendTree, text: &str) -> Result<String, String> {
    wire_for(server, tree)?.text(text, &[])
}

/// #EDE-134: how a send's text leaves, settled BEFORE anything uploads
/// ([`wire_text`]'s rule: a tree this shell cannot seal for refuses first).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Wire {
    Clear,
    Sealed { root_id: String, epoch: u32, public: [u8; 32] },
}

impl Wire {
    /// The text that leaves; a sealed one names its attachments inside it
    /// (`sealed-content.json` `attachmentEmbedCases`: the server never
    /// appends to a sealed body, so the writer does before sealing).
    pub fn text(&self, text: &str, attachment_ids: &[String]) -> Result<String, String> {
        match self {
            Wire::Clear => Ok(text.to_string()),
            Wire::Sealed { root_id, epoch, public } => {
                let text = domain::sealed::with_attachment_embeds(text, attachment_ids, domain::sealed::recorded_upload);
                enet_core::sealed::seal_body(public, *epoch, &text).map_err(|err| format!("{root_id}: {err}"))
            }
        }
    }

    /// #EXP-2067: THE `action` card that leaves: itself into a clear tree,
    /// else its envelope ALONE, `{kind: "action", sealed}` (`sessions.send`
    /// `actionCard`, the IDE's `action_envelope`: every field lives inside).
    pub fn action_card(&self, card: &serde_json::Value) -> Result<serde_json::Value, String> {
        match self {
            Wire::Clear => Ok(card.clone()),
            Wire::Sealed { root_id, epoch, public } => {
                let sealed = enet_core::sealed::seal_card(public, *epoch, card).map_err(|err| format!("{root_id}: {err}"))?;
                Ok(serde_json::json!({"kind": "action", "sealed": sealed.get("sealed").cloned().unwrap_or(serde_json::Value::Null)}))
            }
        }
    }
}

/// [`wire_text`]'s tree resolved to its [`Wire`] (BLOCKING).
pub fn wire_for(server: &dyn TreeServer, tree: &SendTree) -> Result<Wire, String> {
    let sealed = |root_id: &str, epoch: u32, public: [u8; 32]| Wire::Sealed { root_id: root_id.to_string(), epoch, public };
    let by_key = |root_id: &str| match server.key_of(root_id)? {
        Some((epoch, public)) => Ok(sealed(root_id, epoch, public)),
        None => Err(format!("{root_id}: keyless")),
    };
    match tree {
        SendTree::None | SendTree::Clear => Ok(Wire::Clear),
        SendTree::Keyed { root_id, epoch, public } => Ok(sealed(root_id, *epoch, *public)),
        SendTree::Keyless(root_id) => by_key(root_id),
        SendTree::Unsynced(session_id) => match server.tree_of(session_id)? {
            (_, true) => Ok(Wire::Clear),
            (root_id, false) => by_key(&root_id),
        },
        SendTree::Agent(agent_id) => {
            let target = server.send_target(agent_id)?;
            match (target.root_id, target.epoch, target.public_key) {
                (None, ..) => Ok(Wire::Clear),
                (Some(_), ..) if target.clear_tree => Ok(Wire::Clear),
                (Some(root_id), Some(epoch), Some(public)) => {
                    let public = enet_core::sealed::public_key_from_b64(&public).map_err(|err| format!("{root_id}: {err}"))?;
                    Ok(sealed(&root_id, epoch, public))
                }
                (Some(root_id), ..) => by_key(&root_id),
            }
        }
    }
}

/// #EDE-131: a question card's answer as it leaves (the IDE's
/// `answer_wire`): a plaintext card's in clear, an opened sealed card's
/// FULL card re-sealed with the answer to its tree's key; a card the rows
/// lack, one the shell cannot open or a tree without a key here is refused
/// locally, since a plaintext answer would leak it.
pub fn answer_wire(rows: &TeamRows, input: &api::sessions::AnswerInput) -> Result<api::sessions::AnswerInput, String> {
    use domain::sealed::SealState;
    let row = rows.messages.iter().find(|m| m.id == input.message_id).ok_or(LOCKED_ANSWER)?;
    match row.seal {
        None => return Ok(input.clone()),
        Some(SealState::Opened) => {}
        Some(_) => return Err(LOCKED_ANSWER.into()),
    }
    let root_id = row.root_id.as_deref().filter(|id| !id.is_empty()).unwrap_or(row.session_id.as_str());
    let root = rows.account.sessions.iter().find(|s| s.id == root_id).ok_or(LOCKED_ANSWER)?;
    let SendTree::Keyed { epoch, public, .. } = root_tree(root) else { return Err(LOCKED_ANSWER.into()) };
    let card = row.card.as_ref().ok_or(LOCKED_ANSWER)?;
    let answers = input.answers.as_ref().and_then(|answers| match answers {
        serde_json::Value::Array(labels) => Some(domain::plan_card::CardAnswers::Labels(labels.iter().filter_map(|l| l.as_str().map(str::to_owned)).collect())),
        _ => None,
    });
    let answered = domain::plan_card::answered_card(
        card,
        &domain::plan_card::CardAnswerInput { answer: input.answer.clone(), answers, ..Default::default() },
    )
    .ok_or(LOCKED_ANSWER)?;
    let sealed = enet_core::sealed::seal_card(&public, epoch, &answered).map_err(|err| format!("{LOCKED_ANSWER} ({err})"))?;
    Ok(api::sessions::AnswerInput { message_id: input.message_id.clone(), sealed_card: Some(sealed), ..Default::default() })
}

/// The demo's agents' glyphs: Triage, a purple bug.
pub fn demo_glyphs() -> HashMap<String, AgentGlyph> {
    HashMap::from([(agent_icon("a2"), agent_glyph(Some("bug"), Some("#a855f7")))])
}

/// The demo's result card: three counts and a reply button.
pub fn demo_card() -> serde_json::Value {
    let tile = |id: &str, label: &str, path: &str| serde_json::json!({"id": id, "component": "StatTile", "props": {"label": label, "value": {"@path": path}}});
    serde_json::json!({
        "version": "1.0",
        "root": {"id": "root", "component": "Stack", "props": {"gap": "md"}, "children": [
            {"id": "tiles", "component": "Grid", "props": {"columns": 3, "gap": "sm"}, "children": [tile("new", "New", "/new"), tile("merged", "Merged", "/merged"), tile("coding", "Coding", "/coding")]},
            {"id": "ship", "component": "Button", "props": {"label": "Label the rest", "variant": "secondary", "size": "sm"}, "on": {"press": {"event": {"name": "reply", "context": {"text": "Label the rest too"}}}}}
        ]},
        "data": {"new": "2", "merged": "1", "coding": "1"}
    })
}

/// The demo's team chat (the goldens): an agent conversation.
pub fn demo_input() -> ChatInput {
    let message = |id: &str, author: &str, body: &str, time: &str| uicore_model::chat::ChatRowInput::Message {
        id: id.into(),
        author: author.into(),
        body: body.into(),
        name: (author == "user").then(|| "Dana Reyes".to_string()),
        time: time.into(),
        queued: false,
        group: None,
        status: None,
        highlight: None,
    };
    let agents = vec![
        ChatAgentInput { id: "a1".into(), name: "Assistant".into(), icon: ASSISTANT_ICON.into() },
        ChatAgentInput { id: "a2".into(), name: "Triage".into(), icon: agent_icon("a2") },
    ];
    let mut input = ChatInput {
        session_id: Some("s1".into()),
        title: "Triage".into(),
        icon: agent_icon("a2"),
        bubbles: true,
        agent_id: Some("a2".into()),
        slash_commands: true,
        agents,
        rows: vec![
            message("m1", "agent", "Two new widget reports on Product Feedback. FB-8 is a duplicate of FB-3, merged it. FB-9 needs code, so I started a chat:", "09:02"),
            uicore_model::chat::ChatRowInput::Card { id: "c1".into(), icon: "coding-running".into(), title: "Sort today's widget reports".into(), text: "Working".into(), target: Some("s2".into()), surface: None },
            message("m2", "user", "Label the Android ones too, and check the board every hour.", "09:14"),
            message("m3", "agent", "Done, 3 issues labelled android.", "09:14"),
            // Lane Q: an agent's result card (`exponential_show`), drawn in an Embed.
            uicore_model::chat::ChatRowInput::Card {
                id: "c2".into(),
                icon: "ui-widget".into(),
                title: "Widget reports today".into(),
                text: String::new(),
                target: Some("s1".into()),
                surface: Some(demo_card()),
            },
        ],
        ..Default::default()
    };
    crate::chat::demo_group(&mut input.rows, &agent_icon("a2"), "Triage");
    input
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// #EDE-65 lane O: the golden's agent bubbles sit under the agent's
    /// avatar disc with its name like the IDE's, the person's own on the right.
    #[test]
    fn the_demo_groups_like_the_ide_transcript() {
        let view = uicore_model::chat::chat_view(&demo_input());
        let row = |id: &str| view.rows.iter().find(|r| r.id == id).unwrap();
        let avatar = format!("{}{}", crate::chat::AVATAR_ICON_PREFIX, agent_icon("a2"));
        assert_eq!((row("m1").side.as_str(), row("m1").avatar.as_str(), row("m1").indent, row("m1").author.as_str()), ("in", avatar.as_str(), true, "Triage"));
        assert_eq!((row("m2").side.as_str(), row("m2").avatar.as_str()), ("out", ""));
        assert_eq!((row("m3").side.as_str(), row("m3").avatar.as_str(), row("m3").gap), ("in", avatar.as_str(), 12));
        let sender = domain::transcript_grouping::TranscriptSender { kind: domain::transcript_grouping::SenderKind::Agent, id: "a1".into(), name: "Assistant".into(), link: None };
        let icons = HashMap::from([("a1".to_string(), ASSISTANT_ICON.to_string())]);
        assert_eq!(avatar_icon(&sender, &icons), format!("{}{ASSISTANT_ICON}", crate::chat::AVATAR_ICON_PREFIX));
    }

    fn rows() -> TeamRows {
        let account = AccountRows {
            account_id: "acc".into(),
            user_id: "u1".into(),
            teams: vec![serde_json::from_value(json!({"id": "t1", "name": "Acme", "slug": "acme"})).unwrap()],
            agents: vec![
                serde_json::from_value(json!({"id": "a2", "team_id": "t1", "name": "Triage", "icon": "bug", "role": "agent", "sort_order": 1.0})).unwrap(),
                serde_json::from_value(json!({"id": "a1", "team_id": "t1", "name": "Assistant", "role": "assistant", "owner_id": "u1"})).unwrap(),
                serde_json::from_value(json!({"id": "a9", "team_id": "t1", "name": "Their Assistant", "role": "assistant", "owner_id": "u2"})).unwrap(),
            ],
            sessions: vec![serde_json::from_value(json!({"id": "s1", "team_id": "t1", "user_id": "u1", "agent_id": "a2", "status": "idle", "title": "Triage"})).unwrap()],
            ..Default::default()
        };
        let messages = vec![
            serde_json::from_value(json!({"id": "m1", "conversation_id": "c1", "session_id": "s1", "root_id": "s1", "author_kind": "user", "author_id": "u1", "body": "Label them", "created_at": "2026-11-08T09:14:00Z"})).unwrap(),
            serde_json::from_value(json!({"id": "m2", "conversation_id": "c1", "session_id": "s1", "root_id": "s1", "author_kind": "agent", "body": "Done", "created_at": "2026-11-08T09:15:00Z"})).unwrap(),
            serde_json::from_value(json!({"id": "m3", "conversation_id": "c-other", "session_id": "s7", "root_id": "s7", "author_kind": "agent", "body": "elsewhere", "created_at": "2026-11-08T09:16:00Z"})).unwrap(),
        ];
        let users = vec![serde_json::from_value(json!({"id": "u1", "name": "Dana Reyes", "email": "dana@example.com"})).unwrap()];
        TeamRows { account, messages, users, prompts: Vec::new(), members: Vec::new() }
    }

    fn chat(session: Option<&str>) -> TeamChat {
        TeamChat { account_id: "acc".into(), team_id: "t1".into(), session_id: session.map(str::to_string) }
    }

    #[test]
    fn the_home_lists_my_assistant_first_and_never_another_member_s() {
        let input = input(&rows(), &chat(None), 0);
        let names: Vec<&str> = input.agents.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["Assistant", "Triage"]);
        assert_eq!(input.agents[0].icon, ASSISTANT_ICON);
        assert_eq!(input.agent_id.as_deref(), Some("a1"));
        assert!(input.session_id.is_none());
    }

    #[test]
    fn an_agent_conversation_is_bubbles_with_its_own_messages_only() {
        let input = input(&rows(), &chat(Some("s1")), 0);
        assert!(input.bubbles);
        assert_eq!(input.title.as_str(), "Triage");
        assert_eq!(input.icon, agent_icon("a2"));
        assert_eq!(glyphs(&rows().account, "t1").get(&input.icon).map(|g| g.icon.as_str()), Some("bug"));
        let bodies: Vec<String> = input
            .rows
            .iter()
            .filter_map(|r| match r {
                uicore_model::chat::ChatRowInput::Message { body, .. } => Some(body.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(bodies, ["Label them", "Done"]);
        assert!(matches!(&input.rows.iter().find(|r| matches!(r, uicore_model::chat::ChatRowInput::Message { author, .. } if author == "user")), Some(uicore_model::chat::ChatRowInput::Message { name: Some(n), .. }) if n == "Dana Reyes"));
    }

    #[test]
    fn an_agent_s_glyph_is_its_icon_in_its_colour_else_the_fixture_s_defaults() {
        assert_eq!(agent_icon("a2"), "ide:agent:a2", "the IDE rail's name");
        assert_eq!(agent_glyph(Some("bug"), Some("#a855f7")), AgentGlyph { icon: "bug".into(), color: 0xa855f7 });
        let defaults = &domain::session_tree::CHATS_LIST.agent_defaults;
        let fallback = agent_glyph(None, Some("purple"));
        assert_eq!(fallback.icon, defaults.icon);
        assert_eq!(Some(fallback.color), hex(&defaults.color), "the default colour is a hex");
        let glyphs = glyphs(&rows().account, "t1");
        assert_eq!(glyphs.len(), 1, "Triage; my Assistant is the brand mark: {glyphs:?}");
    }

    #[test]
    fn a_send_goes_to_the_open_session_else_to_the_picked_agent() {
        let open = send_input(&chat(Some("s1")), Some("a1"), " hi ", false).unwrap();
        assert_eq!((open.session_id.as_deref(), open.agent_id.as_deref(), open.text.as_deref()), (Some("s1"), None, Some("hi")));
        let home = send_input(&chat(None), Some("a2"), "hi", false).unwrap();
        assert_eq!((home.session_id.as_deref(), home.agent_id.as_deref(), home.team_id.as_deref()), (None, Some("a2"), Some("t1")));
        assert!(send_input(&chat(None), None, "hi", false).is_none());
        assert!(send_input(&chat(Some("s1")), None, "  ", false).is_none());
        // #EDE-65 lane L: attachments go without text.
        let files = send_input(&chat(Some("s1")), None, "  ", true).unwrap();
        assert_eq!((files.session_id.as_deref(), files.text.as_deref()), (Some("s1"), None));
        let r = rows();
        assert_eq!(target_root(&r.account, &home).map(|s| s.id.as_str()), Some("s1"), "my conversation with Triage");
    }

    /// #EDE-131: a server-side tree for [`wire_text`]: `sessions get`'s
    /// root + clear flag, `sessionKeys.publicKeys`, `sessions.sendTarget`,
    /// and every call it saw.
    struct FakeServer {
        root: Option<(String, bool)>,
        key: Option<(u32, [u8; 32])>,
        target: api::sessions::SendTarget,
        calls: std::sync::Mutex<Vec<String>>,
    }

    impl FakeServer {
        fn new(root: Option<(&str, bool)>, key: Option<(u32, [u8; 32])>) -> Self {
            Self { root: root.map(|(id, clear)| (id.to_string(), clear)), key, target: Default::default(), calls: Default::default() }
        }
        fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl TreeServer for FakeServer {
        fn tree_of(&self, session_id: &str) -> Result<(String, bool), String> {
            self.calls.lock().unwrap().push(format!("sessions get {session_id}"));
            self.root.clone().ok_or_else(|| "NOT_FOUND".to_string())
        }
        fn key_of(&self, root_id: &str) -> Result<Option<(u32, [u8; 32])>, String> {
            self.calls.lock().unwrap().push(format!("publicKeys {root_id}"));
            Ok(self.key)
        }
        fn send_target(&self, agent_id: &str) -> Result<api::sessions::SendTarget, String> {
            self.calls.lock().unwrap().push(format!("sendTarget {agent_id}"));
            Ok(self.target.clone())
        }
    }

    const ROOT: &str = "6f1c2a4e-3b7d-4c1e-9a52-0d8e7f6b5a41";
    const CHILD: &str = "a83d5c10-7e2f-4b96-8c3a-51f0e9d2b7c4";

    /// The synced `sessions` row of a root, as the shape sends it.
    fn root_row(extra: serde_json::Value) -> SessionRow {
        let mut row = json!({"id": ROOT, "team_id": "t1", "user_id": "u1", "agent_id": "a2", "status": "idle", "created_at": "2026-11-18T08:00:00Z", "clear_tree": false, "seal_public_key": null, "seal_epoch": null});
        row.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        serde_json::from_value(row).unwrap()
    }

    fn into(session: &str) -> api::sessions::SendInput {
        api::sessions::SendInput { team_id: Some("t1".into()), session_id: Some(session.into()), text: Some("Ship it".into()), ..Default::default() }
    }

    fn opens(key: &enet_core::sealed::TreeKey, epoch: u32, body: &str) -> Option<String> {
        let opened = enet_core::sealed::open_body(body, &|e| (e == epoch).then_some(key.secret));
        opened.text.filter(|_| opened.status == sync::sealed::OpenStatus::Opened)
    }

    /// #EDE-131 (`clearTree.writerCases`): a synced keyed root seals with
    /// its own key and asks nothing; a clear tree is the ONLY plaintext.
    #[test]
    fn a_keyed_tree_is_sealed_and_only_a_clear_tree_goes_in_clear() {
        let key = enet_core::sealed::generate_tree_key();
        let mut r = rows();
        r.account.sessions = vec![root_row(json!({"seal_public_key": key.public_key(), "seal_epoch": 2}))];
        let tree = send_tree(&r.account, &into(ROOT));
        assert!(matches!(tree, SendTree::Keyed { epoch: 2, .. }), "{tree:?}");
        let server = FakeServer::new(None, None);
        let wire = wire_text(&server, &tree, "Ship it").unwrap();
        assert_eq!(opens(&key, 2, &wire).as_deref(), Some("Ship it"));
        assert!(server.calls().is_empty());

        r.account.sessions = vec![root_row(json!({"clear_tree": true}))];
        assert_eq!(send_tree(&r.account, &into(ROOT)), SendTree::Clear);
        assert_eq!(wire_text(&server, &SendTree::Clear, "Ship it").as_deref(), Ok("Ship it"));
        // No text (attachments only): nothing to seal.
        let files = api::sessions::SendInput { text: None, ..into(ROOT) };
        assert_eq!(send_tree(&r.account, &files), SendTree::None);
    }

    /// #EDE-134 (`sealed-content.json` `attachmentEmbedCases`): a sealed
    /// send's files are named INSIDE the sealed text; a clear one's the
    /// server appends.
    #[test]
    fn a_sealed_send_carries_its_attachment_lines_inside_the_seal() {
        let key = enet_core::sealed::generate_tree_key();
        let public = enet_core::sealed::public_key_from_b64(&key.public_key()).unwrap();
        domain::sealed::remember_upload("f1", "spec.pdf", "application/pdf");
        let wire = Wire::Sealed { root_id: ROOT.into(), epoch: 2, public };
        let sealed = wire.text("see attached", &["f1".into()]).unwrap();
        assert_eq!(opens(&key, 2, &sealed).as_deref(), Some("see attached\n\n[spec.pdf](/api/attachments/f1)"));
        assert_eq!(Wire::Clear.text("see attached", &["f1".into()]).as_deref(), Ok("see attached"));
    }

    /// #EDE-131: a pre-#EDE-97 keyless root (or a bad key on the row) asks
    /// the server for its current key; none there = the send FAILS, never
    /// plaintext (this shell holds no install that could key it).
    #[test]
    fn a_keyless_root_takes_the_server_s_key_or_is_refused() {
        let mut r = rows();
        r.account.sessions = vec![root_row(json!({}))];
        let tree = send_tree(&r.account, &into(ROOT));
        assert_eq!(tree, SendTree::Keyless(ROOT.into()));
        let refused = FakeServer::new(None, None);
        assert!(wire_text(&refused, &tree, "Ship it").is_err());
        assert_eq!(refused.calls(), [format!("publicKeys {ROOT}")]);

        let key = enet_core::sealed::generate_tree_key();
        let keyed = FakeServer::new(None, Some((1, key.public)));
        assert_eq!(opens(&key, 1, &wire_text(&keyed, &tree, "Ship it").unwrap()).as_deref(), Some("Ship it"));

        r.account.sessions = vec![root_row(json!({"seal_public_key": "not a key", "seal_epoch": 1}))];
        assert_eq!(send_tree(&r.account, &into(ROOT)), SendTree::Keyless(ROOT.into()), "a bad key is no key");
    }

    /// #EDE-131: a session the rows lack (a deep link before sync) asks the
    /// server for its tree (`sessions get`, then the ROOT's key); a clear
    /// one goes in clear, a keyed one sealed, an unknown or keyless one
    /// fails.
    #[test]
    fn an_unsynced_session_asks_the_server_for_its_tree() {
        let mut r = rows();
        r.account.sessions.clear();
        let tree = send_tree(&r.account, &into(CHILD));
        assert_eq!(tree, SendTree::Unsynced(CHILD.into()));

        let key = enet_core::sealed::generate_tree_key();
        let keyed = FakeServer::new(Some((ROOT, false)), Some((4, key.public)));
        assert_eq!(opens(&key, 4, &wire_text(&keyed, &tree, "Ship it").unwrap()).as_deref(), Some("Ship it"));
        assert_eq!(keyed.calls(), [format!("sessions get {CHILD}"), format!("publicKeys {ROOT}")]);

        let clear = FakeServer::new(Some((ROOT, true)), None);
        assert_eq!(wire_text(&clear, &tree, "Ship it").as_deref(), Ok("Ship it"));
        assert_eq!(clear.calls(), [format!("sessions get {CHILD}")], "a clear tree needs no key");

        assert!(wire_text(&FakeServer::new(Some((ROOT, false)), None), &tree, "Ship it").is_err(), "keyless");
        assert!(wire_text(&FakeServer::new(None, None), &tree, "Ship it").is_err(), "not found / offline");

        // A child synced without its root is unsynced too.
        let child: SessionRow = serde_json::from_value(json!({"id": CHILD, "root_id": ROOT, "parent_id": ROOT, "team_id": "t1", "status": "running"})).unwrap();
        r.account.sessions = vec![child];
        assert_eq!(send_tree(&r.account, &into(CHILD)), SendTree::Unsynced(CHILD.into()));
    }

    /// #EDE-131: an agent send without a synced conversation asks
    /// `sessions.sendTarget`: the conversation's key seals it.
    #[test]
    fn an_agent_send_without_a_synced_conversation_asks_the_server() {
        let mut r = rows();
        r.account.sessions.clear();
        let input = api::sessions::SendInput { team_id: Some("t1".into()), agent_id: Some("a2".into()), text: Some("Ship it".into()), ..Default::default() };
        let tree = send_tree(&r.account, &input);
        assert_eq!(tree, SendTree::Agent("a2".into()));
        let key = enet_core::sealed::generate_tree_key();
        let mut server = FakeServer::new(None, None);
        server.target = serde_json::from_value(json!({"rootId": ROOT, "epoch": 3, "publicKey": key.public_key(), "clearTree": false})).unwrap();
        assert_eq!(opens(&key, 3, &wire_text(&server, &tree, "Ship it").unwrap()).as_deref(), Some("Ship it"));
        server.target = serde_json::from_value(json!({"rootId": ROOT, "epoch": null, "publicKey": null})).unwrap();
        assert!(wire_text(&server, &tree, "Ship it").is_err(), "a keyless conversation");
        server.target = serde_json::from_value(json!({"rootId": ROOT, "clearTree": true})).unwrap();
        assert_eq!(wire_text(&server, &tree, "Ship it").as_deref(), Ok("Ship it"));
    }

    /// #EDE-131: an answer to a card the rows lack is refused locally; a
    /// plaintext card's goes as given; an opened sealed card's is the FULL
    /// card re-sealed with the answer, no plaintext answer fields.
    #[test]
    fn an_answer_is_sealed_for_a_sealed_card_and_refused_for_an_unknown_one() {
        let key = enet_core::sealed::generate_tree_key();
        let mut r = rows();
        r.account.sessions = vec![root_row(json!({"seal_public_key": key.public_key(), "seal_epoch": 1}))];
        let answer = |id: &str| api::sessions::AnswerInput { message_id: id.into(), answer: Some("Yes".into()), answers: Some(json!(["Yes"])), ..Default::default() };
        assert_eq!(answer_wire(&r, &answer("m-unknown")), Err(LOCKED_ANSWER.to_string()));

        let card = json!({"kind": "question", "question": "Ship it?", "options": ["Yes", "No"], "multi": false, "custom": false});
        let mut plain: MessageRow = serde_json::from_value(json!({"id": "m-plain", "conversation_id": "c1", "session_id": ROOT, "root_id": ROOT, "author_kind": "agent", "body": "", "card": card})).unwrap();
        r.messages.push(plain.clone());
        assert_eq!(answer_wire(&r, &answer("m-plain")), Ok(answer("m-plain")));

        plain.id = "m-sealed".into();
        plain.seal = Some(domain::sealed::SealState::Opened);
        r.messages.push(plain.clone());
        let wire = answer_wire(&r, &answer("m-sealed")).unwrap();
        assert!(wire.answer.is_none() && wire.answers.is_none());
        let opened = enet_core::sealed::open_card(wire.sealed_card.as_ref().unwrap(), &|e| (e == 1).then_some(key.secret));
        assert_eq!(opened.card["question"], json!("Ship it?"));
        assert_ne!(opened.card, card, "the answer is in the sealed card");

        plain.id = "m-locked".into();
        plain.seal = Some(domain::sealed::SealState::Locked);
        r.messages.push(plain);
        assert_eq!(answer_wire(&r, &answer("m-locked")), Err(LOCKED_ANSWER.to_string()));
    }

    /// The refusal is the fixture's copy.
    #[test]
    fn the_no_key_refusal_is_the_fixture_s() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!("../../../../../packages/domain-contract/fixtures/sealed-content.json")).unwrap();
        assert_eq!(fixture["clearTree"]["noKeyRefusal"], json!(NO_KEY_REFUSAL));
    }

    /// #EDE-98: the header's marks come off the tree's root (chats.json marksCases).
    #[test]
    fn the_marks_read_the_root_s_shared_and_key() {
        let marks = |r: &TeamRows| input(r, &chat(Some("s1")), 0).marks.iter().map(|m| (m.icon.clone(), m.label.clone(), m.test_id.clone())).collect::<Vec<_>>();
        let team = ("ui-team".to_string(), "Shared with the team".to_string(), "chat-mark-team".to_string());
        let open = ("ui-unencrypted".to_string(), "Not encrypted".to_string(), "chat-mark-unencrypted".to_string());
        let mut r = rows();
        assert_eq!(marks(&r), [open.clone()], "a private chat without a key");
        r.account.sessions[0].shared = true;
        assert_eq!(marks(&r), [team.clone(), open], "a shared chat without a key: the team first");
        r.account.sessions[0].seal_epoch = Some(1);
        assert_eq!(marks(&r), [team], "a shared encrypted chat");
        r.account.sessions[0].shared = false;
        assert!(marks(&r).is_empty(), "a private encrypted chat draws nothing");
        assert!(input(&r, &chat(None), 0).marks.is_empty(), "the home has none");
    }

    #[test]
    fn a_show_card_draws_its_surface_and_its_reply_is_a_send_into_its_session() {
        let mut rows = rows();
        rows.messages.push(
            serde_json::from_value(json!({"id": "m4", "conversation_id": "c1", "session_id": "s1", "root_id": "s1", "author_kind": "agent", "body": "", "created_at": "2026-11-08T09:17:00Z",
                "card": {"kind": "surface", "title": "Reports", "surface": demo_card()}}))
            .unwrap(),
        );
        let open = chat(Some("s1"));
        let card = input(&rows, &open, 0).rows.into_iter().find(|r| matches!(r, uicore_model::chat::ChatRowInput::Card { id, .. } if id == "m4"));
        let Some(uicore_model::chat::ChatRowInput::Card { title, surface: Some(surface), .. }) = card else { panic!("{card:?}") };
        assert_eq!((title.as_str(), &surface["data"]["new"]), ("Reports", &json!("2")));
        // chat-express.json ACTIONS: the reply's text is the body, THE
        // action card rides with it; every other event is forwarded too.
        let reply = card_reply(&rows, &open, "m4", "reply", &json!({"text": "Label the rest too"}), 0).expect("a reply");
        let (send, card) = reply_input(&open, &reply).expect("a send");
        assert_eq!((send.team_id.as_deref(), send.session_id.as_deref(), send.text.as_deref()), (Some("t1"), Some("s1"), Some("Label the rest too")));
        assert_eq!(card, json!({"kind": "action", "name": "reply", "context": {"text": "Label the rest too"}, "surfaceId": "show:m4", "block": null, "messageId": "m4"}));
        assert_eq!(send.action_card.as_ref(), Some(&card));
        let picked = card_reply(&rows, &open, "m4", "picked", &json!({"id": 7}), 0).expect("every event is forwarded");
        let (send, card) = reply_input(&open, &picked).unwrap();
        assert_eq!((send.text, &card["name"], &card["context"]), (None, &json!("picked"), &json!({"id": 7})));
        assert!(card_reply(&rows, &open, "m2", "reply", &json!({"text": "x"}), 0).is_none(), "a message is no card");
        assert!(reply_input(&open, &crate::chat::CardReply::TooLarge).is_none(), "too large: nothing is sent");
    }

    const BLOCK: &str = "Here is the build:\n<a2ui>\nroot = Stack(children=[title, ship])\ntitle = Text(text=\"Build 1482 is green\")\nship = Button(label=\"Ship it\", on={press: Event(\"reply\", {text: \"Ship it\"})})\n</a2ui>\nShip it?";

    /// #EXP-2077 (chat-express.json DRAW, ACTIONS): an agent's row with a
    /// block draws its prose bubbles and the block's frame in one sender
    /// group; the frame's press posts into the chat's root; an action row
    /// is its author's bubble or THE marker.
    #[test]
    fn an_agent_row_with_a_block_draws_its_segments_and_its_press_posts_the_action() {
        use uicore_model::chat::ChatRowInput;
        let mut rows = rows();
        let message = |value: serde_json::Value| -> MessageRow { serde_json::from_value(value).unwrap() };
        rows.messages.push(message(json!({"id": "m7", "conversation_id": "c1", "session_id": "s1", "root_id": "s1", "author_kind": "agent", "body": BLOCK, "created_at": "2026-11-08T09:20:00Z"})));
        rows.messages.push(message(json!({"id": "m8", "conversation_id": "c1", "session_id": "s1", "root_id": "s1", "author_kind": "user", "author_id": "u1", "body": "Ship it", "created_at": "2026-11-08T09:21:00Z", "consumed_at": "2026-11-08T09:21:01Z",
            "card": {"kind": "action", "name": "reply", "context": {"text": "Ship it"}, "surfaceId": "m:m7#1", "block": "m:m7#1", "messageId": "m7"}})));
        rows.messages.push(message(json!({"id": "m9", "conversation_id": "c1", "session_id": "s1", "root_id": "s1", "author_kind": "user", "author_id": "u1", "body": "", "created_at": "2026-11-08T09:22:00Z",
            "card": {"kind": "action", "name": "save_deal", "context": null, "surfaceId": "m:m7#1", "block": "m:m7#1", "messageId": "m7"}})));
        let open = chat(Some("s1"));
        let drawn = input(&rows, &open, 0).rows;
        let at = drawn.iter().position(|row| matches!(row, ChatRowInput::Message { id, .. } if id == "m7")).expect("the row's first prose");
        let ChatRowInput::Message { body, time, group: Some(lead), .. } = &drawn[at] else { panic!() };
        assert_eq!((body.as_str(), time.as_str()), ("Here is the build:", ""), "the time waits for the last prose");
        let ChatRowInput::Card { id, title, text, target, surface: Some(surface), group: Some(frame), .. } = &drawn[at + 1] else { panic!("{:?}", drawn[at + 1]) };
        assert_eq!((id.as_str(), title.as_str(), text.as_str(), target), ("m:m7#1@m:m7#1", "", "", &None));
        assert_eq!((frame.indent, frame.gap, frame.avatar.as_deref()), (lead.indent, 8, None), "the frame sits in the sender's column");
        assert_eq!(surface["root"]["children"][1]["props"]["label"], "Ship it");
        let ChatRowInput::Message { id, body, time, name, group: Some(after), .. } = &drawn[at + 2] else { panic!("{:?}", drawn[at + 2]) };
        assert_eq!((id.as_str(), body.as_str(), name, after.gap, after.tail), ("m7~2", "Ship it?", &None, 8, false));
        assert!(!time.is_empty(), "the last prose carries the time");
        // The action rows: the viewer's bubble with its body, else THE marker.
        assert!(matches!(&drawn[at + 3], ChatRowInput::Message { id, author, body, .. } if id == "m8" && author == "user" && body == "Ship it"), "{:?}", drawn[at + 3]);
        assert_eq!(drawn[at + 4], ChatRowInput::Marker { id: "m9".into(), label: "You pressed save_deal".into(), group: None });
        // As the document binds it: a card with a surface and no title.
        let view = uicore_model::chat::chat_view(&input(&rows, &open, 0));
        let bound = crate::chat::chatting(&view);
        let frame = bound["rows"].as_array().unwrap().iter().find(|row| row["id"] == "m:m7#1@m:m7#1").expect("the frame's row");
        assert_eq!((&frame["kind"], &frame["title"], &frame["indent"], &frame["gap"]), (&json!("card"), &json!(""), &json!(lead.indent), &json!(8)));
        assert_eq!(frame["surface"]["version"], eui_core::catalog::A2UI_VERSION);
        // The frame's press: into the chat's root, naming its block and message.
        let press = card_reply(&rows, &open, "m:m7#1@m:m7#1", "reply", &json!({"text": "Ship it"}), 0).expect("a block's event is forwarded");
        let (send, card) = reply_input(&open, &press).unwrap();
        assert_eq!((send.session_id.as_deref(), send.text.as_deref()), (Some("s1"), Some("Ship it")));
        assert_eq!(card, json!({"kind": "action", "name": "reply", "context": {"text": "Ship it"}, "surfaceId": "m:m7#1", "block": "m:m7#1", "messageId": "m7"}));
    }

    /// #EXP-2067: into a keyed tree THE action card leaves as its envelope
    /// alone (every field inside the seal), also when it goes without text.
    #[test]
    fn an_action_card_is_sealed_into_a_keyed_tree() {
        let key = enet_core::sealed::generate_tree_key();
        let mut r = rows();
        r.account.sessions = vec![root_row(json!({"seal_public_key": key.public_key(), "seal_epoch": 2}))];
        let card = json!({"kind": "action", "name": "save_deal", "context": {"rep": "Ana"}, "surfaceId": "m:m7#1", "block": "m:m7#1", "messageId": "m7"});
        let alone = api::sessions::SendInput { text: None, action_card: Some(card.clone()), ..into(ROOT) };
        let tree = send_tree(&r.account, &alone);
        assert!(matches!(tree, SendTree::Keyed { epoch: 2, .. }), "a card alone is content to seal: {tree:?}");
        let wire = wire_for(&FakeServer::new(None, None), &tree).unwrap();
        let sealed = wire.action_card(&card).unwrap();
        assert_eq!(sealed.as_object().unwrap().keys().collect::<Vec<_>>(), ["kind", "sealed"], "the envelope alone");
        assert_eq!(sealed["kind"], "action");
        let opened = enet_core::sealed::open_card(&sealed, &|e| (e == 2).then_some(key.secret));
        assert_eq!(opened.status, sync::sealed::OpenStatus::Opened);
        assert_eq!(opened.card, card);
        assert_eq!(Wire::Clear.action_card(&card).unwrap(), card, "a clear tree takes it as it is");
    }
}
