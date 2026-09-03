//! The pi **observer sidecar** (EXP-383) — pi's analogue of the claude hooks
//! sidecar ([`crate::hooks`]).
//!
//! Pi has no transcript worth tailing (finalized-only, tree-structured) and
//! no hooks system — but its extension API observes everything AND can inject
//! user messages through pi's own queue semantics (`pi.sendUserMessage`,
//! v0.83). So the launcher loads a second `-e` extension
//! (`.exp-pi-observer.ts`, written by `coding::pi_bridge`) that:
//!
//! * POSTs batched session events to this loopback server (`POST /events`) —
//!   user prompts, assistant text, tool calls, the idle edge
//!   (`agent_settled`);
//! * long-polls `POST /steer` for remote composer messages, applying each via
//!   `pi.sendUserMessage(text, {deliverAs: "steer"})` — strictly better than
//!   PTY keystrokes because it respects pi's mid-turn queue instead of racing
//!   the TUI composer.
//!
//! ONE server per process (like the hooks sidecar); deliveries route to the
//! session whose canonicalized worktree matches the posted `cwd`. Same
//! posture as [`crate::hooks`]: loopback + per-process bearer token,
//! additive-never-load-bearing (a dead sidecar degrades the feed, never the
//! session), defensive parsing throughout.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use serde_json::Value;

use crate::frames::CompactionPhase;
use crate::hooks::{bearer_matches, respond, MAX_BODY_BYTES};

/// Spawn-env vars the observer extension reads (mirrored by
/// `coding::argv` — the two crates cannot depend on each other, §3.1).
pub const OBSERVER_URL_ENV: &str = "EXP_OBSERVER_URL";
pub const OBSERVER_TOKEN_ENV: &str = "EXP_OBSERVER_TOKEN";

/// How long a `/steer` long-poll parks before answering empty. Below typical
/// proxy/idle timeouts and the extension re-polls immediately.
const STEER_POLL_WAIT: Duration = Duration::from_secs(20);

/// Per-connection socket timeout for reads and the `/events` write path. The
/// `/steer` response rides the same write timeout after its park.
const CONN_TIMEOUT: Duration = Duration::from_secs(5);

/// Header-section budget (request line + headers).
const MAX_HEADER_BYTES: usize = 8 * 1024;

/// Undelivered steer messages a session queues at most — the extension polls
/// every few hundred ms, so backlog means it is gone; drop oldest.
const STEER_QUEUE_CAP: usize = 32;

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// One parsed observer delivery. The JSON contract is OURS on both sides
/// (the extension source lives in `coding::pi_bridge`), but parsing stays
/// defensive anyway — the extension runs inside an arbitrary pi version.
#[derive(Clone, Debug, PartialEq)]
pub enum PiEvent {
    SessionStart,
    /// A user prompt: typed in the TUI (`interactive`) or injected by the
    /// observer itself on behalf of a remote steerer (`extension`).
    Input { text: String, source: String },
    AgentStart,
    /// Pi will not continue on its own — THE idle/waiting-for-input signal.
    AgentSettled,
    /// One completed assistant text block.
    AssistantText { text: String },
    /// One completed thinking block.
    Thinking { text: String },
    ToolStart {
        id: String,
        name: String,
        args: Value,
    },
    ToolEnd { id: String, is_error: bool },
    /// EXP-441: the plan extension presented a plan and is now parked on
    /// pi's confirm dialog awaiting approval.
    PlanPending { id: String, plan: String },
    /// EXP-441: the confirm dialog resolved (locally or by an injected
    /// remote keystroke).
    PlanResolved { id: String, approved: bool },
    /// EXP-724: the extension dispatched a remote slash command through pi's
    /// own API (`ctx.compact` / `ctx.newSession`). The
    /// emitter already echoed the command line, so this is confirmation
    /// only.
    Command { name: String, args: String },
    /// EXP-724: the dispatch threw (an unknown model, a pi without the
    /// API) — narrated so a steerer is never left guessing.
    CommandFailed { name: String, message: String },
    /// EXP-724: pi's own compaction edges (`session_before_compact` /
    /// `session_compact`). `trigger` is already folded onto the wire's
    /// `manual`/`auto` by the extension.
    Compaction {
        phase: CompactionPhase,
        trigger: Option<String>,
    },
    SessionShutdown,
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|text| !text.is_empty())
}

