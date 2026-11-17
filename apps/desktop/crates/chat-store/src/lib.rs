//! #EDE-105 THE CHAT STORE (`packages/domain-contract/fixtures/chat-store.json`,
//! whose `$comment` is the spec; the TS reference is `src/chat-store.ts`).
//!
//! Every chat on every device, offline, opened from disk, plus a local search
//! index: the synced `messages` rows (opened with the chat key) and the
//! sessions' transcript events (from the device running them or another
//! reader, over eNet; never through the server).
//!
//! * The CORE ([`text`], [`docs_of_event`], [`collect_hits`], [`retain`],
//!   [`MemoryStore`]) is pure: no I/O, no clock, no async runtime. Phones
//!   (S3) and the web (S4, wasm + OPFS) link the same rules.
//! * [`sqlite::SqliteStore`] (feature `sqlite`) is the adapter the desktop
//!   uses: the same answers, the index an FTS5 trigram table over the folded
//!   text that only NARROWS the candidates the core then matches.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub mod catchup;
pub mod open;
pub mod text;
#[cfg(feature = "sqlite")]
pub mod sqlite;

pub use text::{HITS_MAX, QUERY_MAX_CHARS, SNIPPET_CHARS, SNIPPET_LEAD_CHARS};

/// RETENTION per form factor: days a phone or the web keeps non-prose events
/// (`None` = everything, the desktop).
pub const PHONE_RETENTION_DAYS: u32 = 30;
const DAY_MS: i64 = 86_400_000;

/// One transcript event as stored: the wire `activity` event and where it
/// came from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredEvent {
    pub root_id: String,
    pub session_id: String,
    /// The device that ran it: a session moved in place restarts its seqs
    /// on the new device.
    pub device_id: String,
    pub seq: u64,
    /// Ms the store took it (the doc's time when the event has no `at`).
    pub stored_at: i64,
    pub event: Value,
}

impl StoredEvent {
    pub fn kind(&self) -> &str {
        self.event.get("kind").and_then(Value::as_str).unwrap_or_default()
    }

    /// The event's `at`, else when it was stored.
    pub fn at(&self) -> i64 {
        self.event.get("at").and_then(Value::as_i64).unwrap_or(self.stored_at)
    }

    /// A `tool` event's id.
    pub fn tool_id(&self) -> Option<&str> {
        (self.kind() == "tool").then(|| self.event.get("id").and_then(Value::as_str)).flatten()
    }
}

/// One synced message as stored: `body` = the OPENED body, `None` while its
/// chat key is missing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredMessage {
    pub id: String,
    pub root_id: String,
    pub session_id: String,
    pub author_kind: String,
    pub body: Option<String>,
    pub created_at: i64,
}

/// What a hit points at: the row kind a reader draws it in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocKind {
    UserMessage,
    Narration,
    Tool,
    ToolOutput,
    Message,
}

impl DocKind {
    pub fn as_str(self) -> &'static str {
        match self {
            DocKind::UserMessage => "user_message",
            DocKind::Narration => "narration",
            DocKind::Tool => "tool",
            DocKind::ToolOutput => "tool_output",
            DocKind::Message => "message",
        }
    }

    pub fn parse(kind: &str) -> Option<DocKind> {
        Some(match kind {
            "user_message" => DocKind::UserMessage,
            "narration" => DocKind::Narration,
            "tool" => DocKind::Tool,
            "tool_output" => DocKind::ToolOutput,
            "message" => DocKind::Message,
            _ => return None,
        })
    }

    fn is_prose(self) -> bool {
        matches!(self, DocKind::UserMessage | DocKind::Narration)
    }
}

/// One searchable text (INDEX SCOPE).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Doc {
    pub root_id: String,
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    pub kind: DocKind,
    pub text: String,
    pub at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
}

fn string<'v>(event: &'v Value, key: &str) -> Option<&'v str> {
    event.get(key).and_then(Value::as_str)
}

