//! #EDE-65: the chats the icon rail lists, from the session rows the shell
//! has: the current team's (the account's synced `sessions` shape) or the
//! Local desktop's (elocal's store). The IDE's chats rule
//! (`domain::session_tree::partition_chats`): a root that is neither an
//! agent's conversation nor an app's editor session; it needs you or is
//! busy while any live session of its tree does; its activity is the
//! tree's newest `updated_at`; its agent is the working session's CLI, else
//! the root's; unread is the IDE's own (#EXP-1323/#EXP-1724): the tree's
//! newest agent message (the conversations' digests) newer than the
//! viewer's `session_reads` mark, from `partition_chats` itself.
//! `uicore_model::chats::chat_rows` orders and marks them.

use std::collections::{HashMap, HashSet};

use domain::rows::{ConversationRow, MessageRow, SessionReadRow, SessionRow};
use domain::session_tree::{partition_chats, session_state, SessionState, ChatsListInput};
use serde_json::{json, Map, Value};
use uicore_model::chats::{chat_rows, ChatInput};

/// Store rows as typed rows (a row that does not parse is skipped).
pub fn rows<T: serde::de::DeserializeOwned>(rows: Vec<Map<String, Value>>) -> Vec<T> {
    rows.into_iter().filter_map(|row| serde_json::from_value(Value::Object(row)).ok()).collect()
}

/// Store rows as session rows.
pub fn sessions(rows: Vec<Map<String, Value>>) -> Vec<SessionRow> {
    self::rows(rows)
}

/// The roots of `team_id` the IDE's Chats list shows bold: the agent wrote
/// after the viewer's read mark (`partition_chats`, the IDE's rule).
pub fn unread_roots(sessions: &[SessionRow], conversations: &[ConversationRow], reads: &[SessionReadRow], team_id: &str) -> HashSet<String> {
    let in_team: HashSet<&str> = sessions.iter().filter(|s| s.team_id.as_deref() == Some(team_id)).map(|s| s.id.as_str()).collect();
    let messages: Vec<MessageRow> = conversations
        .iter()
        .filter_map(|c| Some((c.session_id.as_deref().filter(|id| in_team.contains(id))?, c)))
        .flat_map(|(session, c)| {
            let digest = domain::chats::ConversationDigest::decode(c.digest.as_ref());
            domain::chats::digest_messages(session, digest.as_ref()).into_iter().map(|m| MessageRow { conversation_id: c.id.clone(), ..m })
        })
        .collect();
    let rows: Vec<&SessionRow> = sessions.iter().filter(|s| in_team.contains(s.id.as_str())).collect();
    let messages: Vec<&MessageRow> = messages.iter().collect();
    let reads: Vec<&SessionReadRow> = reads.iter().filter(|r| in_team.contains(r.session_id.as_str())).collect();
    partition_chats(ChatsListInput { sessions: &rows, session_issues: &[], issues: &[], messages: &messages, team_id: Some(team_id), reads: &reads, now_ms: None })
        .into_iter()
        .flat_map(|group| group.rows)
        .filter(|row| row.depth == 0 && row.unread)
        .map(|row| row.session_id)
        .collect()
}

/// The chats of `team_id` among `sessions`; `unread` = [`unread_roots`].
pub fn chat_inputs(sessions: &[SessionRow], team_id: &str, unread: &HashSet<String>) -> Vec<ChatInput> {
    let mut trees: HashMap<&str, Vec<&SessionRow>> = HashMap::new();
    for s in sessions {
        trees.entry(s.root_id_or_self()).or_default().push(s);
    }
    sessions
        .iter()
        .filter(|s| s.is_root() && !s.is_agent() && !s.is_app_editor() && s.team_id.as_deref() == Some(team_id))
        .map(|root| {
            let tree = trees.get(root.id.as_str()).map(Vec::as_slice).unwrap_or_default();
            let live: Vec<&&SessionRow> = tree.iter().filter(|s| !s.is_ended()).collect();
            let working = live.iter().find(|s| session_state(s) == SessionState::Working);
            let at = tree.iter().filter_map(|s| s.updated_at.as_deref().or(s.created_at.as_deref())).max().unwrap_or_default();
            ChatInput {
                id: root.id.clone(),
                title: root.title.clone().unwrap_or_default(),
                agent: working.and_then(|s| s.cli.clone()).or_else(|| root.cli.clone()),
                needs_you: live.iter().any(|s| s.needs_input),
                busy: working.is_some(),
                unread: unread.contains(&root.id),
                at: at.to_string(),
            }
        })
        .collect()
}

/// `/shell/chats`: the rail's rows with the glyph the document binds (the
/// agent's mark, wearing its dot in the corner as the IDE's rail does).
pub fn chats_json(chats: &[ChatInput]) -> Value {
    Value::Array(
        chat_rows(chats)
            .into_iter()
            .map(|row| {
                let glyph = match row.dot {
                    Some(dot) => format!("ide:badge:dot:{}|{}", json!(dot).as_str().unwrap_or_default(), row.icon),
                    None => row.icon.clone(),
                };
                json!({"id": row.id, "title": row.title, "glyph": glyph})
            })
            .collect(),
    )
}

