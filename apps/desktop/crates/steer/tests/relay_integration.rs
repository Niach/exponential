//! Integration test against the REAL steer relay (masterplan-v3 §8 / the
//! Phase 6 gate's protocol-conformance half).
//!
//! Boots `apps/steer-relay` locally (bun, `STEER_RELAY_SECRET=test-secret`,
//! ephemeral port), mints tickets by calling `@exp/steer-ticket`'s
//! `signSteerTicket` directly with the same secret (a tiny bun script —
//! `tests/support/mint_ticket.ts`; the ticket FORMAT is the shared package's
//! contract), then drives the production publisher/control-channel machinery
//! plus a fake phone-viewer socket through the frozen protocol:
//!
//! * control `online` → device shows in the admin `GET /devices/:userId` →
//!   `POST /start` routes `start_session` down our socket (§8.3);
//! * publisher `hello` → the EXP-249 history re-publish (`activity_reset` +
//!   the journal) → a viewer joining `channel:'activity'` gets its own reset +
//!   the relay-side replay, then the live tail;
//! * a joined viewer's `input` reaches the publisher's PTY-writer hook
//!   (EXP-312 — steering is seamless and owner-only: no claim, no perm tier),
//!   and an `answer` frame reaches the emitter seam instead (never the PTY);
//! * viewer `kill` → publisher kill hook + clean `bye` → the relay closes the
//!   room (`CLOSE_SESSION_ENDED` at the viewer);
//! * EXP-696: the PRODUCTION viewer client (`steer::spawn_viewer`) against
//!   that same room — join, replay, live tail, steering back — with its
//!   events driving a real [`steer::SteerFeed`], so transport and feed model
//!   are checked against the relay rather than against a fake;
//! * a severed publisher socket (TCP proxy dropped) → re-mint → re-`hello`
//!   resumes the SAME room and REBUILDS the joined viewer's feed (§8.6).
//!
//! Skips (passes) when `bun` is unavailable so plain `cargo test` stays green
//! on machines without the JS toolchain. The relay child is killed on drop.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

use api::error::ApiError;
use api::steer::MintedTicket;
use steer::control_channel::{spawn_control_channel, ControlApi, DeviceIdentity};
use steer::publisher::{publish, KillSignal, PublishSpec, PublisherHooks, PublisherTickets};
use steer::viewer::{spawn_viewer_with, ViewerEvent, ViewerPhase, ViewerTickets, ViewerTimings};
use steer::{ActivityEvent, AnswerLink, RemoteAnswer, SteerFeed, SteerRuntime};

const SECRET: &str = "test-secret";
const SESSION_ID: &str = "11111111-2222-3333-4444-555555555555";

// ---------------------------------------------------------------------------
// Harness: repo layout, relay child, ticket minting, admin HTTP
// ---------------------------------------------------------------------------

fn repo_root() -> PathBuf {
    // ancestors: 0 = crates/steer, 1 = crates, 2 = apps/desktop, 3 = apps,
    // 4 = the repo root
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .expect("repo root above apps/desktop/crates/steer")
        .to_path_buf()
}

fn bun_available() -> bool {
    Command::new("bun")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    listener.local_addr().unwrap().port()
}

/// The relay child process — killed on drop, pass or panic.
struct RelayGuard {
    child: Child,
    port: u16,
}

impl Drop for RelayGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn start_relay() -> RelayGuard {
    let port = free_port();
    let relay_dir = repo_root().join("apps/steer-relay");
    assert!(
        relay_dir.join("src/index.ts").exists(),
        "apps/steer-relay missing at {relay_dir:?}"
    );
    let child = Command::new("bun")
        .arg("src/index.ts")
        .current_dir(&relay_dir)
        .env("PORT", port.to_string())
        .env("STEER_RELAY_SECRET", SECRET)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn bun steer-relay");
    let guard = RelayGuard { child, port };

