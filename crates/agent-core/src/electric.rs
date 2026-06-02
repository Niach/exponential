//! Electric shape long-poll for `/api/shapes/assigned-issues` — a port of
//! `apps/companion/src/event-source.ts` (which used the JS ShapeStream lib) onto
//! the raw wire protocol I already ported to Zig: `offset=-1` for the initial
//! snapshot, then `offset=…&handle=…&live=true` (server holds ~60s); 409 or an
//! inline `must-refetch` control message → drop the cursor and re-snapshot.
//!
//! The pure parts (message parse + event derivation + URL build) are tested
//! against the shared `packages/electric-protocol/fixtures`. The poll loop uses
//! blocking `ureq` on a worker thread (threaded model, no tokio).

use crate::state::{ShapeOffset, State};
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const SHAPE_NAME: &str = "assigned-issues";
const SHAPE_PATH: &str = "/api/shapes/assigned-issues";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShapeMessage {
    Insert { value: Value },
    Update { value: Value, old_value: Option<Value> },
    Delete { value: Option<Value> },
    UpToDate,
    MustRefetch,
}

/// Parse a shape response body (a JSON array of messages) into typed messages.
pub fn parse_messages(arr: &Value) -> Vec<ShapeMessage> {
    let mut out = Vec::new();
    let items = match arr.as_array() {
        Some(a) => a,
        None => return out,
    };
    for m in items {
        let headers = m.get("headers");
        if let Some(ctrl) = headers.and_then(|h| h.get("control")).and_then(|c| c.as_str()) {
            match ctrl {
                "must-refetch" => out.push(ShapeMessage::MustRefetch),
                "up-to-date" => out.push(ShapeMessage::UpToDate),
                _ => {}
            }
            continue;
        }
        let op = headers.and_then(|h| h.get("operation")).and_then(|o| o.as_str());
        let value = m.get("value").cloned();
        match op {
            Some("insert") => out.push(ShapeMessage::Insert { value: value.unwrap_or(Value::Null) }),
            Some("update") => out.push(ShapeMessage::Update {
                value: value.unwrap_or(Value::Null),
                old_value: m.get("old_value").cloned(),
            }),
            Some("delete") => out.push(ShapeMessage::Delete { value }),
            _ => {}
        }
    }
    out
}

