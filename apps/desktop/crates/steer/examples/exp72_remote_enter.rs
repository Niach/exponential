//! EXP-72 manual e2e harness: boots the REAL steer relay (bun), publishes a
//! REAL PTY child through the production publisher, then steers it over a
//! viewer WebSocket with the exact Android frame sequence — steal-claim,
//! message text, immediate bare `\r` — and prints what the CHILD actually
//! received on stdin (`cat -v` renders control bytes visibly: `^[` = ESC,
//! `^M` = CR).
//!
//! Run: `cargo run -p steer --example exp72_remote_enter` (needs `bun`).
//!
//! Expected (the EXP-72 fix): the text lands bracketed
//! (`^[[200~…^[[201~`, because the child enabled mode 2004) and the `^M`
//! lands as its own write ≥150ms later — so the `claude` TUI submits instead
//! of paste-inserting a newline.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::SinkExt;
use tokio_tungstenite::tungstenite::Message;

use api::error::ApiError;
use api::steer::MintedTicket;
use steer::publisher::{
    publish, pty_writer_input_hook, term_geometry_hook, PublishSpec, PublisherHooks,
    PublisherTickets,
};
use steer::SteerRuntime;
use terminal::{screen_lines, SpawnSpec, Terminal};

const SECRET: &str = "exp72-secret";
const SESSION_ID: &str = "72727272-7272-7272-7272-727272727272";

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

/// Sign a ticket via `@exp/steer-ticket` (the relay-integration-test script).
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
            r#"{{"sub":"desk-user","team":"team-72","sessionId":"{SESSION_ID}","role":"publisher","perm":"steer"}}"#
        );
        let ticket = mint_ticket(&claims);
        Ok(Some(MintedTicket {
            url: format!("ws://127.0.0.1:{}/ws?ticket={ticket}", self.relay_port),
            ticket,
        }))
    }
}

