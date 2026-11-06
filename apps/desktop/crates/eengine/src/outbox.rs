//! VAPP-235 (O1) THE OFFLINE OUTBOX, the Engine's (VAPP-261,
//! docs/spec/apps.md §14, `packages/eengine/fixtures/outbox.json` = a `$ref`
//! to `packages/domain-contract/fixtures/app-records.json` `outbox`): a host
//! with a sync provider queues every record write, lays it over the synced
//! rows and replays it in order with an idempotency key; pure, so every host
//! replays the same cases (TS `@exp/domain-contract/app-records`). Moved here
//! from the desktop's `domain::app_records`, which re-exports it.

use std::collections::HashMap;
use std::sync::LazyLock;

use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::contract::APP_RECORDS;

/// outbox.json `modeCases`: a host with a sync provider queues, one without writes through.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutboxMode {
    Queue,
    Direct,
}

impl OutboxMode {
    pub fn of(has_sync: bool) -> Self {
        if has_sync {
            OutboxMode::Queue
        } else {
            OutboxMode::Direct
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            OutboxMode::Queue => "queue",
            OutboxMode::Direct => "direct",
        }
    }
}

/// Deep JSON equality (objects by key, numbers by value: `2` equals `2.0`).
pub fn json_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x == y || x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(a, b)| json_equal(a, b)),
        (Value::Object(x), Value::Object(y)) => x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| json_equal(v, w))),
        _ => a == b,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutboxCopy {
    /// `1 change waiting to sync`.
    pub pending_one: String,
    /// `{n} changes waiting to sync`.
    pub pending: String,
}

/// `outbox` minus its cases.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutboxSpec {
    pub ack_grace_seconds: u64,
    pub retry_codes: Vec<String>,
    pub copy: OutboxCopy,
}

pub static APP_RECORDS_OUTBOX: LazyLock<OutboxSpec> =
    LazyLock::new(|| serde_json::from_value(APP_RECORDS["outbox"].clone()).expect("app-records.json outbox parses"));

/// The host call an entry came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutboxCall {
    Upsert,
    Delete,
    Batch,
}

impl OutboxCall {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Upsert => "upsert",
            Self::Delete => "delete",
            Self::Batch => "batch",
        }
    }

    pub fn parse(call: &str) -> Option<Self> {
        match call {
            "upsert" => Some(Self::Upsert),
            "delete" => Some(Self::Delete),
            "batch" => Some(Self::Batch),
            _ => None,
        }
    }
}

/// One write of an entry: a create names its CLIENT id.
#[derive(Debug, Clone, PartialEq, serde::Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum OutboxWrite {
    Create {
        id: String,
        kind: String,
        #[serde(default)]
        data: Map<String, Value>,
    },
    Update {
        id: String,
        kind: String,
        #[serde(default)]
        data: Map<String, Value>,
    },
    Delete {
        id: String,
    },
}

impl OutboxWrite {
    pub fn id(&self) -> &str {
        match self {
            Self::Create { id, .. } | Self::Update { id, .. } | Self::Delete { id } => id,
        }
    }

    fn is_delete(&self) -> bool {
        matches!(self, Self::Delete { .. })
    }

    fn is_create(&self) -> bool {
        matches!(self, Self::Create { .. })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutboxState {
    Pending,
    Acked,
}

impl OutboxState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Acked => "acked",
        }
    }

    pub fn parse(state: &str) -> Option<Self> {
        match state {
            "pending" => Some(Self::Pending),
            "acked" => Some(Self::Acked),
            _ => None,
        }
    }
}

/// One queued host call (`{writeId, appId, call, writes, at, state, ackedAt?}`).
#[derive(Debug, Clone, PartialEq, serde::Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutboxEntry {
    pub write_id: String,
    pub app_id: String,
    pub call: OutboxCall,
    pub writes: Vec<OutboxWrite>,
    /// ISO time the write was made.
    pub at: String,
    pub state: OutboxState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acked_at: Option<String>,
}

impl OutboxEntry {
    pub fn is_pending(&self) -> bool {
        self.state == OutboxState::Pending
    }

    /// What the host call answers from the entry (offline): `upsert` →
    /// `{id}`, `delete` → null, `batch` → `{ids}`.
    pub fn answer(&self) -> Value {
        match self.call {
            OutboxCall::Upsert => json!({ "id": self.writes.first().map(OutboxWrite::id) }),
            OutboxCall::Delete => Value::Null,
            OutboxCall::Batch => json!({ "ids": self.writes.iter().map(OutboxWrite::id).collect::<Vec<_>>() }),
        }
    }
}

/// A replay's answer: ok, no network, or the server's tRPC code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutboxResult {
    Ok,
    Transport,
    Code(String),
}