pub fn parse_body(s: &str) -> Result<Vec<ShapeMessage>, String> {
    let v: Value = serde_json::from_str(s).map_err(|e| format!("shape body: {e}"))?;
    Ok(parse_messages(&v))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueEventType {
    Assigned,
    Unassigned,
    Updated,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueEvent {
    pub event_type: IssueEventType,
    pub issue_id: String,
    pub identifier: String,
    pub title: String,
    pub project_id: String,
    pub assignee_id: Option<String>,
}

fn str_field(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(|x| x.as_str()).map(|s| s.to_string())
}

/// Classify a change message relative to the agent's user id (mirrors
/// `rowEventType`). Values are raw wire snake_case (`assignee_id`).
fn row_event(msg: &ShapeMessage, bot_user_id: &str) -> Option<IssueEventType> {
    match msg {
        ShapeMessage::Insert { value } => {
            if str_field(value, "assignee_id").as_deref() == Some(bot_user_id) {
                Some(IssueEventType::Assigned)
            } else {
                None
            }
        }
        ShapeMessage::Update { value, old_value } => {
            let new_a = str_field(value, "assignee_id");
            let old_a = old_value.as_ref().and_then(|o| str_field(o, "assignee_id"));
            let is_bot = |a: &Option<String>| a.as_deref() == Some(bot_user_id);
            if is_bot(&new_a) && !is_bot(&old_a) {
                Some(IssueEventType::Assigned)
            } else if is_bot(&old_a) && !is_bot(&new_a) {
                Some(IssueEventType::Unassigned)
            } else if is_bot(&new_a) {
                Some(IssueEventType::Updated)
            } else {
                None
            }
        }
        ShapeMessage::Delete { .. } => Some(IssueEventType::Unassigned),
        _ => None,
    }
}

/// Derive dispatcher events from a batch (port of the subscribe callback).
/// Archived rows become `unassigned` regardless of assignment change.
pub fn derive_events(messages: &[ShapeMessage], bot_user_id: &str) -> Vec<IssueEvent> {
    let mut events = Vec::new();
    for msg in messages {
        let Some(t) = row_event(msg, bot_user_id) else { continue };
        // Pull the row value (delete carries the old row in `value`).
        let value = match msg {
            ShapeMessage::Insert { value } | ShapeMessage::Update { value, .. } => Some(value),
            ShapeMessage::Delete { value } => value.as_ref(),
            _ => None,
        };
        let Some(value) = value else { continue };
        let id = match str_field(value, "id") {
            Some(id) => id,
            None => continue,
        };
        let archived = str_field(value, "archived_at").is_some();
        events.push(IssueEvent {
            event_type: if archived { IssueEventType::Unassigned } else { t },
            issue_id: id,
            identifier: str_field(value, "identifier").unwrap_or_default(),
            title: str_field(value, "title").unwrap_or_default(),
            project_id: str_field(value, "project_id").unwrap_or_default(),
            assignee_id: if archived { None } else { str_field(value, "assignee_id") },
        });
    }
    events
}

/// `{base}/api/shapes/assigned-issues?offset=-1` for the first request, then
/// `…?offset=…&handle=…&live=true`.
pub fn build_url(base_url: &str, offset: Option<&str>, handle: Option<&str>) -> String {
    let base = base_url.trim_end_matches('/');
    match (offset, handle) {
        (Some(off), Some(h)) => format!("{base}{SHAPE_PATH}?offset={off}&handle={h}&live=true"),
        _ => format!("{base}{SHAPE_PATH}?offset=-1"),
    }
}

pub struct PollResult {
    pub messages: Vec<ShapeMessage>,
    pub handle: Option<String>,
    pub offset: Option<String>,
    pub must_refetch: bool,
}

/// One shape request. A 409 or an inline `must-refetch` sets `must_refetch`
/// (caller drops the cursor and re-snapshots).
pub fn poll_once(
    base_url: &str,
    token: Option<&str>,
    offset: Option<&str>,
    handle: Option<&str>,
    timeout_s: u64,
) -> Result<PollResult, String> {
    let url = build_url(base_url, offset, handle);
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(timeout_s)).build();
    let mut req = agent.get(&url);
    if let Some(t) = token {
        req = req.set("authorization", &format!("Bearer {t}"));
    }
    let resp = match req.call() {
        Ok(r) => r,
        Err(ureq::Error::Status(409, _)) => {
            return Ok(PollResult { messages: vec![], handle: None, offset: None, must_refetch: true });
        }
        Err(e) => return Err(format!("shape poll: {e}")),
    };
    let new_handle = resp.header("electric-handle").map(|s| s.to_string());
    let new_offset = resp.header("electric-offset").map(|s| s.to_string());
    let body = resp.into_string().map_err(|e| format!("shape read: {e}"))?;
    let messages = parse_body(&body)?;
    let must_refetch = messages.iter().any(|m| matches!(m, ShapeMessage::MustRefetch));
    Ok(PollResult { messages, handle: new_handle, offset: new_offset, must_refetch })
}

/// Run the long-poll loop on the current thread until `stop` is set, persisting
/// the cursor to `state` after every batch and invoking `on_event` for each
/// derived event. Designed to be `spawn`ed on a worker thread.
pub fn run_loop(
    base_url: &str,
    token: Option<&str>,
    bot_user_id: &str,
    state: &Arc<Mutex<State>>,
    stop: &Arc<AtomicBool>,
    mut on_event: impl FnMut(IssueEvent),
) {
    let mut backoff_ms: u64 = 500;
    while !stop.load(Ordering::Acquire) {
        // Resume from the persisted cursor (None → initial snapshot).
        let cursor = state.lock().unwrap().load_offset(SHAPE_NAME).ok().flatten();
        // An empty handle (set by a must-refetch reset) means "re-snapshot".
        let (offset, handle) = match &cursor {
            Some(c) if !c.handle.is_empty() => (Some(c.offset.as_str()), Some(c.handle.as_str())),
            _ => (None, None),
        };
        match poll_once(base_url, token, offset, handle, 90) {
            Ok(res) => {
                backoff_ms = 500;
                if res.must_refetch {
                    // Drop the cursor; next poll re-snapshots from -1.
                    let _ = state.lock().unwrap().save_offset(&ShapeOffset {
                        shape_name: SHAPE_NAME.to_string(),
                        offset: "-1".to_string(),
                        handle: String::new(),
                    });
                    // Treat handle="" as "no handle" on the next build_url.
                    continue;
                }
                for ev in derive_events(&res.messages, bot_user_id) {
                    on_event(ev);
                }
                if let (Some(off), Some(h)) = (res.offset, res.handle) {
                    let _ = state.lock().unwrap().save_offset(&ShapeOffset { shape_name: SHAPE_NAME.to_string(), offset: off, handle: h });
                }
            }
            Err(_) => {
                sleep_cancellable(backoff_ms, stop);
                backoff_ms = (backoff_ms * 2).min(30_000);
            }
        }
    }
}

fn sleep_cancellable(ms: u64, stop: &Arc<AtomicBool>) {
    let mut left = ms;
    while left > 0 && !stop.load(Ordering::Acquire) {
        let chunk = left.min(100);
        std::thread::sleep(Duration::from_millis(chunk));
        left -= chunk;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The shared cross-client protocol fixtures (also driving the Zig tests).
    const INITIAL: &str = include_str!("../../../packages/electric-protocol/fixtures/initial-snapshot.json");
    const LIVE: &str = include_str!("../../../packages/electric-protocol/fixtures/live-update.json");
    const REFETCH: &str = include_str!("../../../packages/electric-protocol/fixtures/must-refetch.json");

    fn body_of(fixture: &str) -> Value {
        let v: Value = serde_json::from_str(fixture).unwrap();
        v.get("response").unwrap().get("body").unwrap().clone()
    }

    #[test]
    fn build_url_initial_vs_live() {
        assert_eq!(build_url("https://x.at/", None, None), "https://x.at/api/shapes/assigned-issues?offset=-1");
        assert_eq!(
            build_url("https://x.at", Some("0_1"), Some("h-9")),
            "https://x.at/api/shapes/assigned-issues?offset=0_1&handle=h-9&live=true"
        );
    }

    #[test]
    fn parses_initial_snapshot_inserts() {
        let msgs = parse_messages(&body_of(INITIAL));
        assert!(msgs.iter().any(|m| matches!(m, ShapeMessage::Insert { .. })));
        assert!(msgs.iter().any(|m| matches!(m, ShapeMessage::UpToDate)));
    }

    #[test]
    fn parses_must_refetch() {
        let msgs = parse_messages(&body_of(REFETCH));
        assert_eq!(msgs, vec![ShapeMessage::MustRefetch]);
    }

    #[test]
    fn live_update_parses_to_update_or_insert() {
        let msgs = parse_messages(&body_of(LIVE));
        assert!(msgs.iter().any(|m| matches!(m, ShapeMessage::Update { .. } | ShapeMessage::Insert { .. })));
    }

    fn insert(assignee: Option<&str>) -> ShapeMessage {
        ShapeMessage::Insert {
            value: serde_json::json!({
                "id": "i1", "identifier": "EXP-1", "title": "t", "project_id": "p",
                "assignee_id": assignee, "archived_at": null
            }),
        }
    }

    #[test]
    fn insert_assigned_to_bot_is_assigned() {
        let ev = derive_events(&[insert(Some("bot"))], "bot");
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].event_type, IssueEventType::Assigned);
        assert_eq!(ev[0].assignee_id.as_deref(), Some("bot"));
    }

    #[test]
    fn insert_assigned_to_other_is_ignored() {
        assert!(derive_events(&[insert(Some("someone-else"))], "bot").is_empty());
    }

    #[test]
    fn reassignment_away_is_unassigned() {
        let msg = ShapeMessage::Update {
            value: serde_json::json!({ "id": "i1", "identifier": "EXP-1", "title": "t", "project_id": "p", "assignee_id": "other" }),
            old_value: Some(serde_json::json!({ "assignee_id": "bot" })),
        };
        let ev = derive_events(&[msg], "bot");
        assert_eq!(ev[0].event_type, IssueEventType::Unassigned);
    }

    #[test]
    fn archived_row_becomes_unassigned() {
        let msg = ShapeMessage::Update {
            value: serde_json::json!({ "id": "i1", "identifier": "EXP-1", "title": "t", "project_id": "p", "assignee_id": "bot", "archived_at": "2026-06-01T00:00:00Z" }),
            old_value: Some(serde_json::json!({ "assignee_id": "bot" })),
        };
        let ev = derive_events(&[msg], "bot");
        assert_eq!(ev[0].event_type, IssueEventType::Unassigned);
        assert!(ev[0].assignee_id.is_none());
    }

    #[test]
    fn update_still_assigned_to_bot_is_updated() {
        let msg = ShapeMessage::Update {
            value: serde_json::json!({ "id": "i1", "identifier": "EXP-1", "title": "t", "project_id": "p", "assignee_id": "bot" }),
            old_value: Some(serde_json::json!({ "assignee_id": "bot" })),
        };
        assert_eq!(derive_events(&[msg], "bot")[0].event_type, IssueEventType::Updated);
    }
}