/// INDEX SCOPE: the doc one stored event yields (never thoughts).
pub fn docs_of_event(stored: &StoredEvent) -> Option<Doc> {
    let event = &stored.event;
    let (kind, text, tool_id) = match stored.kind() {
        "user_message" => (DocKind::UserMessage, string(event, "text")?.to_string(), None),
        "narration" => (DocKind::Narration, string(event, "text")?.to_string(), None),
        "tool" => {
            let name = string(event, "name").unwrap_or_default();
            let text = match string(event, "detail").filter(|detail| !detail.is_empty()) {
                Some(detail) => format!("{name} {detail}"),
                None => name.to_string(),
            };
            (DocKind::Tool, text, None)
        }
        "tool_update" => {
            let text = string(event, "output").filter(|t| !t.is_empty()).or_else(|| string(event, "diff"))?;
            (DocKind::ToolOutput, text.to_string(), Some(string(event, "id")?.to_string()))
        }
        _ => return None,
    };
    if text.is_empty() || tool_id.as_deref() == Some("") {
        return None;
    }
    Some(Doc {
        root_id: stored.root_id.clone(),
        session_id: stored.session_id.clone(),
        device_id: Some(stored.device_id.clone()),
        kind,
        text,
        at: stored.at(),
        seq: Some(stored.seq),
        tool_id,
        message_id: None,
    })
}

/// INDEX SCOPE: a synced message's doc (an unopened or empty body = none).
pub fn docs_of_message(message: &StoredMessage) -> Option<Doc> {
    let body = message.body.as_deref().filter(|body| !body.is_empty())?;
    Some(Doc {
        root_id: message.root_id.clone(),
        session_id: message.session_id.clone(),
        device_id: None,
        kind: DocKind::Message,
        text: body.to_string(),
        at: message.created_at,
        seq: None,
        tool_id: None,
        message_id: Some(message.id.clone()),
    })
}

/// One match.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    pub root_id: String,
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    pub kind: DocKind,
    pub at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    pub snippet: String,
    pub match_start: usize,
    pub match_len: usize,
}

/// FIND IN CHAT's answer.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ChatResult {
    pub hits: Vec<Hit>,
    pub total: usize,
}

/// One GLOBAL row: a chat, how many hits it has, its newest.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatRow {
    pub root_id: String,
    pub count: usize,
    pub hit: Hit,
}

/// GLOBAL's answer.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AllResult {
    pub chats: Vec<ChatRow>,
    pub total: usize,
}

/// ORDER: newest first, then session id, then device id, then seq (higher first; a message
/// after the events of the same instant), then message id.
pub fn compare_hits(a: &Hit, b: &Hit) -> std::cmp::Ordering {
    let seq = |hit: &Hit| hit.seq.map_or(-1, |seq| seq as i128);
    b.at
        .cmp(&a.at)
        .then_with(|| a.session_id.cmp(&b.session_id))
        .then_with(|| a.device_id.as_deref().unwrap_or_default().cmp(b.device_id.as_deref().unwrap_or_default()))
        .then_with(|| seq(b).cmp(&seq(a)))
        .then_with(|| a.message_id.as_deref().unwrap_or_default().cmp(b.message_id.as_deref().unwrap_or_default()))
}

/// The hits of the folded `query` among `docs` (a tool's output lands on its
/// tool row via `tool_rows`: (session, device, tool id) → the lowest seq), deduped
/// and ordered. Callers may hand in only CANDIDATES, as long as every doc
/// that matches is among them.
pub fn collect_hits(docs: impl IntoIterator<Item = Doc>, tool_rows: &HashMap<ToolKey, u64>, query: &[char]) -> Vec<Hit> {
    let mut hits = Vec::new();
    let mut prose: HashSet<(String, String)> = HashSet::new();
    let mut messages = Vec::new();
    for doc in docs {
        let Some(found) = text::first_match(&doc.text, query) else { continue };
        let seq = match &doc.tool_id {
            Some(tool) => tool_rows
                .get(&(doc.session_id.clone(), doc.device_id.clone().unwrap_or_default(), tool.clone()))
                .copied()
                .or(doc.seq),
            None => doc.seq,
        };
        let hit = Hit {
            root_id: doc.root_id,
            session_id: doc.session_id,
            device_id: doc.device_id,
            kind: doc.kind,
            at: doc.at,
            seq,
            message_id: doc.message_id,
            snippet: found.snippet,
            match_start: found.match_start,
            match_len: found.match_len,
        };
        if doc.kind == DocKind::Message {
            messages.push((hit, text::collapse(&doc.text)));
        } else {
            if doc.kind.is_prose() {
                prose.insert((hit.session_id.clone(), text::collapse(&doc.text)));
            }
            hits.push(hit);
        }
    }
    for (hit, body) in messages {
        if !prose.contains(&(hit.session_id.clone(), body)) {
            hits.push(hit);
        }
    }
    hits.sort_by(compare_hits);
    hits
}