/// Parse one event object. `None` = unknown/junk — dropped, never an error.
fn parse_event(value: &Value) -> Option<PiEvent> {
    match value.get("kind").and_then(Value::as_str)? {
        "session_start" => Some(PiEvent::SessionStart),
        "input" => Some(PiEvent::Input {
            text: string_field(value, "text")?,
            source: string_field(value, "source").unwrap_or_default(),
        }),
        "agent_start" => Some(PiEvent::AgentStart),
        "agent_settled" => Some(PiEvent::AgentSettled),
        "text" => Some(PiEvent::AssistantText {
            text: string_field(value, "text")?,
        }),
        "thinking" => Some(PiEvent::Thinking {
            text: string_field(value, "text")?,
        }),
        "tool_start" => Some(PiEvent::ToolStart {
            id: string_field(value, "id").unwrap_or_default(),
            name: string_field(value, "name")?,
            args: value.get("args").cloned().unwrap_or(Value::Null),
        }),
        "tool_end" => Some(PiEvent::ToolEnd {
            id: string_field(value, "id").unwrap_or_default(),
            is_error: value
                .get("isError")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }),
        "plan_pending" => Some(PiEvent::PlanPending {
            id: string_field(value, "id")?,
            // An empty plan still shows an approval card — the gate is real
            // even when the agent sent no markdown.
            plan: value
                .get("plan")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        }),
        "plan_resolved" => Some(PiEvent::PlanResolved {
            id: string_field(value, "id")?,
            approved: value
                .get("approved")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }),
        "command" => Some(PiEvent::Command {
            name: string_field(value, "name")?,
            args: string_field(value, "args").unwrap_or_default(),
        }),
        "command_failed" => Some(PiEvent::CommandFailed {
            name: string_field(value, "name")?,
            message: string_field(value, "message").unwrap_or_default(),
        }),
        "compaction" => Some(PiEvent::Compaction {
            phase: match value.get("phase").and_then(Value::as_str)? {
                "started" => CompactionPhase::Started,
                "ended" => CompactionPhase::Ended,
                other => {
                    log::debug!("pi observer: ignoring compaction phase {other:?}");
                    return None;
                }
            },
            trigger: string_field(value, "trigger"),
        }),
        "session_shutdown" => Some(PiEvent::SessionShutdown),
        other => {
            log::debug!("pi observer: ignoring event kind {other:?}");
            None
        }
    }
}

// ---------------------------------------------------------------------------
// The steer queue
// ---------------------------------------------------------------------------

/// One thing the extension's next `/steer` poll should apply. Commands ride
/// the SAME queue as messages so their order relative to a steerer's prose
/// is preserved (EXP-724).
#[derive(Clone, Debug, PartialEq, Eq)]
enum SteerItem {
    Message(String),
    Command { name: String, args: String },
}

#[derive(Default)]
struct SteerQueue {
    items: Mutex<VecDeque<SteerItem>>,
    wake: Condvar,
}

impl SteerQueue {
    fn push(&self, item: SteerItem) {
        let mut items = match self.items.lock() {
            Ok(items) => items,
            Err(poisoned) => poisoned.into_inner(),
        };
        if items.len() >= STEER_QUEUE_CAP {
            items.pop_front();
        }
        items.push_back(item);
        self.wake.notify_all();
    }

