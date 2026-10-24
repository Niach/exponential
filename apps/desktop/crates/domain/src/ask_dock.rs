//! THE ask cards ×4 (`packages/domain-contract/src/ask-dock.ts`'s twin, iOS
//! ExpCore `AskDock.swift`, Android `domain/AskDock.kt`): a question or plan
//! card waiting on the viewer is a DockCard of THE Dock ([`crate::dock`]);
//! the composer never leaves. This file keeps the cards' own words: the
//! row's label and title, the plan options ONE list ×4 ([`plan_options`]), a
//! thread's waiting cards. Byte-locked by `fixtures/ask-dock.json`; pure.

use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use std::collections::HashMap;

use crate::session_tree::{SessionThreadRow, ThreadTab};

const ASK_DOCK_JSON: &str = include_str!("../../../../../packages/domain-contract/fixtures/ask-dock.json");

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AskDockCopy {
    pub plan_ready: String,
    pub question: String,
    pub back: String,
    pub feedback_placeholder: String,
    pub cancel: String,
    /// FEED-140: a plan the agent replaced with a newer one.
    pub superseded: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AskDockIcons {
    pub plan: String,
    pub question: String,
}

/// One of THE plan card's options (`planOptions`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PlanOption {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub description: Option<String>,
    /// `approved` | `rejected`.
    pub decision: String,
    pub fresh: bool,
}

/// `fixtures/ask-dock.json` minus its cases.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AskDockSpec {
    pub copy: AskDockCopy,
    pub icons: AskDockIcons,
    pub plan_options: Vec<PlanOption>,
}

/// The cards' copy, icons and plan options.
pub static ASK_DOCK: LazyLock<AskDockSpec> =
    LazyLock::new(|| serde_json::from_str(ASK_DOCK_JSON).expect("ask-dock.json parses"));

/// THE plan card's options ×4: Yes · Yes, and start with a fresh context ·
/// No, keep planning (the last opens the feedback field).
pub fn plan_options() -> &'static [PlanOption] {
    &ASK_DOCK.plan_options
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AskDockKind {
    Plan,
    Question,
}

/// A card waiting on the viewer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AskDockPending {
    /// The card's id (a `messages.id`, or the session view's ask id).
    pub id: String,
    pub kind: AskDockKind,
    /// The plan, or the question's text (markdown).
    pub text: String,
    /// The asking session's name in a thread; `None` in the session view.
    pub session_name: Option<String>,
    /// Dock order, oldest first.
    pub created_at: i64,
    /// A plan's question count (a question: absent).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub questions: Option<usize>,
}

/// The option a decided plan card names as its answer.
pub fn plan_decision_label(decision: &str, fresh: bool) -> String {
    // FEED-140: a plan the agent replaced with a newer one.
    if decision == "superseded" {
        return ASK_DOCK.copy.superseded.clone();
    }
    let wants_fresh = decision == "approved" && fresh;
    plan_options()
        .iter()
        .find(|option| option.decision == decision && option.fresh == wants_fresh)
        .map(|option| option.label.clone())
        .unwrap_or_default()
}

/// The header label of a card of `kind`.
pub fn ask_dock_label(kind: AskDockKind) -> &'static str {
    match kind {
        AskDockKind::Plan => &ASK_DOCK.copy.plan_ready,
        AskDockKind::Question => &ASK_DOCK.copy.question,
    }
}

static BLOCK_MARKER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(#{1,6}\s+|>\s?|[-*+]\s+|\d+[.)]\s+|\[[ xX]\]\s+)").unwrap());
static LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[([^\]]*)\]\([^)]*\)").unwrap());

/// The first plain line of a markdown body: fenced code blocks skipped whole,
/// block markers (heading, quote, list, task box) stripped, links reduced to
/// their text, `**` / `__` / backticks dropped.
pub fn ask_dock_title(text: &str) -> String {
    let mut fenced = false;
    for raw in text.split('\n') {
        let mut line = raw.trim().to_string();
        if line.starts_with("```") || line.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        loop {
            let stripped = BLOCK_MARKER.replace(&line, "").into_owned();
            if stripped == line {
                break;
            }
            line = stripped;
        }
        let line = LINK
            .replace_all(&line, "$1")
            .replace("**", "")
            .replace("__", "")
            .replace('`', "");
        let line = line.trim();
        if !line.is_empty() {
            return line.to_string();
        }
    }
    String::new()
}

/// A thread's waiting cards, in Conversation order: every plan row with no
/// decision and every unanswered question row, of ANY session in the tree.
/// `session_name` names the asking session (`None` = unnamed).
pub fn thread_pending_asks(
    rows: &[SessionThreadRow],
    session_name: impl Fn(&str) -> Option<String>,
) -> Vec<AskDockPending> {
    rows.iter()
        .enumerate()
        .filter_map(|(index, row)| {
            let (kind, text, session_id, questions) = match row {
                SessionThreadRow::Plan { decision: None, plan, session_id, questions, .. } => {
                    (AskDockKind::Plan, plan, session_id, Some(questions.len()))
                }
                SessionThreadRow::Question { answered: false, question, session_id, .. } => {
                    (AskDockKind::Question, question, session_id, None)
                }
                _ => return None,
            };
            Some(AskDockPending {
                id: row.id().to_string(),
                kind,
                text: text.clone(),
                session_name: session_name(session_id),
                created_at: index as i64,
                questions,
            })
        })
        .collect()
}

