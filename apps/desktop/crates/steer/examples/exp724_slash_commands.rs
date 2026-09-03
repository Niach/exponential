//! EXP-724 manual e2e harness: a REMOTE slash command against the real agent
//! TUI — relay (bun) + production publisher + activity emitter + (for claude)
//! the hooks sidecar, with a viewer WebSocket that types `/compact` into the
//! composer exactly like a phone does (text chunks, then a bare `\r`).
//!
//! Run: `cargo run -p steer --example exp724_slash_commands -- [claude|codex|pi]`
//! (needs `bun`, and the chosen agent CLI logged in on PATH; spends tokens).
//!
//! What it answers — the three unknowns the design left open:
//!
//! 1. **Does a BRACKETED `/compact` + Enter run, or only complete?** The
//!    emitter probes for [`COMMAND_SUBMIT_PROBE`] and presses Enter once more
//!    if the composer still holds the text; the harness prints the composer
//!    tail every 250 ms across the dispatch so the answer is visible.
//! 2. **What does a `PreCompact`/`PostCompact` payload actually carry?** The
//!    settings file written here is the PRODUCTION one plus one extra hook
//!    command per compaction event that appends the RAW body to
//!    `<worktree>/hook-payloads.jsonl` — production code is untouched.
//! 3. **Do the compaction edges reach a viewer?** Every activity frame is
//!    printed; `compaction started` / `compaction ended` must bracket the
//!    silence, and the `user_message` echo must appear exactly once.
//!
//! Codex has no start marker at all (see `codex_activity`'s header), so there
//! the expectation is: echo → typed `/compact` → `compaction started` from
//! the dispatch itself → `compaction ended` from `context_compacted`. Pi runs
//! everything through its observer extension and never touches the PTY.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

use api::error::ApiError;
use api::steer::MintedTicket;
use steer::activity::SessionAgent;
use steer::pi_observer::ObserverServer;
use steer::publisher::{
    publish, pty_writer_input_hook, PublishSpec, PublisherHooks, PublisherTickets,
};
use steer::{
    hook_settings_json, write_hook_curl_config, AnswerLink, CommandLink, EmitterConfig, HookServer,
    SteerRuntime, Steering, HOOK_CONFIG_ENV, HOOK_PORT_ENV,
};
use terminal::{screen_lines, SpawnSpec, Terminal};

/// Cheap follow-up turns: `/compact` refuses a conversation with too few
/// messages, so the harness earns some history before the command.
const FOLLOW_UPS: [&str; 4] = [
    "In one sentence: what is a tmux pane?",
    "In one sentence: what is bracketed paste?",
    "In one sentence: what is a control sequence?",
    "In one sentence: what is a terminal grid?",
];

const SECRET: &str = "exp724-secret";
const SESSION_ID: &str = "72472472-4724-4724-8724-724724724724";

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
            r#"{{"sub":"desk-user","team":"team-724","sessionId":"{SESSION_ID}","role":"publisher"}}"#
        );
        let ticket = mint_ticket(&claims);
        Ok(Some(MintedTicket {
            url: format!("ws://127.0.0.1:{}/ws?ticket={ticket}", self.relay_port),
            ticket,
        }))
    }
}

/// The observer extension's source, read straight out of `pi_bridge.rs` (the
/// launcher's constant) so the harness can never drift from production.
fn observer_source() -> String {
    let bridge = repo_root().join("apps/desktop/crates/coding/src/pi_bridge.rs");
    let source = std::fs::read_to_string(&bridge).expect("read pi_bridge.rs");
    let start_marker = "PI_OBSERVER_SOURCE: &str = r#\"";
    let start = source
        .find(start_marker)
        .expect("PI_OBSERVER_SOURCE marker present")
        + start_marker.len();
    let end = source[start..]
        .find("\"#;")
        .expect("raw string terminator present");
    source[start..start + end].to_string()
}

