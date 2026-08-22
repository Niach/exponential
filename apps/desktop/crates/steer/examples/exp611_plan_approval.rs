//! EXP-611 manual e2e harness: remote plan approval with the desktop
//! viewport SCROLLED into history — against the REAL `claude` TUI: relay
//! (bun) + production publisher + hooks sidecar + activity emitter, with a
//! viewer WebSocket that approves the plan remotely the way a mobile steerer
//! does.
//!
//! Run: `cargo run -p steer --example exp611_plan_approval` (needs `bun` and
//! a logged-in `claude` on PATH; spends a few tokens).
//!
//! The field failure (EXP-611): the user read a long plan on the desktop —
//! scrolling the embedded terminal up into history — then tried to approve
//! from the phone. `handle_answer` refused a scrolled viewport as TRANSIENT
//! on every attempt, so each tap parked, expired after the retry TTL, and
//! was dropped without an ack: the phone showed "No confirmation from the
//! desktop. Pick again to retry." forever. The fix snaps the viewport to the
//! live bottom before injecting, exactly like local input would.
//!
//! What to watch: the plan `question` frame must reach the viewer; the
//! harness then scrolls the terminal viewport UP (the reading-the-plan
//! moment) and only then answers remotely ("2" = manually approve). The
//! answer must ack and the question must resolve.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

use api::error::ApiError;
use api::steer::MintedTicket;
use steer::publisher::{
    publish, pty_writer_input_hook, PublishSpec, PublisherHooks, PublisherTickets,
};
use steer::{
    hook_settings_json, write_hook_curl_config, AnswerLink, EmitterConfig, HookServer, Steering,
    SteerRuntime, HOOK_CONFIG_ENV, HOOK_PORT_ENV,
};
use terminal::{display_offset, screen_lines, scroll_up, SpawnSpec, Terminal};

const SECRET: &str = "exp611-secret";
const SESSION_ID: &str = "61161161-1611-4611-8611-611611611611";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .expect("repo root above apps/desktop/crates/steer")
        .to_path_buf()
}

fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
    listener.local_addr().unwrap().port()
}