impl OutboxResult {
    /// The fixture's spelling (`{ok: true}` | `{transport: true}` | `{code}`).
    pub fn from_value(value: &Value) -> Self {
        if value.get("ok").and_then(Value::as_bool) == Some(true) {
            Self::Ok
        } else if let Some(code) = value.get("code").and_then(Value::as_str) {
            Self::Code(code.to_string())
        } else {
            Self::Transport
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutboxOutcome {
    Done,
    Retry,
    Dropped,
    Refused,
}

impl OutboxOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Done => "done",
            Self::Retry => "retry",
            Self::Dropped => "dropped",
            Self::Refused => "refused",
        }
    }
}

/// One record as the overlay reads it (the fixture's row).
#[derive(Debug, Clone, PartialEq, serde::Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutboxRow {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub data: Map<String, Value>,
    #[serde(default)]
    pub created_by: Option<String>,
    #[serde(default)]
    pub author_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// A host call's writes (`writesCases`): a create takes the next client id.
pub fn outbox_writes(call: OutboxCall, args: &Value, new_ids: &[String]) -> Vec<OutboxWrite> {
    let mut next = 0;
    let mut upsert = |w: &Value| -> OutboxWrite {
        let data = w.get("data").and_then(Value::as_object).cloned().unwrap_or_default();
        let kind = w.get("kind").and_then(Value::as_str).unwrap_or_default().to_string();
        match w.get("id").and_then(Value::as_str) {
            Some(id) => OutboxWrite::Update { id: id.to_string(), kind, data },
            None => {
                let id = new_ids.get(next).cloned().unwrap_or_default();
                next += 1;
                OutboxWrite::Create { id, kind, data }
            }
        }
    };
    let delete = |w: &Value| OutboxWrite::Delete { id: w.get("id").and_then(Value::as_str).unwrap_or_default().to_string() };
    match call {
        OutboxCall::Upsert => vec![upsert(args)],
        OutboxCall::Delete => vec![delete(args)],
        OutboxCall::Batch => args
            .get("writes")
            .and_then(Value::as_array)
            .map(|writes| {
                writes
                    .iter()
                    .filter(|w| w.is_object())
                    .map(|w| if w.get("op").and_then(Value::as_str) == Some("delete") { delete(w) } else { upsert(w) })
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// How many client ids a host call may take (an upper bound for
/// [`outbox_writes`]'s `new_ids`).
pub fn outbox_new_id_count(call: OutboxCall, args: &Value) -> usize {
    match call {
        OutboxCall::Upsert => 1,
        OutboxCall::Delete => 0,
        OutboxCall::Batch => args.get("writes").and_then(Value::as_array).map_or(0, Vec::len),
    }
}

/// The replay request of an entry (`requestCases`): the call and its input
/// (`writeId` beside it); the host adds the app.
pub fn outbox_request(entry: &OutboxEntry) -> (OutboxCall, Value) {
    let write_id = entry.write_id.clone();
    if entry.call == OutboxCall::Batch {
        let writes: Vec<Value> = entry
            .writes
            .iter()
            .map(|w| match w {
                OutboxWrite::Delete { id } => json!({"op": "delete", "id": id}),
                OutboxWrite::Create { id, kind, data } => json!({"op": "upsert", "kind": kind, "data": data, "newId": id}),
                OutboxWrite::Update { id, kind, data } => json!({"op": "upsert", "kind": kind, "data": data, "id": id}),
            })
            .collect();
        return (OutboxCall::Batch, json!({"writes": writes, "writeId": write_id}));
    }
    match entry.writes.first() {
        Some(OutboxWrite::Delete { id }) => (OutboxCall::Delete, json!({"recordId": id, "writeId": write_id})),
        Some(OutboxWrite::Create { id, kind, data }) => {
            (OutboxCall::Upsert, json!({"kind": kind, "data": data, "newId": id, "writeId": write_id}))
        }
        Some(OutboxWrite::Update { id, kind, data }) => {
            (OutboxCall::Upsert, json!({"kind": kind, "data": data, "recordId": id, "writeId": write_id}))
        }
        None => (entry.call, json!({"writeId": write_id})),
    }
}

/// The visible rows: the synced rows with every entry laid over them, oldest
/// first (`rowsCases`).
pub fn outbox_rows(rows: &[OutboxRow], entries: &[OutboxEntry], me: Option<&str>) -> Vec<OutboxRow> {
    let mut out = rows.to_vec();
    for entry in entries {
        for w in &entry.writes {
            let i = out.iter().position(|r| r.id == w.id());
            match (w, i) {
                (OutboxWrite::Delete { .. }, Some(i)) => {
                    out.remove(i);
                }
                (OutboxWrite::Delete { .. }, None) => {}
                (OutboxWrite::Create { data, .. } | OutboxWrite::Update { data, .. }, Some(i)) => {
                    for (k, v) in data {
                        out[i].data.insert(k.clone(), v.clone());
                    }
                }
                (OutboxWrite::Create { id, kind, data }, None) => out.push(OutboxRow {
                    id: id.clone(),
                    kind: kind.clone(),
                    data: data.clone(),
                    created_by: me.map(str::to_string),
                    author_id: me.map(str::to_string),
                    created_at: entry.at.clone(),
                    updated_at: entry.at.clone(),
                }),
                (OutboxWrite::Update { .. }, None) => {}
            }
        }
    }
    out
}

/// What a replay's answer does to its entry (`outcomeCases`).
pub fn outbox_outcome(entry: &OutboxEntry, result: &OutboxResult) -> OutboxOutcome {
    match result {
        OutboxResult::Ok => OutboxOutcome::Done,
        OutboxResult::Transport => OutboxOutcome::Retry,
        OutboxResult::Code(code) if APP_RECORDS_OUTBOX.retry_codes.iter().any(|c| c == code) => OutboxOutcome::Retry,
        OutboxResult::Code(code)
            if code == "NOT_FOUND" && entry.call != OutboxCall::Batch && !entry.writes.iter().any(OutboxWrite::is_create) =>
        {
            OutboxOutcome::Dropped
        }
        OutboxResult::Code(_) => OutboxOutcome::Refused,
    }
}

/// The failed-action toast's verb for a refused entry: `delete` when every
/// write is a delete, else `save`.
pub fn outbox_verb(entry: &OutboxEntry) -> &'static str {
    if entry.writes.iter().all(OutboxWrite::is_delete) {
        "delete"
    } else {
        "save"
    }
}

/// One flush's result (`flushCases`).
#[derive(Debug, Clone, PartialEq)]
pub struct OutboxFlush {
    pub sent: Vec<String>,
    /// `(writeId, verb)` of each refused entry.
    pub refused: Vec<(String, &'static str)>,
    pub after: Vec<OutboxEntry>,
}

/// One flush (`flushCases`): pending entries oldest first, one at a time;
/// a retry stops it; `results` = each write id's answer (missing = no
/// network).
pub fn outbox_flush(entries: &[OutboxEntry], results: &HashMap<String, OutboxResult>, now: &str) -> OutboxFlush {
    let mut flush = OutboxFlush { sent: Vec::new(), refused: Vec::new(), after: Vec::new() };
    let mut stopped = false;
    for entry in entries {
        if stopped || !entry.is_pending() {
            flush.after.push(entry.clone());
            continue;
        }
        flush.sent.push(entry.write_id.clone());
        let result = results.get(&entry.write_id).cloned().unwrap_or(OutboxResult::Transport);
        match outbox_outcome(entry, &result) {
            OutboxOutcome::Done => flush.after.push(OutboxEntry {
                state: OutboxState::Acked,
                acked_at: Some(now.to_string()),
                ..entry.clone()
            }),
            OutboxOutcome::Retry => {
                stopped = true;
                flush.after.push(entry.clone());
            }
            OutboxOutcome::Refused => flush.refused.push((entry.write_id.clone(), outbox_verb(entry))),
            OutboxOutcome::Dropped => {}
        }
    }
    flush
}

/// Do the synced rows reflect an entry?
pub fn outbox_reflected(entry: &OutboxEntry, rows: &[OutboxRow]) -> bool {
    entry.writes.iter().all(|w| {
        let row = rows.iter().find(|r| r.id == w.id());
        match w {
            OutboxWrite::Delete { .. } => row.is_none(),
            OutboxWrite::Create { data, .. } | OutboxWrite::Update { data, .. } => row.is_some_and(|row| {
                data.iter().all(|(k, v)| row.data.get(k).is_some_and(|have| json_equal(have, v)))
            }),
        }
    })
}

fn iso_ms(at: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(at).ok().map(|t| t.timestamp_millis())
}

/// The entries that stay (`pruneCases`): pending ones, and acked ones the
/// rows don't reflect yet within the grace.
pub fn outbox_prune(entries: &[OutboxEntry], rows: &[OutboxRow], now: &str) -> Vec<OutboxEntry> {
    let grace = APP_RECORDS_OUTBOX.ack_grace_seconds as i64 * 1000;
    let now = iso_ms(now);
    entries
        .iter()
        .filter(|e| {
            if e.is_pending() {
                return true;
            }
            if outbox_reflected(e, rows) {
                return false;
            }
            match (now, iso_ms(e.acked_at.as_deref().unwrap_or(&e.at))) {
                (Some(now), Some(acked)) => now - acked < grace,
                _ => false,
            }
        })
        .cloned()
        .collect()
}

/// The header glyph's label for an app's pending entries (`pendingCases`);
/// `None` = no glyph.
pub fn outbox_pending_label(entries: &[OutboxEntry], app_id: &str) -> Option<String> {
    let n = entries.iter().filter(|e| e.app_id == app_id && e.is_pending()).count();
    let copy = &APP_RECORDS_OUTBOX.copy;
    match n {
        0 => None,
        1 => Some(copy.pending_one.clone()),
        n => Some(copy.pending.replace("{n}", &n.to_string())),
    }
}

