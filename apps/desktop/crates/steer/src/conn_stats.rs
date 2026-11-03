//! P2P stabilisation: privacy-safe CONNECTION ANALYTICS
//! (`packages/domain-contract/fixtures/steer-enet.json` `connStats`; TS
//! `steerConnEventValid`). eCore envelopes `{v:1, id, kind, at, source,
//! data}` and nothing else: no content, no user, team, session or device id.
//!
//! - `steer.view` (source `steer-viewer`): the desktop viewer, `settled`
//!   once per open and `ended` when it closes (`viewer.rs`).
//! - `steer.serve` (source `steer-device`): one per feed this device
//!   served, `ended` (`enet_steer.rs`).
//!
//! [`install`] once per signed-in account (the app: platform `desktop`, the
//! daemon: `cli`); [`record`] buffers (dropped while nothing is installed)
//! and a task posts the buffer to `connStats.record` every [`FLUSH`] and on
//! [`flush`] (sign-out, quit). Never blocks or errors anything: a failed
//! post is dropped.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use api::error::ApiError;
use serde::Serialize;
use serde_json::Value;

pub const KIND_VIEW: &str = "steer.view";
pub const KIND_SERVE: &str = "steer.serve";
pub const SOURCE_VIEWER: &str = "steer-viewer";
pub const SOURCE_DEVICE: &str = "steer-device";
pub const KINDS: &[&str] = &[KIND_VIEW, KIND_SERVE];
pub const SOURCES: &[&str] = &[SOURCE_VIEWER, SOURCE_DEVICE];
pub const PHASES: &[&str] = &["settled", "ended"];
pub const PLATFORMS: &[&str] = &["web", "desktop", "ios", "android", "cli"];
pub const PATHS: &[&str] = &[PATH_DIRECT, PATH_IROH_RELAY, PATH_SERVER_FALLBACK];
pub const PATH_DIRECT: &str = "direct";
pub const PATH_IROH_RELAY: &str = "iroh_relay";
pub const PATH_SERVER_FALLBACK: &str = "server_fallback";
pub const OUTCOMES: &[&str] = &["ok", "failed"];
pub const ERROR_CLASSES: &[&str] = &[
    "no_endpoint",
    "no_node",
    "refused",
    "unauthorized",
    "join_timeout",
    "stale",
    "send_failed",
    "closed",
    "no_key",
    "device_offline",
    "device_unsupported",
    "ticket_failed",
    "relay_closed",
    "other",
    "dialing",
    "background",
];
/// A view with no first frame by then settles `failed`.
pub const SETTLE_AFTER: Duration = Duration::from_millis(10_000);
pub const FLUSH: Duration = Duration::from_millis(60_000);
pub const BATCH_MAX: usize = 50;
pub const VERSION_MAX: usize = 64;
/// What the buffer holds at most while posts fail (the oldest go first).
const BUFFER_MAX: usize = 500;

/// One event's `data` (every field the schema allows, nothing more).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnData {
    pub phase: &'static str,
    /// Filled in by [`record`] (the installed platform).
    pub platform: &'static str,
    /// Filled in by [`record`] (this build's version).
    pub version: String,
    pub outcome: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_frame_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_class: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallbacks: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reconnects: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failures: Option<BTreeMap<&'static str, u32>>,
}

/// Where the events go: `connStats.record` in the apps, a fake in tests.
/// BLOCKING.
pub trait ConnStatsSink: Send + Sync + 'static {
    fn record(&self, events: &[Value]) -> Result<u32, ApiError>;
}

impl ConnStatsSink for api::TrpcClient {
    fn record(&self, events: &[Value]) -> Result<u32, ApiError> {
        api::steer::record_conn_stats(self, events)
    }
}

struct Recorder {
    sink: Arc<dyn ConnStatsSink>,
    platform: &'static str,
    runtime: tokio::runtime::Handle,
    generation: u64,
}

#[derive(Default)]
struct State {
    recorder: Option<Recorder>,
    buffer: Vec<Value>,
    generation: u64,
}

fn state() -> &'static Mutex<State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE.get_or_init(Default::default)
}

fn lock() -> std::sync::MutexGuard<'static, State> {
    state().lock().unwrap_or_else(|p| p.into_inner())
}

/// Starts recording for this process (`platform` = `desktop` | `cli`),
/// posting through `sink` from `runtime`'s blocking pool every [`FLUSH`].
/// A second install replaces the first (account switch).
pub fn install(runtime: &tokio::runtime::Handle, sink: Arc<dyn ConnStatsSink>, platform: &'static str) {
    debug_assert!(PLATFORMS.contains(&platform));
    let generation = {
        let mut state = lock();
        state.generation += 1;
        let generation = state.generation;
        state.recorder = Some(Recorder { sink, platform, runtime: runtime.clone(), generation });
        generation
    };
    runtime.spawn(async move {
        loop {
            tokio::time::sleep(FLUSH).await;
            if lock().recorder.as_ref().is_none_or(|r| r.generation != generation) {
                return;
            }
            flush();
        }
    });
}

