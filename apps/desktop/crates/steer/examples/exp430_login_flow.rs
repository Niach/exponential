//! EXP-430 manual e2e harness: the remote `/login` pipeline against the REAL
//! `claude` TUI — relay (bun) + production publisher + hooks sidecar +
//! activity emitter — with a viewer WebSocket that drives the whole flow the
//! way a phone steerer does:
//!
//! 1. sends `/login` as ordinary input frames (bracketed paste + `\r`),
//! 2. answers the published method-picker `question` with key "1",
//! 3. prints the sign-in-URL narration,
//! 4. pastes a GARBAGE code (never completes a real login) → expects the
//!    error narration and the re-published picker question,
//! 5. Esc-cancels and expects the fresh question to resolve.
//!
//! Run: `cargo run -p steer --example exp430_login_flow` (needs `bun` and a
//! logged-in `claude` on PATH; the garbage code is rejected by the OAuth
//! endpoint, so the local claude auth is never touched).

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
use terminal::{screen_lines, SpawnSpec, Terminal};

const SECRET: &str = "exp430-secret";
const SESSION_ID: &str = "43043043-4304-4304-4304-430430430430";

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
            r#"{{"sub":"desk-user","team":"team-430","sessionId":"{SESSION_ID}","role":"publisher"}}"#
        );
        let ticket = mint_ticket(&claims);
        Ok(Some(MintedTicket {
            url: format!("ws://127.0.0.1:{}/ws?ticket={ticket}", self.relay_port),
            ticket,
        }))
    }
}

/// The viewer-side script: the ordered milestones the run must hit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Milestone {
    FirstPicker,
    Acked,
    UrlNarration,
    ErrorNarration,
    SecondPicker,
    ResolvedAfterEscape,
}

