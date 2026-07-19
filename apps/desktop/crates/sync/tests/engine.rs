//! Long-poll engine integration tests (masterplan-v3 §5.3/§5.6/§5.10 — the
//! Phase-2 gate bullets that need a live HTTP round-trip): a tiny in-process
//! HTTP/1.1 shape server drives the REAL stack — `UreqTransport` →
//! `ShapeClient` loop → `ShapeStore` — covering snapshot→live, the 409
//! atomic-refetch dance (no visible empty state), 401 → Unauthorized surfaced
//! exactly once, warm-start cursor resume, and the <1s no-hammer repeat
//! guard.

use std::collections::{HashSet, VecDeque};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use sync::client::{ShapeClient, ShapeClientConfig, ShapeDelta, UreqTransport};
use sync::manager::{AccountSyncConfig, SyncManager};
use sync::shapes::{shape_by_name, SHAPES};
use sync::store::ShapeStore;

// ---------------------------------------------------------------------------
// Tiny in-process HTTP/1.1 mock shape server
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct MockResponse {
    status: u16,
    /// Extra response headers (`electric-handle`, `electric-offset`, …).
    headers: Vec<(String, String)>,
    body: String,
    /// Long-poll hold: how long the server sits on the request before
    /// answering.
    hold: Duration,
}

impl MockResponse {
    fn new(status: u16, body: Value) -> MockResponse {
        MockResponse {
            status,
            headers: Vec::new(),
            body: body.to_string(),
            hold: Duration::ZERO,
        }
    }

    fn electric(mut self, handle: &str, offset: &str) -> MockResponse {
        self.headers
            .push(("electric-handle".into(), handle.into()));
        self.headers
            .push(("electric-offset".into(), offset.into()));
        self
    }

    fn hold(mut self, hold: Duration) -> MockResponse {
        self.hold = hold;
        self
    }
}

/// One recorded request (timestamped for the pacing assertions).
#[derive(Clone, Debug)]
struct RecordedRequest {
    path: String,
    query: String,
    bearer: Option<String>,
    cache_control: Option<String>,
    at: Instant,
}

impl RecordedRequest {
    fn param(&self, name: &str) -> Option<&str> {
        self.query.split('&').find_map(|pair| {
            let (k, v) = pair.split_once('=')?;
            (k == name).then_some(v)
        })
    }

    fn is_live(&self) -> bool {
        self.param("live") == Some("true")
    }
}

/// Scripted responses are served in order (across all paths — single-shape
/// tests only poll one path); when the script is empty the fallback answers.
struct MockShapeServer {
    base_url: String,
    requests: Arc<Mutex<Vec<RecordedRequest>>>,
    script: Arc<Mutex<VecDeque<MockResponse>>>,
    shutdown: Arc<AtomicBool>,
    accept_thread: Option<JoinHandle<()>>,
}

impl MockShapeServer {
    fn start(fallback: MockResponse) -> MockShapeServer {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        listener.set_nonblocking(true).expect("nonblocking");
        let addr = listener.local_addr().expect("addr");

        let requests: Arc<Mutex<Vec<RecordedRequest>>> = Arc::default();
        let script: Arc<Mutex<VecDeque<MockResponse>>> = Arc::default();
        let fallback = Arc::new(Mutex::new(fallback));
        let shutdown = Arc::new(AtomicBool::new(false));

        let accept_thread = {
            let requests = Arc::clone(&requests);
            let script = Arc::clone(&script);
            let fallback = Arc::clone(&fallback);
            let shutdown = Arc::clone(&shutdown);
            std::thread::spawn(move || {
                while !shutdown.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let requests = Arc::clone(&requests);
                            let script = Arc::clone(&script);
                            let fallback = Arc::clone(&fallback);
                            let shutdown = Arc::clone(&shutdown);
                            std::thread::spawn(move || {
                                let _ = handle_connection(
                                    stream, &requests, &script, &fallback, &shutdown,
                                );
                            });
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(3));
                        }
                        Err(_) => break,
                    }
                }
            })
        };

        MockShapeServer {
            base_url: format!("http://{addr}"),
            requests,
            script,
            shutdown,
            accept_thread: Some(accept_thread),
        }
    }

    fn push(&self, response: MockResponse) {
        self.script.lock().unwrap().push_back(response);
    }

    fn requests(&self) -> Vec<RecordedRequest> {
        self.requests.lock().unwrap().clone()
    }
}

