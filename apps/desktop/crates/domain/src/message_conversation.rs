//! #EXP-1730 (one-tree model): a message's session is its OWN
//! `messages.session_id` ([`message_session`], [`session_messages`]); the
//! desktop, the CLI daemon and eDE no longer sync `conversations`.
//!
//! NEW-8 (the legacy model, kept as the reference the fixture replays until
//! phase B retires it): messages live in CONVERSATIONS; a session's thread
//! is the messages of its own (kind `session`) conversation. Locked by
//! `fixtures/message-conversation.json`; mirrors: TS
//! `src/message-conversation.ts`, iOS `MessageConversation.swift`, Android
//! `domain/MessageConversation.kt`.

use std::collections::HashMap;

use crate::rows::{ConversationRow, MessageRow};

/// #EXP-1730: the session `message` lives in (`messages.session_id`);
/// `None` = a dm/channel row, or one from a server older than #EXP-1730.
pub fn message_session(message: &MessageRow) -> Option<&str> {
    Some(message.session_id.as_str()).filter(|id| !id.is_empty())
}

/// #EXP-1730: the messages that live in a session (each already carries its
/// `session_id`), cloned in input order; a session-less row is left out.
pub fn session_messages<'a>(messages: impl IntoIterator<Item = &'a MessageRow>) -> Vec<MessageRow> {
    messages.into_iter().filter(|message| message_session(message).is_some()).cloned().collect()
}

/// conversation id → session id, for the conversations that belong to a
/// session (a dm/channel has no session and is left out).
pub type ConversationSessions = HashMap<String, String>;

/// conversation id → session id over the synced `conversations` rows.
pub fn conversation_sessions<'a>(
    conversations: impl IntoIterator<Item = &'a ConversationRow>,
) -> ConversationSessions {
    conversations
        .into_iter()
        .filter_map(|conversation| {
            let session_id = conversation.session_id.as_deref().filter(|id| !id.is_empty())?;
            Some((conversation.id.clone(), session_id.to_string()))
        })
        .collect()
}

/// The session `message` belongs to: its conversation's. `None` = a
/// dm/channel message, or one whose conversation has not synced yet.
pub fn session_of<'s>(message: &MessageRow, sessions: &'s ConversationSessions) -> Option<&'s str> {
    sessions.get(&message.conversation_id).map(String::as_str)
}

/// The messages whose conversation belongs to a session, each keyed to that
/// session (`session_id`), in input order. A dm/channel message, or one
/// whose conversation has not synced yet, is left out.
pub fn key_messages_by_session<'a>(
    messages: impl IntoIterator<Item = &'a MessageRow>,
    sessions: &ConversationSessions,
) -> Vec<MessageRow> {
    messages
        .into_iter()
        .filter_map(|message| {
            let session_id = session_of(message, sessions)?;
            Some(MessageRow { session_id: session_id.to_string(), ..message.clone() })
        })
        .collect()
}

/// The session's own conversation id (the kind `session` row naming it) —
/// what a locally written message lives in.
pub fn conversation_of_session<'a>(
    conversations: impl IntoIterator<Item = &'a ConversationRow>,
    session_id: &str,
) -> Option<&'a str> {
    conversations
        .into_iter()
        .find(|conversation| conversation.session_id.as_deref() == Some(session_id))
        .map(|conversation| conversation.id.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/message-conversation.json");

    fn conversations(value: &Value) -> Vec<ConversationRow> {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|row| {
                serde_json::from_value(json!({
                    "id": row["id"],
                    "kind": row["kind"],
                    "session_id": row["sessionId"],
                }))
                .unwrap()
            })
            .collect()
    }

    fn messages(value: &Value) -> Vec<MessageRow> {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|row| {
                serde_json::from_value(json!({
                    "id": row["id"],
                    "conversation_id": row["conversationId"],
                    // A legacy column the row may still carry: ignored.
                    "session_id": row["sessionId"],
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
            let conversations = conversations(case.get("conversations").unwrap_or(&fixture["conversations"]));
            let messages = messages(&case["messages"]);
            let keyed = key_messages_by_session(&messages, &conversation_sessions(&conversations));
            let actual: Vec<Value> =
                keyed.iter().map(|message| json!({"id": message.id, "sessionId": message.session_id})).collect();
            assert_eq!(Value::Array(actual), case["expected"], "{name}");
        }
    }

    /// #EXP-1730: the row's own `session_id` keys it; a session-less row
    /// (dm/channel, or an older server) belongs to no thread.
    #[test]
    fn a_message_is_keyed_by_its_own_session() {
        let rows: Vec<MessageRow> = [json!({"id": "m1", "session_id": "s1"}), json!({"id": "m2", "session_id": null}), json!({"id": "m3"})]
            .into_iter()
            .map(|row| serde_json::from_value(row).unwrap())
            .collect();
        assert_eq!(message_session(&rows[0]), Some("s1"));
        assert_eq!(message_session(&rows[1]), None);
        let keyed = session_messages(&rows);
        assert_eq!(keyed.len(), 1);
        assert_eq!(keyed[0].id, "m1");
    }

    #[test]
    fn a_session_finds_its_own_conversation() {
        let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
        let conversations = conversations(&fixture["conversations"]);
        assert_eq!(conversation_of_session(&conversations, "s-child"), Some("c-child"));
        assert_eq!(conversation_of_session(&conversations, "s-missing"), None);
    }
}
