//! #EDE-239: the taskbar's session area (shell.json `sessionArea`, the
//! TypeScript reference `sessionArea` in `packages/uicore/src/shell.ts`):
//! the holder's `sessions` feed (the IDE sidebar's Active band, in its
//! order) as the chips and the panel's rows.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use serde_json::Value;

const FIXTURE: &str = include_str!("../../../../../packages/uicore/fixtures/shell.json");

/// The fixture's `sessionArea`: the limits and the copy.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionAreaSpec {
    pub chips_max: usize,
    pub options_max: usize,
    pub trace_max: usize,
    pub copy: SessionAreaCopy,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionAreaCopy {
    pub untitled: String,
    pub running: String,
    pub needs_you: String,
    pub approve: String,
    pub decline: String,
    pub agents: std::collections::BTreeMap<String, String>,
}

pub fn spec() -> &'static SessionAreaSpec {
    static SPEC: OnceLock<SessionAreaSpec> = OnceLock::new();
    SPEC.get_or_init(|| {
        let fixture: Value = serde_json::from_str(FIXTURE).expect("shell.json");
        serde_json::from_value(fixture["sessionArea"].clone()).expect("shell.json sessionArea")
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SessionAreaOption {
    Decision { decision: String },
    Answer { answer: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionAreaAsk {
    pub message_id: String,
    pub kind: String,
    pub title: String,
    #[serde(default)]
    pub options: Vec<SessionAreaOption>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionAreaTrace {
    pub title: String,
    pub icon: String,
    pub status: String,
}

/// One session as the holder publishes it (the feeds' `sessions`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionAreaInput {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub agent: Option<String>,
    pub state: String,
    #[serde(default)]
    pub team: Option<String>,
    #[serde(default)]
    pub device: Option<String>,
    pub key: String,
    #[serde(default)]
    pub stop: Vec<String>,
    #[serde(default)]
    pub ask: Option<SessionAreaAsk>,
    #[serde(default)]
    pub trace: Vec<SessionAreaTrace>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionAreaChip {
    pub id: String,
    pub title: String,
    pub icon: String,
    pub running: bool,
    pub needs: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionAreaAction {
    pub id: String,
    pub label: String,
    pub primary: bool,
    pub message_id: String,
    pub decision: Option<String>,
    pub answer: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionAreaRow {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub icon: String,
    pub running: bool,
    pub needs: bool,
    pub badge: Option<String>,
    pub trace: Vec<SessionAreaTrace>,
    pub question: Option<String>,
    pub actions: Vec<SessionAreaAction>,
    pub stop: bool,
    pub key: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionArea {
    pub count: usize,
    pub needs: usize,
    pub chips: Vec<SessionAreaChip>,
    pub more: usize,
    pub rows: Vec<SessionAreaRow>,
}

fn session_icon(agent: Option<&str>) -> &'static str {
    match agent {
        Some("codex") => "brand-codex",
        Some("claude") => "brand-claude",
        _ => "action-chat",
    }
}

/// The taskbar's session area over the holder's `sessions`: the first
/// `chipsMax` as chips (the band's order), the rest a count; every session
/// a row of the panel. `room`: the chips a narrow output has room for
/// (at least one, never more than `chipsMax`).
pub fn session_area(sessions: &[SessionAreaInput], room: Option<usize>) -> SessionArea {
    let spec = spec();
    let copy = &spec.copy;
    let rows: Vec<SessionAreaRow> = sessions
        .iter()
        .map(|s| {
            let title = match s.title.trim() {
                "" => copy.untitled.clone(),
                title => title.to_string(),
            };
            let agent = s.agent.as_deref().and_then(|agent| copy.agents.get(agent)).map(String::as_str);
            let subtitle = [agent, s.team.as_deref(), s.device.as_deref()]
                .into_iter()
                .map(|part| part.unwrap_or_default().trim())
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join(" · ");
            let needs = s.state == "needs";
            let running = s.state == "running";
            let ask = s.ask.as_ref().filter(|_| needs);
            let options: &[SessionAreaOption] = ask.map_or(&[], |ask| ask.options.as_slice());
            let decided = |want: &str| -> Vec<&SessionAreaOption> { options.iter().filter(|o| matches!(o, SessionAreaOption::Decision { decision } if decision == want)).collect() };
            let plan: Vec<&SessionAreaOption> = decided("rejected").into_iter().chain(decided("approved")).collect();
            let has_plan = options.iter().any(|o| matches!(o, SessionAreaOption::Decision { .. }));
            let ordered: Vec<&SessionAreaOption> = if has_plan { plan } else { options.iter().take(spec.options_max).collect() };
            let actions = ordered
                .into_iter()
                .enumerate()
                .map(|(i, option)| {
                    let (decision, answer) = match option {
                        SessionAreaOption::Decision { decision } => (Some(decision.clone()), None),
                        SessionAreaOption::Answer { answer } => (None, Some(answer.clone())),
                    };
                    let label = match decision.as_deref() {
                        Some("approved") => copy.approve.clone(),
                        Some("rejected") => copy.decline.clone(),
                        _ => answer.clone().unwrap_or_default(),
                    };
                    let primary = match decision.as_deref() {
                        Some(decision) => decision == "approved",
                        None => i == 0,
                    };
                    SessionAreaAction { id: format!("{}:{i}", s.id), label, primary, message_id: ask.map(|ask| ask.message_id.clone()).unwrap_or_default(), decision, answer }
                })
                .collect();
            let badge = if needs {
                Some(copy.needs_you.clone())
            } else if running {
                Some(copy.running.clone())
            } else {
                None
            };
            let skip = s.trace.len().saturating_sub(spec.trace_max);
            SessionAreaRow {
                id: s.id.clone(),
                title,
                subtitle,
                icon: session_icon(s.agent.as_deref()).to_string(),
                running,
                needs,
                badge,
                trace: s.trace.iter().skip(skip).cloned().collect(),
                question: ask.map(|ask| ask.title.trim().to_string()).filter(|title| !title.is_empty()),
                actions,
                stop: !s.stop.is_empty(),
                key: s.key.clone(),
            }
        })
        .collect();
    let chips: Vec<SessionAreaChip> = rows
        .iter()
        .take(room.unwrap_or(spec.chips_max).clamp(1, spec.chips_max))
        .map(|r| SessionAreaChip { id: r.id.clone(), title: r.title.clone(), icon: r.icon.clone(), running: r.running, needs: r.needs })
        .collect();
    SessionArea { count: rows.len(), needs: rows.iter().filter(|r| r.needs).count(), more: rows.len() - chips.len(), chips, rows }
}