/// The production hook settings PLUS a raw-payload tee on both compaction
/// events — the only way to learn a payload's real field names without
/// touching the sidecar's parsing (question 2 in the header).
fn settings_with_payload_dump(dump: &Path) -> String {
    let mut value: serde_json::Value =
        serde_json::from_str(&hook_settings_json()).expect("production settings are JSON");
    let tee = serde_json::json!({
        "hooks": [{
            "type": "command",
            "command": format!("tee -a {} > /dev/null", dump.display()),
        }],
    });
    for event in ["PreCompact", "PostCompact"] {
        value["hooks"][event]
            .as_array_mut()
            .expect("the production settings register both compaction hooks")
            .push(tee.clone());
    }
    serde_json::to_string_pretty(&value).unwrap()
}

fn main() {
    let start = Instant::now();
    let stamp = move || format!("[{:6.1}s]", start.elapsed().as_secs_f32());
    let agent = match std::env::args().nth(1).unwrap_or_else(|| "claude".into()).as_str() {
        "codex" => SessionAgent::Codex,
        "pi" => SessionAgent::Pi,
        _ => SessionAgent::Claude,
    };
    println!("{} agent: {agent:?}", stamp());

    let port = free_port();
    println!("{} starting relay on :{port}", stamp());
    let _relay = start_relay(port);

    let worktree = std::env::temp_dir().join(format!("exp724-worktree-{}", std::process::id()));
    std::fs::create_dir_all(&worktree).expect("create worktree");
    // CANONICAL: on macOS `/var/folders/…` is a symlink to `/private/var/…`,
    // and claude munges its project dir from the RESOLVED cwd — an
    // unresolved worktree makes the emitter tail a directory that never
    // exists and the whole feed goes silent.
    let worktree = std::fs::canonicalize(&worktree).expect("canonicalize worktree");
    let dump = worktree.join("hook-payloads.jsonl");

    // Enough context that `/compact` has something to compact.
    let prompt = "In one sentence: what is a PTY? Then stop.";
    let (spec, hook_server) = match agent {
        SessionAgent::Claude => {
            let hook_server = HookServer::start().expect("hook server");
            let settings_path = worktree.join("exp724-hook-settings.json");
            std::fs::write(&settings_path, settings_with_payload_dump(&dump))
                .expect("write settings");
            let curl_config_path = worktree.join("exp724-hook-curl.cfg");
            write_hook_curl_config(&curl_config_path, hook_server.token())
                .expect("write curl config");
            let spec = SpawnSpec::new("claude")
                .args([
                    "--settings",
                    settings_path.to_str().unwrap(),
                    "--dangerously-skip-permissions",
                    prompt,
                ])
                .env("TERM", "xterm-256color")
                .env(HOOK_PORT_ENV, &hook_server.port().to_string())
                .env(HOOK_CONFIG_ENV, curl_config_path.to_str().unwrap())
                .cwd(worktree.to_str().unwrap());
            (spec, Some(hook_server))
        }
        SessionAgent::Codex => (
            SpawnSpec::new("codex")
                .args(["--dangerously-bypass-approvals-and-sandbox", prompt])
                .env("TERM", "xterm-256color")
                .cwd(worktree.to_str().unwrap()),
            None,
        ),
        // Replaced below: pi's spawn needs the observer sidecar's port.
        SessionAgent::Pi => (SpawnSpec::new("pi"), None),
    };
    // Pi's whole steer path is the observer extension — no PTY, no hooks.
    let pi_observer = (agent == SessionAgent::Pi).then(|| {
        let observer = ObserverServer::start().expect("observer server");
        let (events, steer_handle) = observer.subscribe(&worktree);
        std::fs::write(worktree.join(".exp-pi-observer.ts"), observer_source())
            .expect("write observer extension");
        (observer, events, steer_handle)
    });
    let spec = match &pi_observer {
        Some((observer, _, _)) => SpawnSpec::new("pi")
            .args(["-e", "./.exp-pi-observer.ts", prompt])
            .env("TERM", "xterm-256color")
            .env(
                "EXP_OBSERVER_URL",
                &format!("http://127.0.0.1:{}", observer.port()),
            )
            .env("EXP_OBSERVER_TOKEN", observer.token())
            .env("PI_SKIP_VERSION_CHECK", "1")
            .cwd(worktree.to_str().unwrap()),
        None => spec,
    };
    println!("{} spawning {agent:?} in a 100x30 PTY", stamp());
    let terminal = Terminal::spawn(&spec, 100, 30).expect("spawn agent");

    // Production wiring, exactly like ui/steer_wiring.rs.
    let runtime = SteerRuntime::new().expect("steer runtime");
    let write_input = pty_writer_input_hook(terminal.writer(), terminal.term());
    let (answer_link, answers_rx) = AnswerLink::new();
    // Pi dispatches through the observer extension's queue; claude and codex
    // have no sink, so the emitter types their commands into the TUI.
    let pi_steer = pi_observer
        .as_ref()
        .map(|(_, _, steer_handle)| steer_handle.clone());
    let command_link = CommandLink::new(pi_steer.map(|handle| {
        Arc::new(move |name: &str, args: &str| handle.push_command(name, args)) as steer::CommandSink
    }));
    let hooks = PublisherHooks {
        write_input: write_input.clone(),
        kill: Arc::new(|_| {}),
        error: Arc::new(|message| println!("[publisher error] {message}")),
        answers: Some(answer_link.clone()),
        agent,
        text_sink: pi_observer.as_ref().map(|(_, _, steer_handle)| {
            let handle = steer_handle.clone();
            Arc::new(move |text: String| handle.push(text)) as Arc<dyn Fn(String) + Send + Sync>
        }),
        attachments: None,
        commands: Some(command_link.clone()),
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
            agent,
            worktree: worktree.clone(),
            base_ref: None,
            term: Some(terminal.term()),
            extra_secrets: Vec::new(),
            on_needs_input: None,
            hooks: hook_server.as_ref().map(|server| server.events().clone()),
            steering: Some(Steering {
                answers: answers_rx,
                link: answer_link,
                write_input,
                commands: Some(command_link),
            }),
            bypass_permissions: true,
            plan_mode: false,
            claude_session_id: None,
            codex_originator: None,
            codex_resume_id: None,
            foreign_host: false,
            turn_signal: None,
            pi_events: pi_observer.as_ref().map(|(_, events, _)| events.clone()),
        },
        handle.activity_sender(),
        active.clone(),
    );

    // The viewer: watch the feed, send `/compact` once the first turn ended,
    // then report what came back.
    let sent_at = Arc::new(std::sync::Mutex::new(None::<Instant>));
    let viewer_sent_at = sent_at.clone();
    let viewer_rt = tokio::runtime::Runtime::new().unwrap();
    let viewer_ticket = mint_ticket(&format!(
        r#"{{"sub":"phone-user","team":"team-724","sessionId":"{SESSION_ID}","role":"viewer"}}"#
    ));
    let url = format!("ws://127.0.0.1:{port}/ws?ticket={viewer_ticket}");
    // `EXP724_COMMAND` / `EXP724_TURNS` let one run probe a different catalog
    // command (`/model <id>`) without paying for the compaction warm-up.
    let command_text =
        std::env::var("EXP724_COMMAND").unwrap_or_else(|_| "/compact".to_string());
    let follow_ups: usize = std::env::var("EXP724_TURNS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(FOLLOW_UPS.len());
    let viewer = viewer_rt.spawn(async move {
        let mut ws = loop {
            let (mut candidate, _) = tokio_tungstenite::connect_async(&url)
                .await
                .expect("viewer connect");
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
        // One turn of context first, then the command.
        let mut narrations = 0;
        let mut turns = 0usize;
        let mut sent = false;
        let mut echoes = 0;
        let mut started = 0;
        let mut ended = 0;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(540);
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
                    println!("[viewer<-activity] {event}");
                    match event["kind"].as_str() {
                        Some("narration") => narrations += 1,
                        Some("user_message") => {
                            if event["text"].as_str() == Some(command_text.as_str()) {
                                echoes += 1;
                            }
                        }
                        Some("compaction") => match event["phase"].as_str() {
                            Some("started") => started += 1,
                            Some("ended") => ended += 1,
                            _ => {}
                        },
                        _ => {}
                    }
                    if ended > 0 && echoes > 0 { break }
                }
            }
            // `/compact` refuses a short conversation ("Not enough messages
            // to compact."), so trade a few cheap turns first: one prose
            // message per assistant answer, then the command.
            if !sent && narrations >= 2 + turns {
                if turns < follow_ups {
                    let text = FOLLOW_UPS[turns];
                    turns += 1;
                    println!("[viewer] SEND follow-up {turns}: {text}");
                    let frame = serde_json::json!({ "t": "input", "data": text });
                    ws.send(Message::Text(frame.to_string().into())).await.unwrap();
                    ws.send(Message::Text(r#"{"t":"input","data":"\r"}"#.into()))
                        .await
                        .unwrap();
                } else {
                    sent = true;
                    *viewer_sent_at.lock().unwrap() = Some(Instant::now());
                    println!("[viewer] SEND {command_text} (chunk + bare \\r, like a phone)");
                    let frame = serde_json::json!({ "t": "input", "data": command_text });
                    ws.send(Message::Text(frame.to_string().into())).await.unwrap();
                    ws.send(Message::Text(r#"{"t":"input","data":"\r"}"#.into()))
                        .await
                        .unwrap();
                }
            }
        }
        (echoes, started, ended)
    });

    // Foreground: dump the composer tail across the dispatch — this is what
    // answers "did one Enter submit it?".
    let deadline = Instant::now() + Duration::from_secs(540);
    let mut trusted = false;
    let mut last_tail = String::new();
    let mut last_dump = Instant::now();
    while Instant::now() < deadline && !viewer.is_finished() {
        let lines = screen_lines(&terminal.term());
        // Trust dialogs. Claude's defaults to "No, exit" (arrow DOWN onto
        // "Yes, I trust this folder" first, or a bare Enter quits); codex's
        // already highlights "1. Yes, continue".
        if !trusted {
            let claude_trust = lines
                .iter()
                .any(|line| line.contains("Yes, I trust this folder"));
            let codex_trust = lines
                .iter()
                .any(|line| line.contains("Do you trust the contents of this directory"));
            if claude_trust || codex_trust {
                trusted = true;
                std::thread::sleep(Duration::from_millis(400));
                if claude_trust {
                    terminal.write(b"\x1b[B");
                    std::thread::sleep(Duration::from_millis(200));
                }
                terminal.write(b"\r");
                println!("{} accepted trust dialog", stamp());
            }
        }
        let sent = *sent_at.lock().unwrap();
        if sent.is_some_and(|at| at.elapsed() < Duration::from_secs(8)) {
            let tail: String = lines
                .iter()
                .rev()
                .take(4)
                .map(|line| line.trim_end())
                .collect::<Vec<_>>()
                .join(" ⏎ ");
            if tail != last_tail {
                last_tail = tail.clone();
                println!("{} composer tail: {tail}", stamp());
            }
        }
        if last_dump.elapsed() >= Duration::from_secs(15) {
            last_dump = Instant::now();
            println!("{} ---- grid ----", stamp());
            for line in lines.iter().filter(|l| !l.trim().is_empty()) {
                println!("| {line}");
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    let (echoes, started, ended) = viewer_rt.block_on(viewer).unwrap_or_default();
    println!("\n===== final grid =====");
    for line in screen_lines(&terminal.term()).iter().filter(|l| !l.trim().is_empty()) {
        println!("| {line}");
    }
    if let Ok(payloads) = std::fs::read_to_string(&dump) {
        println!("===== raw compaction hook payloads =====");
        for line in payloads.lines().filter(|line| !line.trim().is_empty()) {
            println!("| {line}");
        }
    }
    println!("======================");
    println!(
        "[result] {} — echoes: {echoes}, compaction started: {started}, ended: {ended}",
        if echoes == 1 && ended >= 1 { "PASS" } else { "FAIL" }
    );
    terminal.kill();
    active.store(false, Ordering::SeqCst);
}
