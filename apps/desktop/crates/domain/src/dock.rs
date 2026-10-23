//! N-12: THE Dock ×4 (`packages/domain-contract/src/dock.ts`'s twin, iOS
//! ExpCore `Dock.swift`, Android `domain/Dock.kt`): the floating band over
//! the transcript's bottom edge holding the DockCards (a waiting plan or
//! question, THE Queue, the queued messages, a status banner) above THE
//! composer, which never leaves. Several cards pile like the toasts
//! ([`crate::stack::stack_layout`] with [`DOCK`]'s `stack`); a click opens a
//! card's bottom sheet. Byte-locked by `fixtures/dock.json`; pure.

use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

use crate::ask_dock::{ask_dock_label, ask_dock_title, AskDockKind};
use crate::stack::StackConstants;

const DOCK_JSON: &str = include_str!("../../../../../packages/domain-contract/fixtures/dock.json");

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Counted {
    pub one: String,
    pub many: String,
}

impl Counted {
    pub fn format(&self, n: usize) -> String {
        if n == 1 {
            self.one.clone()
        } else {
            self.many.replace("{count}", &n.to_string())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockCopy {
    pub queued: Counted,
    pub questions: Counted,
    pub close: String,
    pub show_all: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DockIcons {
    pub plan: String,
    pub question: String,
    pub tasks: String,
    pub queued: String,
    pub open: String,
    pub close: String,
    pub suggestions: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockTestIds {
    pub dock: String,
    pub pile: String,
    pub card: String,
    pub sheet: String,
    pub sheet_body: String,
    pub sheet_footer: String,
    pub close: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockSheetSpec {
    pub max_height_fraction: f32,
    pub radius: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DockMotion {
    pub pile: String,
    pub enter: String,
    pub exit: String,
    pub sheet: String,
}

/// `fixtures/dock.json` minus its cases.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockSpec {
    pub copy: DockCopy,
    pub icons: DockIcons,
    pub test_ids: DockTestIds,
    pub stack: StackConstants,
    pub sheet: DockSheetSpec,
    pub motion: DockMotion,
    pub order: Vec<String>,
}

/// The Dock's copy, icons, test ids, pile, sheet and motion.
pub static DOCK: LazyLock<DockSpec> = LazyLock::new(|| serde_json::from_str(DOCK_JSON).expect("dock.json parses"));

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DockKind {
    Plan,
    Question,
    Tasks,
    Queued,
    Suggestions,
    Banner,
    /// VAPP-129: the app editor's floating chat card (apps.json
    /// `dockCard`; built by the editor, never by [`dock_rows`]).
    Chat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DockTint {
    Plan,
    Question,
    Muted,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DockQueueItem {
    pub label: String,
    pub description: Option<String>,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DockQueueTodo {
    pub title: String,
    pub items: Vec<DockQueueItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DockQueueCompleted {
    pub title: String,
}

/// THE Queue's collapsed facts (task-queue.json `taskQueue`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DockQueue {
    pub todo: DockQueueTodo,
    pub completed: Option<DockQueueCompleted>,
}

/// One item of a suggestions row as [`suggestion_sources`] reads it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DockSuggestionItem {
    /// `open` | `started` | `here` | `filed`.
    pub state: String,
    pub title: String,
}

/// A suggestions row as [`suggestion_sources`] reads it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DockSuggestionsRow {
    pub id: String,
    pub header: String,
    pub items: Vec<DockSuggestionItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DockQueuedMessage {
    pub id: String,
    pub text: String,
}

/// What a host hands the Dock.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DockSource {
    #[serde(rename_all = "camelCase")]
    Plan {
        id: String,
        /// The plan (markdown).
        text: String,
        /// The asking session's name in a thread; `None` in the session view.
        session_name: Option<String>,
        /// How many questions the card asks.
        questions: usize,
        /// The asks' order, oldest first.
        created_at: i64,
    },
    #[serde(rename_all = "camelCase")]
    Question { id: String, text: String, session_name: Option<String>, created_at: i64 },
    Tasks { queue: Option<DockQueue> },
    Queued { messages: Vec<DockQueuedMessage> },
    /// EXP-1323's suggested-tasks card while ANY item is open
    /// ([`suggestion_sources`]).
    Suggestions {
        /// The suggestions row's id (a `messages.id`).
        id: String,
        /// The header's count (`3 suggested tasks`).
        label: String,
        /// The header's `started` part.
        caption: Option<String>,
        /// The first open item's title.
        title: String,
        /// How many items are still open.
        open: usize,
    },
    Banner {
        id: String,
        /// Concept icon.
        icon: String,
        label: String,
        text: String,
        /// `muted` | `warning`.
        tone: DockTint,
        /// The banner's one button word.
        action: Option<String>,
    },
}

/// One DockCard.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DockRow {
    pub id: String,
    pub kind: DockKind,
    pub tint: DockTint,
    /// Concept icon.
    pub icon: String,
    pub label: String,
    pub caption: Option<String>,
    pub meta: Option<String>,
    pub title: String,
    /// A click opens the sheet.
    pub expandable: bool,
    /// A banner's button word.
    pub action: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DockMode {
    None,
    Single,
    Pile,
    Fanned,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockView {
    pub mode: DockMode,
    /// Top to bottom; the front card is the LAST.
    pub rows: Vec<DockRow>,
    pub front_id: Option<String>,
    /// The open sheet's card, when that card exists and is expandable.
    pub open_id: Option<String>,
    /// How many piled cards stay out of sight (beyond `stack.visible`).
    pub hidden: usize,
}

fn order(kind: DockKind) -> usize {
    let key = match kind {
        DockKind::Queued => "queued",
        DockKind::Tasks => "tasks",
        DockKind::Suggestions => "suggestions",
        DockKind::Banner => "banner",
        DockKind::Plan | DockKind::Question => "ask",
        DockKind::Chat => return DOCK.order.len(),
    };
    DOCK.order.iter().position(|k| k == key).unwrap_or(usize::MAX)
}

/// The cards, in the Dock's order (queued, tasks, suggestions, banners, then the asks
/// oldest first, ties by id). An empty queue or no queued message draws
/// nothing.
pub fn dock_rows(sources: &[DockSource]) -> Vec<DockRow> {
    let mut rows: Vec<(DockRow, i64)> = Vec::new();
    for source in sources {
        match source {
            DockSource::Queued { messages } => {
                let Some(first) = messages.first() else { continue };
                rows.push((
                    DockRow {
                        id: "queued".into(),
                        kind: DockKind::Queued,
                        tint: DockTint::Muted,
                        icon: DOCK.icons.queued.clone(),
                        label: DOCK.copy.queued.format(messages.len()),
                        caption: None,
                        meta: None,
                        title: first.text.clone(),
                        expandable: true,
                        action: None,
                    },
                    0,
                ));
            }
            DockSource::Tasks { queue } => {
                let Some(queue) = queue else { continue };
                let Some(first) = queue.todo.items.first() else { continue };
                rows.push((
                    DockRow {
                        id: "tasks".into(),
                        kind: DockKind::Tasks,
                        tint: DockTint::Muted,
                        icon: DOCK.icons.tasks.clone(),
                        label: queue.todo.title.clone(),
                        caption: queue.completed.as_ref().map(|c| c.title.clone()),
                        meta: first.description.clone(),
                        title: first.label.clone(),
                        expandable: true,
                        action: None,
                    },
                    0,
                ));
            }
            DockSource::Suggestions { id, label, caption, title, .. } => rows.push((
                DockRow {
                    id: id.clone(),
                    kind: DockKind::Suggestions,
                    tint: DockTint::Muted,
                    icon: DOCK.icons.suggestions.clone(),
                    label: label.clone(),
                    caption: caption.clone(),
                    meta: None,
                    title: title.clone(),
                    expandable: true,
                    action: None,
                },
                0,
            )),
            DockSource::Banner { id, icon, label, text, tone, action } => rows.push((
                DockRow {
                    id: format!("banner:{id}"),
                    kind: DockKind::Banner,
                    tint: *tone,
                    icon: icon.clone(),
                    label: label.clone(),
                    caption: None,
                    meta: None,
                    title: text.clone(),
                    expandable: false,
                    action: action.clone(),
                },
                0,
            )),
            DockSource::Plan { id, text, session_name, questions, created_at } => {
                rows.push((ask_row(AskDockKind::Plan, id, text, session_name, *questions), *created_at))
            }
            DockSource::Question { id, text, session_name, created_at } => {
                rows.push((ask_row(AskDockKind::Question, id, text, session_name, 0), *created_at))
            }
        }
    }
    rows.sort_by(|(a, at), (b, bt)| {
        order(a.kind).cmp(&order(b.kind)).then(at.cmp(bt)).then_with(|| a.id.cmp(&b.id))
    });
    rows.into_iter().map(|(row, _)| row).collect()
}

fn ask_row(kind: AskDockKind, id: &str, text: &str, session_name: &Option<String>, questions: usize) -> DockRow {
    let label = ask_dock_label(kind).to_string();
    let title = ask_dock_title(text);
    let plan = kind == AskDockKind::Plan;
    DockRow {
        id: id.to_string(),
        kind: if plan { DockKind::Plan } else { DockKind::Question },
        tint: if plan { DockTint::Plan } else { DockTint::Question },
        icon: if plan { DOCK.icons.plan.clone() } else { DOCK.icons.question.clone() },
        caption: session_name.clone(),
        meta: (plan && questions > 0).then(|| DOCK.copy.questions.format(questions)),
        title: if title.is_empty() { label.clone() } else { title },
        label,
        expandable: true,
        action: None,
    }
}

/// The band: the pile (or the fan while `expanded`), the front card, the
/// open sheet.
pub fn dock(rows: &[DockRow], expanded: bool, open_id: Option<&str>) -> DockView {
    let mode = match rows.len() {
        0 => DockMode::None,
        1 => DockMode::Single,
        _ if expanded => DockMode::Fanned,
        _ => DockMode::Pile,
    };
    let open = open_id.and_then(|id| rows.iter().find(|row| row.id == id));
    DockView {
        mode,
        rows: rows.to_vec(),
        front_id: rows.last().map(|row| row.id.clone()),
        open_id: open.filter(|row| row.expandable).map(|row| row.id.clone()),
        hidden: if mode == DockMode::Pile { rows.len().saturating_sub(DOCK.stack.visible) } else { 0 },
    }
}

/// The ask (plan or question) that just arrived, front-most first, else
/// `None`. `previous` = every card id seen before this frame.
pub fn dock_auto_open<'a>(previous: &[String], rows: impl DoubleEndedIterator<Item = (&'a str, DockKind)>) -> Option<String> {
    rows.rev()
        .find(|(id, kind)| {
            matches!(kind, DockKind::Plan | DockKind::Question) && !previous.iter().any(|seen| seen == id)
        })
        .map(|(id, _)| id.to_string())
}

/// EXP-1323's suggested-tasks rows with any OPEN item, as Dock sources: the
/// header splits at ` · ` into the label (the count) and the started
/// caption; the title is the first open item's. A row with no open item is
/// none of the Dock's (it stays in the Conversation).
pub fn suggestion_sources<'a>(rows: impl IntoIterator<Item = &'a DockSuggestionsRow>) -> Vec<DockSource> {
    rows.into_iter()
        .filter_map(|row| {
            let mut open = row.items.iter().filter(|item| item.state == "open");
            let first = open.next()?;
            let mut parts = row.header.split(" · ");
            let label = parts.next().unwrap_or(&row.header).to_string();
            Some(DockSource::Suggestions {
                id: row.id.clone(),
                label,
                caption: parts.next().map(str::to_string),
                title: first.title.clone(),
                open: 1 + open.count(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn fixture() -> Value {
        serde_json::from_str(DOCK_JSON).unwrap()
    }

    #[test]
    fn spec_parses() {
        assert_eq!(DOCK.stack, StackConstants { peek: 14., scale_step: 0.05, visible: 3, gap: 6. });
        assert_eq!(DOCK.sheet, DockSheetSpec { max_height_fraction: 0.85, radius: 24. });
        assert_eq!(DOCK.copy.close, "Close");
    }

    #[test]
    fn rows_match_the_fixture() {
        let fixture = fixture();
        let cases = fixture["rowCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let sources: Vec<DockSource> = serde_json::from_value(case["sources"].clone()).unwrap();
            assert_eq!(serde_json::to_value(dock_rows(&sources)).unwrap(), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn dock_matches_the_fixture() {
        let fixture = fixture();
        let cases = fixture["cases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let input = &case["input"];
            let rows: Vec<DockRow> = serde_json::from_value(input["rows"].clone()).unwrap();
            let view = dock(&rows, input["expanded"].as_bool().unwrap(), input["openId"].as_str());
            assert_eq!(serde_json::to_value(&view).unwrap(), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn auto_open_matches_the_fixture() {
        let fixture = fixture();
        let cases = fixture["autoOpenCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let previous: Vec<String> = serde_json::from_value(case["previous"].clone()).unwrap();
            let rows: Vec<(String, DockKind)> = serde_json::from_value(case["rowIds"].clone()).unwrap();
            let got = dock_auto_open(&previous, rows.iter().map(|(id, kind)| (id.as_str(), *kind)));
            assert_eq!(got.as_deref(), case["expected"].as_str(), "{}", case["name"]);
        }
    }

    #[test]
    fn suggestion_sources_match_the_fixture() {
        let fixture = fixture();
        let cases = fixture["suggestionCases"].as_array().unwrap();
        assert!(!cases.is_empty());
        for case in cases {
            let rows: Vec<DockSuggestionsRow> = case["rows"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|row| row["type"] == "suggestions")
                .map(|row| serde_json::from_value(row.clone()).unwrap())
                .collect();
            let got = suggestion_sources(&rows);
            assert_eq!(serde_json::to_value(&got).unwrap(), case["expected"], "{}", case["name"]);
        }
    }

    #[test]
    fn the_pile_replays_the_stack_geometry() {
        let (height, items) = crate::stack::stack_layout(&[40., 40., 40., 40.], false, true, DOCK.stack);
        assert_eq!(height, 82.);
        assert_eq!(items.iter().filter(|item| item.visible).count(), 3);
        assert_eq!(items[3].offset, 42.);
        let (height, _) = crate::stack::stack_layout(&[40., 40.], true, true, DOCK.stack);
        assert_eq!(height, 86.);
    }
}
