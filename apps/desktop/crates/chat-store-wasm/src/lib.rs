//! The wasm-bindgen facade over `chat-store` (#EDE-115, phase S4 of
//! `packages/domain-contract/fixtures/chat-store.json`): the web's chat
//! store. JSON strings in, JSON strings out.
//!
//! The web keeps ONE APPEND-ONLY LOG per chat in OPFS ([`file::ChatFile`]):
//! the worker (`apps/web/src/lib/chat-store/`) reads a chat's file into a
//! [`ChatFile`], appends what each write returns, and rewrites it from
//! `snapshot` after RETENTION. The catch-up rules (`catchUp`) are the
//! crate's own: `have`, the holder's chunk, the asker's progress, holders,
//! the byte bucket.
//!
//! A malformed JSON argument throws.

use chat_store::catchup::{self, Install, Power};
use serde_json::Value;
use wasm_bindgen::prelude::*;

pub mod file;

fn parse<T: serde::de::DeserializeOwned>(json: &str) -> Result<T, JsError> {
    serde_json::from_str(json).map_err(|err| JsError::new(&err.to_string()))
}

fn seq(value: Option<f64>) -> Option<u64> {
    value.filter(|v| v.is_finite() && *v >= 0.0).map(|v| v as u64)
}

/// One chat's store (its OPFS log, replayed).
#[wasm_bindgen]
pub struct ChatFile(file::ChatFile);

#[wasm_bindgen]
impl ChatFile {
    #[wasm_bindgen(constructor)]
    pub fn new() -> ChatFile {
        ChatFile(file::ChatFile::new())
    }

    /// Replays the log as read; returns the lines it took.
    pub fn load(&mut self, text: &str) -> u32 {
        self.0.load(text) as u32
    }

    /// `StoredEvent[]` → the lines to append ("" = nothing new).
    #[wasm_bindgen(js_name = putEvents)]
    pub fn put_events(&mut self, events_json: &str) -> Result<String, JsError> {
        Ok(self.0.put_events(parse(events_json)?))
    }

    /// `StoredMessage[]` → the lines to append.
    #[wasm_bindgen(js_name = putMessages)]
    pub fn put_messages(&mut self, messages_json: &str) -> Result<String, JsError> {
        Ok(self.0.put_messages(parse(messages_json)?))
    }

    #[wasm_bindgen(js_name = completeTo)]
    pub fn complete_to(&self, session_id: &str, device_id: &str) -> Option<f64> {
        self.0.complete_to(session_id, device_id).map(|seq| seq as f64)
    }

    pub fn meta(&self, key: &str) -> Option<String> {
        self.0.meta(key).map(str::to_string)
    }

    #[wasm_bindgen(js_name = setMeta)]
    pub fn set_meta(&mut self, key: &str, value: &str) -> String {
        self.0.set_meta(key, value)
    }

    pub fn records(&self) -> u32 {
        self.0.records() as u32
    }

    pub fn lines(&self) -> u32 {
        self.0.lines() as u32
    }

    /// The whole log (a rewrite).
    pub fn snapshot(&mut self) -> String {
        self.0.snapshot()
    }

    /// RETENTION; returns how many events went.
    #[wasm_bindgen(js_name = applyRetention)]
    pub fn apply_retention(&mut self, now_ms: f64, window_days: Option<u32>) -> u32 {
        self.0.apply_retention(now_ms as i64, window_days) as u32
    }

    /// OPEN: `{events: [{seq, event}], truncated}` through the REPLAY TAIL.
    #[wasm_bindgen(js_name = openTail)]
    pub fn open_tail(&self, session_id: &str, device_id: &str, tail: u32) -> String {
        self.0.open_tail(session_id, device_id, tail as usize).to_string()
    }

    /// An older page below `before`: `{events, seqs}`.
    pub fn page(&self, session_id: &str, device_id: &str, before: f64, limit: u32) -> String {
        self.0.page(session_id, device_id, before.max(0.0) as u64, limit as usize).to_string()
    }

    /// The asker's `have` runs above `afterSeq` (JSON `[[from, to]]`).
    #[wasm_bindgen(js_name = haveRuns)]
    pub fn have_runs(&self, session_id: &str, device_id: &str, after_seq: Option<f64>) -> String {
        serde_json::to_string(&self.0.have_runs(session_id, device_id, seq(after_seq))).unwrap_or_default()
    }