    // Wait for /healthz.
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(body) = http_request(port, "GET", "/healthz", &[], None) {
            if body.contains("\"ok\":true") {
                return guard;
            }
        }
        assert!(Instant::now() < deadline, "relay did not become healthy");
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Minimal raw HTTP/1.1 client (this crate has no HTTP client dep): returns the body
/// on any complete response, `None` on connect failure.
fn http_request(
    port: u16,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<&str>,
) -> Option<String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    let body = body.unwrap_or("");
    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\nContent-Length: {}\r\n",
        body.len()
    );
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("\r\n");
    request.push_str(body);
    stream.write_all(request.as_bytes()).ok()?;
    let mut response = String::new();
    stream.read_to_string(&mut response).ok()?;
    response
        .split_once("\r\n\r\n")
        .map(|(_, body)| body.to_string())
}

/// Sign a ticket via `@exp/steer-ticket` (bun) — the shared-format contract.
fn mint_ticket(claims_json: &str) -> String {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/mint_ticket.ts");
    let output = Command::new("bun")
        .arg(&script)
        .arg(claims_json)
        .arg(SECRET)
        .output()
        .expect("run mint_ticket.ts");
    assert!(
        output.status.success(),
        "mint_ticket.ts failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

fn ws_url(port: u16, ticket: &str) -> String {
    format!("ws://127.0.0.1:{port}/ws?ticket={ticket}")
}

fn publisher_claims() -> String {
    format!(
        r#"{{"sub":"user-int","team":"team-int","sessionId":"{SESSION_ID}","role":"publisher"}}"#
    )
}

fn viewer_claims() -> String {
    format!(
        r#"{{"sub":"viewer-int","team":"team-int","sessionId":"{SESSION_ID}","role":"viewer"}}"#
    )
}

const CONTROL_CLAIMS: &str =
    r#"{"sub":"user-int","team":"","deviceLabel":"IntTestBox","role":"control"}"#;

// ---------------------------------------------------------------------------
// Test doubles over the production traits
// ---------------------------------------------------------------------------

/// Mints REAL tickets (bun + shared secret) — stands in for `steer.mintTicket`.
/// Optionally routes the FIRST connection through a severable proxy port.
struct BunTickets {
    relay_port: u16,
    proxy_port_once: Mutex<Option<u16>>,
}

impl PublisherTickets for BunTickets {
    fn mint(&self) -> Result<Option<MintedTicket>, ApiError> {
        let ticket = mint_ticket(&publisher_claims());
        let port = self
            .proxy_port_once
            .lock()
            .unwrap()
            .take()
            .unwrap_or(self.relay_port);
        Ok(Some(MintedTicket {
            url: ws_url(port, &ticket),
            ticket,
        }))
    }
}

/// The viewer half of [`BunTickets`] — real `role:"viewer"` tickets for the
/// same room (EXP-696).
struct BunViewerTickets {
    relay_port: u16,
}

impl ViewerTickets for BunViewerTickets {
    fn mint(&self) -> Result<Option<MintedTicket>, ApiError> {
        let ticket = mint_ticket(&viewer_claims());
        Ok(Some(MintedTicket {
            url: ws_url(self.relay_port, &ticket),
            ticket,
        }))
    }
}

struct BunControlApi {
    relay_port: u16,
}

impl ControlApi for BunControlApi {
    fn config_enabled(&self) -> Result<bool, ApiError> {
        Ok(true)
    }

    fn mint_control(&self, _device_label: &str) -> Result<Option<String>, ApiError> {
        Ok(Some(ws_url(self.relay_port, &mint_ticket(CONTROL_CLAIMS))))
    }
}

#[derive(Default)]
struct Recorded {
    inputs: Mutex<Vec<Vec<u8>>>,
    kills: Mutex<Vec<KillSignal>>,
    errors: Mutex<Vec<String>>,
}

fn recording_hooks(recorded: Arc<Recorded>) -> PublisherHooks {
    recording_hooks_with(recorded, None)
}

fn recording_hooks_with(
    recorded: Arc<Recorded>,
    answers: Option<Arc<AnswerLink>>,
) -> PublisherHooks {
    let r1 = recorded.clone();
    let r2 = recorded.clone();
    let r3 = recorded;
    PublisherHooks {
        write_input: Arc::new(move |bytes| r1.inputs.lock().unwrap().push(bytes.to_vec())),
        kill: Arc::new(move |signal| r2.kills.lock().unwrap().push(signal)),
        error: Arc::new(move |message| r3.errors.lock().unwrap().push(message)),
        answers,
        agent: steer::activity::SessionAgent::Claude,
        text_sink: None,
        attachments: None,
    }
}

fn wait_for(what: &str, predicate: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !predicate() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// The issue id of a single-issue [`steer::RemoteStart`], `None` for a batch.
fn issue_of(start: &steer::RemoteStart) -> Option<&str> {
    match &start.subject {
        steer::RemoteStartSubject::Issue(id) => Some(id.as_str()),
        steer::RemoteStartSubject::Batch { .. }
        | steer::RemoteStartSubject::Action { .. }
        | steer::RemoteStartSubject::Resume { .. } => None,
    }
}

// ---------------------------------------------------------------------------
// A severable TCP proxy (to force a publisher socket drop, §8.6)
// ---------------------------------------------------------------------------

struct SeverableProxy {
    port: u16,
    severed: Arc<AtomicBool>,
}

fn spawn_proxy(runtime: &SteerRuntime, target_port: u16) -> SeverableProxy {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let severed = Arc::new(AtomicBool::new(false));
    let severed_task = severed.clone();
    runtime.handle().spawn(async move {
        let listener = tokio::net::TcpListener::from_std(listener).unwrap();
        while let Ok((mut inbound, _)) = listener.accept().await {
            let Ok(mut outbound) =
                tokio::net::TcpStream::connect(("127.0.0.1", target_port)).await
            else {
                continue;
            };
            let severed = severed_task.clone();
            tokio::spawn(async move {
                let copy = tokio::io::copy_bidirectional(&mut inbound, &mut outbound);
                tokio::pin!(copy);
                loop {
                    tokio::select! {
                        _ = &mut copy => return,
                        _ = tokio::time::sleep(Duration::from_millis(25)) => {
                            if severed.load(Ordering::SeqCst) {
                                return; // drop both halves → RST/EOF each side
                            }
                        }
                    }
                }
            });
        }
    });
    SeverableProxy { port, severed }
}

// ---------------------------------------------------------------------------
// Viewer-side helpers (a fake phone over a raw tokio-tungstenite socket)
// ---------------------------------------------------------------------------

type ViewerWs = tokio_tungstenite::WebSocketStream<
    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
>;

/// Frames observed at the viewer, drained in the background.
#[derive(Default)]
struct ViewerLog {
    texts: Mutex<Vec<String>>,
    close: Mutex<Option<Option<u16>>>,
}

struct Viewer {
    tx: flume::Sender<Message>,
    log: Arc<ViewerLog>,
}

impl Viewer {
    fn send_text(&self, text: &str) {
        self.tx.send(Message::Text(text.to_string())).unwrap();
    }

    /// Every text frame seen so far.
    fn texts(&self) -> Vec<String> {
        self.log.texts.lock().unwrap().clone()
    }

    /// Whether an `activity` frame containing `needle` has arrived.
    fn saw_activity(&self, needle: &str) -> bool {
        self.texts()
            .iter()
            .any(|text| text.contains(r#""t":"activity""#) && text.contains(needle))
    }
}

fn connect_viewer(runtime: &SteerRuntime, port: u16) -> Viewer {
    connect_viewer_on(runtime, port, r#"{"t":"join","channel":"activity"}"#)
}

fn connect_viewer_on(runtime: &SteerRuntime, port: u16, join: &str) -> Viewer {
    let join = join.to_string();
    let ticket = mint_ticket(&viewer_claims());
    let url = ws_url(port, &ticket);
    let (tx, rx) = flume::unbounded::<Message>();
    let log = Arc::new(ViewerLog::default());
    let log_task = log.clone();
    let (ready_tx, ready_rx) = flume::bounded::<()>(1);
    runtime.handle().spawn(async move {
        let (mut ws, _): (ViewerWs, _) = tokio_tungstenite::connect_async(&url)
            .await
            .expect("viewer connect");
        ws.send(Message::Text(join)).await.expect("viewer join");
        let _ = ready_tx.send(());
        loop {
            tokio::select! {
                outbound = rx.recv_async() => {
                    let Ok(message) = outbound else { break };
                    if ws.send(message).await.is_err() { break; }
                }
                msg = ws.next() => match msg {
                    Some(Ok(Message::Text(text))) => log_task.texts.lock().unwrap().push(text),
                    Some(Ok(Message::Close(frame))) => {
                        *log_task.close.lock().unwrap() =
                            Some(frame.map(|f| u16::from(f.code)));
                    }
                    Some(Ok(_)) => {}
                    Some(Err(_)) | None => break,
                }
            }
        }
    });
    ready_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("viewer joined");
    Viewer { tx, log }
}

// ---------------------------------------------------------------------------
// The tests
// ---------------------------------------------------------------------------

#[test]
fn full_protocol_flow_against_the_real_relay() {
    if !bun_available() {
        eprintln!("skipping relay integration test: bun not on PATH");
        return;
    }
    let relay = start_relay();
    let runtime = SteerRuntime::new().unwrap();

    // ── Control channel: online presence + remote start routing (§8.3) ────
    let started: Arc<Mutex<Vec<steer::RemoteStart>>> = Arc::new(Mutex::new(Vec::new()));
    let started_clone = started.clone();
    let checked_in = Arc::new(AtomicBool::new(false));
    let checked_in_clone = checked_in.clone();
    let control = spawn_control_channel(
        &runtime,
        DeviceIdentity {
            caps: vec![],
            device_id: "device-int-1".to_string(),
            device_label: "IntTestBox".to_string(),
        },
        Arc::new(BunControlApi {
            relay_port: relay.port,
        }),
        Arc::new(move |start| started_clone.lock().unwrap().push(start)),
        Arc::new(move || checked_in_clone.store(true, Ordering::SeqCst)),
    );

    // The device appears in the phone picker's backing endpoint.
    wait_for("device presence", || {
        http_request(relay.port, "GET", "/devices/user-int", &[("x-relay-secret", SECRET)], None)
            .is_some_and(|body| body.contains("device-int-1") && body.contains("IntTestBox"))
    });

    // EXP-481: the check-in nudge rides the same control socket.
    let response = http_request(
        relay.port,
        "POST",
        "/devices/user-int/device-int-1/nudge",
        &[("x-relay-secret", SECRET)],
        None,
    )
    .expect("POST nudge");
    assert!(response.contains("\"delivered\":true"), "nudge delivered: {response}");
    wait_for("check_in delivery", || checked_in.load(Ordering::SeqCst));

    // Remote "Start on my desktop" → start_session lands on our socket.
    // Option-less body (an old client) → every option arrives None.
    let start_body = r#"{"userId":"user-int","deviceId":"device-int-1","issueId":"issue-remote-1"}"#;
    let response = http_request(
        relay.port,
        "POST",
        "/start",
        &[("x-relay-secret", SECRET), ("content-type", "application/json")],
        Some(start_body),
    )
    .expect("POST /start");
    assert!(response.contains("\"ok\":true"), "start routed: {response}");
    wait_for("start_session delivery", || {
        started.lock().unwrap().iter().any(|s| issue_of(s) == Some("issue-remote-1"))
    });
    {
        let starts = started.lock().unwrap();
        let start = starts.iter().find(|s| issue_of(s) == Some("issue-remote-1")).unwrap();
        assert_eq!(start.model, None);
        assert_eq!(start.effort, None);
        assert_eq!(start.ultracode, None);
        assert_eq!(start.plan_mode, None);
    }

    // Start-coding dialog options (EXP-149) ride the same route end-to-end.
    let options_body = r#"{"userId":"user-int","deviceId":"device-int-1","issueId":"issue-remote-2","model":"opus","effort":"","ultracode":true,"planMode":true}"#;
    let response = http_request(
        relay.port,
        "POST",
        "/start",
        &[("x-relay-secret", SECRET), ("content-type", "application/json")],
        Some(options_body),
    )
    .expect("POST /start with options");
    assert!(response.contains("\"ok\":true"), "options start routed: {response}");
    wait_for("start_session options delivery", || {
        started.lock().unwrap().iter().any(|s| issue_of(s) == Some("issue-remote-2"))
    });
    {
        let starts = started.lock().unwrap();
        let start = starts.iter().find(|s| issue_of(s) == Some("issue-remote-2")).unwrap();
        assert_eq!(start.model.as_deref(), Some("opus"));
        assert_eq!(start.effort.as_deref(), Some(""));
        assert_eq!(start.ultracode, Some(true));
        assert_eq!(start.plan_mode, Some(true));
    }

    // Batch start (EXP-106): issueIds + teamId + repo ride the same
    // /start route → the frame's `RemoteStartSubject::Batch` lands on our
    // socket. (The relay's batch /start support lands concurrently — this
    // case exercises the frozen wire contract end-to-end.)
    let batch_body = r#"{"userId":"user-int","deviceId":"device-int-1","issueIds":["issue-b1","issue-b2"],"teamId":"ws-b","repo":{"repositoryId":"repo-b","fullName":"acme/api","defaultBranch":"main"},"model":"sonnet","effort":"high","ultracode":true,"planMode":false}"#;
    let response = http_request(
        relay.port,
        "POST",
        "/start",
        &[("x-relay-secret", SECRET), ("content-type", "application/json")],
        Some(batch_body),
    )
    .expect("POST /start batch");
    assert!(response.contains("\"ok\":true"), "batch start routed: {response}");
    wait_for("batch start_session delivery", || {
        started.lock().unwrap().iter().any(|s| {
            matches!(
                &s.subject,
                steer::RemoteStartSubject::Batch { issue_ids, .. }
                    if issue_ids.as_slice() == ["issue-b1".to_string(), "issue-b2".to_string()]
            )
        })
    });
    {
        let starts = started.lock().unwrap();
        let start = starts
            .iter()
            .find(|s| matches!(s.subject, steer::RemoteStartSubject::Batch { .. }))
            .unwrap();
        let steer::RemoteStartSubject::Batch {
            issue_ids,
            team_id,
            repo,
        } = &start.subject
        else {
            unreachable!("filtered to a batch above");
        };
        assert_eq!(
            issue_ids.as_slice(),
            ["issue-b1".to_string(), "issue-b2".to_string()]
        );
        assert_eq!(team_id, "ws-b");
        assert_eq!(
            repo,
            &steer::StartRepoGroup {
                repository_id: "repo-b".to_string(),
                full_name: "acme/api".to_string(),
                default_branch: "main".to_string(),
            }
        );
        assert_eq!(start.model.as_deref(), Some("sonnet"));
        assert_eq!(start.effort.as_deref(), Some("high"));
        assert_eq!(start.ultracode, Some(true));
        assert_eq!(start.plan_mode, Some(false));
    }

    // ── Publisher: hello, room goes live (§8.4) ──────────────────────────
    let recorded = Arc::new(Recorded::default());
    let (answer_link, answers_rx) = AnswerLink::new();
    let handle = publish(
        &runtime,
        PublishSpec {
            session_id: SESSION_ID.to_string(),
            issue_id: Some("issue-int-1".to_string()),
        },
        Arc::new(BunTickets {
            relay_port: relay.port,
            proxy_port_once: Mutex::new(None),
        }),
        recording_hooks_with(recorded.clone(), Some(answer_link)),
    );
    wait_for("room live", || {
        http_request(relay.port, "GET", &format!("/sessions/{SESSION_ID}"), &[("x-relay-secret", SECRET)], None)
            .is_some_and(|body| body.contains("\"live\":true"))
    });

    // Activity published BEFORE any viewer joins → the relay's replay log.
    let activity = handle.activity_sender();
    activity.send(ActivityEvent::narration("early-scrollback"));
    std::thread::sleep(Duration::from_millis(300)); // let the relay ingest

    // ── Viewer join: the relay's own reset + replay of the log ────────────
    let viewer = connect_viewer(&runtime, relay.port);
    wait_for("viewer activity_reset", || {
        viewer.texts().iter().any(|t| t == r#"{"t":"activity_reset"}"#)
    });
    wait_for("replay at the viewer", || viewer.saw_activity("early-scrollback"));

    // ── Steer input reaches the PTY-writer hook directly (EXP-312 —
    // seamless and owner-only: no claim, no perm tier) ────────────────────
    viewer.send_text(r#"{"t":"input","data":"echo hi\r"}"#);
    wait_for("input injected", || {
        recorded.inputs.lock().unwrap().iter().any(|bytes| bytes == b"echo hi\r")
    });

    // ── EXP-249: a semantic `answer` rides the same membership gate, and
    // reaches the emitter seam instead of the PTY writer ─────────────────
    viewer.send_text(r#"{"t":"answer","questionId":"toolu_1#0","askId":"toolu_1","keys":["2"]}"#);
    let answer = answers_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("answer forwarded to the emitter");
    assert_eq!(
        answer,
        RemoteAnswer {
            question_id: "toolu_1#0".to_string(),
            ask_id: Some("toolu_1".to_string()),
            keys: vec!["2".to_string()],
            text: None,
        }
    );
    assert_eq!(
        recorded.inputs.lock().unwrap().len(),
        1,
        "an answer is never replayed as keystrokes"
    );

    // ── Live tail: a fresh activity event reaches the joined viewer ───────
    activity.send(ActivityEvent::tool("Edit", Some("src/main.rs".to_string())));
    wait_for("live tail at the viewer", || viewer.saw_activity("src/main.rs"));

    // ── Kill from the phone: publisher tears down, room closes (§8.4) ─────
    viewer.send_text(r#"{"t":"kill"}"#);
    wait_for("kill hook", || {
        recorded.kills.lock().unwrap().contains(&KillSignal::RemoteKill)
    });
    wait_for("publisher stopped", || !handle.is_active());
    // The publisher's clean bye closes the room: viewer gets bye + 4001.
    wait_for("viewer bye", || {
        viewer
            .log
            .texts
            .lock()
            .unwrap()
            .iter()
            .any(|t| t.contains("\"t\":\"bye\""))
    });
    wait_for("viewer closed 4001", || {
        *viewer.log.close.lock().unwrap() == Some(Some(4001))
    });
    wait_for("room gone", || {
        http_request(relay.port, "GET", &format!("/sessions/{SESSION_ID}"), &[("x-relay-secret", SECRET)], None)
            .is_some_and(|body| body.contains("\"live\":false"))
    });

    assert!(recorded.errors.lock().unwrap().is_empty(), "no surfaced errors");
    control.stop();
}

#[test]
fn publisher_reconnects_and_resumes_the_room_after_a_socket_drop() {
    if !bun_available() {
        eprintln!("skipping relay integration test: bun not on PATH");
        return;
    }
    let relay = start_relay();
    let runtime = SteerRuntime::new().unwrap();

    // First connection rides a severable proxy; the re-mint after the drop
    // returns the direct relay URL (a fresh ticket each attempt, §8.6).
    let proxy = spawn_proxy(&runtime, relay.port);
    let recorded = Arc::new(Recorded::default());
    let handle = publish(
        &runtime,
        PublishSpec {
            session_id: SESSION_ID.to_string(),
            issue_id: Some("issue-int-2".to_string()),
        },
        Arc::new(BunTickets {
            relay_port: relay.port,
            proxy_port_once: Mutex::new(Some(proxy.port)),
        }),
        recording_hooks(recorded.clone()),
    );
    wait_for("room live via proxy", || {
        http_request(relay.port, "GET", &format!("/sessions/{SESSION_ID}"), &[("x-relay-secret", SECRET)], None)
            .is_some_and(|body| body.contains("\"live\":true"))
    });

    // A viewer joins and sees the pre-drop activity.
    let viewer = connect_viewer(&runtime, relay.port);
    let activity = handle.activity_sender();
    activity.send(ActivityEvent::narration("before-drop"));
    wait_for("pre-drop tail", || viewer.saw_activity("before-drop"));

    // Sever the proxied publisher socket — an unexpected drop, no bye.
    proxy.severed.store(true, Ordering::SeqCst);

    // The publisher re-mints (direct URL now), re-hellos, and the relay
    // RESUMES the same room (staleTimer cleared) — the viewer never left.
    wait_for("room live again after reconnect", || {
        http_request(relay.port, "GET", &format!("/sessions/{SESSION_ID}"), &[("x-relay-secret", SECRET)], None)
            .is_some_and(|body| body.contains("\"live\":true") && body.contains("\"viewers\":1"))
    });
    // EXP-249: the reconnect REBUILDS the feed — the joined viewer sees a
    // second `activity_reset` followed by the whole journal again.
    wait_for("history re-published after reconnect", || {
        viewer
            .texts()
            .iter()
            .filter(|text| *text == r#"{"t":"activity_reset"}"#)
            .count()
            >= 2
            && viewer
                .texts()
                .iter()
                .filter(|text| text.contains("before-drop"))
                .count()
                >= 2
    });
    // …and live publishing resumes into the SAME room.
    wait_for("post-reconnect tail at the same viewer", || {
        activity.send(ActivityEvent::narration("after-reconnect"));
        std::thread::sleep(Duration::from_millis(50));
        viewer.saw_activity("after-reconnect")
    });
    assert!(handle.is_active(), "publisher still active after resume");
    assert!(recorded.errors.lock().unwrap().is_empty());

    handle.shutdown(Some("exit:0".to_string()));
    wait_for("clean end", || !handle.is_active());
}

/// EXP-696: the desktop's own VIEWER client against a real relay room fed by
/// the real publisher — the seam the IDE's watch/steer UI sits on. The fake
/// phone above proves the relay's contract; this proves OURS: the join frame
/// the relay's zod accepts, the phases, the events, and the feed they build.
#[test]
fn the_production_viewer_watches_and_steers_a_real_room() {
    if !bun_available() {
        eprintln!("skipping relay integration test: bun not on PATH");
        return;
    }
    let relay = start_relay();
    let runtime = SteerRuntime::new().unwrap();

    // The room, published by the production publisher.
    let recorded = Arc::new(Recorded::default());
    let (answer_link, answers_rx) = AnswerLink::new();
    let publisher = publish(
        &runtime,
        PublishSpec {
            session_id: SESSION_ID.to_string(),
            issue_id: None,
        },
        Arc::new(BunTickets {
            relay_port: relay.port,
            proxy_port_once: Mutex::new(None),
        }),
        recording_hooks_with(recorded.clone(), Some(answer_link)),
    );
    wait_for("room live", || {
        http_request(
            relay.port,
            "GET",
            &format!("/sessions/{SESSION_ID}"),
            &[("x-relay-secret", SECRET)],
            None,
        )
        .is_some_and(|body| body.contains("\"live\":true"))
    });
    let activity = publisher.activity_sender();
    // Published BEFORE the viewer exists → it must arrive via the replay.
    activity.send(ActivityEvent::narration("early-scrollback"));

    // The viewer, with a short starting-retry so a race with the room's
    // creation costs milliseconds rather than seconds.
    let (events_tx, events) = flume::unbounded();
    let viewer = spawn_viewer_with(
        &runtime,
        Arc::new(BunViewerTickets {
            relay_port: relay.port,
        }),
        SESSION_ID.to_string(),
        events_tx,
        ViewerTimings {
            retry_base: Duration::from_millis(100),
            retry_cap: Duration::from_millis(400),
            ..ViewerTimings::default()
        },
    );

    // Everything the viewer reports goes straight into the feed model — no
    // interpretation in between, which is the whole point of the split.
    let feed = Arc::new(Mutex::new(SteerFeed::new()));
    let phase = Arc::new(Mutex::new(ViewerPhase::Connecting));
    let feed_task = feed.clone();
    let phase_task = phase.clone();
    std::thread::spawn(move || {
        while let Ok(event) = events.recv() {
            let mut feed = feed_task.lock().unwrap();
            match event {
                ViewerEvent::Phase(next) => *phase_task.lock().unwrap() = next,
                ViewerEvent::Activity(activity) => feed.apply(activity),
                ViewerEvent::Reset => feed.apply_reset(),
                ViewerEvent::Synced => feed.apply_synced(),
                ViewerEvent::LocalMessage(text) => {
                    feed.push_local_message(&text);
                }
                ViewerEvent::Keepalive | ViewerEvent::Connected(_) => {}
            }
        }
    });

    // ── Join → live, and the relay's replay lands in the feed ─────────────
    wait_for("viewer live", || {
        *phase.lock().unwrap() == ViewerPhase::Live
    });
    wait_for("replayed scrollback in the feed", || {
        let feed = feed.lock().unwrap();
        // The relay answers a join with reset + replay + `activity_synced`
        // (EXP-656), so the staged swap commits on its own.
        !feed.is_staging() && feed_carries(&feed, "early-scrollback")
    });

    // ── The live tail ─────────────────────────────────────────────────────
    activity.send(ActivityEvent::tool("Edit", Some("src/main.rs".to_string())));
    wait_for("live tail in the feed", || {
        feed_carries(&feed.lock().unwrap(), "Edit")
    });

    // ── Steering back: a message reaches the publisher's PTY writer as the
    // text, then a SEPARATE `\r` (EXP-72) ─────────────────────────────────
    wait_for("message sent", || viewer.send_message("do the thing"));
    wait_for("message injected", || {
        let inputs = recorded.inputs.lock().unwrap();
        inputs.iter().any(|bytes| bytes == b"do the thing")
            && inputs.iter().any(|bytes| bytes == b"\r")
    });
    // It shows locally at once, and the publisher's transcript echo of the
    // same text is deduped away rather than rendering twice.
    assert!(feed_carries(&feed.lock().unwrap(), "do the thing"));
    activity.send(ActivityEvent::user_message("do the thing"));
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(
        feed_occurrences(&feed.lock().unwrap(), "do the thing"),
        1,
        "the local echo swallowed its transcript twin"
    );

    // ── A semantic answer reaches the emitter seam, never the PTY ─────────
    let before = recorded.inputs.lock().unwrap().len();
    assert!(viewer.send_answer(
        "toolu_1#0",
        Some("toolu_1"),
        &["2".to_string()],
        Some("purple"),
    ));
    let answer = answers_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("answer forwarded to the emitter");
    assert_eq!(
        answer,
        RemoteAnswer {
            question_id: "toolu_1#0".to_string(),
            ask_id: Some("toolu_1".to_string()),
            keys: vec!["2".to_string()],
            text: Some("purple".to_string()),
        }
    );
    assert_eq!(
        recorded.inputs.lock().unwrap().len(),
        before,
        "an answer is never replayed as keystrokes"
    );

    // ── The publisher's clean end closes the room; the viewer reports it ──
    publisher.shutdown(Some("exit:0".to_string()));
    wait_for("viewer sees the end", || {
        matches!(*phase.lock().unwrap(), ViewerPhase::Ended { .. })
    });
    assert!(!viewer.is_active(), "an ended room is never redialed");
    assert!(recorded.errors.lock().unwrap().is_empty());
}

/// Whether any feed item's text/name carries `needle`.
fn feed_carries(feed: &SteerFeed, needle: &str) -> bool {
    feed_occurrences(feed, needle) > 0
}

fn feed_occurrences(feed: &SteerFeed, needle: &str) -> usize {
    feed.items()
        .iter()
        .filter(|item| match &item.kind {
            steer::FeedKind::Narration { text } | steer::FeedKind::UserMessage { text } => {
                text.contains(needle)
            }
            steer::FeedKind::Tool { name, .. } => name.contains(needle),
            _ => false,
        })
        .count()
}
