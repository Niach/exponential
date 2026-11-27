//! #EDE-239: the taskbar's session area, fed by the holder. The feeds
//! ([`crate::ede_feeds`]) carry `sessions`: the IDE sidebar's ACTIVE band
//! (domain-contract sidebar-tree.json: "ACTIVE stays at the bottom across
//! all teams", chats.json `railChats` per team, the rail's order), never a
//! list of its own. The band's rows are `shell_regions::sidebar_input`'s
//! `active`, so the taskbar's chips and the sidebar's Active rows are one
//! fold over the same data. Each entry adds what the panel draws:
//!
//! - `state`: `needs` (a session of the tree waits on the person), else
//!   `running` (one works), else `idle`: the sidebar's run mark
//!   (`sidebar::running_row_state`);
//! - `agent`: the working session's CLI, else the root's (the sidebar's
//!   mark), as `claude` / `codex`;
//! - `team`, `device`: the subtitle's parts;
//! - `stop`: the working sessions (the IDE's Stop, `sessions.stop` each);
//! - `ask`: the waiting card the panel answers in place: a plan without
//!   questions (`approved` / `rejected`), a single-choice question's options
//!   (each its label as the answer); a card it cannot answer in place
//!   (questions, a surface, several picks, a sealed card) has no options:
//!   Open;
//! - `trace`: the working session's last [`TRACE_LINES`] tool lines, from
//!   the chat store ([`trace_lines`]: steer's `trace_tool`, the transcript's
//!   own titles).
//!
//! The shell shapes chips and rows from it (uicore shell.json
//! `sessionArea`); its presses come back as the bus's `stop`, `answer` and
//! `page` (`chat:<root>`).

use std::collections::HashMap;

use chat_store::sqlite::SqliteStore;
use gpui::App;
use serde_json::{json, Value};
use steer::frames::{ActivityEvent, ToolUpdateStatus};
use sync::Store;
use uicore_model::ide::IdeSidebarInput;

/// The tool lines a session's trace shows.
pub(crate) const TRACE_LINES: usize = 3;

/// The newest events read for a trace.
const TRACE_READ: usize = 120;

/// A trace to read off the main thread: the chat `root`'s working
/// `session` on `device`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TraceRead {
    pub(crate) root: String,
    pub(crate) session: String,
    pub(crate) device: String,
    /// The session still works: an open tool runs, else it settled.
    pub(crate) working: bool,
}

/// The `ask` of a waiting card `card` (`message_id`): its options as the
/// panel's buttons (none = Open only).
pub(crate) fn ask_of(message_id: &str, kind: &str, title: &str, card: Option<&Value>) -> Value {
    let field = |key: &str| card.and_then(|card| card.get(key));
    let sealed = field("sealed").is_some();
    let surface = field("surface").is_some_and(Value::is_object);
    let options: Vec<Value> = match kind {
        "plan" if !sealed && !surface && field("questions").and_then(Value::as_array).is_none_or(Vec::is_empty) && card.is_some() => {
            vec![json!({"decision": "approved"}), json!({"decision": "rejected"})]
        }
        "question" if !sealed && !surface && field("multi").and_then(Value::as_bool) != Some(true) => field("options")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|option| option.get("label").and_then(Value::as_str))
            .map(str::trim)
            .filter(|label| !label.is_empty())
            .map(|label| json!({"answer": label}))
            .collect(),
        _ => Vec::new(),
    };
    json!({"messageId": message_id, "kind": kind, "title": title, "options": options})
}

