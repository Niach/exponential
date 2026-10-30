//! #EXP-1464: what a conversation's SESSION rows do when tapped ×4 (a
//! thread's Conversation and a team agent's conversation alike). A session
//! card whose session waits on a question or plan EXPANDS in place into that
//! card; every other row naming a session (a session card, a `Report from`
//! line, an event line such as `… was not picked up`) OPENS it by
//! inbox-route.json `session_target` ([`crate::inbox_route`]). Byte-locked by
//! `packages/domain-contract/fixtures/thread-row-action.json`; mirrors TS
//! `src/thread-row-action.ts`, iOS `ThreadRowAction.swift`, Android
//! `ThreadRowAction.kt`.

use std::sync::LazyLock;

use serde::Deserialize;

use crate::session_tree::{SessionState, SessionThreadRow};

const THREAD_ROW_ACTION_JSON: &str =
    include_str!("../../../../../packages/domain-contract/fixtures/thread-row-action.json");

/// The fixture's `copy`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadRowActionCopy {
    /// The expanded card's `Open chat` link.
    pub open_chat: String,
    /// The answered row's quiet state word.
    pub answered: String,
}

/// The fixture's `testIds` (element-id prefixes).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadRowActionTestIds {
    /// The expanded card's `Open chat` link.
    pub open_chat: String,
    /// The expanded row (the card + its question / plan card).
    pub expanded: String,
    /// An event line that opens its session.
    pub event: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fixture {
    copy: ThreadRowActionCopy,
    test_ids: ThreadRowActionTestIds,
}

static FIXTURE: LazyLock<Fixture> =
    LazyLock::new(|| serde_json::from_str(THREAD_ROW_ACTION_JSON).expect("thread-row-action.json parses"));

/// `copy` of thread-row-action.json.
pub fn copy() -> &'static ThreadRowActionCopy {
    &FIXTURE.copy
}

/// `testIds` of thread-row-action.json.
pub fn test_ids() -> &'static ThreadRowActionTestIds {
    &FIXTURE.test_ids
}

/// What a tap on the row does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowAction {
    /// The session card unfolds its question / plan card in place.
    Expand,
    /// The session opens (`session_target`).
    Open,
    /// Nothing: no chevron, no hover, no press.
    None,
}

impl RowAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Expand => "expand",
            Self::Open => "open",
            Self::None => "none",
        }
    }
}

/// The row's action (TS `ThreadRowAction`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadRowAction {
    pub action: RowAction,
    pub session_id: Option<String>,
    /// Only a row that does something carries the chevron (and hover/press).
    pub chevron: bool,
    /// Overrides the card's state word (`copy.answered`), `None` = its own.
    pub kind_word: Option<String>,
    /// Overrides the card's tone with the word, `None` = its own.
    pub tone: Option<&'static str>,
}

impl ThreadRowAction {
    const NONE: Self = Self {
        action: RowAction::None,
        session_id: None,
        chevron: false,
        kind_word: None,
        tone: None,
    };
}

/// The action of a Conversation row of type `kind` (`session`, `report`,
/// `event`, …) naming `session_id` in the conversation rooted at `root_id`.
/// `state` = that session's card state, `blocked` = a flag stands behind its
/// `needs_you`, `inline` = its waiting card already is a row of this
/// conversation ([`waiting_card_inline`]), `answered` = the viewer answered
/// its card here (client-held).
pub fn thread_row_action(
    kind: &str,
    session_id: Option<&str>,
    root_id: &str,
    state: Option<SessionState>,
    blocked: bool,
    inline: bool,
    answered: bool,
) -> ThreadRowAction {
    let Some(session_id) = session_id.filter(|id| !id.is_empty() && *id != root_id) else {
        return ThreadRowAction::NONE;
    };
    if !matches!(kind, "session" | "report" | "event") {
        return ThreadRowAction::NONE;
    }
    if kind == "session" && state == Some(SessionState::NeedsYou) && !blocked && !inline {
        return ThreadRowAction {
            action: RowAction::Expand,
            session_id: Some(session_id.to_string()),
            chevron: true,
            kind_word: None,
            tone: None,
        };
    }
    let answered = kind == "session"
        && answered
        && matches!(state, Some(SessionState::Working | SessionState::Idle));
    ThreadRowAction {
        action: RowAction::Open,
        session_id: Some(session_id.to_string()),
        chevron: true,
        kind_word: answered.then(|| copy().answered.clone()),
        tone: answered.then_some("muted"),
    }
}

