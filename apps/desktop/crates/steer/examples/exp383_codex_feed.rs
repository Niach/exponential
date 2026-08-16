//! EXP-383 manual e2e harness: the codex activity feed + steering against the
//! REAL `codex` TUI — relay (bun) + production publisher + codex rollout
//! emitter — with a viewer WebSocket that watches the feed and steers a
//! message mid-session the way the web composer does (text chunks + a bare
//! `\r`).
//!
//! Run: `cargo run -p steer --example exp383_codex_feed` (needs `bun` and a
//! logged-in `codex` on PATH; spends a few tokens).
//!
//! PASS criteria, all observed as viewer-side activity frames:
//!   1. a `tool` event (codex ran a command / wrote a file)
//!   2. a `narration` beyond "Session started" (agent prose / reasoning)
//!   3. after the steer, a `user_message` echo of the steered text — this is
//!      the true input round-trip: paste+Enter landed in codex's composer,
//!      codex submitted it, the rollout recorded it, the tail republished it
//!   4. a `diff` mentioning the file the prompt asks for

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
use steer::{EmitterConfig, SteerRuntime};
use terminal::{screen_lines, SpawnSpec, Terminal};

const SECRET: &str = "exp383-secret";
const SESSION_ID: &str = "38338338-3833-4833-8383-383383383383";

const PROMPT: &str = "Replace the content of the tracked file hello.txt so it contains exactly \
one line: 'hello from exp383'. Use apply_patch or a shell command, your choice. Then tell me in \
one short sentence what you did, and wait for my next instruction — do not do anything else.";

