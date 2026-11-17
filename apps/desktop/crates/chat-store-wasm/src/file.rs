//! One chat's store as the web keeps it: a [`MemoryStore`] plus the
//! catch-up bookkeeping, persisted as an APPEND-ONLY LOG of JSON lines (one
//! OPFS file per chat). Pure: no I/O, no clock; the worker reads and writes
//! the file and passes the time in.
//!
//! LINES: `{"e": StoredEvent}`, `{"m": StoredMessage}`, `{"c": [sessionId,
//! deviceId, seq]}` (a `completeTo` watermark), `{"k": [key, value]}` (meta).
//! A torn or unknown line is skipped (a crash mid-append costs one record).

use std::collections::BTreeMap;

use chat_store::catchup::{self, Answer, Ask};
use chat_store::{open, MemoryStore, StoredEvent, StoredMessage};
use serde_json::{json, Value};

#[derive(Default)]
pub struct ChatFile {
    store: MemoryStore,
    complete: BTreeMap<(String, String), u64>,
    meta: BTreeMap<String, String>,
    /// Lines the file holds (a rewrite is due when they far outnumber the
    /// records: superseded messages, watermarks, retention).
    lines: usize,
}

fn line(value: Value) -> String {
    let mut out = value.to_string();
    out.push('\n');
    out
}

impl ChatFile {
    pub fn new() -> ChatFile {
        ChatFile::default()
    }

    /// Replays the log; returns how many lines it took.
    pub fn load(&mut self, text: &str) -> usize {
        let mut taken = 0;
        for raw in text.lines() {
            let Ok(value) = serde_json::from_str::<Value>(raw) else { continue };
            if self.apply(&value) {
                taken += 1;
            }
        }
        self.lines += taken;
        taken
    }

    fn apply(&mut self, value: &Value) -> bool {
        if let Some(event) = value.get("e") {
            let Ok(event) = serde_json::from_value::<StoredEvent>(event.clone()) else { return false };
            self.store.put_events([event]);
        } else if let Some(message) = value.get("m") {
            let Ok(message) = serde_json::from_value::<StoredMessage>(message.clone()) else { return false };
            self.take_message(message);
        } else if let Some(complete) = value.get("c") {
            let Ok((session, device, seq)) = serde_json::from_value::<(String, String, u64)>(complete.clone()) else { return false };
            self.complete.insert((session, device), seq);
        } else if let Some((key, val)) = value.get("k").and_then(|k| serde_json::from_value::<(String, String)>(k.clone()).ok()) {
            self.meta.insert(key, val);
        } else {
            return false;
        }
        true
    }

    /// Latest wins, by id; a body that did not open HERE never replaces
    /// one that did (another tab, or this one before its key loaded).
    fn take_message(&mut self, message: StoredMessage) -> bool {
        if let Some(held) = self.store.message(&message.id) {
            if *held == message || (message.body.is_none() && held.body.is_some()) {
                return false;
            }
        }
        self.store.put_messages([message]);
        true
    }

    fn appended(&mut self, out: Vec<String>) -> String {
        self.lines += out.len();
        out.concat()
    }

    /// Stores what it does not hold yet; returns the lines to append.
    pub fn put_events(&mut self, events: Vec<StoredEvent>) -> String {
        let mut out = Vec::new();
        for event in events {
            if self.store.has_event(&event.session_id, &event.device_id, event.seq) {
                continue;
            }
            out.push(line(json!({ "e": event })));
            self.store.put_events([event]);
        }
        self.appended(out)
    }

    pub fn put_messages(&mut self, messages: Vec<StoredMessage>) -> String {
        let mut out = Vec::new();
        for message in messages {
            let record = line(json!({ "m": message }));
            if self.take_message(message) {
                out.push(record);
            }
        }
        self.appended(out)
    }

    pub fn complete_to(&self, session_id: &str, device_id: &str) -> Option<u64> {
        self.complete.get(&(session_id.to_string(), device_id.to_string())).copied()
    }

    pub fn set_complete_to(&mut self, session_id: &str, device_id: &str, seq: u64) -> String {
        if self.complete_to(session_id, device_id) == Some(seq) {
            return String::new();
        }
        self.complete.insert((session_id.to_string(), device_id.to_string()), seq);
        self.appended(vec![line(json!({ "c": [session_id, device_id, seq] }))])
    }

    pub fn meta(&self, key: &str) -> Option<&str> {
        self.meta.get(key).map(String::as_str)
    }