/// Whether `session_id`'s waiting card already is a row of `rows` (an
/// unanswered question or an undecided plan of that session: a thread
/// child's own card, an escalated ask); such a session row OPENS.
pub fn waiting_card_inline(rows: &[SessionThreadRow], session_id: &str) -> bool {
    rows.iter().any(|row| match row {
        SessionThreadRow::Question { session_id: id, answered, .. } => id == session_id && !answered,
        SessionThreadRow::Plan { session_id: id, decision, .. } => id == session_id && decision.is_none(),
        _ => false,
    })
}

/// One message as [`session_waiting_card`] reads it.
#[derive(Clone, Copy, Debug)]
pub struct WaitingMessage<'a> {
    pub id: &'a str,
    pub session_id: &'a str,
    pub created_at: &'a str,
    pub answered_at: Option<&'a str>,
    /// The message's `card` JSON, if any.
    pub card: Option<&'a serde_json::Value>,
}

/// The card an expanding row opens: the session's NEWEST (createdAt, ties by
/// the higher id) waiting `question` (`answered_at` none) or `plan` (no
/// decision; `superseded` is a decision) message id, else `None`.
pub fn session_waiting_card<'a>(
    session_id: &str,
    messages: impl IntoIterator<Item = WaitingMessage<'a>>,
) -> Option<&'a str> {
    let mut best: Option<(i64, &'a str)> = None;
    for message in messages {
        if message.session_id != session_id {
            continue;
        }
        let Some(card) = message.card.filter(|card| card.is_object()) else {
            continue;
        };
        let waiting = match card.get("kind").and_then(|kind| kind.as_str()) {
            Some("question") => message.answered_at.is_none(),
            Some("plan") => card.get("decision").is_none_or(|decision| decision.is_null()),
            _ => false,
        };
        if !waiting {
            continue;
        }
        let at = crate::instant::parse_instant(message.created_at).unwrap_or(i64::MIN);
        if best.is_none_or(|(best_at, best_id)| at > best_at || (at == best_at && message.id > best_id)) {
            best = Some((at, message.id));
        }
    }
    best.map(|(_, id)| id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn fixture() -> Value {
        serde_json::from_str(THREAD_ROW_ACTION_JSON).unwrap()
    }

    fn state(word: Option<&str>) -> Option<SessionState> {
        word.map(|word| match word {
            "working" => SessionState::Working,
            "needs_you" => SessionState::NeedsYou,
            "idle" => SessionState::Idle,
            "done" => SessionState::Done,
            other => panic!("unknown state {other}"),
        })
    }

    fn to_json(action: &ThreadRowAction) -> Value {
        serde_json::json!({
            "action": action.action.as_str(),
            "sessionId": action.session_id,
            "chevron": action.chevron,
            "kindWord": action.kind_word,
            "tone": action.tone,
        })
    }

    #[test]
    fn every_case_matches() {
        let fixture = fixture();
        let cases = fixture["cases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let input = &case["input"];
            let got = thread_row_action(
                input["type"].as_str().unwrap(),
                input["sessionId"].as_str(),
                input["rootId"].as_str().unwrap(),
                state(input["state"].as_str()),
                input["blocked"].as_bool().unwrap(),
                input["inline"].as_bool().unwrap(),
                input["answered"].as_bool().unwrap(),
            );
            assert_eq!(to_json(&got), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn every_waiting_case_matches() {
        let fixture = fixture();
        let cases = fixture["waitingCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let messages = case["messages"].as_array().unwrap();
            let got = session_waiting_card(
                case["sessionId"].as_str().unwrap(),
                messages.iter().map(|message| WaitingMessage {
                    id: message["id"].as_str().unwrap(),
                    session_id: message["sessionId"].as_str().unwrap(),
                    created_at: message["createdAt"].as_str().unwrap(),
                    answered_at: message["answeredAt"].as_str(),
                    card: message.get("card").filter(|card| !card.is_null()),
                }),
            );
            assert_eq!(got.map(Value::from).unwrap_or(Value::Null), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn copy_reads_the_fixture() {
        assert_eq!(copy().open_chat, "Open chat");
        assert_eq!(copy().answered, "Answered");
        assert_eq!(test_ids().open_chat, "thread-open-chat");
    }
}