const STEER_TEXT: &str = "Now append a second line saying 'steered remotely' to hello.txt, then \
confirm in one short sentence.";

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
            r#"{{"sub":"desk-user","team":"team-383","sessionId":"{SESSION_ID}","role":"publisher"}}"#
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

    // A scratch git worktree with hello.txt TRACKED in the baseline commit —
    // the diff snapshot is `git diff` (tracked changes only, same as claude
    // sessions), so the file the agent edits must pre-exist.
    let worktree = std::env::temp_dir().join(format!("exp383-worktree-{}", std::process::id()));
    std::fs::create_dir_all(&worktree).expect("create worktree");
    std::fs::write(worktree.join("hello.txt"), "placeholder\n").expect("seed hello.txt");
    for args in [
        vec!["init"],
        vec!["add", "hello.txt"],
        vec!["commit", "-m", "baseline"],
    ] {
        let ok = Command::new("git")
            .arg("-C")
            .arg(&worktree)
            .args(&args)
            .env("GIT_AUTHOR_NAME", "exp383")
            .env("GIT_AUTHOR_EMAIL", "exp383@example.com")
            .env("GIT_COMMITTER_NAME", "exp383")
            .env("GIT_COMMITTER_EMAIL", "exp383@example.com")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        assert!(ok, "git {args:?} failed");
    }

    println!("{} spawning codex in a 100x30 PTY (cwd {})", stamp(), worktree.display());
    let spec = SpawnSpec::new("codex")
        .args(["--dangerously-bypass-approvals-and-sandbox", PROMPT])
        .env("TERM", "xterm-256color")
        .cwd(worktree.to_str().unwrap());
    let terminal = Terminal::spawn(&spec, 100, 30).expect("spawn codex");

    // Production wiring, exactly like ui/steer_wiring.rs does for codex:
    // no answers link, no hooks, no steering — the rollout tail is the feed.
    let runtime = SteerRuntime::new().expect("steer runtime");
    let write_input = pty_writer_input_hook(terminal.writer(), terminal.term());
    let hooks = PublisherHooks {
        write_input: write_input.clone(),
        kill: Arc::new(|_| {}),
        error: Arc::new(|message| println!("[publisher error] {message}")),
        answers: None,
        agent: steer::activity::SessionAgent::Codex,
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
            agent: steer::activity::SessionAgent::Codex,
            worktree: worktree.clone(),
            term: Some(terminal.term()),
            extra_secrets: Vec::new(),
            on_needs_input: Some(Arc::new(|pending| {
                println!("[needs_input] -> {pending}");
                true
            })),
            hooks: None,
            steering: None,
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

    // The viewer: join the activity channel, log every frame, and once the
    // agent has demonstrably worked (tool + narration + first idle), steer a
    // follow-up message the way the web composer does.
    let viewer_rt = tokio::runtime::Runtime::new().unwrap();
    let viewer_ticket = mint_ticket(&format!(
        r#"{{"sub":"phone-user","team":"team-383","sessionId":"{SESSION_ID}","role":"viewer"}}"#
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
                Some(Ok(first)) => {
                    if let Message::Text(text) = &first {
                        println!("[viewer<-relay] {}", &text[..text.len().min(160)]);
                    }
                    break candidate;
                }
                other => {
                    println!("[viewer] connect attempt got {other:?}; retrying");
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            }
        };

        let mut saw_tool = false;
        let mut saw_narration = false;
        let mut saw_diff_with_file = false;
        // The steered edit visible in a diff snapshot — proves the remotely
        // steered instruction was EXECUTED, not just accepted.
        let mut saw_steered_diff = false;
        let mut saw_steer_echo = false;
        let mut narrations_after_steer = 0u32;
        let mut steered_at: Option<Instant> = None;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(300);
        loop {
            // Success early-exit: everything observed, including the steered
            // edit landing on disk (via the diff snapshot).
            if saw_tool && saw_narration && saw_diff_with_file && saw_steer_echo
                && saw_steered_diff && narrations_after_steer > 0
            {
                break;
            }
            let tick = tokio::time::sleep(Duration::from_millis(250));
            tokio::pin!(tick);
            tokio::select! {
                _ = &mut tick => {}
                _ = tokio::time::sleep_until(deadline) => {
                    println!("[viewer] deadline reached");
                    break;
                }
                frame = ws.next() => {
                    let Some(Ok(Message::Text(text))) = frame else {
                        println!("[viewer] non-text frame: {frame:?}");
                        continue;
                    };
                    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
                    if value["t"] != "activity" { continue }
                    let event = &value["event"];
                    match event["kind"].as_str() {
                        Some("narration") => {
                            let text = event["text"].as_str().unwrap_or_default();
                            println!("[feed] narration: {}", &text[..text.len().min(140)]);
                            if text != "Session started" {
                                saw_narration = true;
                                if steered_at.is_some() && !text.starts_with("Turn aborted") {
                                    narrations_after_steer += 1;
                                }
                            }
                        }
                        Some("tool") => {
                            println!(
                                "[feed] tool: {} {:?}",
                                event["name"].as_str().unwrap_or("?"),
                                event["detail"].as_str(),
                            );
                            saw_tool = true;
                        }
                        Some("user_message") => {
                            let text = event["text"].as_str().unwrap_or_default();
                            println!("[feed] user_message: {}", &text[..text.len().min(140)]);
                            if text.contains("steered remotely") {
                                saw_steer_echo = true;
                                println!("[viewer] STEER ECHO CONFIRMED (rollout round-trip)");
                            }
                        }
                        Some("diff") => {
                            let diff = event["diff"].as_str().unwrap_or_default();
                            println!("[feed] diff ({} bytes)", diff.len());
                            if diff.contains("hello.txt") {
                                saw_diff_with_file = true;
                            }
                            if diff.contains("steered remotely") {
                                saw_steered_diff = true;
                                println!("[viewer] STEERED EDIT VISIBLE IN DIFF");
                            }
                        }
                        Some("question") => {
                            println!(
                                "[feed] question: {:?} options={}",
                                event["text"].as_str().map(|t| &t[..t.len().min(80)]),
                                event["options"].as_array().map(|a| a.len()).unwrap_or(0),
                            );
                        }
                        other => println!("[feed] {other:?}"),
                    }
                }
            }
            // Steer once the first task visibly completed (tool + narration
            // seen, small settle pause) — exactly the web composer shape:
            // one text frame, then a bare Enter frame.
            if steered_at.is_none() && saw_tool && saw_narration {
                tokio::time::sleep(Duration::from_secs(3)).await;
                println!("[viewer] STEERING: {STEER_TEXT}");
                let text_frame = serde_json::json!({ "t": "input", "data": STEER_TEXT });
                ws.send(Message::Text(text_frame.to_string().into())).await.unwrap();
                let enter_frame = serde_json::json!({ "t": "input", "data": "\r" });
                ws.send(Message::Text(enter_frame.to_string().into())).await.unwrap();
                steered_at = Some(Instant::now());
            }
        }
        (
            saw_tool,
            saw_narration,
            saw_diff_with_file && saw_steered_diff,
            saw_steer_echo,
            narrations_after_steer,
        )
    });

    // Foreground: accept codex's directory-trust dialog when it appears, and
    // dump the grid periodically so a stall is diagnosable.
    let deadline = Instant::now() + Duration::from_secs(310);
    let mut last_dump = Instant::now();
    let mut trusted = false;
    while Instant::now() < deadline && !viewer.is_finished() {
        if !trusted {
            let lines = screen_lines(&terminal.term());
            if lines
                .iter()
                .any(|line| line.contains("Do you trust the contents of this directory"))
            {
                trusted = true;
                terminal.write(b"\r");
                println!("{} accepted codex trust dialog", stamp());
            }
        }
        if last_dump.elapsed() >= Duration::from_secs(20) {
            last_dump = Instant::now();
            println!("{} ---- grid ----", stamp());
            for line in screen_lines(&terminal.term()).iter().filter(|l| !l.trim().is_empty()) {
                println!("| {line}");
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }

    let (tool, narration, diff, echo, after) = viewer_rt.block_on(viewer).unwrap_or_default();
    println!("\n===== final grid =====");
    for line in screen_lines(&terminal.term()).iter().filter(|l| !l.trim().is_empty()) {
        println!("| {line}");
    }
    println!("======================");
    let file = std::fs::read_to_string(worktree.join("hello.txt")).unwrap_or_default();
    println!("[file] hello.txt on disk: {file:?}");
    let pass = tool && narration && diff && echo && after > 0 && file.contains("steered remotely");
    println!(
        "[result] {} — tool:{tool} narration:{narration} diff:{diff} steer-echo:{echo} \
         narrations-after-steer:{after} file-updated:{}",
        if pass { "PASS" } else { "FAIL" },
        file.contains("steered remotely"),
    );
    terminal.kill();
    active.store(false, std::sync::atomic::Ordering::SeqCst);
}