    pub fn set_meta(&mut self, key: &str, value: &str) -> String {
        if self.meta(key) == Some(value) {
            return String::new();
        }
        self.meta.insert(key.to_string(), value.to_string());
        self.appended(vec![line(json!({ "k": [key, value] }))])
    }

    /// Records held (what a rewrite writes).
    pub fn records(&self) -> usize {
        self.store.events().count() + self.store.messages().count() + self.complete.len() + self.meta.len()
    }

    pub fn lines(&self) -> usize {
        self.lines
    }

    /// The whole log, one line per record (a rewrite).
    pub fn snapshot(&mut self) -> String {
        let mut out: Vec<String> = Vec::new();
        out.extend(self.store.events().map(|event| line(json!({ "e": event }))));
        out.extend(self.store.messages().map(|message| line(json!({ "m": message }))));
        out.extend(self.complete.iter().map(|((s, d), seq)| line(json!({ "c": [s, d, seq] }))));
        out.extend(self.meta.iter().map(|(k, v)| line(json!({ "k": [k, v] }))));
        self.lines = out.len();
        out.concat()
    }

    /// RETENTION (`window_days` `None` = keep everything); returns how many
    /// events went (the caller rewrites the file when any did).
    /// FIND IN CHAT over this chat.
    pub fn search_chat(&self, root_id: &str, query: &str, limit: usize) -> chat_store::ChatResult {
        self.store.search_chat(root_id, query, limit)
    }

    pub fn apply_retention(&mut self, now_ms: i64, window_days: Option<u32>) -> usize {
        self.store.apply_retention(now_ms, window_days)
    }

    /// OPEN: the session's stored events on `device_id` through the REPLAY
    /// TAIL: `{events: [{seq, event}], truncated}` (none = empty).
    pub fn open_tail(&self, session_id: &str, device_id: &str, tail: usize) -> Value {
        let held = self.store.read_events(session_id, device_id, None, None);
        let wire: Vec<&Value> = held.iter().map(|event| &event.event).collect();
        let (kept, truncated) = open::replay_tail(&wire, tail);
        let events: Vec<Value> = kept.into_iter().map(|ix| json!({ "seq": held[ix].seq, "event": held[ix].event })).collect();
        json!({ "events": events, "truncated": truncated })
    }

    /// An older page: the newest `limit` events below `before`, oldest
    /// first, `{events, seqs}` like a `history_chunk`.
    pub fn page(&self, session_id: &str, device_id: &str, before: u64, limit: usize) -> Value {
        let held = self.store.read_events(session_id, device_id, Some(before), Some(limit));
        json!({
            "events": held.iter().map(|event| &event.event).collect::<Vec<_>>(),
            "seqs": held.iter().map(|event| event.seq).collect::<Vec<_>>(),
        })
    }

    /// The asker's `have` above `after_seq`.
    pub fn have_runs(&self, session_id: &str, device_id: &str, after_seq: Option<u64>) -> Vec<catchup::SeqRun> {
        self.store.have_runs(session_id, device_id, after_seq, catchup::HAVE_RUNS_MAX)
    }

    /// THE HOLDER: one `history_chunk` frame answering a device-naming
    /// `history_page` off this store (`fullTo` = its own `completeTo`), cut
    /// from its end to fit `max_bytes`.
    pub fn answer_chunk(&self, session_id: &str, page: &Value, max_bytes: usize) -> Option<String> {
        let request_id = page.get("requestId")?.as_str()?;
        let device_id = page.get("deviceId")?.as_str().filter(|id| !id.is_empty())?;
        let ask: Ask = serde_json::from_value(page.clone()).ok()?;
        let (answer, events) = self.store.answer_page(session_id, device_id, &ask, self.complete_to(session_id, device_id));
        let events: Vec<(u64, &Value)> = events.iter().map(|event| (event.seq, &event.event)).collect();
        Some(chunk_frame(request_id, device_id, &events, &answer, None, max_bytes))
    }