    /// Drain everything queued, parking up to [`STEER_POLL_WAIT`] when empty.
    fn drain_or_wait(&self) -> Vec<SteerItem> {
        let items = match self.items.lock() {
            Ok(items) => items,
            Err(poisoned) => poisoned.into_inner(),
        };
        let (mut items, _) = self
            .wake
            .wait_timeout_while(items, STEER_POLL_WAIT, |items| items.is_empty())
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        items.drain(..).collect()
    }
}

/// The publisher's steer seam for ONE pi session: `push` queues a remote
/// composer message and `push_command` a remote slash command for the
/// extension's next `/steer` poll.
#[derive(Clone)]
pub struct PiSteerHandle(Arc<SteerQueue>);

impl PiSteerHandle {
    pub fn push(&self, text: String) {
        self.0.push(SteerItem::Message(text));
    }

    /// EXP-724: `{name, args}` for the extension to run through pi's own API
    /// — never `pi.sendUserMessage`, which sends `/compact` to the model as
    /// literal text (built-in commands dispatch only in the TUI's onSubmit).
    pub fn push_command(&self, name: &str, args: &str) {
        self.0.push(SteerItem::Command {
            name: name.to_string(),
            args: args.to_string(),
        });
    }
}

// ---------------------------------------------------------------------------
// The server
// ---------------------------------------------------------------------------

/// One session's subscription: events in, steer messages out, keyed by the
/// canonicalized worktree.
struct Subscriber {
    worktree: PathBuf,
    tx: flume::Sender<PiEvent>,
    steer: Arc<SteerQueue>,
}

/// The loopback observer sidecar — ONE per process, alive for its lifetime.
pub struct ObserverServer {
    port: u16,
    token: String,
    subscribers: Arc<Mutex<Vec<Subscriber>>>,
    shutdown: Arc<AtomicBool>,
    accept: Option<JoinHandle<()>>,
}

