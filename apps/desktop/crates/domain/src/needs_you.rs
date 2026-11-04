//! #FEED-191 NEEDS YOU ×4 (`packages/domain-contract/src/needs-you.ts`'s twin,
//! iOS ExpCore `NeedsYou.swift`, Android `domain/NeedsYou.kt`): every plan and
//! question waiting on the viewer. Byte-locked by `fixtures/needs-you.json` (its
//! `$comment` is the spec); pure.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ask_dock::{AskDockKind, ask_dock_label, ask_dock_title};
use crate::chats::{ChatPreviewKind, ask_preview};
use crate::plan_page::{plan_counts, plan_counts_line, plan_row_label};
use crate::results_tab::result_age;
use crate::rows::MessageRow;
use crate::session_tree::iso_ms;
use crate::waiting_cards::{CardState, card_state};

const NEEDS_YOU_JSON: &str = include_str!("../../../../../packages/domain-contract/fixtures/needs-you.json");

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct NeedsYouCopy {
    pub entry: String,
    pub empty: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct NeedsYouPrefix {
    pub question: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct NeedsYouTestIds {
    pub entry: String,
    pub row: String,
    pub list: String,
}

/// `fixtures/needs-you.json` minus its cases.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NeedsYouSpec {
    pub route: String,
    pub copy: NeedsYouCopy,
    pub prefix: NeedsYouPrefix,
    pub test_ids: NeedsYouTestIds,
}

pub static NEEDS_YOU: LazyLock<NeedsYouSpec> =
    LazyLock::new(|| serde_json::from_str(NEEDS_YOU_JSON).expect("needs-you.json parses"));

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NeedsYouSessionInput {
    pub id: String,
    pub root_id: String,
    /// The session's display name (session-title; an agent conversation = the agent's).
    pub name: String,
    pub status: String,
    pub agent_id: Option<String>,
    pub user_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NeedsYouMessageInput {
    pub id: String,
    pub session_id: String,
    pub author_session_id: Option<String>,
    pub body: String,
    /// ISO timestamps, as the digests carry them.
    pub created_at: String,
    pub answered_at: Option<String>,
    pub card: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct NeedsYouInput {
    pub now: i64,
    pub me: Option<String>,
    pub sessions: Vec<NeedsYouSessionInput>,
    pub messages: Vec<NeedsYouMessageInput>,
}

/// `Plan` = the chat's Plan tab; `Card` = its card in the Conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NeedsYouOpens {
    Plan,
    Card,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NeedsYouRow {
    /// The waiting card's message: the origin's for a mirror it may read.
    pub message_id: String,
    pub session_id: String,
    pub root_id: String,
    pub kind: AskDockKind,
    pub opens: NeedsYouOpens,
    pub title: String,
    pub chat: String,
    pub meta: String,
    pub age: String,
    pub created_at: String,
}

fn ms(iso: &str) -> i64 {
    iso_ms(iso).unwrap_or(0)
}

fn row_of(
    message: &NeedsYouMessageInput,
    sessions: &HashMap<&str, &NeedsYouSessionInput>,
    visible: &HashSet<&str>,
    now: i64,
) -> Option<NeedsYouRow> {
    let card = message.card.as_ref()?;
    let session = *sessions.get(message.session_id.as_str())?;
    let root = *sessions.get(session.root_id.as_str())?;
    let age = result_age(ms(&message.created_at), now);
    let answered_at = message.answered_at.as_deref();
    let field = |key: &str| card.get(key).and_then(Value::as_str);
    let kind = field("kind");
    if matches!(kind, Some("plan" | "question")) {
        if card_state(card, answered_at) != Some(CardState::Waiting) {
            return None;
        }
        let plan = kind == Some("plan");
        let title = if plan {
            plan_row_label(field("plan").unwrap_or_default())
        } else {
            let title = ask_dock_title(field("question").unwrap_or_default());
            if title.is_empty() { ask_dock_label(AskDockKind::Question).to_string() } else { title }
        };
        return Some(NeedsYouRow {
            message_id: message.id.clone(),
            session_id: session.id.clone(),
            root_id: root.id.clone(),
            kind: if plan { AskDockKind::Plan } else { AskDockKind::Question },
            opens: if plan { NeedsYouOpens::Plan } else { NeedsYouOpens::Card },
            title,
            chat: root.name.clone(),
            meta: if plan { plan_counts_line(&plan_counts(card)) } else { String::new() },
            age,
            created_at: message.created_at.clone(),
        });
    }
    if kind != Some("ask") || answered_at.is_some() {
        return None;
    }
    // A mirror: the origin's own ids where the viewer may read it, else the
    // mirror's, with the neutral copy (chats.json `askPreview`).
    let origin = field("sessionId").or(message.author_session_id.as_deref());
    let preview = ask_preview(
        &MessageRow {
            id: message.id.clone(),
            session_id: message.session_id.clone(),
            author_session_id: message.author_session_id.clone(),
            body: Some(message.body.clone()),
            card: Some(card.clone()),
            ..MessageRow::default()
        },
        Some(visible),
    );
    let plan = preview.kind == ChatPreviewKind::Plan;
    let title = preview.text.strip_prefix(NEEDS_YOU.prefix.question.as_str()).unwrap_or(&preview.text).to_string();
    let kind = if plan { AskDockKind::Plan } else { AskDockKind::Question };
    let readable = origin.and_then(|id| sessions.get(id).copied()).zip(field("messageId"));
    let (message_id, session_id, root_id, chat, opens) = match readable {
        Some((origin, message_id)) => (
            message_id,
            origin.id.as_str(),
            origin.root_id.as_str(),
            sessions.get(origin.root_id.as_str()).unwrap_or(&root).name.as_str(),
            if plan { NeedsYouOpens::Plan } else { NeedsYouOpens::Card },
        ),
        None => (message.id.as_str(), session.id.as_str(), root.id.as_str(), root.name.as_str(), NeedsYouOpens::Card),
    };
    Some(NeedsYouRow {
        message_id: message_id.to_string(),
        session_id: session_id.to_string(),
        root_id: root_id.to_string(),
        kind,
        opens,
        title,
        chat: chat.to_string(),
        meta: String::new(),
        age,
        created_at: message.created_at.clone(),
    })
}

/// THE fold: one row per waiting card, once, newest first.
pub fn needs_you(input: &NeedsYouInput) -> Vec<NeedsYouRow> {
    // Another member's agent conversation is theirs (chats.json `listAgents`).
    let sessions: HashMap<&str, &NeedsYouSessionInput> = input
        .sessions
        .iter()
        .filter(|session| session.status != "ended" && (session.agent_id.is_none() || session.user_id == input.me))
        .map(|session| (session.id.as_str(), session))
        .collect();
    let visible: HashSet<&str> = sessions.keys().copied().collect();
    let mut rows: HashMap<String, NeedsYouRow> = HashMap::new();
    let mut seen: HashSet<&str> = HashSet::new();
    for message in &input.messages {
        if !seen.insert(message.id.as_str()) {
            continue;
        }
        let Some(row) = row_of(message, &sessions, &visible, input.now) else { continue };
        // The card itself beats its mirror's copy of it.
        if message.id == row.message_id || !rows.contains_key(&row.message_id) {
            rows.insert(row.message_id.clone(), row);
        }
    }
    let mut out: Vec<NeedsYouRow> = rows.into_values().collect();
    out.sort_by(|a, b| ms(&b.created_at).cmp(&ms(&a.created_at)).then_with(|| b.message_id.cmp(&a.message_id)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needs_you_spec_parses() {
        assert_eq!(NEEDS_YOU.route, "/t/$teamSlug/needs-you");
        assert_eq!(NEEDS_YOU.copy.entry, "Needs you");
        assert_eq!(NEEDS_YOU.copy.empty, "Nothing needs you");
        assert_eq!(NEEDS_YOU.prefix.question, "Question: ");
        assert_eq!(NEEDS_YOU.test_ids.row, "needs-you-row");
    }

    #[test]
    fn needs_you_matches_the_fixture() {
        let fixture: Value = serde_json::from_str(NEEDS_YOU_JSON).unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let input: NeedsYouInput = serde_json::from_value(case["input"].clone()).unwrap();
            let actual = serde_json::to_value(needs_you(&input)).unwrap();
            assert_eq!(actual, case["expected"], "{}", case["name"]);
        }
    }
}
