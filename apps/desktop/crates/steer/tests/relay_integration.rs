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
//! * publisher `hello` (true geometry) → viewer `join` gets resize + ring
//!   REPLAY (relay-side, transparent to the publisher) → live `0x01` tail;
//! * viewer `claim` → `input` reaches the publisher's PTY-writer hook;
//! * publisher take-over (`claim` on the publisher socket) force-clears the
//!   steerer (`publisherTakeover`) and a de-claimed viewer's input no longer
//!   flows (§8.5);
//! * viewer `kill` → publisher kill hook + clean `bye` → the relay closes the
//!   room (`CLOSE_SESSION_ENDED` at the viewer);
//! * a severed publisher socket (TCP proxy dropped) → re-mint → re-`hello`
//!   resumes the SAME room; the joined viewer keeps streaming (§8.6).
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
use steer::publisher::{publish, KillSignal, Presence, PublishSpec, PublisherHooks, PublisherTickets};
use steer::{SteerRuntime, OUTPUT_OPCODE};

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

/// Minimal raw HTTP/1.1 client (no ureq dep in this crate): returns the body
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
        r#"{{"sub":"user-int","ws":"ws-int","sessionId":"{SESSION_ID}","role":"publisher","perm":"steer"}}"#
    )
}

fn viewer_claims() -> String {
    format!(
        r#"{{"sub":"viewer-int","ws":"ws-int","name":"Phone","sessionId":"{SESSION_ID}","role":"viewer","perm":"steer"}}"#
    )
}

const CONTROL_CLAIMS: &str =
    r#"{"sub":"user-int","ws":"","deviceLabel":"IntTestBox","role":"control","perm":"steer"}"#;

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
    presences: Mutex<Vec<Presence>>,
    errors: Mutex<Vec<String>>,
}

fn recording_hooks(recorded: Arc<Recorded>) -> PublisherHooks {
    let r1 = recorded.clone();
    let r2 = recorded.clone();
    let r3 = recorded.clone();
    let r4 = recorded;
    PublisherHooks {
        write_input: Arc::new(move |bytes| r1.inputs.lock().unwrap().push(bytes.to_vec())),
        resize: Arc::new(|_cols, _rows| {}),
        geometry: Arc::new(|| (100, 30)),
        kill: Arc::new(move |signal| r2.kills.lock().unwrap().push(signal)),
        presence: Arc::new(move |presence| r3.presences.lock().unwrap().push(presence)),
        error: Arc::new(move |message| r4.errors.lock().unwrap().push(message)),
    }
}

fn wait_for(what: &str, predicate: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !predicate() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(25));
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
    binaries: Mutex<Vec<Vec<u8>>>,
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

    /// The latest `presence` frame's `steererId`, if any presence was seen.
    fn last_steerer(&self) -> Option<Option<String>> {
        let texts = self.log.texts.lock().unwrap();
        texts
            .iter()
            .rfind(|t| t.contains("\"t\":\"presence\""))
            .map(|t| {
                let value: serde_json::Value = serde_json::from_str(t).unwrap();
                value["steererId"].as_str().map(|s| s.to_string())
            })
    }

    fn binary_payloads(&self) -> Vec<Vec<u8>> {
        self.log
            .binaries
            .lock()
            .unwrap()
            .iter()
            .map(|frame| {
                assert_eq!(frame[0], OUTPUT_OPCODE, "binary frames carry 0x01");
                frame[1..].to_vec()
            })
            .collect()
    }
}