impl ObserverServer {
    pub fn start() -> std::io::Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let port = listener.local_addr()?.port();
        let token = format!("{}", uuid::Uuid::new_v4().simple());
        let subscribers: Arc<Mutex<Vec<Subscriber>>> = Arc::new(Mutex::new(Vec::new()));
        let shutdown = Arc::new(AtomicBool::new(false));
        let accept = {
            let token = token.clone();
            let shutdown = Arc::clone(&shutdown);
            let subscribers = Arc::clone(&subscribers);
            std::thread::Builder::new()
                .name("exp-pi-observer".to_string())
                .spawn(move || accept_loop(listener, token, subscribers, shutdown))?
        };
        Ok(Self {
            port,
            token,
            subscribers,
            shutdown,
            accept: Some(accept),
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    /// Subscribe a session's worktree: its emitter drains the receiver, the
    /// publisher pushes remote messages through the handle. Dropping the
    /// receiver unsubscribes on the next delivery.
    pub fn subscribe(&self, worktree: &Path) -> (flume::Receiver<PiEvent>, PiSteerHandle) {
        let (tx, rx) = flume::unbounded();
        let steer = Arc::new(SteerQueue::default());
        let mut subscribers = match self.subscribers.lock() {
            Ok(subscribers) => subscribers,
            Err(poisoned) => poisoned.into_inner(),
        };
        subscribers.retain(|subscriber| !subscriber.tx.is_disconnected());
        subscribers.push(Subscriber {
            worktree: canonical(worktree),
            tx,
            steer: Arc::clone(&steer),
        });
        (rx, PiSteerHandle(steer))
    }

    /// Stop accepting and join the accept thread. Idempotent.
    pub fn shutdown(&mut self) {
        if self.shutdown.swap(true, Ordering::SeqCst) {
            return;
        }
        if let Ok(stream) = TcpStream::connect((Ipv4Addr::LOCALHOST, self.port)) {
            let _ = stream.shutdown(Shutdown::Both);
        }
        if let Some(handle) = self.accept.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for ObserverServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn accept_loop(
    listener: TcpListener,
    token: String,
    subscribers: Arc<Mutex<Vec<Subscriber>>>,
    shutdown: Arc<AtomicBool>,
) {
    for stream in listener.incoming() {
        if shutdown.load(Ordering::SeqCst) {
            return;
        }
        let Ok(stream) = stream else { continue };
        let token = token.clone();
        let subscribers = Arc::clone(&subscribers);
        // One thread per request; `/steer` parks its thread for the poll
        // window, so the accept loop must never handle inline.
        let spawned = std::thread::Builder::new()
            .name("exp-pi-observer-conn".to_string())
            .spawn(move || {
                if let Err(err) = handle_connection(stream, &token, &subscribers) {
                    log::debug!("pi observer connection: {err}");
                }
            });
        if let Err(err) = spawned {
            log::debug!("pi observer handler thread: {err}");
        }
    }
}

fn handle_connection(
    stream: TcpStream,
    token: &str,
    subscribers: &Arc<Mutex<Vec<Subscriber>>>,
) -> std::io::Result<()> {
    stream.set_read_timeout(Some(CONN_TIMEOUT))?;
    stream.set_write_timeout(Some(CONN_TIMEOUT))?;
    let mut writer = stream.try_clone()?;
    let mut reader = BufReader::new(stream);

    let mut budget = MAX_HEADER_BYTES;
    let mut request_line = String::new();
    if reader
        .by_ref()
        .take(budget as u64)
        .read_line(&mut request_line)?
        == 0
    {
        return Ok(()); // the shutdown poke
    }
    budget = budget.saturating_sub(request_line.len());
    let mut fields = request_line.split_whitespace();
    let method = fields.next().unwrap_or_default().to_string();
    let path = fields.next().unwrap_or_default().to_string();

    let mut content_length = 0usize;
    let mut authorization = String::new();
    loop {
        let mut line = String::new();
        let read = reader.by_ref().take(budget as u64).read_line(&mut line)?;
        if read == 0 {
            break;
        }
        budget = budget.saturating_sub(read);
        if budget == 0 {
            return respond(&mut writer, "431 Request Header Fields Too Large");
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match name.trim().to_ascii_lowercase().as_str() {
            "content-length" => content_length = value.parse().unwrap_or(0),
            "authorization" => authorization = value.to_string(),
            _ => {}
        }
    }

    let drain_body = |reader: &mut BufReader<TcpStream>, len: usize| {
        let _ = std::io::copy(
            &mut reader.by_ref().take(len.min(MAX_BODY_BYTES) as u64),
            &mut std::io::sink(),
        );
    };
    let route = match (method.as_str(), path.as_str()) {
        ("POST", "/events") => Route::Events,
        ("POST", "/steer") => Route::Steer,
        _ => {
            drain_body(&mut reader, content_length);
            return respond(&mut writer, "404 Not Found");
        }
    };
    if !bearer_matches(&authorization, token) {
        drain_body(&mut reader, content_length);
        return respond(&mut writer, "401 Unauthorized");
    }
    if content_length > MAX_BODY_BYTES {
        return respond(&mut writer, "413 Payload Too Large");
    }
    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body)?;

    let value: Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(err) => {
            log::debug!("pi observer body is not JSON: {err}");
            return respond(&mut writer, "204 No Content");
        }
    };
    let Some(cwd) = string_field(&value, "cwd") else {
        return respond(&mut writer, "204 No Content");
    };
    match route {
        Route::Events => {
            // Answer BEFORE routing — the extension batches on a timer and
            // must never stall on our parsing.
            respond(&mut writer, "204 No Content")?;
            let events: Vec<PiEvent> = value
                .get("events")
                .and_then(Value::as_array)
                .map(|items| items.iter().filter_map(parse_event).collect())
                .unwrap_or_default();
            if !events.is_empty() {
                route_events(subscribers, &cwd, events);
            }
            Ok(())
        }
        Route::Steer => {
            let queue = subscriber_queue(subscribers, &cwd);
            let items = match queue {
                Some(queue) => queue.drain_or_wait(),
                // No session for this cwd (yet) — park anyway so the
                // extension's poll loop doesn't spin.
                None => {
                    std::thread::sleep(STEER_POLL_WAIT);
                    Vec::new()
                }
            };
            let mut messages: Vec<&String> = Vec::new();
            let mut commands: Vec<serde_json::Value> = Vec::new();
            for item in &items {
                match item {
                    SteerItem::Message(text) => messages.push(text),
                    SteerItem::Command { name, args } => {
                        commands.push(serde_json::json!({ "name": name, "args": args }))
                    }
                }
            }
            let body = serde_json::to_string(
                &serde_json::json!({ "messages": messages, "commands": commands }),
            )
            .unwrap_or_else(|_| r#"{"messages":[],"commands":[]}"#.to_string());
            write!(
                writer,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )?;
            writer.flush()
        }
    }
}

enum Route {
    Events,
    Steer,
}

fn route_events(subscribers: &Arc<Mutex<Vec<Subscriber>>>, cwd: &str, events: Vec<PiEvent>) {
    let cwd = canonical(Path::new(cwd));
    let mut subscribers = match subscribers.lock() {
        Ok(subscribers) => subscribers,
        Err(poisoned) => poisoned.into_inner(),
    };
    subscribers.retain(|subscriber| !subscriber.tx.is_disconnected());
    let Some(target) = subscribers
        .iter()
        .find(|subscriber| subscriber.worktree == cwd)
    else {
        log::debug!("pi observer: events with no matching session, dropped");
        return;
    };
    for event in events {
        let _ = target.tx.send(event);
    }
}

fn subscriber_queue(
    subscribers: &Arc<Mutex<Vec<Subscriber>>>,
    cwd: &str,
) -> Option<Arc<SteerQueue>> {
    let cwd = canonical(Path::new(cwd));
    let mut subscribers = match subscribers.lock() {
        Ok(subscribers) => subscribers,
        Err(poisoned) => poisoned.into_inner(),
    };
    subscribers.retain(|subscriber| !subscriber.tx.is_disconnected());
    subscribers
        .iter()
        .find(|subscriber| subscriber.worktree == cwd)
        .map(|subscriber| Arc::clone(&subscriber.steer))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn post(port: u16, token: &str, path: &str, body: &str) -> String {
        let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .unwrap();
        write!(
            stream,
            "POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        stream.flush().unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    }

    fn temp_worktree(tag: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "exp-pi-observer-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn events_route_to_the_matching_worktree_subscriber() {
        let server = ObserverServer::start().unwrap();
        let worktree = temp_worktree("route");
        let (events, _steer) = server.subscribe(&worktree);
        let body = serde_json::json!({
            "cwd": worktree.to_string_lossy(),
            "events": [
                { "kind": "session_start" },
                { "kind": "input", "text": "fix it", "source": "interactive" },
                { "kind": "tool_start", "id": "t1", "name": "bash", "args": { "command": "ls -la" } },
                { "kind": "future_kind", "x": 1 },
                { "kind": "plan_pending", "id": "call-1", "plan": "## Plan\n1. do it" },
                { "kind": "plan_resolved", "id": "call-1", "approved": true },
                { "kind": "agent_settled" }
            ]
        })
        .to_string();
        let response = post(server.port(), server.token(), "/events", &body);
        assert!(response.starts_with("HTTP/1.1 204"), "{response}");
        assert_eq!(
            events.recv_timeout(Duration::from_secs(2)).unwrap(),
            PiEvent::SessionStart
        );
        assert_eq!(
            events.recv_timeout(Duration::from_secs(2)).unwrap(),
            PiEvent::Input {
                text: "fix it".into(),
                source: "interactive".into(),
            }
        );
        assert_eq!(
            events.recv_timeout(Duration::from_secs(2)).unwrap(),
            PiEvent::ToolStart {
                id: "t1".into(),
                name: "bash".into(),
                args: serde_json::json!({ "command": "ls -la" }),
            }
        );
        // The unknown kind was dropped; the plan lifecycle (EXP-441) comes
        // straight after.
        assert_eq!(
            events.recv_timeout(Duration::from_secs(2)).unwrap(),
            PiEvent::PlanPending {
                id: "call-1".into(),
                plan: "## Plan\n1. do it".into(),
            }
        );
        assert_eq!(
            events.recv_timeout(Duration::from_secs(2)).unwrap(),
            PiEvent::PlanResolved {
                id: "call-1".into(),
                approved: true,
            }
        );
        assert_eq!(
            events.recv_timeout(Duration::from_secs(2)).unwrap(),
            PiEvent::AgentSettled
        );
        let _ = std::fs::remove_dir_all(&worktree);
    }

    #[test]
    fn wrong_token_is_rejected_and_routes_nothing() {
        let server = ObserverServer::start().unwrap();
        let worktree = temp_worktree("auth");
        let (events, _steer) = server.subscribe(&worktree);
        let body = serde_json::json!({
            "cwd": worktree.to_string_lossy(),
            "events": [ { "kind": "agent_start" } ]
        })
        .to_string();
        let response = post(server.port(), "not-the-token", "/events", &body);
        assert!(response.starts_with("HTTP/1.1 401"), "{response}");
        assert!(events.recv_timeout(Duration::from_millis(200)).is_err());
        let _ = std::fs::remove_dir_all(&worktree);
    }

    #[test]
    fn steer_long_poll_returns_pushed_messages_and_commands() {
        let server = ObserverServer::start().unwrap();
        let worktree = temp_worktree("steer");
        let (_events, steer) = server.subscribe(&worktree);
        // Push first, poll second: an already-queued message answers
        // immediately.
        steer.push("focus on the failing test".to_string());
        let body = serde_json::json!({ "cwd": worktree.to_string_lossy() }).to_string();
        let response = post(server.port(), server.token(), "/steer", &body);
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        assert!(
            response.contains(r#""messages":["focus on the failing test"]"#),
            "{response}"
        );
        assert!(response.contains(r#""commands":[]"#), "{response}");

        // EXP-724: a slash command rides the same poll under its own key —
        // the extension runs it through pi's API, never sendUserMessage.
        steer.push_command("compact", "keep the diff");
        let response = post(server.port(), server.token(), "/steer", &body);
        // Compare parsed, not as text: serde_json's key order flips with the
        // `preserve_order` feature, which cargo unifies in from other
        // workspace crates in a multi-crate test run.
        let payload: serde_json::Value = serde_json::from_str(
            response.split("\r\n\r\n").nth(1).unwrap_or_default(),
        )
        .unwrap_or_else(|err| panic!("{err}: {response}"));
        assert_eq!(
            payload["commands"],
            serde_json::json!([{ "name": "compact", "args": "keep the diff" }]),
            "{response}"
        );
        assert_eq!(payload["messages"], serde_json::json!([]), "{response}");

        // Poll first, push from another thread: the park wakes and delivers.
        let port = server.port();
        let token = server.token().to_string();
        let poll_body = body.clone();
        let poller =
            std::thread::spawn(move || post(port, &token, "/steer", &poll_body));
        std::thread::sleep(Duration::from_millis(300));
        steer.push("second".to_string());
        let response = poller.join().unwrap();
        assert!(response.contains(r#""messages":["second"]"#), "{response}");
        let _ = std::fs::remove_dir_all(&worktree);
    }

    #[test]
    fn wrong_routes_404() {
        let server = ObserverServer::start().unwrap();
        let response = post(server.port(), server.token(), "/hook", "{}");
        assert!(response.starts_with("HTTP/1.1 404"), "{response}");
    }
}