impl Drop for MockShapeServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(handle) = self.accept_thread.take() {
            let _ = handle.join();
        }
    }
}

fn handle_connection(
    mut stream: TcpStream,
    requests: &Mutex<Vec<RecordedRequest>>,
    script: &Mutex<VecDeque<MockResponse>>,
    fallback: &Mutex<MockResponse>,
    shutdown: &AtomicBool,
) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            return Ok(()); // closed before a full request — ignore
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    let text = String::from_utf8_lossy(&buf);
    let mut lines = text.split("\r\n");
    let request_line = lines.next().unwrap_or_default();
    let target = request_line.split(' ').nth(1).unwrap_or_default();
    let (path, query) = target.split_once('?').unwrap_or((target, ""));

    let mut bearer = None;
    let mut cache_control = None;
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            let value = value.trim();
            if name.eq_ignore_ascii_case("authorization") {
                bearer = value.strip_prefix("Bearer ").map(str::to_string);
            } else if name.eq_ignore_ascii_case("cache-control") {
                cache_control = Some(value.to_string());
            }
        }
    }
    requests.lock().unwrap().push(RecordedRequest {
        path: path.to_string(),
        query: query.to_string(),
        bearer,
        cache_control,
        at: Instant::now(),
    });

    let response = script
        .lock()
        .unwrap()
        .pop_front()
        .unwrap_or_else(|| fallback.lock().unwrap().clone());

    // Long-poll hold, sliced so a test teardown isn't blocked on it.
    let deadline = Instant::now() + response.hold;
    while Instant::now() < deadline && !shutdown.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(5));
    }

    let mut out = format!(
        "HTTP/1.1 {} X\r\ncontent-type: application/json\r\ncache-control: private, no-store\r\n",
        response.status
    );
    for (name, value) in &response.headers {
        out.push_str(&format!("{name}: {value}\r\n"));
    }
    out.push_str(&format!(
        "content-length: {}\r\nconnection: close\r\n\r\n",
        response.body.len()
    ));
    out.push_str(&response.body);
    stream.write_all(out.as_bytes())?;
    stream.flush()
}

// ---------------------------------------------------------------------------
// Wire-body builders
// ---------------------------------------------------------------------------

fn insert_msg(table: &str, id: &str, title: &str) -> Value {
    json!({
        "headers": {"operation": "insert"},
        "key": format!("\"{table}\"/\"{id}\""),
        "value": {"id": id, "title": title}
    })
}

fn up_to_date_msg() -> Value {
    json!({"headers": {"control": "up-to-date"}})
}

/// A full snapshot response: inserts + `up-to-date`, with handle/offset.
fn snapshot(handle: &str, offset: &str, rows: &[(&str, &str)]) -> MockResponse {
    let mut msgs: Vec<Value> = rows
        .iter()
        .map(|(id, title)| insert_msg("issues", id, title))
        .collect();
    msgs.push(up_to_date_msg());
    MockResponse::new(200, Value::Array(msgs)).electric(handle, offset)
}

/// An idle live long-poll answer: bare `up-to-date` after `hold`.
fn live_idle(handle: &str, offset: &str, hold: Duration) -> MockResponse {
    MockResponse::new(200, Value::Array(vec![up_to_date_msg()]))
        .electric(handle, offset)
        .hold(hold)
}

/// A 409 rotation carrying the replacement handle (Electric sends
/// `must-refetch` in the body too; the client must key off status + header).
fn conflict_409(replacement_handle: &str) -> MockResponse {
    let mut resp = MockResponse::new(
        409,
        Value::Array(vec![json!({"headers": {"control": "must-refetch"}})]),
    );
    resp.headers
        .push(("electric-handle".into(), replacement_handle.into()));
    resp
}

fn unauthorized_401() -> MockResponse {
    MockResponse::new(401, json!({"message": "unauthorized"}))
}