    /// THE HOLDER: the `history_chunk` frame for a device-naming page
    /// (undefined = not such a page).
    #[wasm_bindgen(js_name = answerChunk)]
    pub fn answer_chunk(&self, session_id: &str, page_json: &str, max_bytes: u32) -> Result<Option<String>, JsError> {
        let page: Value = parse(page_json)?;
        Ok(self.0.answer_chunk(session_id, &page, max_bytes as usize))
    }

    /// THE ASKER: takes one chunk → `{append, added, completeTo, next, end}`.
    pub fn take(&mut self, root_id: &str, session_id: &str, device_id: &str, after_seq: Option<f64>, chunk_json: &str, now_ms: f64) -> Result<String, JsError> {
        let chunk: Value = parse(chunk_json)?;
        Ok(self.0.take(root_id, session_id, device_id, seq(after_seq), &chunk, now_ms as i64).to_string())
    }

    /// FIND IN CHAT: `{hits, total}`.
    #[wasm_bindgen(js_name = searchChat)]
    pub fn search_chat(&self, root_id: &str, query: &str, limit: u32) -> String {
        serde_json::to_string(&self.0.search_chat(root_id, query, limit as usize)).unwrap_or_default()
    }
}

impl Default for ChatFile {
    fn default() -> Self {
        ChatFile::new()
    }
}

/// A refusal chunk (`unauthorized`, `no_such_session`) for a page.
#[wasm_bindgen(js_name = refusalFrame)]
pub fn refusal_frame(page_json: &str, code: &str) -> Result<Option<String>, JsError> {
    let page: Value = parse(page_json)?;
    Ok(file::refusal_frame(&page, code))
}

/// HOLDERS for a session on `deviceId`: `Install[]` → `[{endpointId, runner}]`.
pub fn holders(installs_json: &str, device_id: &str, self_endpoint: &str, user_id: &str, skip_json: &str) -> Result<String, JsError> {
    let installs: Vec<Install> = parse(installs_json)?;
    let skip: Vec<String> = parse(skip_json)?;
    Ok(serde_json::to_string(&catchup::holders(&installs, device_id, self_endpoint, user_id, &skip)).unwrap_or_default())
}

/// POWER: `ac` | `battery` | `unknown`.
#[wasm_bindgen(js_name = mayRun)]
pub fn may_run(power: &str, idle_ms: f64) -> bool {
    let power = match power {
        "battery" => Power::Battery,
        "ac" => Power::Ac,
        _ => Power::Unknown,
    };
    catchup::may_run(power, idle_ms.max(0.0) as u64)
}

/// When a session readers alone served is tried again.
#[wasm_bindgen(js_name = retryMs)]
pub fn retry_ms(attempts: u32) -> f64 {
    catchup::retry_ms(attempts) as f64
}

/// The catch-up constants the loop paces by.
#[wasm_bindgen(js_name = catchUpConstants)]
pub fn catch_up_constants() -> String {
    serde_json::json!({
        "pageMax": catchup::PAGE_MAX,
        "haveRunsMax": catchup::HAVE_RUNS_MAX,
        "concurrency": catchup::CONCURRENCY,
        "askTimeoutMs": catchup::ASK_TIMEOUT_MS,
        "passIntervalMs": catchup::PASS_INTERVAL_MS,
        "endpointBackoffMs": catchup::ENDPOINT_BACKOFF_MS,
        "retentionDays": chat_store::PHONE_RETENTION_DAYS,
    })
    .to_string()
}

/// The TOKEN BUCKET the asks spend from.
#[wasm_bindgen]
pub struct Bucket(catchup::Bucket);

#[wasm_bindgen]
impl Bucket {
    #[wasm_bindgen(constructor)]
    pub fn new(now_ms: f64) -> Bucket {
        Bucket(catchup::Bucket::new(now_ms as i64))
    }

    pub fn spend(&mut self, now_ms: f64, bytes: u32) {
        self.0.spend(now_ms as i64, bytes as usize);
    }

    #[wasm_bindgen(js_name = waitMs)]
    pub fn wait_ms(&mut self, now_ms: f64) -> f64 {
        self.0.wait_ms(now_ms as i64) as f64
    }
}