fn connect_viewer(runtime: &SteerRuntime, port: u16) -> Viewer {
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
        ws.send(Message::Text(r#"{"t":"join"}"#.to_string()))
            .await
            .expect("viewer join");
        let _ = ready_tx.send(());
        loop {
            tokio::select! {
                outbound = rx.recv_async() => {
                    let Ok(message) = outbound else { break };
                    if ws.send(message).await.is_err() { break; }
                }
                msg = ws.next() => match msg {
                    Some(Ok(Message::Text(text))) => log_task.texts.lock().unwrap().push(text),
                    Some(Ok(Message::Binary(bytes))) => {
                        log_task.binaries.lock().unwrap().push(bytes);
                    }
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
    let control = spawn_control_channel(
        &runtime,
        DeviceIdentity {
            device_id: "device-int-1".to_string(),
            device_label: "IntTestBox".to_string(),
        },
        Arc::new(BunControlApi {
            relay_port: relay.port,
        }),
        Arc::new(move |start| started_clone.lock().unwrap().push(start)),
    );

    // The device appears in the phone picker's backing endpoint.
    wait_for("device presence", || {
        http_request(relay.port, "GET", "/devices/user-int", &[("x-relay-secret", SECRET)], None)
            .is_some_and(|body| body.contains("device-int-1") && body.contains("IntTestBox"))
    });

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
        started.lock().unwrap().iter().any(|s| s.issue_id == "issue-remote-1")
    });
    {
        let starts = started.lock().unwrap();
        let start = starts.iter().find(|s| s.issue_id == "issue-remote-1").unwrap();
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
        started.lock().unwrap().iter().any(|s| s.issue_id == "issue-remote-2")
    });
    {
        let starts = started.lock().unwrap();
        let start = starts.iter().find(|s| s.issue_id == "issue-remote-2").unwrap();
        assert_eq!(start.model.as_deref(), Some("opus"));
        assert_eq!(start.effort.as_deref(), Some(""));
        assert_eq!(start.ultracode, Some(true));
        assert_eq!(start.plan_mode, Some(true));
    }

    // ── Publisher: hello with true geometry, room goes live (§8.4) ────────
    let recorded = Arc::new(Recorded::default());
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
        recording_hooks(recorded.clone()),
    );
    wait_for("room live", || {
        http_request(relay.port, "GET", &format!("/sessions/{SESSION_ID}"), &[("x-relay-secret", SECRET)], None)
            .is_some_and(|body| body.contains("\"live\":true"))
    });

    // Output BEFORE any viewer joins → lands in the relay's ring.
    handle.raw_sink().on_output(b"early-scrollback\r\n");
    std::thread::sleep(Duration::from_millis(300)); // let the relay ingest

    // ── Viewer join: resize + relay-side ring replay, NO publisher resync ─
    let viewer = connect_viewer(&runtime, relay.port);
    wait_for("viewer resize frame", || {
        viewer
            .log
            .texts
            .lock()
            .unwrap()
            .iter()
            .any(|t| t == r#"{"t":"resize","cols":100,"rows":30}"#)
    });
    wait_for("ring replay at the viewer", || {
        viewer
            .binary_payloads()
            .iter()
            .any(|payload| payload == b"early-scrollback\r\n")
    });
    // The publisher saw a presence broadcast for the join.
    wait_for("publisher presence", || {
        recorded
            .presences
            .lock()
            .unwrap()
            .iter()
            .any(|presence| presence.viewers.iter().any(|viewer| viewer.name == "Phone"))
    });

    // ── Claim → steer input reaches the PTY-writer hook (§8.5) ────────────
    viewer.send_text(r#"{"t":"claim"}"#);
    wait_for("steerer claimed", || {
        viewer.last_steerer() == Some(Some("viewer-int".to_string()))
    });
    viewer.send_text(r#"{"t":"input","data":"echo hi\r"}"#);
    wait_for("input injected", || {
        recorded.inputs.lock().unwrap().iter().any(|bytes| bytes == b"echo hi\r")
    });

    // ── Live tail: teed output reaches the viewer as 0x01 frames ──────────
    handle.raw_sink().on_output(b"live-tail-bytes");
    wait_for("live tail at the viewer", || {
        viewer
            .binary_payloads()
            .iter()
            .any(|payload| payload == b"live-tail-bytes")
    });

    // ── Take over: publisher claim force-clears the remote steerer ────────
    handle.take_over();
    wait_for("steerer cleared", || viewer.last_steerer() == Some(None));
    // A de-claimed viewer's input no longer flows (single-steerer rule).
    let inputs_before = recorded.inputs.lock().unwrap().len();
    viewer.send_text(r#"{"t":"input","data":"blocked\r"}"#);
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(
        recorded.inputs.lock().unwrap().len(),
        inputs_before,
        "input from a non-steerer must be dropped by the relay"
    );

    // ── Kill from the phone: publisher tears down, room closes (§8.4/§8.5) ─
    viewer.send_text(r#"{"t":"claim"}"#);
    wait_for("steerer re-claimed", || {
        viewer.last_steerer() == Some(Some("viewer-int".to_string()))
    });
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

    // A viewer joins and sees the pre-drop output.
    let viewer = connect_viewer(&runtime, relay.port);
    handle.raw_sink().on_output(b"before-drop");
    wait_for("pre-drop tail", || {
        viewer.binary_payloads().iter().any(|payload| payload == b"before-drop")
    });

    // Sever the proxied publisher socket — an unexpected drop, no bye.
    proxy.severed.store(true, Ordering::SeqCst);

    // The publisher re-mints (direct URL now), re-hellos, and the relay
    // RESUMES the same room (staleTimer cleared) — the viewer never left.
    wait_for("room live again after reconnect", || {
        http_request(relay.port, "GET", &format!("/sessions/{SESSION_ID}"), &[("x-relay-secret", SECRET)], None)
            .is_some_and(|body| body.contains("\"live\":true") && body.contains("\"viewers\":1"))
    });
    // Live teeing resumes into the SAME room, reaching the same viewer —
    // and no resync was needed (§8.6: reconnect does not wait for one).
    wait_for("post-reconnect tail at the same viewer", || {
        handle.raw_sink().on_output(b"after-reconnect");
        std::thread::sleep(Duration::from_millis(50));
        viewer.binary_payloads().iter().any(|payload| payload == b"after-reconnect")
    });
    assert!(handle.is_active(), "publisher still active after resume");
    assert!(recorded.errors.lock().unwrap().is_empty());

    handle.shutdown(Some("exit:0".to_string()));
    wait_for("clean end", || !handle.is_active());
}
