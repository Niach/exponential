//! OPEN (`chat-store.json`): a chat opens from the store before any live
//! feed attaches, per session the stored events of the device it is on now
//! through the same REPLAY TAIL rule a join replays by
//! (`fixtures/steer-enet.json`, reference `steer::history::replay_tail`),
//! older ones paged from the store. Here over the stored wire JSON, so a
//! client without steer's typed events (the web's wasm, S4) cuts the same
//! tail; `tests::matches_steer` holds it to steer's rule.

use std::collections::HashSet;

use serde_json::Value;

/// The latest-wins STATE slots (steer `slot_of` + `workflow`): never a
/// transcript row, always replayed.
pub const SLOT_KINDS: [&str; 10] =
    ["config_state", "usage", "context_layout", "rate_limit", "turn", "queue", "background_tasks", "task_list", "diff", "workflow"];

fn kind(event: &Value) -> &str {
    event.get("kind").and_then(Value::as_str).unwrap_or_default()
}

fn field<'v>(event: &'v Value, key: &str) -> Option<&'v str> {
    event.get(key).and_then(Value::as_str)
}

pub fn is_state_slot(event: &Value) -> bool {
    SLOT_KINDS.contains(&kind(event))
}

/// THE REPLAY TAIL over `events` (seq order): the indexes it keeps (order
/// unchanged) and whether older log events were left out. The newest `tail`
/// LOG events, contiguous, plus every slot where it stands; the cut moves
/// back (by at most another `tail`) so the kept log holds the `tool` row of
/// every kept `tool_update` and every question nothing resolved.
pub fn replay_tail(events: &[&Value], tail: usize) -> (Vec<usize>, bool) {
    let log: Vec<usize> = (0..events.len()).filter(|&ix| !is_state_slot(events[ix])).collect();
    if log.len() <= tail {
        return ((0..events.len()).collect(), false);
    }
    let mut cut = log.len() - tail;
    let floor = cut.saturating_sub(tail);
    let mut held: HashSet<&str> = HashSet::new();
    let mut wanted: HashSet<&str> = HashSet::new();
    for &ix in &log[cut..] {
        let event = events[ix];
        match kind(event) {
            "tool" => {
                if let Some(id) = field(event, "id") {
                    held.insert(id);
                }
            }
            "tool_update" => {
                if let Some(id) = field(event, "id").filter(|id| !held.contains(id)) {
                    wanted.insert(id);
                }
            }
            _ => {}
        }
    }
    let resolved: Vec<(Option<&str>, Option<&str>)> = log
        .iter()
        .map(|&ix| events[ix])
        .filter(|event| kind(event) == "question_resolved")
        .map(|event| (field(event, "id"), field(event, "askId")))
        .collect();
    let open = |qid: &str| {
        !resolved.iter().any(|(id, ask)| match (id, ask) {
            (Some(id), _) => *id == qid,
            (None, Some(ask)) => qid == *ask || qid.strip_prefix(*ask).is_some_and(|rest| rest.starts_with('#')),
            (None, None) => false,
        })
    };
    for at in (floor..cut).rev() {
        let event = events[log[at]];
        let pull = match kind(event) {
            "tool" => field(event, "id").is_some_and(|id| wanted.contains(id)),
            "question" => field(event, "id").is_some_and(open),
            _ => false,
        };
        if pull {
            cut = at;
        }
    }
    let first_kept = log[cut];
    let kept = (0..events.len()).filter(|&ix| ix >= first_kept || is_state_slot(events[ix])).collect();
    (kept, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn synthetic(count: usize) -> Vec<Value> {
        let mut events = Vec::new();
        let mut i = 0;
        while events.len() < count {
            if i % 40 == 0 {
                events.push(json!({ "kind": "user_message", "text": format!("request {i}") }));
                events.push(json!({ "kind": "turn", "state": "started" }));
            }
            if i % 97 == 0 {
                events.push(json!({ "kind": "question", "id": format!("q{i}"), "text": "?", "options": [] }));
            }
            if i % 194 == 0 && i > 0 {
                events.push(json!({ "kind": "question_resolved", "id": format!("q{}", i - 97) }));
            }
            events.push(json!({ "kind": "narration", "text": format!("step {i}") }));
            events.push(json!({ "kind": "tool", "name": "Bash", "id": format!("t{i}") }));
            // Every tenth update lands far after its call.
            let late = if i % 10 == 0 && i >= 30 { i - 30 } else { i };
            events.push(json!({ "kind": "tool_update", "id": format!("t{late}"), "status": "completed", "output": "ok" }));
            if i % 50 == 0 {
                events.push(json!({ "kind": "usage", "contextUsed": i, "contextSize": 1000 }));
            }
            i += 1;
        }
        events.truncate(count);
        events
    }

    #[test]
    fn matches_steer() {
        for (count, tail) in [(10, 50), (500, 50), (3000, 200), (2500, 1000)] {
            let events = synthetic(count);
            let typed: Vec<(u64, steer::frames::ActivityEvent)> = events
                .iter()
                .enumerate()
                .filter_map(|(seq, event)| Some((seq as u64, serde_json::from_value(event.clone()).ok()?)))
                .collect();
            assert_eq!(typed.len(), events.len(), "every synthetic event parses");
            let (want, want_cut) = steer::replay_tail(typed.iter().map(|(seq, event)| (*seq, event)).collect(), tail);
            let refs: Vec<&Value> = events.iter().collect();
            let (got, got_cut) = replay_tail(&refs, tail);
            assert_eq!(got_cut, want_cut, "{count}/{tail}");
            assert_eq!(got.iter().map(|ix| *ix as u64).collect::<Vec<_>>(), want.iter().map(|(seq, _)| *seq).collect::<Vec<_>>(), "{count}/{tail}");
        }
    }
}
