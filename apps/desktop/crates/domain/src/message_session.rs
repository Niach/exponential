//! #EXP-1730 (one-tree model): a message lives on a SESSION, its own
//! `messages.session_id`; a session's thread is the messages carrying that
//! session. Nothing keys through `conversations` (the desktop, the CLI
//! daemon and eDE no longer sync it).
//!
//! Locked by `fixtures/message-session.json`; mirrors: TS
//! `src/message-session.ts`, iOS `MessageSession.swift`, Android
//! `domain/MessageSession.kt`.

use crate::rows::MessageRow;

/// The session `message` lives in (`messages.session_id`); `None` = a
/// dm/channel row, or one from a server older than #EXP-1730.
pub fn message_session(message: &MessageRow) -> Option<&str> {
    Some(message.session_id.as_str()).filter(|id| !id.is_empty())
}

/// The messages that carry a session, each with its `session_id`, cloned in
/// input order; a session-less row belongs to none.
pub fn key_messages_by_session<'a>(messages: impl IntoIterator<Item = &'a MessageRow>) -> Vec<MessageRow> {
    messages.into_iter().filter(|message| message_session(message).is_some()).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    const FIXTURE: &str = include_str!("../../../../../packages/domain-contract/fixtures/message-session.json");

    fn messages(value: &Value) -> Vec<MessageRow> {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|row| {
                serde_json::from_value(json!({
                    "id": row["id"],
                    // A legacy column the row may still carry: ignored.
                    "conversation_id": row.get("conversationId").cloned().unwrap_or(Value::Null),
                    "session_id": row.get("sessionId").cloned().unwrap_or(Value::Null),
                }))
                .unwrap()
            })
            .collect()
    }

    #[test]
    fn every_fixture_case_matches() {
        let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let keyed = key_messages_by_session(&messages(&case["messages"]));
            let actual: Vec<Value> =
                keyed.iter().map(|message| json!({"id": message.id, "sessionId": message.session_id})).collect();
            assert_eq!(Value::Array(actual), case["expected"], "{name}");
        }
    }

    #[test]
    fn a_session_less_row_has_no_session() {
        let row: MessageRow = serde_json::from_value(json!({"id": "m", "session_id": null})).unwrap();
        assert_eq!(message_session(&row), None);
        let row: MessageRow = serde_json::from_value(json!({"id": "m", "session_id": "s1"})).unwrap();
        assert_eq!(message_session(&row), Some("s1"));
    }
}
