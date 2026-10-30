//! Integration test against the REAL steer relay (masterplan-v3 §8): the
//! device's CONTROL socket, the only socket a desktop or daemon opens to the
//! relay since VAPP-43 (the relay carries no session content; sessions are
//! served over eNet, `tests/enet_steer.rs`).
//!
//! Boots `apps/steer-relay` locally (bun, `STEER_RELAY_SECRET=test-secret`,
//! ephemeral port), mints a control ticket by calling `@exp/steer-ticket`'s
//! `signSteerTicket` directly with the same secret (a tiny bun script —
//! `tests/support/mint_ticket.ts`), then drives the production control
//! channel and a live publisher through the relay's admin routes:
//!
//! * control `online` → `POST /devices/:userId/:deviceId/nudge` reaches our
//!   socket as `check_in` (§8.3; EXP-1292: starts never ride the relay);
//! * `POST /sessions/:id/interrupt` → `interrupt {sessionId}` down the control
//!   socket → the live run's interrupt hook;
//! * `POST /sessions/:id/compact` → `compact_request {sessionId, keep}` → the
//!   run's compact hook → `compact_verdict` back up the control socket → the
//!   route's answer;
//! * `POST /sessions/:id/kill` → `kill {sessionId}` → the run's kill hook.
//!
//! Skips (passes) when `bun` is unavailable so plain `cargo test` stays green
//! on machines without the JS toolchain. The relay child is killed on drop.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use api::error::ApiError;
use steer::control_channel::{spawn_control_channel, ControlApi, DeviceIdentity};
use steer::publisher::{publish, KillSignal, PublishSpec, PublisherHooks};
use steer::{CompactRefusal, CompactVerdict, SteerRuntime};

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
    stream.set_read_timeout(Some(Duration::from_secs(10))).ok()?;
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
    // Read until the body is complete (by `content-length`) or EOF: a
    // response the relay sends late may leave the connection open.
    let mut response = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let text = String::from_utf8_lossy(&response).to_string();
        if let Some((head, body)) = text.split_once("\r\n\r\n") {
            let length = head
                .lines()
                .find_map(|line| line.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().to_string()))
                .and_then(|v| v.parse::<usize>().ok());
            if length.is_some_and(|length| body.len() >= length) {
                return Some(body.to_string());
            }
        }
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => response.extend_from_slice(&chunk[..n]),
            Err(_) => return None,
        }
    }
    String::from_utf8_lossy(&response)
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

const CONTROL_CLAIMS: &str =
    r#"{"sub":"user-int","team":"","deviceLabel":"IntTestBox","role":"control"}"#;

struct BunControlApi {
    relay_port: u16,
}

impl ControlApi for BunControlApi {
    fn config_enabled(&self) -> Result<bool, ApiError> {
        Ok(true)
    }

    fn mint_control(&self) -> Result<Option<String>, ApiError> {
        Ok(Some(ws_url(self.relay_port, &mint_ticket(CONTROL_CLAIMS))))
    }
}