fn main() {
    let start = Instant::now();
    let stamp = move || format!("[{:6.1}s]", start.elapsed().as_secs_f32());

    let port = free_port();
    println!("{} starting relay on :{port}", stamp());
    let _relay = start_relay(port);

    let worktree = std::env::temp_dir().join(format!("exp430-worktree-{}", std::process::id()));
    std::fs::create_dir_all(&worktree).expect("create worktree");

    let hook_server = HookServer::start().expect("hook server");
    let settings_path = worktree.join("exp430-hook-settings.json");
    std::fs::write(&settings_path, hook_settings_json()).expect("write settings");
    let curl_config_path = worktree.join("exp430-hook-curl.cfg");
    write_hook_curl_config(&curl_config_path, hook_server.token()).expect("write curl config");

    println!("{} spawning claude (idle REPL) in 120x36 PTY", stamp());
    let spec = SpawnSpec::new("claude")
        .args(["--settings", settings_path.to_str().unwrap()])
        .env("TERM", "xterm-256color")
        .env(HOOK_PORT_ENV, &hook_server.port().to_string())
        .env(HOOK_CONFIG_ENV, curl_config_path.to_str().unwrap())
        .cwd(worktree.to_str().unwrap());
    let terminal = Terminal::spawn(&spec, 120, 36).expect("spawn claude");

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
            bypass_permissions: true,
            claude_session_id: None,
            codex_originator: None,
            codex_resume_id: None,
            foreign_host: false,
            pi_events: None,
        },
        handle.activity_sender(),
        active.clone(),
    );

    let viewer_rt = tokio::runtime::Runtime::new().unwrap();
    let viewer_ticket = mint_ticket(&format!(
        r#"{{"sub":"phone-user","team":"team-430","sessionId":"{SESSION_ID}","role":"viewer"}}"#
    ));
    let url = format!("ws://127.0.0.1:{port}/ws?ticket={viewer_ticket}");
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

        async fn send_input(
            ws: &mut (impl SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin),
            data: &str,
        ) {
            let frame = serde_json::json!({ "t": "input", "data": data });
            ws.send(Message::Text(frame.to_string().into())).await.unwrap();
        }

        // Give claude a few seconds to reach the REPL, then steer `/login`
        // exactly like the production composer: text frame + separate CR.
        tokio::time::sleep(Duration::from_secs(8)).await;
        println!("[viewer] steering /login");
        send_input(&mut ws, "/login").await;
        send_input(&mut ws, "\r").await;

        let mut hit: Vec<Milestone> = Vec::new();
        let mut first_picker_id: Option<String> = None;
        let mut second_picker_id: Option<String> = None;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
        loop {
            let frame = tokio::select! {
                _ = tokio::time::sleep_until(deadline) => break,
                frame = ws.next() => frame,
            };
            let Some(Ok(Message::Text(text))) = frame else {
                println!("[viewer] non-text frame: {frame:?}");
                continue;
            };
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
            if value["t"] != "activity" {
                continue;
            }
            let event = &value["event"];
            match event["kind"].as_str() {
                Some("question") => {
                    let id = event["id"].as_str().unwrap_or("<none>").to_string();
                    let options = event["options"].as_array().map(|a| a.len()).unwrap_or(0);
                    println!(
                        "[viewer] question id={id} header={:?} options={options}",
                        event["header"].as_str(),
                    );
                    if !id.starts_with("login:") {
                        continue;
                    }
                    if first_picker_id.is_none() {
                        first_picker_id = Some(id.clone());
                        hit.push(Milestone::FirstPicker);
                        // No askId key at all — the relay's zod schema is
                        // `.optional()`, which rejects an explicit null.
                        let frame = serde_json::json!({
                            "t": "answer", "questionId": id, "keys": ["1"],
                        });
                        println!("[viewer] TAP method option 1");
                        ws.send(Message::Text(frame.to_string().into())).await.unwrap();
                    } else if second_picker_id.is_none() && hit.contains(&Milestone::ErrorNarration)
                    {
                        second_picker_id = Some(id.clone());
                        hit.push(Milestone::SecondPicker);
                        println!("[viewer] second picker up — Esc-cancelling the flow");
                        send_input(&mut ws, "\u{1b}").await;
                    }
                }
                Some("answer_ack") => {
                    println!("[viewer] ANSWER_ACK id={:?}", event["id"].as_str());
                    if !hit.contains(&Milestone::Acked) {
                        hit.push(Milestone::Acked);
                    }
                }
                Some("narration") => {
                    let text = event["text"].as_str().unwrap_or_default();
                    println!("[viewer] narration: {}", &text[..text.len().min(160)]);
                    if text.contains("open this link") && text.contains("https://") {
                        if !hit.contains(&Milestone::UrlNarration) {
                            hit.push(Milestone::UrlNarration);
                            println!("[viewer] pasting a garbage code");
                            send_input(&mut ws, "bogus-remote-code#430").await;
                            send_input(&mut ws, "\r").await;
                        }
                    } else if text.contains("Claude sign-in failed")
                        && !hit.contains(&Milestone::ErrorNarration)
                    {
                        hit.push(Milestone::ErrorNarration);
                        // "Send any message to retry" — a bare Enter retries,
                        // which lands back on the method picker.
                        send_input(&mut ws, "\r").await;
                    }
                }
                Some("question_resolved") => {
                    let id = event["id"].as_str().map(str::to_string);
                    println!("[viewer] QUESTION_RESOLVED id={id:?}");
                    if id == second_picker_id && hit.contains(&Milestone::SecondPicker) {
                        hit.push(Milestone::ResolvedAfterEscape);
                        break;
                    }
                }
                _ => {}
            }
        }
        hit
    });

    // Foreground: accept the trust dialog if it appears; dump the grid
    // periodically so a stall is diagnosable.
    let deadline = Instant::now() + Duration::from_secs(150);
    let mut last_dump = Instant::now();
    let mut trusted = false;
    while Instant::now() < deadline && !viewer.is_finished() {
        let lines = screen_lines(&terminal.term());
        if !trusted && lines.iter().any(|line| line.contains("trust this folder")) {
            trusted = true;
            terminal.write(b"\r");
            println!("{} accepted trust dialog", stamp());
        }
        if last_dump.elapsed() >= Duration::from_secs(15) {
            last_dump = Instant::now();
            println!("{} ---- grid ----", stamp());
            for line in lines.iter().filter(|l| !l.trim().is_empty()) {
                println!("| {line}");
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }

    let hit = viewer_rt.block_on(viewer).unwrap_or_default();
    println!("\n===== final grid =====");
    for line in screen_lines(&terminal.term()).iter().filter(|l| !l.trim().is_empty()) {
        println!("| {line}");
    }
    println!("======================");
    let expected = [
        Milestone::FirstPicker,
        Milestone::Acked,
        Milestone::UrlNarration,
        Milestone::ErrorNarration,
        Milestone::SecondPicker,
        Milestone::ResolvedAfterEscape,
    ];
    let pass = expected.iter().all(|m| hit.contains(m));
    println!(
        "[result] {} — milestones hit: {hit:?}",
        if pass { "PASS" } else { "FAIL" },
    );
    terminal.kill();
    active.store(false, std::sync::atomic::Ordering::SeqCst);
}