/// FIND IN CHAT over ordered `hits` of one chat.
pub fn chat_result(hits: Vec<Hit>, limit: usize) -> ChatResult {
    let total = hits.len();
    ChatResult { hits: hits.into_iter().take(text::clamp_limit(limit)).collect(), total }
}

/// GLOBAL over ordered `hits` of every chat.
pub fn all_result(hits: Vec<Hit>, limit: usize) -> AllResult {
    let mut rows: Vec<ChatRow> = Vec::new();
    let mut at: HashMap<String, usize> = HashMap::new();
    for hit in hits {
        match at.get(&hit.root_id) {
            Some(&ix) => rows[ix].count += 1,
            None => {
                at.insert(hit.root_id.clone(), rows.len());
                rows.push(ChatRow { root_id: hit.root_id.clone(), count: 1, hit });
            }
        }
    }
    let total = rows.len();
    rows.truncate(text::clamp_limit(limit));
    AllResult { chats: rows, total }
}

/// RETENTION: whether an event stays (`window_days` `None` = everything):
/// prose always, any other event only inside the window by its time.
pub fn retain(event: &StoredEvent, now_ms: i64, window_days: Option<u32>) -> bool {
    match window_days {
        None => true,
        Some(days) => {
            matches!(event.kind(), "user_message" | "narration") || event.at() >= now_ms - i64::from(days) * DAY_MS
        }
    }
}

/// (session, device, tool id): where a tool's output lands.
pub type ToolKey = (String, String, String);

/// The pure store (the reference the SQLite adapter answers like): DEDUP
/// events by (session, device, seq), first write wins; messages by id,
/// latest wins.
#[derive(Clone, Debug, Default)]
pub struct MemoryStore {
    events: BTreeMap<(String, String, u64), StoredEvent>,
    messages: BTreeMap<String, StoredMessage>,
}

impl MemoryStore {
    pub fn new() -> MemoryStore {
        MemoryStore::default()
    }

    /// Stores what it does not hold yet; returns how many events were new.
    pub fn put_events(&mut self, events: impl IntoIterator<Item = StoredEvent>) -> usize {
        let mut added = 0;
        for event in events {
            let key = (event.session_id.clone(), event.device_id.clone(), event.seq);
            if let std::collections::btree_map::Entry::Vacant(slot) = self.events.entry(key) {
                slot.insert(event);
                added += 1;
            }
        }
        added
    }

    pub fn put_messages(&mut self, messages: impl IntoIterator<Item = StoredMessage>) {
        for message in messages {
            self.messages.insert(message.id.clone(), message);
        }
    }

    /// Every stored event, per session and device in seq order.
    pub fn events(&self) -> impl Iterator<Item = &StoredEvent> {
        self.events.values()
    }

    pub fn messages(&self) -> impl Iterator<Item = &StoredMessage> {
        self.messages.values()
    }

    fn hits(&self, query: &[char], root_id: Option<&str>) -> Vec<Hit> {
        let mut tool_rows: HashMap<ToolKey, u64> = HashMap::new();
        for event in self.events.values() {
            if let Some(id) = event.tool_id() {
                let key = (event.session_id.clone(), event.device_id.clone(), id.to_string());
                let row = tool_rows.entry(key).or_insert(event.seq);
                *row = (*row).min(event.seq);
            }
        }
        let docs = self
            .events
            .values()
            .filter_map(docs_of_event)
            .chain(self.messages.values().filter_map(docs_of_message))
            .filter(|doc| root_id.is_none_or(|root| doc.root_id == root));
        collect_hits(docs, &tool_rows, query)
    }

    /// FIND IN CHAT.
    pub fn search_chat(&self, root_id: &str, query: &str, limit: usize) -> ChatResult {
        let query = text::fold_query(query);
        if query.is_empty() {
            return ChatResult::default();
        }
        chat_result(self.hits(&query, Some(root_id)), limit)
    }

    /// GLOBAL.
    pub fn search_all(&self, query: &str, limit: usize) -> AllResult {
        let query = text::fold_query(query);
        if query.is_empty() {
            return AllResult::default();
        }
        all_result(self.hits(&query, None), limit)
    }

    /// Applies RETENTION; returns how many events went.
    pub fn apply_retention(&mut self, now_ms: i64, window_days: Option<u32>) -> usize {
        let before = self.events.len();
        self.events.retain(|_, event| retain(event, now_ms, window_days));
        before - self.events.len()
    }