/// The goldens' chats: a few rows in every state.
pub fn demo_chats() -> Vec<ChatInput> {
    let chat = |id: &str, title: &str, agent: &str, at: &str| ChatInput { id: id.into(), title: title.into(), agent: Some(agent.into()), at: at.into(), ..Default::default() };
    vec![
        ChatInput { needs_you: true, ..chat("c1", "Plan the printer setup", "claude", "2026-11-08T10:05:00.000Z") },
        ChatInput { busy: true, ..chat("c2", "Back up the photos", "codex", "2026-11-08T10:04:00.000Z") },
        ChatInput { unread: true, ..chat("c3", "Fix the Wi-Fi drop-outs", "claude", "2026-11-08T10:03:00.000Z") },
        chat("c4", "Sort the downloads folder", "claude", "2026-11-08T10:02:00.000Z"),
        chat("c5", "Draft the family newsletter", "codex", "2026-11-08T10:01:00.000Z"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(v: Value) -> SessionRow {
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn a_chat_is_a_root_session_of_the_team_with_its_trees_state() {
        let rows = vec![
            row(json!({"id": "r1", "team_id": "t1", "title": "Fix it", "cli": "claude", "status": "active", "updated_at": "2026-11-08T09:00:00.000Z"})),
            row(json!({"id": "k1", "team_id": "t1", "parent_id": "r1", "root_id": "r1", "depth": 1, "cli": "codex", "status": "active", "busy": true, "updated_at": "2026-11-08T11:00:00.000Z"})),
            row(json!({"id": "r2", "team_id": "t1", "status": "active", "needs_input": true, "updated_at": "2026-11-08T10:00:00.000Z"})),
            row(json!({"id": "agent", "team_id": "t1", "agent_id": "a1", "status": "active"})),
            row(json!({"id": "editor", "team_id": "t1", "app_id": "app1", "status": "active"})),
            row(json!({"id": "other", "team_id": "t2", "status": "active"})),
        ];
        let chats = chat_inputs(&rows, "t1", &HashSet::new());
        assert_eq!(chats.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["r1", "r2"]);
        let (r1, r2) = (&chats[0], &chats[1]);
        assert_eq!((r1.agent.as_deref(), r1.busy, r1.needs_you, r1.at.as_str()), (Some("codex"), true, false, "2026-11-08T11:00:00.000Z"));
        assert_eq!((r2.needs_you, r2.busy, r2.title.as_str()), (true, false, ""));
        assert_eq!(
            chats_json(&chats),
            json!([
                {"id": "r1", "title": "Fix it", "glyph": "ide:badge:dot:green|agent:codex"},
                {"id": "r2", "title": "Untitled", "glyph": "ide:badge:dot:amber|agent:claude"}
            ])
        );
    }

    #[test]
    fn an_ended_tree_is_quiet() {
        let rows = vec![row(json!({"id": "r1", "team_id": "t1", "status": "ended", "busy": true, "needs_input": true}))];
        let chats = chat_inputs(&rows, "t1", &HashSet::new());
        assert_eq!((chats[0].busy, chats[0].needs_you), (false, false));
    }

    /// #EDE-65: the IDE's unread rule: an agent message after the read mark.
    #[test]
    fn a_chat_is_unread_while_the_agent_wrote_after_the_read_mark() {
        let sessions = vec![
            row(json!({"id": "r1", "team_id": "t1", "status": "active", "updated_at": "2026-11-08T10:00:00.000Z"})),
            row(json!({"id": "r2", "team_id": "t1", "status": "active", "updated_at": "2026-11-08T10:00:00.000Z"})),
            row(json!({"id": "r3", "team_id": "t1", "status": "active", "updated_at": "2026-11-08T10:00:00.000Z"})),
        ];
        let digest = |at: &str| json!({"preview": {"id": format!("m-{at}"), "authorKind": "agent", "body": "done", "createdAt": at}, "agentAt": at, "agentTimes": [at]});
        let conversations: Vec<ConversationRow> = vec![
            serde_json::from_value(json!({"id": "c1", "kind": "session", "session_id": "r1", "digest": digest("2026-11-08T10:05:00.000Z")})).unwrap(),
            serde_json::from_value(json!({"id": "c2", "kind": "session", "session_id": "r2", "digest": digest("2026-11-08T10:05:00.000Z")})).unwrap(),
        ];
        let reads: Vec<SessionReadRow> = vec![
            serde_json::from_value(json!({"id": "x1", "session_id": "r1", "read_at": "2026-11-08T10:00:00.000Z"})).unwrap(),
            serde_json::from_value(json!({"id": "x2", "session_id": "r2", "read_at": "2026-11-08T10:06:00.000Z"})).unwrap(),
        ];
        let unread = unread_roots(&sessions, &conversations, &reads, "t1");
        assert_eq!(unread, HashSet::from(["r1".to_string()]), "r2 was read after, r3 has no agent message");
        let chats = chat_inputs(&sessions, "t1", &unread);
        assert_eq!(chats.iter().map(|c| c.unread).collect::<Vec<_>>(), [true, false, false]);
    }
}