/// Posts what is buffered, then stops recording (sign-out).
pub fn uninstall() {
    flush();
    lock().recorder = None;
}

/// Buffers one event (`kind` + `source` + `data`, platform and version
/// stamped here). Dropped while nothing is installed.
pub fn record(kind: &'static str, source: &'static str, mut data: ConnData) {
    let mut state = lock();
    let Some(recorder) = state.recorder.as_ref() else { return };
    data.platform = recorder.platform;
    data.version = version();
    let event = envelope(kind, source, &data);
    if !event_valid(&event) {
        log::warn!("conn stats: an off-schema {kind} event was not recorded: {event}");
        return;
    }
    if state.buffer.len() >= BUFFER_MAX {
        state.buffer.remove(0);
    }
    state.buffer.push(event);
}

/// Posts the buffer now, in batches of [`BATCH_MAX`], off the caller's
/// thread. A failed batch is dropped (analytics never retry into a storm).
pub fn flush() {
    let (events, sink, runtime) = {
        let mut state = lock();
        let Some(recorder) = state.recorder.as_ref() else { return };
        let (sink, runtime) = (Arc::clone(&recorder.sink), recorder.runtime.clone());
        (std::mem::take(&mut state.buffer), sink, runtime)
    };
    if events.is_empty() {
        return;
    }
    runtime.spawn_blocking(move || {
        for batch in events.chunks(BATCH_MAX) {
            if let Err(err) = sink.record(batch) {
                // EXP-1593: visible at the default level (a debug line hid
                // every failed post); at most one per batch a minute.
                log::warn!("conn stats: connStats.record failed ({} events dropped): {err}", batch.len());
            }
        }
    });
}

/// This build's version, as the schema bounds it.
fn version() -> String {
    env!("CARGO_PKG_VERSION").chars().take(VERSION_MAX).collect()
}

fn envelope(kind: &str, source: &str, data: &ConnData) -> Value {
    let at = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(1).max(1);
    serde_json::json!({
        "v": 1,
        "id": uuid::Uuid::new_v4().simple().to_string(),
        "kind": kind,
        "at": at,
        "source": source,
        "data": data,
    })
}

const EVENT_KEYS: &[&str] = &["v", "id", "kind", "at", "source", "data"];
const DATA_KEYS: &[&str] = &[
    "phase",
    "platform",
    "version",
    "outcome",
    "path",
    "firstFrameMs",
    "errorClass",
    "fallbacks",
    "reconnects",
    "durationMs",
    "failures",
];

fn is_count(value: &Value) -> bool {
    value.as_u64().is_some_and(|n| n <= 2_147_483_647)
}

fn one_of(value: Option<&Value>, allowed: &[&str]) -> bool {
    value.and_then(Value::as_str).is_some_and(|v| allowed.contains(&v))
}

/// TS `steerConnEventValid`: the server's validation of one event; anything
/// off-schema (an extra key included: content must never ride along) is
/// refused whole.
pub fn event_valid(value: &Value) -> bool {
    let Some(e) = value.as_object() else { return false };
    if e.keys().any(|k| !EVENT_KEYS.contains(&k.as_str())) {
        return false;
    }
    if e.get("v").and_then(Value::as_u64) != Some(1) {
        return false;
    }
    if !e.get("id").and_then(Value::as_str).is_some_and(|id| !id.is_empty() && id.len() <= 64) {
        return false;
    }
    if !one_of(e.get("kind"), KINDS) || !one_of(e.get("source"), SOURCES) {
        return false;
    }
    if !e.get("at").and_then(Value::as_u64).is_some_and(|at| at > 0) {
        return false;
    }
    let Some(d) = e.get("data").and_then(Value::as_object) else { return false };
    if d.keys().any(|k| !DATA_KEYS.contains(&k.as_str())) {
        return false;
    }
    if !one_of(d.get("phase"), PHASES) || !one_of(d.get("platform"), PLATFORMS) {
        return false;
    }
    if !d.get("version").and_then(Value::as_str).is_some_and(|v| !v.is_empty() && v.len() <= VERSION_MAX) {
        return false;
    }
    if !one_of(d.get("outcome"), OUTCOMES) {
        return false;
    }
    if d.get("path").is_some_and(|path| !one_of(Some(path), PATHS)) {
        return false;
    }
    if d.get("outcome").and_then(Value::as_str) == Some("ok") && d.get("path").is_none() {
        return false;
    }
    if d.get("errorClass").is_some_and(|class| !one_of(Some(class), ERROR_CLASSES)) {
        return false;
    }
    for key in ["firstFrameMs", "fallbacks", "reconnects", "durationMs"] {
        if d.get(key).is_some_and(|v| !is_count(v)) {
            return false;
        }
    }
    if let Some(failures) = d.get("failures") {
        let Some(failures) = failures.as_object() else { return false };
        if failures.iter().any(|(k, n)| !ERROR_CLASSES.contains(&k.as_str()) || !is_count(n)) {
            return false;
        }
    }
    true
}