    /// THE ASKER takes one answer to an ask from `after_seq`: stores its
    /// events (`stored_at` = now), moves `completeTo` (PROGRESS) and says
    /// where the walk goes next: `{append, added, completeTo, next, end}`
    /// (`end` = this holder's walk is over).
    pub fn take(&mut self, root_id: &str, session_id: &str, device_id: &str, after_seq: Option<u64>, chunk: &Value, now_ms: i64) -> Value {
        let seqs: Vec<u64> = serde_json::from_value(chunk.get("seqs").cloned().unwrap_or_default()).unwrap_or_default();
        let events: Vec<Value> = serde_json::from_value(chunk.get("events").cloned().unwrap_or_default()).unwrap_or_default();
        let opt = |key: &str| chunk.get(key).and_then(Value::as_u64);
        let done = chunk.get("done").and_then(Value::as_bool).unwrap_or(true);
        let stored: Vec<StoredEvent> = seqs
            .iter()
            .zip(events)
            .map(|(seq, event)| StoredEvent {
                root_id: root_id.to_string(),
                session_id: session_id.to_string(),
                device_id: device_id.to_string(),
                seq: *seq,
                stored_at: now_ms,
                event,
            })
            .collect();
        let before = self.lines;
        let mut append = self.put_events(stored);
        let added = self.lines - before;
        let complete = self.complete_to(session_id, device_id);
        let (next_complete, next) = catchup::progress(complete, after_seq, opt("throughSeq"), done, opt("fullTo"));
        if next_complete != complete {
            if let Some(seq) = next_complete {
                append.push_str(&self.set_complete_to(session_id, device_id, seq));
            }
        }
        // A holder that does not move: done with it.
        let end = match next {
            Some(after) => after == after_seq && seqs.is_empty(),
            None => true,
        };
        json!({ "append": append, "added": added, "completeTo": next_complete, "next": next.flatten(), "end": end })
    }
}

/// ONE `history_chunk`, cut from its end to fit `max_bytes` (an event too
/// large alone is skipped: the walk moves past it).
pub fn chunk_frame(request_id: &str, device_id: &str, events: &[(u64, &Value)], answer: &Answer, error: Option<&str>, max_bytes: usize) -> String {
    let budget = max_bytes.saturating_sub(512);
    let mut kept: Vec<(u64, &Value)> = Vec::new();
    let mut size = 0usize;
    let mut answer = answer.clone();
    for (seq, event) in events {
        let len = event.to_string().len() + 24;
        if size + len > budget {
            answer.through_seq = if kept.is_empty() { Some(*seq) } else { kept.last().map(|(seq, _)| *seq) };
            answer.done = answer.last_seq.is_none_or(|last| answer.through_seq.is_some_and(|through| through >= last));
            break;
        }
        size += len;
        kept.push((*seq, event));
    }
    let mut frame = json!({
        "t": "history_chunk",
        "requestId": request_id,
        "deviceId": device_id,
        "seqs": kept.iter().map(|(seq, _)| *seq).collect::<Vec<_>>(),
        "events": kept.iter().map(|(_, event)| *event).collect::<Vec<_>>(),
        "done": answer.done,
        "throughSeq": answer.through_seq,
        "lastSeq": answer.last_seq,
        "fullTo": answer.full_to,
    });
    if let Some(code) = error {
        frame["error"] = json!(code);
    }
    frame.to_string()
}

