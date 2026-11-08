//! #EDE-65: the chats the icon rail lists, from the session rows the shell
//! has: the current team's (the account's synced `sessions` shape) or the
//! Local desktop's (elocal's store). The IDE's threads rule
//! (`domain::session_tree::partition_threads`): a root that is neither an
//! agent's conversation nor an app's editor session; it needs you or is
//! busy while any live session of its tree does; its activity is the
//! tree's newest `updated_at`; its agent is the working session's CLI, else
//! the root's. `uicore_model::chats::chat_rows` orders and marks them.

use std::collections::HashMap;

use domain::rows::SessionRow;
use domain::session_tree::{session_state, SessionState};
use serde_json::{json, Map, Value};
use uicore_model::chats::{chat_rows, ChatInput};

/// Store rows as session rows (a row that does not parse is skipped).
pub fn sessions(rows: Vec<Map<String, Value>>) -> Vec<SessionRow> {
    rows.into_iter().filter_map(|row| serde_json::from_value(Value::Object(row)).ok()).collect()
}

/// The chats of `team_id` among `sessions`.
pub fn chat_inputs(sessions: &[SessionRow], team_id: &str) -> Vec<ChatInput> {
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
                unread: false,
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
    fn a_chat_is_a_root_thread_of_the_team_with_its_trees_state() {
        let rows = vec![
            row(json!({"id": "r1", "team_id": "t1", "title": "Fix it", "cli": "claude", "status": "active", "updated_at": "2026-11-08T09:00:00.000Z"})),
            row(json!({"id": "k1", "team_id": "t1", "parent_id": "r1", "root_id": "r1", "depth": 1, "cli": "codex", "status": "active", "busy": true, "updated_at": "2026-11-08T11:00:00.000Z"})),
            row(json!({"id": "r2", "team_id": "t1", "status": "active", "needs_input": true, "updated_at": "2026-11-08T10:00:00.000Z"})),
            row(json!({"id": "agent", "team_id": "t1", "agent_id": "a1", "status": "active"})),
            row(json!({"id": "editor", "team_id": "t1", "app_id": "app1", "status": "active"})),
            row(json!({"id": "other", "team_id": "t2", "status": "active"})),
        ];
        let chats = chat_inputs(&rows, "t1");
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
        let chats = chat_inputs(&rows, "t1");
        assert_eq!((chats[0].busy, chats[0].needs_you), (false, false));
    }
}
