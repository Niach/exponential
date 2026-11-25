//! #EDE-65: the chats the icon rail lists, from the session rows the shell
//! has: the current team's (the account's synced `sessions` shape) or the
//! Local desktop's (elocal's store). The IDE's chats rule
//! (`domain::session_tree::partition_chats`): a root that is neither an
//! agent's conversation nor an app's editor session; it needs you while any
//! live session of its tree asks (`session_asks`: a wall never) and is busy
//! while one works; its activity is the
//! tree's newest `updated_at`; its agent is the working session's CLI, else
//! the root's; its unread count is the IDE's own (chats.json `unreadCount`,
//! #EXP-1730: over the root's digest and the viewer's `session_reads` mark).
//! `uicore_model::chats::chat_rows` orders and badges them.

use std::{collections::HashMap, sync::Arc};

use domain::rows::{MessageRow, SessionReadRow, SessionRow};
use domain::session_tree::{session_asks, session_state, SessionState};
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

/// Each root's unread agent messages (chats.json `unreadCount`).
pub fn unread_counts(sessions: &[SessionRow], reads: &[SessionReadRow], team_id: &str) -> HashMap<String, usize> {
    let reads: Vec<&SessionReadRow> = reads.iter().collect();
    sessions
        .iter()
        .filter(|s| s.is_root() && s.team_id.as_deref() == Some(team_id))
        .map(|root| {
            let digest = domain::chats::ConversationDigest::decode(root.digest.as_ref());
            let messages: Vec<MessageRow> = domain::chats::digest_messages(&root.id, digest.as_ref());
            let messages: Vec<&MessageRow> = messages.iter().collect();
            (root.id.clone(), domain::chats::unread_count(&root.id, &messages, &reads))
        })
        .collect()
}

/// The chats of `team_id` among `sessions`; `unread` = [`unread_counts`].
pub fn chat_inputs(sessions: &[SessionRow], team_id: &str, unread: &HashMap<String, usize>) -> Vec<ChatInput> {
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
                needs_you: live.iter().any(|s| session_asks(s)),
                busy: working.is_some(),
                unread_count: unread.get(&root.id).copied().unwrap_or(0),
                at: at.to_string(),
            }
        })
        .collect()
}

/// Where the rail's chats are read: the current entry's sessions.
pub enum Source {
    /// A Local desktop's own store.
    Local { store: Arc<elocal::LocalStore>, team_id: String },
    /// The team's account's synced store.
    Team { store: Arc<sync::store::ShapeStore>, team_id: String },
}

impl Source {
    /// The rail's chats: BLOCKING (SQLite, every row of `sessions` and
    /// `session_reads`). #EDE-204: off the main loop on the deltas' path.
    pub fn read(&self) -> Vec<ChatInput> {
        match self {
            Source::Local { store, team_id } => chat_inputs(&sessions(store.rows("sessions").unwrap_or_default()), team_id, &HashMap::new()),
            Source::Team { store, team_id } => {
                let read = |shape: &str| sync::shapes::shape_by_name(shape).and_then(|spec| store.read_all(spec).ok()).unwrap_or_default();
                let sessions = sessions(read("sessions"));
                let unread = unread_counts(&sessions, &rows(read("session_reads")), team_id);
                chat_inputs(&sessions, team_id, &unread)
            }
        }
    }
}

/// `/shell/chats`: the rail's rows with the glyph the document binds (the
/// agent's mark, wearing THE count badge in its corner, else its working
/// dot).
pub fn chats_json(chats: &[ChatInput]) -> Value {
    Value::Array(
        chat_rows(chats)
            .into_iter()
            .map(|row| {
                let glyph = match (&row.count, row.tone) {
                    (Some(count), Some(tone)) => format!("ide:badge:count:{}:{count}|{}", json!(tone).as_str().unwrap_or_default(), row.icon),
                    _ if row.working => format!("ide:badge:dot:green|{}", row.icon),
                    _ => row.icon.clone(),
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
        ChatInput { unread_count: 2, ..chat("c3", "Fix the Wi-Fi drop-outs", "claude", "2026-11-08T10:03:00.000Z") },
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
        let chats = chat_inputs(&rows, "t1", &HashMap::new());
        assert_eq!(chats.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["r1", "r2"]);
        let (r1, r2) = (&chats[0], &chats[1]);
        assert_eq!((r1.agent.as_deref(), r1.busy, r1.needs_you, r1.at.as_str()), (Some("codex"), true, false, "2026-11-08T11:00:00.000Z"));
        assert_eq!((r2.needs_you, r2.busy, r2.title.as_str()), (true, false, ""));
        assert_eq!(
            chats_json(&chats),
            json!([
                {"id": "r1", "title": "Fix it", "glyph": "ide:badge:dot:green|agent:codex"},
                {"id": "r2", "title": "Untitled", "glyph": "ide:badge:count:attention:1|agent:claude"}
            ])
        );
    }

    #[test]
    fn an_ended_tree_is_quiet() {
        let rows = vec![row(json!({"id": "r1", "team_id": "t1", "status": "ended", "busy": true, "needs_input": true}))];
        let chats = chat_inputs(&rows, "t1", &HashMap::new());
        assert_eq!((chats[0].busy, chats[0].needs_you), (false, false));
    }

    /// A wall asks nothing: no badge.
    #[test]
    fn a_walled_chat_has_no_badge() {
        let rows = vec![row(json!({"id": "r1", "team_id": "t1", "title": "Fix it", "status": "idle", "needs_input": true, "blocked": {"kind": "rate_limit"}}))];
        assert_eq!(chats_json(&chat_inputs(&rows, "t1", &HashMap::new()))[0]["glyph"], "agent:claude");
    }

    /// #EDE-65: the IDE's unread rule: an agent message after the read mark.
    #[test]
    fn a_chat_counts_what_the_agent_wrote_after_the_read_mark() {
        let digest = |at: &str| json!({"preview": {"id": format!("m-{at}"), "authorKind": "agent", "body": "done", "createdAt": at}, "agentAt": at, "agentTimes": [at]});
        let sessions = vec![
            row(json!({"id": "r1", "team_id": "t1", "status": "active", "updated_at": "2026-11-08T10:00:00.000Z", "digest": digest("2026-11-08T10:05:00.000Z")})),
            row(json!({"id": "r2", "team_id": "t1", "status": "active", "updated_at": "2026-11-08T10:00:00.000Z", "digest": digest("2026-11-08T10:05:00.000Z")})),
            row(json!({"id": "r3", "team_id": "t1", "status": "active", "updated_at": "2026-11-08T10:00:00.000Z"})),
        ];
        let reads: Vec<SessionReadRow> = vec![
            serde_json::from_value(json!({"id": "x1", "session_id": "r1", "read_at": "2026-11-08T10:00:00.000Z"})).unwrap(),
            serde_json::from_value(json!({"id": "x2", "session_id": "r2", "read_at": "2026-11-08T10:06:00.000Z"})).unwrap(),
        ];
        let unread = unread_counts(&sessions, &reads, "t1");
        assert_eq!(unread["r1"], 1);
        assert_eq!((unread["r2"], unread["r3"]), (0, 0), "r2 was read after, r3 has no agent message");
        let chats = chat_inputs(&sessions, "t1", &unread);
        assert_eq!(chats.iter().map(|c| c.unread_count).collect::<Vec<_>>(), [1, 0, 0]);
    }
}