/// The last [`TRACE_LINES`] tool lines of the main lane (no subagent's), in
/// order: each `{title, icon, status}` (`running` while it runs and the
/// session works, `error` when it failed, else `done`).
pub(crate) fn trace_lines(events: &[ActivityEvent], working: bool) -> Vec<Value> {
    struct Line {
        id: Option<String>,
        name: String,
        detail: Option<String>,
        kind: &'static str,
        settled: bool,
        failed: bool,
    }
    let mut lines: Vec<Line> = Vec::new();
    for event in events {
        match event {
            ActivityEvent::Tool { name, detail, id, tool_kind, subagent_id: None, .. } => lines.push(Line {
                id: id.clone(),
                name: name.clone(),
                detail: detail.clone(),
                kind: tool_kind.map(|kind| kind.as_str()).unwrap_or("other"),
                settled: false,
                failed: false,
            }),
            ActivityEvent::ToolUpdate { id, status: Some(status), .. } => {
                if let Some(line) = lines.iter_mut().rev().find(|line| line.id.as_deref() == Some(id.as_str())) {
                    line.settled = true;
                    line.failed = *status == ToolUpdateStatus::Failed;
                }
            }
            _ => {}
        }
    }
    let skip = lines.len().saturating_sub(TRACE_LINES);
    lines
        .into_iter()
        .skip(skip)
        .map(|line| {
            let settled = line.settled || !working;
            let (title, icon) = match steer::exp_tool_display(&line.name, settled) {
                Some(display) => (display.caption.to_string(), steer::trace::icons::TOOL.to_string()),
                None => {
                    let model = steer::trace::trace_tool(line.kind, &line.name, line.detail.as_deref(), settled, line.failed);
                    (model.title, model.icon)
                }
            };
            let status = if line.failed {
                "error"
            } else if settled {
                "done"
            } else {
                "running"
            };
            json!({"title": title, "icon": icon, "status": status})
        })
        .collect()
}

/// The traces of `reads` from the chat store (off the main thread), by root.
pub(crate) fn read_traces(store: &SqliteStore, reads: &[TraceRead]) -> HashMap<String, Vec<Value>> {
    reads
        .iter()
        .filter_map(|read| {
            let mut stored = store.read_events(&read.session, &read.device, None, Some(TRACE_READ)).ok()?;
            stored.sort_by_key(|event| event.seq);
            let events: Vec<ActivityEvent> = stored.into_iter().filter_map(|event| serde_json::from_value(event.event).ok()).collect();
            Some((read.root.clone(), trace_lines(&events, read.working)))
        })
        .collect()
}

/// `traces` into the feeds' `sessions` entries.
pub(crate) fn with_traces(feeds: &mut Value, traces: &HashMap<String, Vec<Value>>) {
    for entry in feeds["sessions"].as_array_mut().into_iter().flatten() {
        if let Some(lines) = entry["id"].as_str().and_then(|root| traces.get(root)) {
            entry["trace"] = json!(lines);
        }
    }
}