    /// One stored message.
    pub fn message(&self, id: &str) -> Option<&StoredMessage> {
        self.messages.get(id)
    }

    /// Whether the event at (session, device, seq) is held.
    pub fn has_event(&self, session_id: &str, device_id: &str, seq: u64) -> bool {
        self.events.contains_key(&(session_id.to_string(), device_id.to_string(), seq))
    }

    fn session_range<'s>(&'s self, session_id: &str, device_id: &str) -> impl DoubleEndedIterator<Item = &'s StoredEvent> + 's {
        let (s, d) = (session_id.to_string(), device_id.to_string());
        self.events.range((s.clone(), d.clone(), 0)..=(s, d, u64::MAX)).map(|(_, event)| event)
    }

    /// A session's events on `device_id` in seq order (`before` = only below
    /// that seq; the newest `limit` of them when given): the SQLite
    /// adapter's `read_events`.
    pub fn read_events(&self, session_id: &str, device_id: &str, before: Option<u64>, limit: Option<usize>) -> Vec<&StoredEvent> {
        let mut out: Vec<&StoredEvent> = self
            .session_range(session_id, device_id)
            .rev()
            .filter(|event| before.is_none_or(|before| event.seq < before))
            .take(limit.unwrap_or(usize::MAX))
            .collect();
        out.reverse();
        out
    }

    /// The asker's `have` (`catchUp`): the runs of held seqs above `after_seq`.
    pub fn have_runs(&self, session_id: &str, device_id: &str, after_seq: Option<u64>, max: usize) -> Vec<catchup::SeqRun> {
        catchup::have_runs(self.session_range(session_id, device_id).map(|event| event.seq), after_seq, max)
    }

    /// THE HOLDER's page off this store (`full_to` = its own `completeTo`):
    /// the answer and the events it takes, in seq order.
    pub fn answer_page(&self, session_id: &str, device_id: &str, ask: &catchup::Ask, full_to: Option<u64>) -> (catchup::Answer, Vec<&StoredEvent>) {
        let held: Vec<&StoredEvent> = self.session_range(session_id, device_id).collect();
        let answer = catchup::answer(held.iter().map(|event| (event.seq, event.kind(), event.at())), full_to, ask);
        let taken: HashSet<u64> = answer.seqs.iter().copied().collect();
        let events = held.into_iter().filter(|event| taken.contains(&event.seq)).collect();
        (answer, events)
    }
}

#[cfg(test)]
pub(crate) mod fixture {
    use serde_json::Value;

    pub const FIXTURE: &str = include_str!("../../../../../packages/domain-contract/fixtures/chat-store.json");

    pub fn spec() -> Value {
        serde_json::from_str(FIXTURE).unwrap()
    }

