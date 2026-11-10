//! The agent chat on a team (#EDE-65 lane I): the account's synced rows
//! (sessions, agents, messages, users: the IDE's shapes) as
//! the chat model's input, and the sends through `sessions.send` like the
//! IDE's (`chat_actions::send_to_agent`, `chat_launch`): a message into an
//! open session, or to the picked agent (the server lands it in the
//! person's conversation with that agent). A sealed tree's text is sealed
//! to its public key first (`coding::sealing::seal_body`'s envelope); a
//! tree without one takes the plaintext, as the IDE does without its
//! sealing service.

use std::collections::HashMap;

use domain::rows::{AgentDefRow, MessageRow, SessionRow, User};
use uicore_model::chat::{ChatAgentInput, ChatInput};

use crate::inbox::AccountRows;

/// The chat's place on a team: the account, the team, the open session
/// (`None` = the home).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamChat {
    pub account_id: String,
    pub team_id: String,
    pub session_id: Option<String>,
}

/// What the chat reads off the account's store.
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
            Some(crate::chat::Member { name: u.name.clone().filter(|n| !n.is_empty()).unwrap_or_else(|| email.clone()), email })
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
    input.rows = crate::chat::rows(&view.rows, &by_id, |id| Some(author_label(id.and_then(|id| users.get(id).copied()))));
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
    input
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

/// Lane Q: what an Embed's inner event on card `message_id` asks
/// (`domain::chat_document::card_reply` over the open session's rows).
pub fn card_reply(rows: &TeamRows, chat: &TeamChat, message_id: &str, name: &str, context: &serde_json::Value, now_ms: i64) -> Option<crate::chat::CardReply> {
    let (_, view, _) = session_chat(rows, chat, now_ms)?;
    crate::chat::card_reply(&view.rows, message_id, Some(name), Some(context))
}

/// A card reply's send: the text into the card's session.
pub fn reply_input(chat: &TeamChat, reply: &crate::chat::CardReply) -> api::sessions::SendInput {
    let crate::chat::CardReply::Send { session_id, text } = reply;
    api::sessions::SendInput { team_id: Some(chat.team_id.clone()), session_id: Some(session_id.clone()), text: Some(text.clone()), ..Default::default() }
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

/// The tree a send lands in (the open session's root, else the person's
/// conversation with the agent), when it is synced.
pub fn target_root<'a>(rows: &'a AccountRows, input: &api::sessions::SendInput) -> Option<&'a SessionRow> {
    if let Some(session) = &input.session_id {
        let row = rows.sessions.iter().find(|s| &s.id == session)?;
        return rows.sessions.iter().find(|s| s.id == row.root_id_or_self());
    }
    let agent = input.agent_id.as_deref()?;
    rows.sessions.iter().find(|s| s.is_root() && s.agent_id.as_deref() == Some(agent) && s.user_id.as_deref() == Some(rows.user_id.as_str()))
}

/// The text sealed to `root`'s tree key when it has one (the IDE's
/// envelope); `None` = no key: the text goes as it is.
pub fn seal(root: &SessionRow, text: &str) -> Option<String> {
    let public = enet_core::sealed::public_key_from_b64(root.seal_public_key.as_deref()?).ok()?;
    let epoch = u32::try_from(root.seal_epoch.unwrap_or(0)).ok()?;
    enet_core::sealed::seal_body(&public, epoch, text).ok()
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
    if std::env::var("EXP1780_LONG").is_ok() {
        for i in 0..24 { input.rows.push(message(&format!("x{i}"), if i % 2 == 0 { "user" } else { "agent" }, &format!("Long conversation line {i}: this keeps going to overflow the panel."), "10:00")); }
    }
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

    #[test]
    fn a_tree_without_a_key_is_not_sealed_and_one_with_a_key_is() {
        let r = rows();
        assert!(seal(&r.account.sessions[0], "hi").is_none());
        let public = enet_core::sealed::generate_tree_key().public_key();
        let keyed: SessionRow = serde_json::from_value(json!({"id": "s1", "seal_public_key": public, "seal_epoch": 2})).unwrap();
        let sealed = seal(&keyed, "hi").expect("sealed");
        assert!(sealed.starts_with("exp-sealed:v1:2:"), "{sealed}");
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
        let reply = card_reply(&rows, &open, "m4", "reply", &json!({"text": "Label the rest too"}), 0).expect("a reply");
        let send = reply_input(&open, &reply);
        assert_eq!((send.team_id.as_deref(), send.session_id.as_deref(), send.text.as_deref()), (Some("t1"), Some("s1"), Some("Label the rest too")));
        assert!(card_reply(&rows, &open, "m4", "picked", &json!({"text": "x"}), 0).is_none(), "only reply answers");
        assert!(card_reply(&rows, &open, "m2", "reply", &json!({"text": "x"}), 0).is_none(), "a message is no card");
    }
}