/// The feeds' `sessions` (the sidebar's Active band, `input.active`) and
/// the traces to read for them.
pub(crate) fn gather(input: &IdeSidebarInput, needs: &[domain::needs_you::NeedsYouRow], cx: &mut App) -> (Vec<Value>, Vec<TraceRead>) {
    let Some(store) = Store::try_global(cx) else { return (Vec::new(), Vec::new()) };
    let teams = store.collections().teams_sorted(cx);
    let team_names: HashMap<String, String> = teams.iter().map(|team| (team.id.clone(), team.name.clone())).collect();
    // The band's live facts, by root (`live_roots_for_rail`, memoized: the
    // same call the sidebar's band made).
    let mut live: HashMap<String, crate::session_data::LiveRootRow> = HashMap::new();
    for team in &teams {
        for row in crate::session_data::live_roots_for_rail(&team.id, cx) {
            live.insert(row.root_id.clone(), row);
        }
    }
    let mut out = Vec::new();
    let mut reads = Vec::new();
    for active in &input.active {
        let row = live.get(&active.id);
        let (busy, needs_you) = row.map_or((false, false), |row| (row.busy, row.needs_you));
        let state = match crate::sidebar::running_row_state(busy, needs_you) {
            domain::session_tree::SessionState::NeedsYou => "needs",
            domain::session_tree::SessionState::Working => "running",
            _ => "idle",
        };
        let tree = crate::session_data::tree_of_root(&active.id, cx);
        let working: Vec<&domain::rows::SessionRow> = tree.iter().filter(|session| crate::session_data::session_is_working(session)).collect();
        // The sidebar's mark: the newest working session's CLI, else the root's.
        let lead = working.last().copied().or_else(|| tree.iter().find(|session| session.id == active.id));
        let agent = lead.and_then(|session| crate::run_rows::run_agent(session.cli.as_deref())).map(|agent| agent.id());
        let device = row.and_then(|row| row.device_id.clone()).filter(|device| !device.is_empty());
        let device_name = device.as_deref().map(|device| crate::device_page::device_title(device, cx).to_string());
        let ask = needs.iter().find(|wait| wait.root_id == active.id && wait.kind != domain::needs_you::NeedsYouKind::Pr).map(|wait| {
            let kind = if wait.kind == domain::needs_you::NeedsYouKind::Plan { "plan" } else { "question" };
            let session_id = wait.session_id.clone();
            let card = crate::session_data::digest_messages(cx, |id| id == session_id).into_iter().find(|message| message.id == wait.message_id).and_then(|message| message.card);
            ask_of(&wait.message_id, kind, &wait.title, card.as_ref())
        });
        if let Some(session) = lead.filter(|session| session.device_id.as_deref().is_some_and(|device| !device.is_empty())) {
            reads.push(TraceRead { root: active.id.clone(), session: session.id.clone(), device: session.device_id.clone().unwrap_or_default(), working: !working.is_empty() });
        }
        out.push(json!({
            "id": active.id,
            "title": active.title,
            "agent": agent,
            "state": state,
            "team": active.team.as_ref().and_then(|team| team_names.get(team)),
            "device": device_name,
            "key": format!("chat:{}", active.id),
            "stop": working.iter().map(|session| session.id.clone()).collect::<Vec<_>>(),
            "ask": ask,
            "trace": [],
        }));
    }
    (out, reads)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(id: &str, name: &str, kind: Option<&str>, detail: &str) -> ActivityEvent {
        serde_json::from_value(json!({"kind": "tool", "name": name, "id": id, "toolKind": kind, "detail": detail})).unwrap()
    }

    fn update(id: &str, status: &str) -> ActivityEvent {
        serde_json::from_value(json!({"kind": "tool_update", "id": id, "status": status})).unwrap()
    }

    #[test]
    fn the_trace_is_the_last_three_main_lane_tools() {
        let mut events = vec![tool("t1", "Read", Some("read"), "a.rs"), update("t1", "completed")];
        events.push(tool("t2", "Edit", Some("edit"), "b.rs"));
        events.push(update("t2", "failed"));
        events.push(tool("t3", "Bash", Some("execute"), "cargo test"));
        events.push(update("t3", "completed"));
        events.push(serde_json::from_value(json!({"kind": "tool", "name": "Grep", "id": "s1", "subagentId": "sub"})).unwrap());
        events.push(tool("t4", "Bash", Some("execute"), "cargo check"));
        let lines = trace_lines(&events, true);
        let statuses: Vec<&str> = lines.iter().map(|line| line["status"].as_str().unwrap()).collect();
        assert_eq!(statuses, ["error", "done", "running"], "{lines:?}");
        assert_eq!(lines.len(), TRACE_LINES);
        assert!(lines[2]["title"].as_str().unwrap().contains("cargo check"), "{lines:?}");
        // A session that stopped working settles its open line.
        assert_eq!(trace_lines(&events, false)[2]["status"], "done");
    }

    #[test]
    fn a_plan_without_questions_is_approved_in_place_a_question_by_its_options() {
        let plan = ask_of("m1", "plan", "Plan ready", Some(&json!({"kind": "plan", "plan": "Do it"})));
        assert_eq!(plan["options"], json!([{"decision": "approved"}, {"decision": "rejected"}]));
        let with_questions = ask_of("m1", "plan", "Plan", Some(&json!({"kind": "plan", "questions": [{"id": "q", "prompt": "?"}]})));
        assert_eq!(with_questions["options"], json!([]));
        let question = ask_of("m2", "question", "Run it?", Some(&json!({"kind": "question", "options": [{"label": "Allow"}, {"label": " "}, {"label": "Decline"}]})));
        assert_eq!(question["options"], json!([{"answer": "Allow"}, {"answer": "Decline"}]));
        assert_eq!(question["messageId"], "m2");
        let multi = ask_of("m3", "question", "Pick", Some(&json!({"kind": "question", "multi": true, "options": [{"label": "A"}]})));
        assert_eq!(multi["options"], json!([]));
        let sealed = ask_of("m4", "question", "Pick", Some(&json!({"kind": "question", "sealed": "x", "options": [{"label": "A"}]})));
        assert_eq!(sealed["options"], json!([]));
        assert_eq!(ask_of("m5", "plan", "Plan", None)["options"], json!([]), "no card read: Open");
    }
}