    pub fn from<T: serde::de::DeserializeOwned>(value: &Value) -> T {
        serde_json::from_value(value.clone()).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::fixture::{from, spec};
    use super::*;
    use serde_json::json;

    #[test]
    fn constants_match_the_contract() {
        let f = spec();
        let c = &f["constants"];
        assert_eq!(c["queryMaxChars"], QUERY_MAX_CHARS as u64);
        assert_eq!(c["hitsMax"], HITS_MAX as u64);
        assert_eq!(c["snippetChars"], SNIPPET_CHARS as u64);
        assert_eq!(c["snippetLeadChars"], SNIPPET_LEAD_CHARS as u64);
        assert_eq!(c["retentionDays"], json!({ "desktop": null, "phone": PHONE_RETENTION_DAYS, "web": PHONE_RETENTION_DAYS }));
    }

    #[test]
    fn docs_cases() {
        let f = spec();
        for case in f["docs"]["cases"].as_array().unwrap() {
            let docs: Vec<Doc> = docs_of_event(&from(&case["event"])).into_iter().collect();
            assert_eq!(serde_json::to_value(docs).unwrap(), case["expect"], "{}", case["name"]);
        }
        for case in f["docs"]["messageCases"].as_array().unwrap() {
            let docs: Vec<Doc> = docs_of_message(&from(&case["message"])).into_iter().collect();
            assert_eq!(serde_json::to_value(docs).unwrap(), case["expect"], "{}", case["name"]);
        }
    }

    #[test]
    fn ingest_cases() {
        for case in spec()["ingest"]["cases"].as_array().unwrap() {
            let mut store = MemoryStore::new();
            for batch in case["batches"].as_array().unwrap() {
                if let Some(events) = batch.get("events") {
                    store.put_events(from::<Vec<StoredEvent>>(events));
                }
                if let Some(messages) = batch.get("messages") {
                    store.put_messages(from::<Vec<StoredMessage>>(messages));
                }
            }
            let got = json!({
                "events": store.events().map(|e| json!({ "sessionId": e.session_id, "deviceId": e.device_id, "seq": e.seq, "text": e.event.get("text") })).collect::<Vec<_>>(),
                "messages": store.messages().map(|m| json!({ "id": m.id, "body": m.body })).collect::<Vec<_>>(),
            });
            assert_eq!(got, case["expect"], "{}", case["name"]);
        }
    }

    fn search_store() -> MemoryStore {
        let f = spec();
        let mut store = MemoryStore::new();
        store.put_events(from::<Vec<StoredEvent>>(&f["search"]["state"]["events"]));
        store.put_messages(from::<Vec<StoredMessage>>(&f["search"]["state"]["messages"]));
        store
    }

    #[test]
    fn search_cases() {
        let f = spec();
        let store = search_store();
        for case in f["search"]["chatCases"].as_array().unwrap() {
            let got = store.search_chat(case["rootId"].as_str().unwrap(), case["query"].as_str().unwrap(), case["limit"].as_u64().unwrap() as usize);
            assert_eq!(serde_json::to_value(got).unwrap(), case["expect"], "{}", case["name"]);
        }
        for case in f["search"]["allCases"].as_array().unwrap() {
            let got = store.search_all(case["query"].as_str().unwrap(), case["limit"].as_u64().unwrap() as usize);
            assert_eq!(serde_json::to_value(got).unwrap(), case["expect"], "{}", case["name"]);
        }
    }

    #[test]
    fn memory_store_reads_and_answers_like_the_rules() {
        for case in spec()["catchUp"]["answerCases"].as_array().unwrap() {
            let mut store = MemoryStore::new();
            store.put_events(crate::catchup::tests::case_events(case).into_iter().map(|(seq, kind, at)| StoredEvent {
                root_id: "r".into(),
                session_id: "s".into(),
                device_id: "d".into(),
                seq,
                stored_at: at,
                event: json!({ "kind": kind, "at": at }),
            }));
            let ask: catchup::Ask = from(&case["ask"]);
            let (answer, taken) = store.answer_page("s", "d", &ask, from(&case["fullTo"]));
            assert_eq!(serde_json::to_value(&answer).unwrap(), case["expect"], "{}", case["name"]);
            assert_eq!(taken.iter().map(|event| event.seq).collect::<Vec<_>>(), answer.seqs, "{}", case["name"]);
            assert!(store.answer_page("s", "other", &ask, None).1.is_empty());
        }
        let mut store = MemoryStore::new();
        let event = |session: &str, seq: u64| StoredEvent {
            root_id: "r".into(),
            session_id: session.into(),
            device_id: "d".into(),
            seq,
            stored_at: 0,
            event: json!({ "kind": "narration", "text": "x" }),
        };
        store.put_events([1, 2, 3, 5, 8].map(|seq| event("s", seq)));
        store.put_events([4].map(|seq| event("s2", seq)));
        let seqs = |events: Vec<&StoredEvent>| events.iter().map(|event| event.seq).collect::<Vec<_>>();
        assert_eq!(seqs(store.read_events("s", "d", None, None)), [1, 2, 3, 5, 8]);
        assert_eq!(seqs(store.read_events("s", "d", Some(5), Some(2))), [2, 3]);
        assert_eq!(store.have_runs("s", "d", Some(1), 64), vec![[2, 3], [5, 5], [8, 8]]);
        assert!(store.has_event("s2", "d", 4) && !store.has_event("s", "d", 4));
    }

    #[test]
    fn retention_cases() {
        let f = spec();
        let events: Vec<StoredEvent> = from(&f["retention"]["events"]);
        for case in f["retention"]["cases"].as_array().unwrap() {
            let mut store = MemoryStore::new();
            store.put_events(events.clone());
            store.apply_retention(case["now"].as_i64().unwrap(), case["windowDays"].as_u64().map(|d| d as u32));
            let kept: Vec<u64> = store.events().map(|e| e.seq).collect();
            assert_eq!(json!(kept), case["expect"], "{}", case["name"]);
        }
    }
}