/// A refusal: the same chunk empty, with `error`.
pub fn refusal_frame(page: &Value, code: &str) -> Option<String> {
    let request_id = page.get("requestId")?.as_str()?;
    let device_id = page.get("deviceId")?.as_str()?;
    let ask: Ask = serde_json::from_value(page.clone()).ok()?;
    let empty = catchup::finish(Vec::new(), ask.limit(), None, None, &ask);
    Some(chunk_frame(request_id, device_id, &[], &empty, Some(code), usize::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(session: &str, seq: u64, event: Value) -> StoredEvent {
        StoredEvent { root_id: "r".into(), session_id: session.into(), device_id: "d".into(), seq, stored_at: 0, event }
    }

    fn message(id: &str, body: Option<&str>) -> StoredMessage {
        StoredMessage { id: id.into(), root_id: "r".into(), session_id: "s".into(), author_kind: "agent".into(), body: body.map(Into::into), created_at: 5 }
    }

    #[test]
    fn the_log_round_trips_and_appends_only_what_changed() {
        let mut file = ChatFile::new();
        let mut log = file.put_events(vec![event("s", 1, json!({ "kind": "user_message", "text": "Fix the café" })), event("s", 2, json!({ "kind": "narration", "text": "Done." }))]);
        assert_eq!(log.lines().count(), 2);
        assert_eq!(file.put_events(vec![event("s", 2, json!({ "kind": "narration", "text": "a repeat" }))]), "", "first write wins");
        log += &file.put_messages(vec![message("m1", None)]);
        log += &file.put_messages(vec![message("m1", Some("opened"))]);
        assert_eq!(file.put_messages(vec![message("m1", None)]), "", "a body that did not open never replaces one that did");
        assert_eq!(file.put_messages(vec![message("m1", Some("opened"))]), "");
        log += &file.set_complete_to("s", "d", 2);
        assert_eq!(file.set_complete_to("s", "d", 2), "");
        log += &file.set_meta("checked.s", "d|t");
        log += "{\"e\":{\"torn\""; // a crash mid-append

        let mut again = ChatFile::new();
        assert_eq!(again.load(&log), 6);
        assert_eq!(again.complete_to("s", "d"), Some(2));
        assert_eq!(again.meta("checked.s"), Some("d|t"));
        assert_eq!(again.store.message("m1").unwrap().body.as_deref(), Some("opened"));
        let tail = again.open_tail("s", "d", 50);
        assert_eq!(tail["truncated"], false);
        assert_eq!(tail["events"][1], json!({ "seq": 2, "event": { "kind": "narration", "text": "Done." } }));
        assert_eq!(again.records(), 5);
        let snapshot = again.snapshot();
        assert_eq!(snapshot.lines().count(), 5);
        assert_eq!(again.lines(), 5);
    }

    #[test]
    fn retention_keeps_prose_and_fresh_steps() {
        let mut file = ChatFile::new();
        let day = 86_400_000;
        file.put_events(vec![
            event("s", 1, json!({ "kind": "user_message", "text": "old", "at": 0 })),
            event("s", 2, json!({ "kind": "tool", "name": "Bash", "id": "t", "at": 0 })),
            event("s", 3, json!({ "kind": "tool", "name": "Read", "id": "u", "at": 40 * day })),
        ]);
        assert_eq!(file.apply_retention(60 * day, Some(chat_store::PHONE_RETENTION_DAYS)), 1);
        let seqs: Vec<u64> = file.store.events().map(|event| event.seq).collect();
        assert_eq!(seqs, [1, 3]);
    }

    #[test]
    fn a_walk_over_a_holder_fills_the_store() {
        // The holder: a store with seqs 1..=450 complete through 450.
        let mut holder = ChatFile::new();
        holder.put_events((1..=450).map(|seq| event("s", seq, json!({ "kind": "narration", "text": format!("n{seq}") }))).collect());
        holder.set_complete_to("s", "d", 450);
        // The asker already holds 100..=120.
        let mut asker = ChatFile::new();
        asker.put_events((100..=120).map(|seq| event("s", seq, json!({ "kind": "narration", "text": format!("n{seq}") }))).collect());
        let mut cursor: Option<u64> = None;
        let mut asks = 0;
        loop {
            asks += 1;
            let page = json!({ "t": "history_page", "requestId": format!("q{asks}"), "deviceId": "d", "afterSeq": cursor, "limit": catchup::PAGE_MAX, "have": asker.have_runs("s", "d", cursor) });
            let chunk: Value = serde_json::from_str(&holder.answer_chunk("s", &page, 1 << 20).unwrap()).unwrap();
            let took = asker.take("r", "s", "d", cursor, &chunk, 7);
            if took["end"] == true {
                break;
            }
            cursor = took["next"].as_u64();
        }
        assert_eq!(asks, 3);
        assert_eq!(asker.store.events().count(), 450);
        assert_eq!(asker.complete_to("s", "d"), Some(450));
        // A refused page is an empty chunk with the error.
        let refused: Value = serde_json::from_str(&refusal_frame(&json!({ "requestId": "x", "deviceId": "d", "limit": 5, "have": [] }), "unauthorized").unwrap()).unwrap();
        assert_eq!(refused["error"], "unauthorized");
        assert_eq!(refused["seqs"], json!([]));
    }

    #[test]
    fn an_oversized_page_is_cut_from_its_end() {
        let mut holder = ChatFile::new();
        holder.put_events((1..=10).map(|seq| event("s", seq, json!({ "kind": "narration", "text": "x".repeat(400) }))).collect());
        let page = json!({ "requestId": "q", "deviceId": "d", "limit": 10, "have": [] });
        let chunk: Value = serde_json::from_str(&holder.answer_chunk("s", &page, 2000).unwrap()).unwrap();
        let kept = chunk["seqs"].as_array().unwrap().len();
        assert!(kept > 0 && kept < 10);
        assert_eq!(chunk["throughSeq"], json!(kept));
        assert_eq!(chunk["done"], false);
    }
}