/// An error class by name (the viewer's and device's failure labels are
/// these constants; anything else reads `other`).
pub fn error_class(name: &str) -> &'static str {
    ERROR_CLASSES.iter().copied().find(|class| *class == name).unwrap_or("other")
}

/// #EXP-1678 (`errorClassCases`, TS `steerViewErrorClass`): a FAILED view
/// always names a class: `background` while the app is in the background,
/// else the last carrier failure, else `dialing` (nothing answered or failed
/// yet). An OK view keeps its last failure, if any.
pub fn view_error_class(ok: bool, last_error: Option<&'static str>, background: bool) -> Option<&'static str> {
    if ok {
        return last_error;
    }
    if background {
        return Some("background");
    }
    Some(last_error.unwrap_or("dialing"))
}

/// The eNet path a chat link runs on, as the schema names it.
pub fn enet_path(path: Option<enet_core::ChatPath>) -> &'static str {
    match path {
        Some(enet_core::ChatPath::Relay) => PATH_IROH_RELAY,
        // No live link to read = the direct path it was dialed on.
        Some(enet_core::ChatPath::Direct) | None => PATH_DIRECT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        let f: Value =
            serde_json::from_str(include_str!("../../../../../packages/domain-contract/fixtures/steer-enet.json")).unwrap();
        f["connStats"].clone()
    }

    #[test]
    fn valid_cases() {
        let f = fixture();
        for case in f["validCases"].as_array().unwrap() {
            assert_eq!(event_valid(&case["event"]), case["valid"].as_bool().unwrap(), "{}", case["name"]);
        }
    }

    #[test]
    fn the_vocabularies_are_the_fixtures() {
        let f = fixture();
        let list = |key: &str| -> Vec<String> {
            f[key].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect()
        };
        assert_eq!(list("kinds"), KINDS);
        assert_eq!(list("sources"), SOURCES);
        assert_eq!(list("phases"), PHASES);
        assert_eq!(list("platforms"), PLATFORMS);
        assert_eq!(list("paths"), PATHS);
        assert_eq!(list("outcomes"), OUTCOMES);
        assert_eq!(list("errorClasses"), ERROR_CLASSES);
        assert_eq!(f["settleAfterMs"], SETTLE_AFTER.as_millis() as u64);
        assert_eq!(f["flushMs"], FLUSH.as_millis() as u64);
        assert_eq!(f["batchMax"], BATCH_MAX);
        assert_eq!(f["versionMax"], VERSION_MAX);
    }

    #[test]
    fn what_this_build_records_is_valid_and_carries_no_ids() {
        let mut failures = BTreeMap::new();
        failures.insert("stale", 1);
        let data = ConnData {
            phase: "ended",
            platform: "desktop",
            version: version(),
            outcome: "ok",
            path: Some(PATH_SERVER_FALLBACK),
            first_frame_ms: Some(2_900),
            fallbacks: Some(1),
            reconnects: Some(0),
            duration_ms: Some(5_000),
            failures: Some(failures),
            ..Default::default()
        };
        let event = envelope(KIND_VIEW, SOURCE_VIEWER, &data);
        assert!(event_valid(&event), "{event}");
        assert!(event.get("sessionId").is_none());
        let failed = ConnData {
            phase: "settled",
            platform: "cli",
            version: version(),
            outcome: "failed",
            error_class: Some(error_class("join_timeout")),
            ..Default::default()
        };
        assert!(event_valid(&envelope(KIND_SERVE, SOURCE_DEVICE, &failed)));
        assert_eq!(error_class("a prompt"), "other");
    }

    #[test]
    fn a_failed_view_always_names_its_class() {
        let cases = fixture()["errorClassCases"].as_array().unwrap().clone();
        assert!(!cases.is_empty());
        for c in cases {
            let last = c["lastError"].as_str().map(error_class);
            let got = view_error_class(c["ok"].as_bool().unwrap(), last, c["background"].as_bool().unwrap());
            assert_eq!(got, c["errorClass"].as_str(), "{}", c["name"]);
        }
    }
}