fn upgrade_required_426() -> MockResponse {
    MockResponse::new(
        426,
        json!({"error": "client_upgrade_required", "platform": "desktop", "min": "0.9.0"}),
    )
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(tag: &str) -> TempDir {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "exp-sync-engine-{tag}-{}-{n}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).unwrap();
        TempDir { path }
    }

    fn db_path(&self) -> PathBuf {
        self.path.join("sync.sqlite")
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn wait_until(timeout: Duration, mut cond: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if cond() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    cond()
}

/// One directly-constructed shape client running on its own thread (the
/// manager-level spawn is covered separately).
struct ClientHarness {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    deltas: flume::Receiver<ShapeDelta>,
}

impl ClientHarness {
    fn spawn(server: &MockShapeServer, store: Arc<ShapeStore>, shape: &'static str) -> Self {
        let (tx, rx) = flume::unbounded();
        let stop = Arc::new(AtomicBool::new(false));
        let client = ShapeClient::new(ShapeClientConfig {
            account_id: "acct-1".into(),
            base_url: server.base_url.clone(),
            spec: shape_by_name(shape).unwrap(),
            store,
            token: Arc::new(|| Some("tok-1".to_string())),
            transport: Arc::new(UreqTransport::new()),
            deltas: tx,
            unauthorized_reported: Arc::new(AtomicBool::new(false)),
            on_unauthorized: None,
            upgrade_required_reported: Arc::new(AtomicBool::new(false)),
            on_upgrade_required: None,
        });
        let thread_stop = Arc::clone(&stop);
        let handle = std::thread::spawn(move || client.run(&thread_stop));
        ClientHarness {
            stop,
            handle: Some(handle),
            deltas: rx,
        }
    }

    fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for ClientHarness {
    fn drop(&mut self) {
        self.stop();
    }
}

fn issue_ids(store: &ShapeStore) -> HashSet<String> {
    store
        .read_all(shape_by_name("issues").unwrap())
        .unwrap()
        .iter()
        .filter_map(|row| row.get("id").and_then(Value::as_str).map(str::to_string))
        .collect()
}

// ---------------------------------------------------------------------------
// 1. Snapshot → live transition + warm-start cursor resume (gate #2/#3)
// ---------------------------------------------------------------------------

#[test]
fn snapshot_then_live_then_warm_start_resumes_cursor() {
    let server = MockShapeServer::start(live_idle("h-1", "0_5", Duration::from_millis(150)));
    server.push(snapshot("h-1", "0_0", &[("a", "A"), ("b", "B")]));

    let dir = TempDir::new("live");
    let store = Arc::new(ShapeStore::open(&dir.db_path()).unwrap());
    let mut harness = ClientHarness::spawn(&server, Arc::clone(&store), "issues");

    // Snapshot lands and the shape flips live.
    assert!(wait_until(Duration::from_secs(5), || {
        store
            .shape_state("issues")
            .unwrap()
            .is_some_and(|s| s.is_live)
    }));
    assert_eq!(issue_ids(&store), HashSet::from(["a".into(), "b".into()]));

    // The first request was the bare initial snapshot with the bearer + the
    // explicit no-cache discipline, and NEVER where/columns (§5.2).
    let first = &server.requests()[0];
    assert_eq!(first.path, "/api/shapes/issues");
    assert_eq!(first.param("offset"), Some("-1"));
    assert!(!first.is_live());
    assert!(first.param("handle").is_none());
    assert!(first.param("where").is_none());
    assert!(first.param("columns").is_none());
    assert_eq!(first.bearer.as_deref(), Some("tok-1"));
    assert_eq!(first.cache_control.as_deref(), Some("no-store"));

    // The loop transitions to a live long-poll carrying the saved cursor.
    assert!(wait_until(Duration::from_secs(5), || {
        server.requests().iter().any(|r| r.is_live())
    }));
    let live = server
        .requests()
        .into_iter()
        .find(|r| r.is_live())
        .unwrap();
    assert_eq!(live.param("handle"), Some("h-1"));
    assert_eq!(live.param("offset"), Some("0_0"));

    // The snapshot batch surfaced as ONE Applied delta with both keys.
    let delta = harness.deltas.recv_timeout(Duration::from_secs(2)).unwrap();
    match delta {
        ShapeDelta::Applied {
            shape,
            ref keys,
            full_replace,
            up_to_date,
            ..
        } => {
            assert_eq!(shape, "issues");
            assert_eq!(keys.len(), 2);
            assert!(!full_replace);
            assert!(up_to_date);
        }
        other => panic!("expected Applied, got {other:?}"),
    }

    // Let at least one live poll APPLY (the fallback advances the persisted
    // offset to 0_5) before simulating the quit.
    assert!(wait_until(Duration::from_secs(5), || {
        store
            .shape_state("issues")
            .unwrap()
            .is_some_and(|s| s.offset == "0_5")
    }));
    harness.stop();

    // Warm restart over the SAME store: the first request must carry the
    // persisted handle/offset — no offset=-1 re-snapshot (gate #3).
    let before = server.requests().len();
    let mut harness = ClientHarness::spawn(&server, Arc::clone(&store), "issues");
    assert!(wait_until(Duration::from_secs(5), || {
        server.requests().len() > before
    }));
    let resumed = &server.requests()[before];
    assert_eq!(resumed.param("offset"), Some("0_5"));
    assert_eq!(resumed.param("handle"), Some("h-1"));
    assert!(resumed.is_live(), "warm start resumes straight into live");
    harness.stop();
}

// ---------------------------------------------------------------------------
// 2. 409 mid-live → atomic refetch, stale rows visible throughout (gate #4)
// ---------------------------------------------------------------------------

#[test]
fn conflict_409_refetches_atomically_with_no_empty_state() {
    let server = MockShapeServer::start(live_idle("h-2", "0_7", Duration::from_millis(200)));
    server.push(snapshot("h-1", "0_0", &[("a", "A"), ("b", "B")]));
    server.push(conflict_409("h-2").hold(Duration::from_millis(50)));
    server.push(snapshot("h-2", "0_7", &[("b", "B v2"), ("c", "C")]));

    let dir = TempDir::new("409");
    let store = Arc::new(ShapeStore::open(&dir.db_path()).unwrap());
    let mut harness = ClientHarness::spawn(&server, Arc::clone(&store), "issues");

    // Wait for the initial snapshot…
    assert!(wait_until(Duration::from_secs(5), || {
        store.count(shape_by_name("issues").unwrap()).unwrap() == 2
    }));

    // …then sample the row count continuously through the 409 + refetch. The
    // §5.6c dance means a reader NEVER observes fewer rows than the stale
    // set: the marker keeps the old rows, and DELETE + fresh inserts share
    // one commit.
    let sampler_stop = Arc::new(AtomicBool::new(false));
    let sampler = {
        let store = Arc::clone(&store);
        let stop = Arc::clone(&sampler_stop);
        std::thread::spawn(move || {
            let spec = shape_by_name("issues").unwrap();
            let mut min_count = i64::MAX;
            while !stop.load(Ordering::Relaxed) {
                min_count = min_count.min(store.count(spec).unwrap());
                std::thread::sleep(Duration::from_millis(1));
            }
            min_count
        })
    };

    // The refetch snapshot replaces the table under the new handle.
    assert!(wait_until(Duration::from_secs(10), || {
        store
            .shape_state("issues")
            .unwrap()
            .is_some_and(|s| s.handle == "h-2" && !s.needs_refetch)
    }));
    assert!(wait_until(Duration::from_secs(2), || {
        issue_ids(&store) == HashSet::from(["b".into(), "c".into()])
    }));

    sampler_stop.store(true, Ordering::Relaxed);
    let min_count = sampler.join().unwrap();
    assert!(
        min_count >= 2,
        "reader observed {min_count} rows mid-refetch — the EXP-1 #13 empty flicker"
    );

    // The refetch went out as offset=-1 WITH the replacement handle (§5.6c).
    assert!(server
        .requests()
        .iter()
        .any(|r| r.param("offset") == Some("-1") && r.param("handle") == Some("h-2")));

    // And it surfaced as a full_replace delta so collections re-hydrate
    // wholesale.
    let saw_full_replace = std::iter::from_fn(|| harness.deltas.try_recv().ok()).any(|d| {
        matches!(
            d,
            ShapeDelta::Applied {
                full_replace: true,
                ..
            }
        )
    });
    assert!(saw_full_replace);

    harness.stop();
}

/// §5.6c hardening: a refetch (post-409) response that decodes to ZERO
/// messages AND carries no `snapshot-end` (empty or malformed body) must NOT
/// wipe the table or clear the refetch marker. An unguarded apply would run
/// the synthetic DELETE head with no re-inserts and adopt the cursor: a
/// durable empty board (the vanished-issues symptom).
#[test]
fn empty_refetch_response_keeps_stale_rows_and_marker() {
    // Fallback: the pathological refetch answer — 200, valid electric
    // headers, zero-message body — served forever.
    let server = MockShapeServer::start(
        MockResponse::new(200, json!([]))
            .electric("h-2", "0_9")
            .hold(Duration::from_millis(30)),
    );
    server.push(snapshot("h-1", "0_0", &[("a", "A"), ("b", "B")]));
    server.push(conflict_409("h-2"));

    let dir = TempDir::new("empty-refetch");
    let store = Arc::new(ShapeStore::open(&dir.db_path()).unwrap());
    let mut harness = ClientHarness::spawn(&server, Arc::clone(&store), "issues");

    // Snapshot lands, then the 409 marks the refetch.
    assert!(wait_until(Duration::from_secs(5), || {
        store
            .shape_state("issues")
            .unwrap()
            .is_some_and(|s| s.needs_refetch)
    }));
    // Let several empty refetch answers cycle through.
    assert!(wait_until(Duration::from_secs(5), || {
        server
            .requests()
            .iter()
            .filter(|r| r.param("offset") == Some("-1") && r.param("handle") == Some("h-2"))
            .count()
            >= 2
    }));

    // Stale rows survive and the marker still forces the refetch.
    assert_eq!(issue_ids(&store), HashSet::from(["a".into(), "b".into()]));
    let state = store.shape_state("issues").unwrap().unwrap();
    assert!(state.needs_refetch, "marker must survive an empty refetch answer");
    assert_eq!(state.handle, "h-2");

    harness.stop();
}

/// The legitimate zero-message refetch answer: live Electric (1.6.9) serves a
/// genuinely EMPTY shape's `offset=-1` snapshot as a LONE `snapshot-end`
/// control — no rows, no `up-to-date` (that only arrives on the follow-up
/// poll). The refetch must COMPLETE off it: DELETE the stale rows + adopt the
/// replacement cursor + clear the marker, then go live on the next poll.
/// (Found by the §11.4 Phase-2 runtime gate, 2026-07-03: all zero-row shapes
/// were stuck hammering `offset=-1` forever after a forced 409.)
#[test]
fn empty_snapshot_refetch_completes_via_snapshot_end() {
    let server = MockShapeServer::start(live_idle("h-2", "0_0", Duration::from_millis(50)));
    server.push(snapshot("h-1", "0_0", &[("a", "A"), ("b", "B")]));
    server.push(conflict_409("h-2"));
    // The refetch answer: Electric's empty-shape snapshot — a lone
    // `snapshot-end`, exactly as observed on the wire.
    server.push(
        MockResponse::new(
            200,
            json!([{"headers": {"control": "snapshot-end", "xip_list": [], "xmax": "1", "xmin": "1"}}]),
        )
        .electric("h-2", "0_0"),
    );

    let dir = TempDir::new("empty-snapshot-refetch");
    let store = Arc::new(ShapeStore::open(&dir.db_path()).unwrap());
    let mut harness = ClientHarness::spawn(&server, Arc::clone(&store), "issues");

    // Initial snapshot lands…
    assert!(wait_until(Duration::from_secs(5), || {
        store.count(shape_by_name("issues").unwrap()).unwrap() == 2
    }));

    // …the 409 + lone-snapshot-end refetch completes: marker cleared, the
    // replacement handle adopted, the table now (correctly) empty, and the
    // follow-up poll flips the shape live.
    assert!(wait_until(Duration::from_secs(10), || {
        store
            .shape_state("issues")
            .unwrap()
            .is_some_and(|s| s.handle == "h-2" && !s.needs_refetch && s.is_live)
    }));
    assert_eq!(issue_ids(&store), HashSet::new());

    // No offset=-1 hammering: once the swap landed, polls moved on.
    let refetch_polls = server
        .requests()
        .iter()
        .filter(|r| r.param("offset") == Some("-1") && r.param("handle") == Some("h-2"))
        .count();
    assert_eq!(refetch_polls, 1, "the lone-snapshot-end answer must complete the refetch in one poll");

    harness.stop();
}

// ---------------------------------------------------------------------------
// 3. Hard 401 → Unauthorized surfaced exactly once, pipeline tears down
//    (gate #5)
// ---------------------------------------------------------------------------

#[test]
fn dead_token_surfaces_unauthorized_once_and_tears_down() {
    let server = MockShapeServer::start(unauthorized_401());

    let dir = TempDir::new("401");
    let unauthorized_calls = Arc::new(AtomicUsize::new(0));
    let calls = Arc::clone(&unauthorized_calls);
    let manager = SyncManager::new().on_unauthorized(Arc::new(move |account_id| {
        assert_eq!(account_id, "acct-1");
        calls.fetch_add(1, Ordering::SeqCst);
    }));
    let deltas = manager.deltas();

    let started = manager
        .start_account(AccountSyncConfig {
            account_id: "acct-1".into(),
            base_url: server.base_url.clone(),
            db_path: dir.db_path(),
            token: Arc::new(|| Some("dead-token".to_string())),
        })
        .unwrap();
    assert!(started);

    // All 15 threads 401 near-simultaneously — exactly ONE Unauthorized.
    let delta = deltas.recv_timeout(Duration::from_secs(10)).unwrap();
    match delta {
        ShapeDelta::Unauthorized { ref account_id } => assert_eq!(account_id, "acct-1"),
        other => panic!("expected Unauthorized, got {other:?}"),
    }
    std::thread::sleep(Duration::from_millis(300));
    while let Ok(extra) = deltas.try_recv() {
        assert!(
            !matches!(extra, ShapeDelta::Unauthorized { .. }),
            "Unauthorized must be emitted exactly once"
        );
    }
    assert_eq!(unauthorized_calls.load(Ordering::SeqCst), 1);

    // The pipeline tore itself down — no thread keeps polling the dead token.
    assert!(wait_until(Duration::from_secs(5), || {
        manager.running_accounts().is_empty()
    }));
    let polls_after_teardown = server.requests().len();
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(
        server.requests().len(),
        polls_after_teardown,
        "threads must stop polling after the 401 teardown"
    );

    // And stopping the (dead) account returns promptly.
    let stopped_at = Instant::now();
    assert!(manager.stop_account("acct-1"));
    assert!(stopped_at.elapsed() < Duration::from_secs(2));
}

// ---------------------------------------------------------------------------
// 3b. HTTP 426 → on_upgrade_required fires exactly once, pipeline tears down,
//     token is NOT cleared and NO Unauthorized delta is emitted (EXP-104)
// ---------------------------------------------------------------------------

#[test]
fn stale_client_surfaces_upgrade_required_once_and_tears_down() {
    let server = MockShapeServer::start(upgrade_required_426());

    let dir = TempDir::new("426");
    let upgrade_calls = Arc::new(AtomicUsize::new(0));
    let calls = Arc::clone(&upgrade_calls);
    let manager = SyncManager::new().on_upgrade_required(Arc::new(move || {
        calls.fetch_add(1, Ordering::SeqCst);
    }));
    let deltas = manager.deltas();

    let started = manager
        .start_account(AccountSyncConfig {
            account_id: "acct-1".into(),
            base_url: server.base_url.clone(),
            db_path: dir.db_path(),
            token: Arc::new(|| Some("live-token".to_string())),
        })
        .unwrap();
    assert!(started);

    // The pipeline tears itself down — no thread keeps polling a build the
    // server refuses.
    assert!(wait_until(Duration::from_secs(10), || {
        manager.running_accounts().is_empty()
    }));

    // Exactly one hook call despite all threads 426-ing near-simultaneously.
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(upgrade_calls.load(Ordering::SeqCst), 1);

    // No polling after teardown.
    let polls_after_teardown = server.requests().len();
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(
        server.requests().len(),
        polls_after_teardown,
        "threads must stop polling after the 426 teardown"
    );

    // The 426 path emits NO delta — the session is fine, only the binary is
    // stale (contrast the 401 path, which routes to login).
    while let Ok(delta) = deltas.try_recv() {
        assert!(
            !matches!(delta, ShapeDelta::Unauthorized { .. }),
            "426 must never surface as Unauthorized (that would clear the token)"
        );
    }
}

// ---------------------------------------------------------------------------
// 4. The <1s repeat guard: an instant-answering live endpoint never gets
//    hammered (gate #6's client half)
// ---------------------------------------------------------------------------

#[test]
fn idle_live_loop_never_repolls_under_one_second() {
    // Pathological server: answers live long-polls INSTANTLY with a bare
    // up-to-date instead of holding ~60s. Without the repeat guard the loop
    // would degrade into a tight short-poll spin.
    let server = MockShapeServer::start(live_idle("h-1", "0_0", Duration::ZERO));
    server.push(snapshot("h-1", "0_0", &[("a", "A")]));

    let dir = TempDir::new("pace");
    let store = Arc::new(ShapeStore::open(&dir.db_path()).unwrap());
    let mut harness = ClientHarness::spawn(&server, Arc::clone(&store), "issues");

    assert!(wait_until(Duration::from_secs(5), || {
        server.requests().iter().any(|r| r.is_live())
    }));
    std::thread::sleep(Duration::from_secs(3));
    harness.stop();

    let live_times: Vec<Instant> = server
        .requests()
        .iter()
        .filter(|r| r.is_live())
        .map(|r| r.at)
        .collect();
    assert!(live_times.len() >= 2, "expected repeated live polls");
    let window = live_times[live_times.len() - 1].duration_since(live_times[0]);
    let max_expected = (window.as_secs_f64() / 0.9).floor() as usize + 1;
    assert!(
        live_times.len() <= max_expected,
        "{} live polls in {window:?} — the loop is hammering",
        live_times.len()
    );
    for pair in live_times.windows(2) {
        let gap = pair[1].duration_since(pair[0]);
        assert!(
            gap >= Duration::from_millis(850),
            "live re-poll after only {gap:?} (<1s repeat guard violated)"
        );
    }
}

// ---------------------------------------------------------------------------
// 5. Manager lifecycle: 15 named threads per account, clean stop, first-sync
//    wait (§5.10)
// ---------------------------------------------------------------------------

#[test]
fn manager_runs_all_15_shapes_and_stops_cleanly() {
    let server = MockShapeServer::start(live_idle("h-1", "0_0", Duration::from_millis(30)));

    let dir = TempDir::new("mgr");
    let manager = SyncManager::new();
    let config = || AccountSyncConfig {
        account_id: "acct-1".into(),
        base_url: server.base_url.clone(),
        db_path: dir.db_path(),
        token: Arc::new(|| Some("tok-1".to_string())),
    };

    assert!(manager.start_account(config()).unwrap());
    // Idempotent: already running → no second pipeline.
    assert!(!manager.start_account(config()).unwrap());
    assert_eq!(manager.running_accounts(), vec!["acct-1".to_string()]);

    // Every one of the 15 shape proxies gets polled (with the bearer).
    let expected: HashSet<&str> = SHAPES.iter().map(|s| s.path).collect();
    assert!(
        wait_until(Duration::from_secs(10), || {
            let seen: HashSet<String> =
                server.requests().iter().map(|r| r.path.clone()).collect();
            seen.len() == expected.len()
        }),
        "not all shape proxies were polled"
    );
    let seen: HashSet<String> = server.requests().iter().map(|r| r.path.clone()).collect();
    assert_eq!(
        seen,
        expected.iter().map(|p| p.to_string()).collect::<HashSet<_>>()
    );
    assert!(server
        .requests()
        .iter()
        .all(|r| r.bearer.as_deref() == Some("tok-1")));

    // First-sync wait: the teams shape reaches head (up-to-date).
    assert!(manager.wait_for_first_sync("acct-1", Duration::from_secs(5)));
    assert!(manager.store("acct-1").is_some());

    // Reconcile to an empty signed-in set stops the pipeline promptly (§5.10
    // — quit must never wait on a 90s long-poll).
    let stopped_at = Instant::now();
    manager.reconcile(Vec::new());
    assert!(stopped_at.elapsed() < Duration::from_secs(2));
    assert!(manager.running_accounts().is_empty());
    assert!(manager.store("acct-1").is_none());

    // The DB stays on disk for offline resume (§5.10).
    assert!(dir.db_path().exists());
}