// NEW-93: the panic names the CALLER's line, not this helper's.
#[track_caller]
fn wait_for(what: &str, predicate: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !predicate() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// `POST /sessions/:id/<verb>` naming this test's device.
fn session_route(port: u16, session_id: &str, verb: &str, extra: &str) -> Option<String> {
    let body = format!(r#"{{"userId":"user-int","deviceId":"device-int-1"{extra}}}"#);
    http_request(
        port,
        "POST",
        &format!("/sessions/{session_id}/{verb}"),
        &[("x-relay-secret", SECRET), ("content-type", "application/json")],
        Some(&body),
    )
}

#[test]
fn the_control_socket_carries_presence_and_session_commands() {
    if !bun_available() {
        eprintln!("skipping relay integration test: bun not on PATH");
        return;
    }
    let relay = start_relay();
    let runtime = SteerRuntime::new().unwrap();

    // ── Presence + the check-in nudge (§8.3) ─────────────────────────────
    let checked_in = Arc::new(AtomicBool::new(false));
    let checked_in_clone = checked_in.clone();
    let control = spawn_control_channel(
        &runtime,
        DeviceIdentity {
            device_id: "device-int-1".to_string(),
            device_label: "IntTestBox".to_string(),
        },
        Arc::new(BunControlApi {
            relay_port: relay.port,
        }),
        Arc::new(move || checked_in_clone.store(true, Ordering::SeqCst)),
        // NEW-55: no voice here.
        Arc::new(|_ring| {}),
    );
    // EXP-672: `delivered:true` IS the presence assertion: the relay only
    // reports a delivery once `online` registered the device.
    wait_for("device presence (nudge delivered)", || {
        http_request(
            relay.port,
            "POST",
            "/devices/user-int/device-int-1/nudge",
            &[("x-relay-secret", SECRET)],
            None,
        )
        .is_some_and(|body| body.contains("\"delivered\":true"))
    });
    wait_for("check_in delivery", || checked_in.load(Ordering::SeqCst));

    // ── A live run on this device ────────────────────────────────────────
    let kills: Arc<Mutex<Vec<KillSignal>>> = Arc::new(Mutex::new(Vec::new()));
    let interrupts = Arc::new(AtomicUsize::new(0));
    let asked: Arc<Mutex<Vec<Option<String>>>> = Arc::new(Mutex::new(Vec::new()));
    let (kills_hook, interrupts_hook, asked_hook) = (kills.clone(), interrupts.clone(), asked.clone());
    let handle = publish(
        &runtime,
        PublishSpec { session_id: SESSION_ID.to_string(), ..Default::default() },
        PublisherHooks {
            write_input: Arc::new(|_| {}),
            kill: Arc::new(move |signal| kills_hook.lock().unwrap().push(signal)),
            answers: None,
            agent: steer::activity::SessionAgent::Claude,
            text_sink: None,
            attachments: None,
            commands: None,
            config: None,
            interrupt: Some(Arc::new(move || {
                interrupts_hook.fetch_add(1, Ordering::SeqCst);
            })),
            unqueue: None,
            compact: Some(Arc::new(move |keep: Option<String>| {
                let verdict = match keep.as_deref() {
                    Some("open threads") => CompactVerdict::Accepted,
                    _ => CompactVerdict::Refused(CompactRefusal::TooEarly),
                };
                asked_hook.lock().unwrap().push(keep);
                verdict
            })),
        },
    );

    // ── interrupt → the run's Stop, the run lives on ─────────────────────
    let delivered = session_route(relay.port, SESSION_ID, "interrupt", "").unwrap_or_default();
    assert!(delivered.contains("\"delivered\":true"), "{delivered}");
    wait_for("interrupt hook", || interrupts.load(Ordering::SeqCst) == 1);
    assert!(kills.lock().unwrap().is_empty());

    // ── compact → the hook's verdict, back up the control socket ─────────
    let accepted = session_route(relay.port, SESSION_ID, "compact", r#","keep":"open threads""#).unwrap_or_default();
    assert!(accepted.contains("\"delivered\":true") && accepted.contains("\"accepted\":true"), "{accepted}");
    let refused = session_route(relay.port, SESSION_ID, "compact", "").unwrap_or_default();
    assert!(refused.contains("\"accepted\":false") && refused.contains("\"refusedBecause\":\"too_early\""), "{refused}");
    assert_eq!(asked.lock().unwrap().clone(), vec![Some("open threads".to_string()), None]);

    // A session this device does not run is ignored: the relay's own timer
    // answers (`no_verdict`), and nothing here moves.
    let other = session_route(relay.port, "sess-not-here", "compact", "").unwrap_or_default();
    assert!(other.contains("\"no_verdict\""), "{other}");

    // ── kill → the run ends ──────────────────────────────────────────────
    let delivered = session_route(relay.port, SESSION_ID, "kill", "").unwrap_or_default();
    assert!(delivered.contains("\"delivered\":true"), "{delivered}");
    wait_for("kill hook", || !kills.lock().unwrap().is_empty());
    assert_eq!(kills.lock().unwrap()[0], KillSignal::RemoteKill);
    wait_for("publisher stopped", || !handle.is_active());

    control.stop();
}
