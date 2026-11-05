//! #FEED-191 NEEDS YOU ×4 (`packages/domain-contract/src/needs-you.ts`'s twin,
//! iOS ExpCore `NeedsYou.swift`, Android `domain/NeedsYou.kt`): every plan and
//! question waiting on the viewer, plus (#EXP-1699) the PRs ready to merge.
//! Byte-locked by `fixtures/needs-you.json` (its `$comment` is the spec); pure.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ask_dock::{AskDockKind, ask_dock_label, ask_dock_title};
use crate::chats::{ChatPreviewKind, ask_preview};
use crate::plan_page::plan_row_label;
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

/// The neutral copy of a mirror's origin the viewer may not read.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct NeedsYouPrivate {
    pub plan: String,
    pub question: String,
}

/// `fixtures/needs-you.json` minus its cases.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NeedsYouSpec {
    pub route: String,
    pub copy: NeedsYouCopy,
    pub prefix: NeedsYouPrefix,
    pub test_ids: NeedsYouTestIds,
    pub private: NeedsYouPrivate,
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
    /// #EXP-1699: the session works now (a PR row waits on a person only when none does).
    #[serde(default)]
    pub busy: bool,
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

/// #EXP-1699: the pull requests shape's row, as far as the fold reads it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NeedsYouPullRequestInput {
    pub id: String,
    pub root_session_id: Option<String>,
    pub number: i64,
    pub title: String,
    pub state: String,
    pub checks_state: Option<String>,
    pub base_pull_request_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NeedsYouInput {
    pub now: i64,
    pub me: Option<String>,
    pub sessions: Vec<NeedsYouSessionInput>,
    pub messages: Vec<NeedsYouMessageInput>,
    #[serde(default)]
    pub pull_requests: Vec<NeedsYouPullRequestInput>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NeedsYouKind {
    Plan,
    Question,
    Pr,
}

/// `Plan` = the chat's Plan tab; `Card` = its card in the Conversation;
/// `Results` = its Results tab; `Chat` = an agent conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NeedsYouOpens {
    Plan,
    Card,
    Results,
    Chat,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NeedsYouRow {
    /// The waiting card's message (the origin's for a mirror it may read), or the PR's id.
    pub message_id: String,
    pub session_id: String,
    pub root_id: String,
    pub kind: NeedsYouKind,
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
    // Every session the viewer may read, ended ones included.
    readable: &HashMap<&str, &NeedsYouSessionInput>,
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
        // A digest's card is cut (no `decision`): `answeredAt` settles it.
        if answered_at.is_some() || card_state(card, answered_at) != Some(CardState::Waiting) {
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
            kind: if plan { NeedsYouKind::Plan } else { NeedsYouKind::Question },
            opens: if plan { NeedsYouOpens::Plan } else { NeedsYouOpens::Card },
            title,
            chat: root.name.clone(),
            meta: String::new(),
            age,
            created_at: message.created_at.clone(),
        });
    }
    if kind != Some("ask") || answered_at.is_some() {
        return None;
    }
    // A mirror: the origin's own ids where the viewer may read it, else the
    // mirror's, with the neutral copy (chats.json `askPreview`). A digest from
    // before it kept `messageId` opens the origin's chat, no card to reach.
    let origin = field("sessionId").or(message.author_session_id.as_deref());
    let origin_session = origin.and_then(|id| readable.get(id).copied());
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
    let opaque = preview.text == NEEDS_YOU.private.plan || preview.text == NEEDS_YOU.private.question;
    let target = if opaque { None } else { origin_session };
    let named = target.and(field("messageId"));
    let plan = preview.kind == ChatPreviewKind::Plan;
    let title = preview.text.strip_prefix(NEEDS_YOU.prefix.question.as_str()).unwrap_or(&preview.text).to_string();
    Some(NeedsYouRow {
        message_id: named.unwrap_or(&message.id).to_string(),
        session_id: target.map_or(&session.id, |target| &target.id).clone(),
        root_id: target.map_or(&root.id, |target| &target.root_id).clone(),
        kind: if plan { NeedsYouKind::Plan } else { NeedsYouKind::Question },
        opens: if plan && named.is_some() { NeedsYouOpens::Plan } else { NeedsYouOpens::Card },
        title,
        chat: target
            .and_then(|target| readable.get(target.root_id.as_str()))
            .map_or(&root.name, |origin_root| &origin_root.name)
            .clone(),
        meta: String::new(),
        age,
        created_at: message.created_at.clone(),
    })
}

/// #EXP-1699: the PRs ready to merge (`prCases`).
fn pull_request_rows(input: &NeedsYouInput) -> Vec<NeedsYouRow> {
    let sessions: HashMap<&str, &NeedsYouSessionInput> =
        input.sessions.iter().map(|session| (session.id.as_str(), session)).collect();
    let open: HashSet<&str> =
        input.pull_requests.iter().filter(|pr| pr.state == "open").map(|pr| pr.id.as_str()).collect();
    let working: HashSet<&str> =
        input.sessions.iter().filter(|session| session.busy).map(|session| session.root_id.as_str()).collect();
    input
        .pull_requests
        .iter()
        .filter_map(|pr| {
            let root = *sessions.get(pr.root_session_id.as_deref()?)?;
            if pr.state != "open" || pr.checks_state.as_deref() != Some("success") {
                return None;
            }
            if root.agent_id.is_some() && root.user_id != input.me {
                return None;
            }
            let stacked = pr.base_pull_request_id.as_deref().is_some_and(|base| open.contains(base));
            if working.contains(root.id.as_str()) || stacked {
                return None;
            }
            Some(NeedsYouRow {
                message_id: pr.id.clone(),
                session_id: root.id.clone(),
                root_id: root.id.clone(),
                kind: NeedsYouKind::Pr,
                opens: if root.agent_id.is_none() { NeedsYouOpens::Results } else { NeedsYouOpens::Chat },
                title: pr.title.clone(),
                chat: root.name.clone(),
                meta: format!("#{}", pr.number),
                age: result_age(ms(&pr.created_at), input.now),
                created_at: pr.created_at.clone(),
            })
        })
        .collect()
}