struct RelayGuard(Child);
impl Drop for RelayGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn healthz(port: u16) -> bool {
    let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let request =
        format!("GET /healthz HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }
    let mut response = String::new();
    let _ = stream.read_to_string(&mut response);
    response.contains(r#""ok":true"#)
}

fn start_relay(port: u16) -> RelayGuard {
    let child = Command::new("bun")
        .arg("src/index.ts")
        .current_dir(repo_root().join("apps/steer-relay"))
        .env("PORT", port.to_string())
        .env("STEER_RELAY_SECRET", SECRET)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn bun steer-relay");
    let guard = RelayGuard(child);
    let deadline = Instant::now() + Duration::from_secs(15);
    while !healthz(port) {
        assert!(Instant::now() < deadline, "relay did not become healthy");
        std::thread::sleep(Duration::from_millis(100));
    }
    guard
}

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

struct HarnessTickets {
    relay_port: u16,
}
impl PublisherTickets for HarnessTickets {
    fn mint(&self) -> Result<Option<MintedTicket>, ApiError> {
        let claims = format!(
            r#"{{"sub":"desk-user","team":"team-611","sessionId":"{SESSION_ID}","role":"publisher"}}"#
        );
        let ticket = mint_ticket(&claims);
        Ok(Some(MintedTicket {
            url: format!("ws://127.0.0.1:{}/ws?ticket={ticket}", self.relay_port),
            ticket,
        }))
    }
}

fn main() {
    let start = Instant::now();
    let stamp = move || format!("[{:6.1}s]", start.elapsed().as_secs_f32());

    let port = free_port();
    println!("{} starting relay on :{port}", stamp());
    let _relay = start_relay(port);

    let worktree = std::env::temp_dir().join(format!("exp611-worktree-{}", std::process::id()));
    std::fs::create_dir_all(&worktree).expect("create worktree");

    // The hooks sidecar, exactly as the launcher wires it (EXP-249) — the
    // PlanProposed hook is what carries the plan body to the card.
    let hook_server = HookServer::start().expect("hook server");
    let settings_path = worktree.join("exp611-hook-settings.json");
    std::fs::write(&settings_path, hook_settings_json()).expect("write settings");
    let curl_config_path = worktree.join("exp611-hook-curl.cfg");
    write_hook_curl_config(&curl_config_path, hook_server.token()).expect("write curl config");

    let prompt = "Plan how to create a file hello.txt containing the word hello. \
                  Keep the plan to two short steps.";
    let cols: u16 = std::env::var("EXP611_COLS").ok().and_then(|v| v.parse().ok()).unwrap_or(100);
    let rows: u16 = std::env::var("EXP611_ROWS").ok().and_then(|v| v.parse().ok()).unwrap_or(24);
    println!("{} spawning claude in {}x{} PTY (plan mode)", stamp(), cols, rows);
    let spec = SpawnSpec::new("claude")
        .args([
            "--settings",
            settings_path.to_str().unwrap(),
            "--permission-mode",
            "plan",
            "--allow-dangerously-skip-permissions",
            prompt,
        ])
        .env("TERM", "xterm-256color")
        .env(HOOK_PORT_ENV, &hook_server.port().to_string())
        .env(HOOK_CONFIG_ENV, curl_config_path.to_str().unwrap())
        .cwd(worktree.to_str().unwrap());
    let terminal = Terminal::spawn(&spec, cols, rows).expect("spawn claude");

    // Production wiring, exactly like ui/steer_wiring.rs.
    let runtime = SteerRuntime::new().expect("steer runtime");
    let write_input = pty_writer_input_hook(terminal.writer(), terminal.term());
    let (answer_link, answers_rx) = AnswerLink::new();
    let hooks = PublisherHooks {
        write_input: write_input.clone(),
        kill: Arc::new(|_| {}),
        error: Arc::new(|message| println!("[publisher error] {message}")),
        answers: Some(answer_link.clone()),
        agent: steer::activity::SessionAgent::Claude,
        text_sink: None,
        attachments: None,
    };
    let handle = publish(
        &runtime,
        PublishSpec {
            session_id: SESSION_ID.to_string(),
            issue_id: None,
        },
        Arc::new(HarnessTickets { relay_port: port }),
        hooks,
    );

    let active = Arc::new(AtomicBool::new(true));
    steer::spawn_activity_emitter(
        EmitterConfig {
            agent: steer::activity::SessionAgent::Claude,
            worktree: worktree.clone(),
            term: Some(terminal.term()),
            extra_secrets: Vec::new(),
            on_needs_input: None,
            hooks: Some(hook_server.events().clone()),
            steering: Some(Steering {
                answers: answers_rx,
                link: answer_link,
                write_input,
            }),
            bypass_permissions: false,
            plan_mode: true,
            claude_session_id: None,
            codex_originator: None,
            codex_resume_id: None,
            foreign_host: false,
            pi_events: None,
        },
        handle.activity_sender(),
        active.clone(),
    );

    // The scroll trigger: once the plan question publishes, the foreground
    // loop scrolls the viewport into history BEFORE the viewer answers.
    let scrolled = Arc::new(AtomicBool::new(false));
    let scroll_request = Arc::new(AtomicBool::new(false));

    // The viewer: wait for the plan question, request the scroll, then answer
    // remotely and expect ack + resolution.
    let viewer_rt = tokio::runtime::Runtime::new().unwrap();
    let viewer_ticket = mint_ticket(&format!(
        r#"{{"sub":"phone-user","team":"team-611","sessionId":"{SESSION_ID}","role":"viewer"}}"#
    ));
    let url = format!("ws://127.0.0.1:{port}/ws?ticket={viewer_ticket}");
    let viewer_scroll_request = scroll_request.clone();
    let viewer_scrolled = scrolled.clone();
    let viewer = viewer_rt.spawn(async move {
        let mut ws = loop {
            let (mut candidate, _) =
                tokio_tungstenite::connect_async(&url).await.expect("viewer connect");
            candidate
                .send(Message::Text(r#"{"t":"join","channel":"activity"}"#.into()))
                .await
                .unwrap();
            match candidate.next().await {
                Some(Ok(Message::Text(text))) if text.contains("no_such_session") => {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
                Some(Ok(_)) => break candidate,
                other => {
                    println!("[viewer] connect attempt got {other:?}; retrying");
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            }
        };
        let mut plan: Option<(String, String)> = None; // (id, key to answer)
        let mut answered_id: Option<String> = None;
        let mut acked = false;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(180);
        loop {
            let tick = tokio::time::sleep(Duration::from_millis(200));
            tokio::pin!(tick);
            tokio::select! {
                _ = &mut tick => {}
                _ = tokio::time::sleep_until(deadline) => break,
                frame = ws.next() => {
                    let Some(Ok(Message::Text(text))) = frame else { continue };
                    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
                    if value["t"] != "activity" { continue }
                    let event = &value["event"];
                    match event["kind"].as_str() {
                        Some("question") if event["planMode"].as_bool() == Some(true) => {
                            let id = event["id"].as_str().unwrap_or("<none>").to_string();
                            let options: Vec<(String, String)> = event["options"]
                                .as_array()
                                .map(|a| a.iter().map(|o| (
                                    o["key"].as_str().unwrap_or("?").to_string(),
                                    o["label"].as_str().unwrap_or("?").to_string(),
                                )).collect())
                                .unwrap_or_default();
                            println!("[viewer] PLAN QUESTION id={id} options={options:?}");
                            // "Yes, manually approve edits" — never the bypass row.
                            let key = options
                                .iter()
                                .find(|(_, label)| label.contains("manually approve"))
                                .map(|(key, _)| key.clone())
                                .unwrap_or_else(|| "2".to_string());
                            plan = Some((id, key));
                            viewer_scroll_request.store(true, std::sync::atomic::Ordering::SeqCst);
                        }
                        Some("answer_ack") => {
                            println!("[viewer] ANSWER_ACK id={:?}", event["id"].as_str());
                            if event["id"].as_str() == answered_id.as_deref() {
                                acked = true;
                            }
                        }
                        Some("question_resolved") => {
                            let id = event["id"].as_str().unwrap_or("<none>").to_string();
                            println!("[viewer] QUESTION_RESOLVED id={id}");
                            if answered_id.as_deref() == Some(id.as_str()) {
                                return (true, acked, true);
                            }
                        }
                        _ => {}
                    }
                }
            }
            // Tap only once the foreground confirmed the viewport is scrolled
            // into history — the EXP-611 moment.
            if answered_id.is_none()
                && viewer_scrolled.load(std::sync::atomic::Ordering::SeqCst)
            {
                if let Some((id, key)) = plan.clone() {
                    let frame = serde_json::json!({
                        "t": "answer",
                        "questionId": id,
                        "keys": [key],
                    });
                    println!("[viewer] TAP answer id={id} key={key} (viewport scrolled)");
                    ws.send(Message::Text(frame.to_string().into())).await.unwrap();
                    answered_id = Some(id);
                }
            }
        }
        (plan.is_some(), acked, false)
    });

    // Foreground: accept the trust dialog, and when the viewer saw the plan
    // card, scroll the viewport up into history like a user reading the plan.
    let deadline = Instant::now() + Duration::from_secs(180);
    let mut trusted = false;
    let mut last_dump = Instant::now();
    while Instant::now() < deadline && !viewer.is_finished() {
        let lines = screen_lines(&terminal.term());
        if !trusted && lines.iter().any(|line| line.contains("trust this folder")) {
            trusted = true;
            terminal.write(b"\r");
            println!("{} accepted trust dialog", stamp());
        }
        if scroll_request.load(std::sync::atomic::Ordering::SeqCst)
            && !scrolled.load(std::sync::atomic::Ordering::SeqCst)
        {
            scroll_up(&terminal.term(), 5);
            let offset = display_offset(&terminal.term());
            println!("{} scrolled viewport into history (display_offset={offset})", stamp());
            assert!(offset > 0, "scrollback should exist under a rendered plan");
            scrolled.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        if last_dump.elapsed() >= Duration::from_secs(20) {
            last_dump = Instant::now();
            println!("{} ---- grid ----", stamp());
            for line in lines.iter().filter(|l| !l.trim().is_empty()) {
                println!("| {line}");
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    let (question_seen, acked, resolved) = viewer_rt.block_on(viewer).unwrap_or_default();
    println!("\n===== final grid =====");
    for line in screen_lines(&terminal.term()).iter().filter(|l| !l.trim().is_empty()) {
        println!("| {line}");
    }
    println!("======================");
    println!(
        "[result] {} — plan question: {question_seen}, answer acked: {acked}, resolved: {resolved}",
        if question_seen && acked && resolved { "PASS" } else { "FAIL" },
    );
    terminal.kill();
    active.store(false, std::sync::atomic::Ordering::SeqCst);
}