fn main() {
    let port = free_port();
    println!("[harness] starting relay on :{port}");
    let _relay = start_relay(port);

    // The "claude" stand-in: enables bracketed paste (mode 2004) like the
    // real TUI, then echoes every stdin byte visibly. `EXP72_NO_2004=1` skips
    // the mode-set to probe the raw-passthrough branch (plain shells).
    let mode_2004 = std::env::var("EXP72_NO_2004").is_err();
    let script = if mode_2004 {
        "stty raw -echo; printf '\\033[?2004h'; exec cat -v"
    } else {
        "stty raw -echo; exec cat -v"
    };
    println!("[harness] spawning PTY child (mode 2004: {mode_2004}): {script}");
    let terminal = Terminal::spawn(&SpawnSpec::new("sh").args(["-c", script]), 100, 30)
        .expect("spawn PTY child");

    // Production wiring, exactly like ui/steer_wiring.rs.
    let runtime = SteerRuntime::new().expect("steer runtime");
    let hooks = PublisherHooks {
        write_input: pty_writer_input_hook(terminal.writer(), terminal.term()),
        resize: Arc::new(|_, _| {}),
        geometry: term_geometry_hook(terminal.term()),
        kill: Arc::new(|_| {}),
        presence: Arc::new(|_| {}),
        error: Arc::new(|message| println!("[publisher error] {message}")),
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
    terminal.attach_sink(handle.raw_sink());

    // Give the child time to enable mode 2004 and the publisher to hello.
    if mode_2004 {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !terminal::bracketed_paste_enabled(&terminal.term()) {
            assert!(Instant::now() < deadline, "child never enabled mode 2004");
            std::thread::sleep(Duration::from_millis(25));
        }
    }
    std::thread::sleep(Duration::from_millis(1500));
    println!(
        "[harness] child enabled bracketed paste: {}",
        terminal::bracketed_paste_enabled(&terminal.term())
    );

    // The Android viewer: steal-claim + text + IMMEDIATE bare `\r`
    // (apps/android AgentSessionViewModel.sendMessage — no client-side delay).
    let viewer_rt = tokio::runtime::Runtime::new().unwrap();
    let viewer_ticket = mint_ticket(&format!(
        r#"{{"sub":"phone-user","team":"team-72","name":"Phone","sessionId":"{SESSION_ID}","role":"viewer","perm":"steer"}}"#
    ));
    let url = format!("ws://127.0.0.1:{port}/ws?ticket={viewer_ticket}");
    let _viewer = viewer_rt.spawn(async move {
        let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.expect("viewer connect");
        ws.send(Message::Text(r#"{"t":"join"}"#.into())).await.unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await; // joined; now "Send" is tapped
        ws.send(Message::Text(r#"{"t":"claim","steal":true}"#.into())).await.unwrap();
        ws.send(Message::Text(r#"{"t":"input","data":"fix the login bug"}"#.into()))
            .await
            .unwrap();
        ws.send(Message::Text(r#"{"t":"input","data":"\r"}"#.into())).await.unwrap();
        println!("[viewer] claim + text + \\r sent back-to-back");
        // The Escape interrupt (sendEscape on all clients) must land raw.
        tokio::time::sleep(Duration::from_millis(700)).await;
        ws.send(Message::Text(r#"{"t":"input","data":"\u001b"}"#.into())).await.unwrap();
        println!("[viewer] escape interrupt sent");
        // Hold the socket open while the harness observes the child, and
        // print everything the relay sends back (presence/errors).
        use futures_util::StreamExt;
        let hold = tokio::time::sleep(Duration::from_secs(6));
        tokio::pin!(hold);
        loop {
            tokio::select! {
                _ = &mut hold => break,
                frame = ws.next() => match frame {
                    Some(Ok(Message::Text(text))) => println!("[viewer<-relay] {text}"),
                    Some(Ok(Message::Binary(bytes))) => {
                        println!("[viewer<-relay] {} binary bytes", bytes.len())
                    }
                    Some(Ok(Message::Close(frame))) => println!("[viewer<-relay] CLOSE {frame:?}"),
                    other => { println!("[viewer<-relay] {other:?}"); break }
                },
            }
        }
    });

    // Observe what the CHILD received: poll the grid (cat -v echoes stdin).
    let start = Instant::now();
    let mut text_at: Option<Duration> = None;
    let mut enter_at: Option<Duration> = None;
    let deadline = Instant::now() + Duration::from_secs(10);
    while enter_at.is_none() && Instant::now() < deadline {
        let grid = screen_lines(&terminal.term()).join("\n");
        if text_at.is_none() && grid.contains("fix the login bug") {
            text_at = Some(start.elapsed());
        }
        if text_at.is_some() && grid.contains("^M") {
            enter_at = Some(start.elapsed());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    // Give the trailing escape-interrupt probe time to land too.
    std::thread::sleep(Duration::from_millis(1500));

    println!("\n===== what the child received (cat -v) =====");
    for line in screen_lines(&terminal.term()).iter().filter(|l| !l.is_empty()) {
        println!("| {line}");
    }
    println!("=============================================");
    match (text_at, enter_at) {
        (Some(text), Some(enter)) => {
            let gap = enter - text;
            let grid = screen_lines(&terminal.term()).join("\n");
            let bracketed = grid.contains("^[[200~fix the login bug^[[201~");
            let plain = grid.contains("fix the login bug^M") && !grid.contains("200~");
            // cat -v renders a raw lone ESC as `^[` right after the `^M`.
            let escape_raw = grid.contains("^M^[")
                && !grid.contains("^M^[[200~^[")
                && !grid.ends_with("201~");
            println!("[result] text at {text:?}, Enter (^M) at {enter:?}, gap = {gap:?}");
            println!("[result] text bracketed as paste: {bracketed} (raw passthrough: {plain})");
            println!("[result] escape interrupt landed raw: {escape_raw}");
            let text_ok = if mode_2004 { bracketed } else { plain };
            println!(
                "[result] {}",
                if text_ok && escape_raw && gap >= Duration::from_millis(100) {
                    "PASS — correct text framing + separated Enter + raw escape"
                } else {
                    "FAIL — see above"
                }
            );
        }
        _ => println!("[result] FAIL — text/Enter never reached the child (text_at={text_at:?})"),
    }

    handle.shutdown(Some("exit:0".to_string()));
    terminal.kill();
}