/// THE fold: one row per waiting card, once, plus (#EXP-1699) the PRs ready to
/// merge, newest first.
pub fn needs_you(input: &NeedsYouInput) -> Vec<NeedsYouRow> {
    // Another member's agent conversation is theirs (chats.json `listAgents`).
    let readable: HashMap<&str, &NeedsYouSessionInput> = input
        .sessions
        .iter()
        .filter(|session| session.agent_id.is_none() || session.user_id == input.me)
        .map(|session| (session.id.as_str(), session))
        .collect();
    let sessions: HashMap<&str, &NeedsYouSessionInput> =
        readable.iter().filter(|(_, session)| session.status != "ended").map(|(id, session)| (*id, *session)).collect();
    let visible: HashSet<&str> = readable.keys().copied().collect();
    let mut rows: HashMap<String, NeedsYouRow> = HashMap::new();
    let mut seen: HashSet<&str> = HashSet::new();
    let mut unnamed: Vec<NeedsYouRow> = Vec::new();
    for message in &input.messages {
        if !seen.insert(message.id.as_str()) {
            continue;
        }
        let Some(row) = row_of(message, &sessions, &readable, &visible, input.now) else { continue };
        if message.id == row.message_id && row.session_id != message.session_id {
            // An older digest's mirror naming only a session waits until every card is known.
            unnamed.push(row);
        } else if message.id == row.message_id || !rows.contains_key(&row.message_id) {
            // The card itself beats its mirror's copy of it.
            rows.insert(row.message_id.clone(), row);
        }
    }
    let mut waiting_in: HashSet<String> = rows.values().map(|row| row.session_id.clone()).collect();
    for row in unnamed {
        if waiting_in.insert(row.session_id.clone()) {
            rows.insert(row.message_id.clone(), row);
        }
    }
    let mut out: Vec<NeedsYouRow> = rows.into_values().collect();
    out.extend(pull_request_rows(input));
    out.sort_by(|a, b| {
        (a.kind == NeedsYouKind::Pr)
            .cmp(&(b.kind == NeedsYouKind::Pr))
            .then_with(|| ms(&b.created_at).cmp(&ms(&a.created_at)))
            .then_with(|| b.message_id.cmp(&a.message_id))
    });
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
        assert_eq!(NEEDS_YOU.private.plan, "Plan in a private chat");
        assert_eq!(NEEDS_YOU.private.question, "Question in a private chat");
    }

    #[test]
    fn needs_you_matches_the_fixture() {
        let fixture: Value = serde_json::from_str(NEEDS_YOU_JSON).unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        let pr_cases = fixture["prCases"].as_array().unwrap();
        assert!(!cases.is_empty() && !pr_cases.is_empty());
        for case in cases.iter().chain(pr_cases) {
            let input: NeedsYouInput = serde_json::from_value(case["input"].clone()).unwrap();
            let actual = serde_json::to_value(needs_you(&input)).unwrap();
            assert_eq!(actual, case["expected"], "{}", case["name"]);
        }
    }

    /// A digest's `ask` card keeps its origin's ids through the decode
    /// (chats.ts `digestMessage`), so the fold can name the origin's card.
    #[test]
    fn a_digest_keeps_an_asks_origin_ids() {
        let raw: Value = serde_json::from_str(
            r#"{"first": null, "preview": null, "waiting": {"id": "mirror", "authorKind": "agent",
                "authorSessionId": "c1", "body": "Asks: Ship it?",
                "card": {"kind": "ask", "sessionId": "c1", "messageId": "q2"},
                "answeredAt": null, "createdAt": "2027-01-15T07:55:00.000Z"},
                "agentAt": null, "agentTimes": []}"#,
        )
        .unwrap();
        let digest = crate::chats::ConversationDigest::decode(Some(&raw));
        let kept = crate::chats::digest_messages("r1", digest.as_ref());
        assert_eq!(kept.len(), 1);
        let card = kept[0].card.as_ref().unwrap();
        assert_eq!(card["sessionId"], "c1");
        assert_eq!(card["messageId"], "q2");
        let session = |id: &str| NeedsYouSessionInput {
            id: id.to_string(),
            root_id: "r1".to_string(),
            name: id.to_string(),
            status: "idle".to_string(),
            agent_id: None,
            user_id: Some("me".to_string()),
            busy: false,
        };
        let rows = needs_you(&NeedsYouInput {
            now: 1_800_000_000_000,
            me: Some("me".to_string()),
            sessions: vec![session("r1"), session("c1")],
            messages: vec![NeedsYouMessageInput {
                id: kept[0].id.clone(),
                session_id: kept[0].session_id.clone(),
                author_session_id: kept[0].author_session_id.clone(),
                body: kept[0].body.clone().unwrap_or_default(),
                created_at: kept[0].created_at.clone().unwrap_or_default(),
                answered_at: kept[0].answered_at.clone(),
                card: kept[0].card.clone(),
            }],
            pull_requests: Vec::new(),
        });
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].message_id, "q2");
        assert_eq!(rows[0].session_id, "c1");
    }
}