/// A thread's card captions: each session TAB's name (nested ones
/// included); the root (no tab) = `None`.
pub fn thread_session_names(tabs: &[ThreadTab]) -> impl Fn(&str) -> Option<String> {
    let mut names: HashMap<String, String> = HashMap::new();
    for tab in tabs {
        if let ThreadTab::Session { session_id, name, nested, .. } = tab {
            names.insert(session_id.clone(), name.clone());
            for item in nested {
                names.insert(item.session_id.clone(), item.name.clone());
            }
        }
    }
    move |session_id: &str| names.get(session_id).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_tree::{SessionTabMarkState, ThreadTabNestedSession};
    use serde_json::Value;

    fn fixture() -> Value {
        serde_json::from_str(ASK_DOCK_JSON).unwrap()
    }

    #[test]
    fn spec_parses() {
        assert_eq!(ASK_DOCK.copy.plan_ready, "Plan ready");
        assert_eq!(plan_options().len(), 3);
        assert_eq!(ASK_DOCK.copy.cancel, "Cancel");
    }

    #[test]
    fn title_matches_the_fixture() {
        let fixture = fixture();
        let cases = fixture["titleCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            assert_eq!(ask_dock_title(case["text"].as_str().unwrap()), case["expected"].as_str().unwrap());
        }
    }

    #[test]
    fn decision_label_matches_the_fixture() {
        let fixture = fixture();
        let cases = fixture["decisionLabelCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            assert_eq!(
                plan_decision_label(case["decision"].as_str().unwrap(), case["fresh"].as_bool().unwrap()),
                case["expected"].as_str().unwrap()
            );
        }
    }

    /// The fixture's rows carry only what the pick reads; every other row
    /// type becomes a placeholder event.
    fn thread_row(row: &Value) -> SessionThreadRow {
        let s = |key: &str| row[key].as_str().unwrap_or_default().to_string();
        match row["type"].as_str() {
            Some("plan") => SessionThreadRow::Plan {
                id: s("id"),
                session_id: s("sessionId"),
                plan: s("plan"),
                questions: Vec::new(),
                surface: None,
                decision: row["decision"].as_str().map(str::to_string),
                fresh: row["fresh"].as_bool().unwrap_or(false),
                answers: None,
                feedback: None,
                surface_data: None,
            },
            Some("question") => SessionThreadRow::Question {
                id: s("id"),
                session_id: s("sessionId"),
                question: s("question"),
                options: Vec::new(),
                custom: true,
                multi: false,
                surface: None,
                answer: row["answer"].as_str().map(str::to_string),
                answers: None,
                surface_data: None,
                answered: row["answered"].as_bool().unwrap(),
            },
            _ => SessionThreadRow::Event { id: s("id"), event: String::new(), text: String::new() },
        }
    }

    #[test]
    fn thread_pending_asks_match_the_fixture() {
        let fixture = fixture();
        let cases = fixture["threadCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let rows: Vec<SessionThreadRow> = case["rows"].as_array().unwrap().iter().map(thread_row).collect();
            let names: HashMap<String, String> = serde_json::from_value(case["sessionNames"].clone()).unwrap();
            let pending = thread_pending_asks(&rows, |id| names.get(id).cloned());
            assert_eq!(serde_json::to_value(&pending).unwrap(), case["expected"], "{}", case["name"]);
        }
    }

    /// The fixture's tabs carry only what the lookup reads (`markState`
    /// is not read).
    fn tab(tab: &Value) -> ThreadTab {
        let s = |value: &Value, key: &str| value[key].as_str().unwrap_or_default().to_string();
        match tab["kind"].as_str() {
            Some("session") => ThreadTab::Session {
                session_id: s(tab, "sessionId"),
                name: s(tab, "name"),
                mark_state: SessionTabMarkState::Waiting,
                nested: tab["nested"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|item| ThreadTabNestedSession {
                        session_id: s(item, "sessionId"),
                        name: s(item, "name"),
                        mark_state: SessionTabMarkState::Waiting,
                        depth: item["depth"].as_u64().unwrap() as usize,
                    })
                    .collect(),
            },
            Some("guide") => ThreadTab::Guide { dot: tab["dot"].as_bool().unwrap_or(false) },
            Some("add") => ThreadTab::Add,
            _ => ThreadTab::Conversation,
        }
    }

    #[test]
    fn thread_session_names_match_the_fixture() {
        let fixture = fixture();
        let cases = fixture["sessionNameCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let tabs: Vec<ThreadTab> = case["tabs"].as_array().unwrap().iter().map(tab).collect();
            let name = thread_session_names(&tabs);
            for (id, expected) in case["lookups"].as_object().unwrap() {
                assert_eq!(name(id).as_deref(), expected.as_str(), "{} {id}", case["name"]);
            }
        }
    }
}
