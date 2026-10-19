//! THE ask dock ×4 (`packages/domain-contract/src/ask-dock.ts`'s twin, iOS
//! ExpCore `AskDock.swift`, Android `domain/AskDock.kt`): the question/plan
//! cards WAITING on the viewer take the composer's slot, in the session view
//! AND the thread alike. One pending card = the big card (the composer
//! hides); several = compact cards stacked in the composer's slot, a click
//! grows one into the big card (only that one answers), a back arrow
//! returns to the stack. The plan card's options are ONE list ×4
//! ([`PLAN_OPTIONS`]). Byte-locked by `fixtures/ask-dock.json`; pure.

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
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AskDockIcons {
    pub plan: String,
    pub question: String,
    pub back: String,
    pub open: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AskDockTestIds {
    pub dock: String,
    pub row: String,
    pub back: String,
    pub card: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AskDockMotion {
    pub resize: String,
    pub enter: String,
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
    pub test_ids: AskDockTestIds,
    pub motion: AskDockMotion,
    pub body_max_height_fraction: f32,
    pub stack_gap: f32,
    pub plan_options: Vec<PlanOption>,
}

/// The dock's copy, icons, geometry, motion and plan options.
pub static ASK_DOCK: LazyLock<AskDockSpec> =
    LazyLock::new(|| serde_json::from_str(ASK_DOCK_JSON).expect("ask-dock.json parses"));

/// THE plan card's options ×4: Yes · Yes, and start with a fresh context ·
/// No, keep planning (the last opens the feedback field).
pub fn plan_options() -> &'static [PlanOption] {
    &ASK_DOCK.plan_options
}

/// The big card's body cap, a fraction of the window height.
pub fn body_max_height_fraction() -> f32 {
    ASK_DOCK.body_max_height_fraction
}

/// The gap between two stacked compact cards (px).
pub fn stack_gap() -> f32 {
    ASK_DOCK.stack_gap
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
    /// Stack order, oldest first.
    pub created_at: i64,
}

/// A compact card of the stack.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AskDockRow {
    pub id: String,
    pub kind: AskDockKind,
    /// "Plan ready" / "Question".
    pub label: String,
    /// The first plain line of the text (the row truncates it).
    pub title: String,
    /// The asking session's name, else `None`.
    pub caption: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AskDockMode {
    None,
    Single,
    Stack,
    Focused,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AskDockView {
    pub mode: AskDockMode,
    /// Whether the composer yields its slot to the dock.
    pub composer_hidden: bool,
    /// The back arrow (only a focused card out of a stack).
    pub back: bool,
    /// The big card's id (`single` | `focused`).
    pub card_id: Option<String>,
    /// The compact cards (`stack` only), oldest first.
    pub rows: Vec<AskDockRow>,
}

/// The option a decided plan card names as its answer.
pub fn plan_decision_label(decision: &str, fresh: bool) -> String {
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

/// The dock's view over the cards waiting on the viewer; `focused_id` = the
/// card a click grew out of the stack (a stale one is ignored).
pub fn ask_dock(pending: &[AskDockPending], focused_id: Option<&str>) -> AskDockView {
    let mut pending: Vec<&AskDockPending> = pending.iter().collect();
    pending.sort_by(|a, b| a.created_at.cmp(&b.created_at).then_with(|| a.id.cmp(&b.id)));
    let view = |mode, card_id: Option<&str>, rows| AskDockView {
        mode,
        composer_hidden: mode != AskDockMode::None,
        back: mode == AskDockMode::Focused,
        card_id: card_id.map(str::to_string),
        rows,
    };
    match pending.as_slice() {
        [] => view(AskDockMode::None, None, Vec::new()),
        [only] => view(AskDockMode::Single, Some(&only.id), Vec::new()),
        _ => {
            if let Some(focused) = pending.iter().find(|p| Some(p.id.as_str()) == focused_id) {
                return view(AskDockMode::Focused, Some(&focused.id), Vec::new());
            }
            let rows = pending
                .iter()
                .map(|p| {
                    let label = ask_dock_label(p.kind).to_string();
                    let title = ask_dock_title(&p.text);
                    AskDockRow {
                        id: p.id.clone(),
                        kind: p.kind,
                        title: if title.is_empty() { label.clone() } else { title },
                        label,
                        caption: p.session_name.clone(),
                    }
                })
                .collect();
            view(AskDockMode::Stack, None, rows)
        }
    }
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
            let (kind, text, session_id) = match row {
                SessionThreadRow::Plan { decision: None, plan, session_id, .. } => {
                    (AskDockKind::Plan, plan, session_id)
                }
                SessionThreadRow::Question { answered: false, question, session_id, .. } => {
                    (AskDockKind::Question, question, session_id)
                }
                _ => return None,
            };
            Some(AskDockPending {
                id: row.id().to_string(),
                kind,
                text: text.clone(),
                session_name: session_name(session_id),
                created_at: index as i64,
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
        assert_eq!(stack_gap(), 6.);
        assert!((body_max_height_fraction() - 0.4).abs() < 1e-6);
    }

    #[test]
    fn ask_dock_matches_the_fixture() {
        let fixture = fixture();
        let cases = fixture["cases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let pending: Vec<AskDockPending> =
                serde_json::from_value(case["input"]["pending"].clone()).unwrap();
            let view = ask_dock(&pending, case["input"]["focusedId"].as_str());
            assert_eq!(serde_json::to_value(&view).unwrap(), case["expected"], "{}", case["name"]);
        }
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
