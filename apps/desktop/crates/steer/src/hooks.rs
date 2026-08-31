//! The per-session Claude **hooks sidecar** (EXP-249) — the structured half
//! of steering v2.
//!
//! Screen-scraping claude's TUI grid tells us a picker is on screen but never
//! WHAT it is: plan bodies had to be guessed from `~/.claude/plans`, question
//! identity did not exist, and resolution was inferred from magic narration
//! strings. Claude's own hooks carry all of it as JSON. So every claude
//! coding session spawns with a `--settings` file ([`hook_settings_json`])
//! whose command hooks `curl` their payload into a loopback HTTP server owned
//! by this process ([`HookServer`]), and the emitter reads structured
//! [`HookEvent`]s off a channel while the grid watchers stay for keystroke
//! mapping only.
//!
//! Posture:
//!
//! * **Loopback + bearer token** — the listener binds `127.0.0.1:0` (ephemeral
//!   port) and rejects any request without the random per-session token, so a
//!   second local user cannot inject session events. The token itself rides a
//!   0600 curl config file (referenced via `-K "$EXP_HOOK_CONFIG"`), never the
//!   hook shell line — the shell expands `$VAR`s into curl's argv and
//!   `/proc/<pid>/cmdline` is world-readable on Linux, so an argv token would
//!   hand that second user exactly the credential the check exists to demand
//!   (REV-51).
//! * **Additive, never load-bearing** — the hook command is one `curl` with a
//!   3s cap. If `curl` is missing, the port is gone, or the payload is junk,
//!   claude logs a non-blocking hook error and the session runs exactly as it
//!   did before: grid-only detection, no loss of function.
//! * **Defensive parsing** — unknown fields are ignored, unparseable payloads
//!   are logged at debug and dropped. A hook payload is untrusted input on a
//!   hot path; it must never panic the thread.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;

/// Spawn-env var carrying the sidecar's port (mirrored by
/// `coding::launcher`'s `HOOK_PORT_ENV` — the two crates cannot depend on
/// each other, §3.1). Expanded by the hook shell, so the settings file itself
/// holds no per-session values.
pub const HOOK_PORT_ENV: &str = "EXP_HOOK_PORT";
/// Spawn-env var carrying the PATH of the 0600 curl config file that holds
/// the sidecar's bearer token (content: [`hook_curl_config`]; see
/// [`HOOK_PORT_ENV`]). The path — not the token — is what the hook shell
/// expands into curl's argv (REV-51, module header).
pub const HOOK_CONFIG_ENV: &str = "EXP_HOOK_CONFIG";

/// Which `PreToolUse` calls the settings file forwards. Anything else stays
/// on the grid/transcript path — the sidecar is for the blocking, structured
/// moments only. `Agent` is claude ≥2.1.220's name for the subagent tool
/// (`Task` before it) — both dispatch shapes are identical (EXP-356).
pub const PRE_TOOL_USE_MATCHER: &str = "ExitPlanMode|AskUserQuestion|Task|Agent";

/// Largest hook body accepted. A `PreToolUse:ExitPlanMode` payload carries
/// the whole plan verbatim in `tool_input`, and real plans cleared the old
/// 64KiB cap (EXP-691) — a rejected body dropped the plan card's entire
/// text, leaving only the generic headline. The listener is loopback-only
/// and bearer-authed, and the emitter truncates every string downstream, so
/// a generous cap costs one bounded allocation per delivery.
pub const MAX_BODY_BYTES: usize = 1024 * 1024;

/// Per-connection socket timeout — a hook that stalls must never pin a
/// handler thread.
const CONN_TIMEOUT: Duration = Duration::from_secs(2);

/// Header-section budget (request line + headers).
const MAX_HEADER_BYTES: usize = 8 * 1024;

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// The fields every claude hook payload carries, whatever the event.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HookContext {
    /// Claude's own session id (NOT the `coding_sessions` row id).
    pub session_id: Option<String>,
    pub transcript_path: Option<String>,
    pub cwd: Option<String>,
}

/// One parsed hook delivery.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HookEvent {
    pub context: HookContext,
    pub kind: HookEventKind,
}

/// One `AskUserQuestion` question (claude's `tool_input.questions[i]`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HookQuestion {
    pub question: String,
    pub header: Option<String>,
    pub options: Vec<HookQuestionOption>,
    pub multi_select: bool,
}

/// One choice of a [`HookQuestion`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HookQuestionOption {
    pub label: String,
    pub description: Option<String>,
}

/// What the sidecar learned. `tool_use_id` is the identity anchor: the
/// emitter derives every question id from it, so a card can be replaced and
/// retired instead of guessed at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HookEventKind {
    /// `ExitPlanMode` — the plan body, verbatim, before the approval picker
    /// paints.
    PlanProposed {
        tool_use_id: Option<String>,
        plan: String,
    },
    /// `AskUserQuestion` — every question of ONE ask, in order.
    QuestionsAsked {
        tool_use_id: Option<String>,
        questions: Vec<HookQuestion>,
    },
    /// `Task` — a subagent is about to start.
    SubagentDispatched {
        tool_use_id: Option<String>,
        description: Option<String>,
        subagent_type: Option<String>,
    },
    SubagentStarted {
        agent_id: Option<String>,
        agent_type: Option<String>,
    },
    SubagentStopped {
        agent_id: Option<String>,
        agent_type: Option<String>,
    },
    /// A `Notification` classified as a permission prompt.
    PermissionPrompt {
        message: String,
        tool: Option<String>,
    },
    /// Any other `Notification` (idle nudges are the common one).
    Idle { message: String },
    /// The turn ended.
    Stop,
    /// The session ended (only delivered if a future settings file registers
    /// `SessionEnd`; parsed here so it never falls off a cliff).
    SessionEnd { reason: Option<String> },
    /// EXP-443: claude booted (or `/clear`/resume rotated the conversation).
    /// Carries no card of its own — its whole value is the context's
    /// `session_id` reaching the transcript pin at startup instead of at the
    /// first plan/notification hook, minutes in.
    SessionStarted { source: Option<String> },
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

fn string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|text| !text.is_empty())
}

/// Parse one hook body. `None` = "not something we act on" (junk, an
/// unregistered event, a tool we do not specialize) — never an error path.
pub fn parse_hook_event(body: &[u8]) -> Option<HookEvent> {
    let value: Value = match serde_json::from_slice(body) {
        Ok(value) => value,
        Err(err) => {
            log::debug!("hook payload is not JSON: {err}");
            return None;
        }
    };
    let context = HookContext {
        session_id: string_field(&value, "session_id"),
        transcript_path: string_field(&value, "transcript_path"),
        cwd: string_field(&value, "cwd"),
    };
    // `hook_event_name` is claude's own discriminator; infer from the payload
    // shape when a future build omits it.
    let event_name = string_field(&value, "hook_event_name").unwrap_or_else(|| {
        if value.get("tool_name").is_some() {
            "PreToolUse".to_string()
        } else if value.get("message").is_some() {
            "Notification".to_string()
        } else {
            String::new()
        }
    });
    let kind = match event_name.as_str() {
        "PreToolUse" => parse_pre_tool_use(&value)?,
        "SubagentStart" => HookEventKind::SubagentStarted {
            agent_id: string_field(&value, "agent_id"),
            agent_type: string_field(&value, "agent_type"),
        },
        "SubagentStop" => HookEventKind::SubagentStopped {
            agent_id: string_field(&value, "agent_id"),
            agent_type: string_field(&value, "agent_type"),
        },
        "Notification" => parse_notification(&value),
        "Stop" => HookEventKind::Stop,
        "SessionEnd" => HookEventKind::SessionEnd {
            reason: string_field(&value, "reason"),
        },
        "SessionStart" => HookEventKind::SessionStarted {
            source: string_field(&value, "source"),
        },
        other => {
            log::debug!("ignoring hook event {other:?}");
            return None;
        }
    };
    Some(HookEvent { context, kind })
}

fn parse_pre_tool_use(value: &Value) -> Option<HookEventKind> {
    let tool_name = string_field(value, "tool_name")?;
    let tool_use_id = string_field(value, "tool_use_id");
    let empty = Value::Null;
    let input = value.get("tool_input").unwrap_or(&empty);
    match tool_name.as_str() {
        "ExitPlanMode" => {
            // The plan is `tool_input.plan`; a shape change must still yield
            // SOMETHING readable rather than an empty approval card.
            let plan = string_field(input, "plan")
                .unwrap_or_else(|| serde_json::to_string_pretty(input).unwrap_or_default());
            Some(HookEventKind::PlanProposed { tool_use_id, plan })
        }
        "AskUserQuestion" => {
            let questions: Vec<HookQuestion> = input
                .get("questions")
                .and_then(Value::as_array)
                .map(|items| items.iter().filter_map(parse_question).collect())
                .unwrap_or_default();
            if questions.is_empty() {
                log::debug!("AskUserQuestion hook carried no parseable questions");
                return None;
            }
            Some(HookEventKind::QuestionsAsked {
                tool_use_id,
                questions,
            })
        }
        // `Task` through claude v2.1.21x, renamed `Agent` since — same input
        // shape (EXP-356).
        "Task" | "Agent" => Some(HookEventKind::SubagentDispatched {
            tool_use_id,
            description: string_field(input, "description"),
            subagent_type: string_field(input, "subagent_type"),
        }),
        other => {
            log::debug!("ignoring PreToolUse for {other}");
            None
        }
    }
}

fn parse_question(value: &Value) -> Option<HookQuestion> {
    let question = string_field(value, "question")?;
    let options = value
        .get("options")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|option| {
                    Some(HookQuestionOption {
                        label: string_field(option, "label")?,
                        description: string_field(option, "description"),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Some(HookQuestion {
        question,
        header: string_field(value, "header"),
        options,
        multi_select: value
            .get("multiSelect")
            .or_else(|| value.get("multi_select"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

/// Classify a `Notification`. Conservative on purpose: only an explicitly
/// permission-flavoured payload becomes a permission card (that card claims
/// the session is BLOCKED); everything else is an idle nudge.
fn parse_notification(value: &Value) -> HookEventKind {
    let message = string_field(value, "message").unwrap_or_default();
    let type_ish = ["type", "notification_type", "kind", "reason"]
        .iter()
        .find_map(|key| string_field(value, key))
        .unwrap_or_default()
        .to_ascii_lowercase();
    let lowered = message.to_ascii_lowercase();
    if type_ish.contains("permission") || lowered.contains("permission") {
        // claude ≤2.0.x sent `tool_name`; ≥2.1.233 sends only a message,
        // sometimes still naming the tool ("Claude needs your permission to
        // use Bash"), usually not ("Claude needs your permission",
        // "Session paused") — fall back to parsing the message so the
        // degraded card can at least name what is blocked (EXP-512).
        let tool = string_field(value, "tool_name").or_else(|| tool_from_message(&message));
        HookEventKind::PermissionPrompt { message, tool }
    } else {
        HookEventKind::Idle { message }
    }
}

/// Best-effort tool name out of a permission Notification's message — the
/// word(s) after "to use", trimmed of trailing punctuation. Empty ⇒ `None`.
fn tool_from_message(message: &str) -> Option<String> {
    let (_, rest) = message.split_once(" to use ")?;
    let tool = rest.trim().trim_end_matches(['.', '!']).trim();
    (!tool.is_empty()).then(|| tool.to_string())
}

// ---------------------------------------------------------------------------
// The settings file
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct HookSettings {
    hooks: HookRegistrations,
}

#[derive(Serialize)]
struct HookRegistrations {
    #[serde(rename = "PreToolUse")]
    pre_tool_use: Vec<HookGroup>,
    #[serde(rename = "SubagentStart")]
    subagent_start: Vec<HookGroup>,
    #[serde(rename = "SubagentStop")]
    subagent_stop: Vec<HookGroup>,
    #[serde(rename = "Notification")]
    notification: Vec<HookGroup>,
    #[serde(rename = "Stop")]
    stop: Vec<HookGroup>,
    #[serde(rename = "SessionStart")]
    session_start: Vec<HookGroup>,
}

#[derive(Serialize)]
struct HookGroup {
    #[serde(skip_serializing_if = "Option::is_none")]
    matcher: Option<String>,
    hooks: Vec<HookCommand>,
}

#[derive(Serialize)]
struct HookCommand {
    #[serde(rename = "type")]
    kind: &'static str,
    command: String,
}

impl HookGroup {
    fn new(matcher: Option<&str>) -> Self {
        Self {
            matcher: matcher.map(str::to_string),
            hooks: vec![HookCommand {
                kind: "command",
                command: hook_command(),
            }],
        }
    }
}

/// The shell command every hook runs: POST the payload it got on stdin to the
/// sidecar. Both per-session values are `$ENV` references expanded by the
/// hook shell, so the file content is constant and carries no secret — and
/// the bearer token stays behind the `-K` config-file indirection so it
/// never lands in curl's world-readable argv (REV-51).
pub fn hook_command() -> String {
    format!(
        "curl -s -m 3 -X POST -K \"${HOOK_CONFIG_ENV}\" \
         --data-binary @- \"http://127.0.0.1:${HOOK_PORT_ENV}/hook\""
    )
}

/// The [`HOOK_CONFIG_ENV`] curl config file's content: the bearer header the
/// hook command used to carry on its argv. Mirrored by `coding::launcher`,
/// which writes the per-session file (the two crates cannot depend on each
/// other, §3.1).
pub fn hook_curl_config(token: &str) -> String {
    format!("header = \"Authorization: Bearer {token}\"\n")
}

/// Write the curl config owner-only (0600) — it holds the session credential.
pub fn write_hook_curl_config(path: &std::path::Path, token: &str) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(hook_curl_config(token).as_bytes())
    }
    #[cfg(not(unix))]
    std::fs::write(path, hook_curl_config(token))
}

/// The `--settings <path>` file content for a hooked claude session.
pub fn hook_settings_json() -> String {
    let settings = HookSettings {
        hooks: HookRegistrations {
            pre_tool_use: vec![HookGroup::new(Some(PRE_TOOL_USE_MATCHER))],
            subagent_start: vec![HookGroup::new(None)],
            subagent_stop: vec![HookGroup::new(None)],
            notification: vec![HookGroup::new(None)],
            stop: vec![HookGroup::new(None)],
            // EXP-443: fires at claude startup AND on /clear/resume rotation
            // — the pin (and the router's bound set) learn the live session
            // id before any plan/notification hook would have.
            session_start: vec![HookGroup::new(None)],
        },
    };
    serde_json::to_string_pretty(&settings).expect("hook settings serialization cannot fail")
}

// ---------------------------------------------------------------------------
// The server
// ---------------------------------------------------------------------------

/// The loopback sidecar. Alive for the session's lifetime; dropping it stops
/// the accept loop (in-flight handler threads finish on their own timeouts).
pub struct HookServer {
    port: u16,
    token: String,
    events: flume::Receiver<HookEvent>,
    shutdown: Arc<AtomicBool>,
    accept: Option<JoinHandle<()>>,
}

impl HookServer {
    pub fn start() -> std::io::Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        let port = listener.local_addr()?.port();
        let token = format!("{}", uuid::Uuid::new_v4().simple());
        let (tx, events) = flume::unbounded();
        let shutdown = Arc::new(AtomicBool::new(false));
        let accept = {
            let token = token.clone();
            let shutdown = Arc::clone(&shutdown);
            std::thread::Builder::new()
                .name("exp-claude-hooks".to_string())
                .spawn(move || accept_loop(listener, token, tx, shutdown))?
        };
        Ok(Self {
            port,
            token,
            events,
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

    /// The parsed-event stream. Clone it freely (flume receivers are
    /// multi-consumer); a dropped [`HookServer`] closes it.
    pub fn events(&self) -> &flume::Receiver<HookEvent> {
        &self.events
    }

    /// The `--settings` file content for the session this sidecar serves.
    pub fn settings_json(&self) -> String {
        hook_settings_json()
    }

    /// Stop accepting and join the accept thread. Idempotent.
    pub fn shutdown(&mut self) {
        if self.shutdown.swap(true, Ordering::SeqCst) {
            return;
        }
        // `accept` is blocking: wake it with a throwaway connection so the
        // loop can observe the flag and return.
        if let Ok(stream) = TcpStream::connect((Ipv4Addr::LOCALHOST, self.port)) {
            let _ = stream.shutdown(Shutdown::Both);
        }
        if let Some(handle) = self.accept.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for HookServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn accept_loop(
    listener: TcpListener,
    token: String,
    tx: flume::Sender<HookEvent>,
    shutdown: Arc<AtomicBool>,
) {
    for stream in listener.incoming() {
        if shutdown.load(Ordering::SeqCst) {
            return;
        }
        let Ok(stream) = stream else { continue };
        let token = token.clone();
        let tx = tx.clone();
        // One short-lived thread per delivery: a slow client must never delay
        // the next hook (subagents fire concurrently).
        let spawned = std::thread::Builder::new()
            .name("exp-claude-hook".to_string())
            .spawn(move || {
                if let Err(err) = handle_connection(stream, &token, &tx) {
                    log::debug!("hook connection: {err}");
                }
            });
        if let Err(err) = spawned {
            log::debug!("hook handler thread: {err}");
        }
    }
}

fn handle_connection(
    stream: TcpStream,
    token: &str,
    tx: &flume::Sender<HookEvent>,
) -> std::io::Result<()> {
    stream.set_read_timeout(Some(CONN_TIMEOUT))?;
    stream.set_write_timeout(Some(CONN_TIMEOUT))?;
    let mut writer = stream.try_clone()?;
    let mut reader = BufReader::new(stream);

    // Every header line is read through a `take` over the SHARED BufReader —
    // it bounds the line without buffering ahead, so the body stays intact
    // for the `read_exact` below.
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

    // Reject responses must still drain the announced body first: closing
    // with unread bytes in the socket makes the kernel RST the connection,
    // and the client may then never see the status line.
    let drain_body = |reader: &mut BufReader<TcpStream>, len: usize| {
        let _ = std::io::copy(
            &mut reader.by_ref().take(len.min(MAX_BODY_BYTES) as u64),
            &mut std::io::sink(),
        );
    };
    if method != "POST" || !path.starts_with("/hook") {
        drain_body(&mut reader, content_length);
        return respond(&mut writer, "404 Not Found");
    }
    if !bearer_matches(&authorization, token) {
        drain_body(&mut reader, content_length);
        return respond(&mut writer, "401 Unauthorized");
    }
    if content_length > MAX_BODY_BYTES {
        return respond(&mut writer, "413 Payload Too Large");
    }
    // `curl --data-binary @-` buffers stdin and always sends a
    // Content-Length; a chunked body would read as empty and be dropped.
    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body)?;
    // Answer BEFORE parsing: claude blocks on the hook command, and parsing
    // is our problem, not its latency.
    respond(&mut writer, "204 No Content")?;
    if let Some(event) = parse_hook_event(&body) {
        let _ = tx.send(event);
    }
    Ok(())
}

pub(crate) fn respond(writer: &mut TcpStream, status: &str) -> std::io::Result<()> {
    write!(
        writer,
        "HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    )?;
    writer.flush()
}

/// `Authorization: Bearer <token>` in constant time over the token bytes (a
/// local attacker guessing byte-by-byte is cheap to rule out).
pub(crate) fn bearer_matches(header: &str, token: &str) -> bool {
    let Some(presented) = header.strip_prefix("Bearer ").map(str::trim) else {
        return false;
    };
    if presented.len() != token.len() {
        return false;
    }
    presented
        .bytes()
        .zip(token.bytes())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> HookEvent {
        parse_hook_event(json.as_bytes()).expect("parsed")
    }

    #[test]
    fn parses_exit_plan_mode_payload() {
        let event = parse(
            r###"{
              "session_id": "sess-abc",
              "transcript_path": "/home/u/.claude/projects/p/sess-abc.jsonl",
              "cwd": "/home/u/repo",
              "permission_mode": "plan",
              "hook_event_name": "PreToolUse",
              "tool_name": "ExitPlanMode",
              "tool_use_id": "toolu_01ABC",
              "tool_input": { "plan": "## Plan\n1. Do the thing" }
            }"###,
        );
        assert_eq!(
            event.context,
            HookContext {
                session_id: Some("sess-abc".into()),
                transcript_path: Some("/home/u/.claude/projects/p/sess-abc.jsonl".into()),
                cwd: Some("/home/u/repo".into()),
            }
        );
        assert_eq!(
            event.kind,
            HookEventKind::PlanProposed {
                tool_use_id: Some("toolu_01ABC".into()),
                plan: "## Plan\n1. Do the thing".into(),
            }
        );
    }

    #[test]
    fn parses_the_renamed_agent_tool_as_a_dispatch() {
        // claude ≥2.1.220 renamed the subagent tool `Task` → `Agent`
        // (EXP-356); the input shape is unchanged.
        let event = parse(
            r#"{"hook_event_name":"PreToolUse","tool_name":"Agent","tool_use_id":"toolu_03",
                "tool_input":{"description":"Find the flow","subagent_type":"Explore","prompt":"..."}}"#,
        );
        assert_eq!(
            event.kind,
            HookEventKind::SubagentDispatched {
                tool_use_id: Some("toolu_03".into()),
                description: Some("Find the flow".into()),
                subagent_type: Some("Explore".into()),
            }
        );
        assert!(
            PRE_TOOL_USE_MATCHER.split('|').any(|t| t == "Agent"),
            "the settings matcher must forward Agent dispatches"
        );
    }

    #[test]
    fn plan_falls_back_to_the_whole_tool_input() {
        // A future claude renames `plan` — the approval card must still show
        // something rather than an empty body.
        let event = parse(
            r#"{"hook_event_name":"PreToolUse","tool_name":"ExitPlanMode","tool_input":{"planMarkdown":"do x"}}"#,
        );
        match event.kind {
            HookEventKind::PlanProposed { plan, tool_use_id } => {
                assert!(plan.contains("do x"), "{plan}");
                assert_eq!(tool_use_id, None);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn parses_ask_user_question_payload() {
        let event = parse(
            r#"{
              "session_id": "sess-abc",
              "hook_event_name": "PreToolUse",
              "tool_name": "AskUserQuestion",
              "tool_use_id": "toolu_02",
              "tool_input": {
                "questions": [
                  {
                    "question": "Which color?",
                    "header": "Color",
                    "multiSelect": false,
                    "options": [
                      { "label": "Red", "description": "warm" },
                      { "label": "Blue" }
                    ]
                  },
                  {
                    "question": "Ship it?",
                    "multiSelect": true,
                    "options": [{ "label": "Yes" }]
                  }
                ]
              }
            }"#,
        );
        assert_eq!(
            event.kind,
            HookEventKind::QuestionsAsked {
                tool_use_id: Some("toolu_02".into()),
                questions: vec![
                    HookQuestion {
                        question: "Which color?".into(),
                        header: Some("Color".into()),
                        options: vec![
                            HookQuestionOption {
                                label: "Red".into(),
                                description: Some("warm".into()),
                            },
                            HookQuestionOption {
                                label: "Blue".into(),
                                description: None,
                            },
                        ],
                        multi_select: false,
                    },
                    HookQuestion {
                        question: "Ship it?".into(),
                        header: None,
                        options: vec![HookQuestionOption {
                            label: "Yes".into(),
                            description: None,
                        }],
                        multi_select: true,
                    },
                ],
            }
        );
    }

    #[test]
    fn parses_task_subagent_and_lifecycle_payloads() {
        assert_eq!(
            parse(
                r#"{"hook_event_name":"PreToolUse","tool_name":"Task","tool_use_id":"toolu_03",
                    "tool_input":{"description":"Map the crate","subagent_type":"explore","prompt":"…"}}"#
            )
            .kind,
            HookEventKind::SubagentDispatched {
                tool_use_id: Some("toolu_03".into()),
                description: Some("Map the crate".into()),
                subagent_type: Some("explore".into()),
            }
        );
        assert_eq!(
            parse(
                r#"{"session_id":"s","hook_event_name":"SubagentStart","agent_id":"agent_01","agent_type":"explore"}"#
            )
            .kind,
            HookEventKind::SubagentStarted {
                agent_id: Some("agent_01".into()),
                agent_type: Some("explore".into()),
            }
        );
        assert_eq!(
            parse(
                r#"{"session_id":"s","hook_event_name":"SubagentStop","agent_id":"agent_01","stop_hook_active":false}"#
            )
            .kind,
            HookEventKind::SubagentStopped {
                agent_id: Some("agent_01".into()),
                agent_type: None,
            }
        );
    }

    #[test]
    fn classifies_notifications_conservatively() {
        assert_eq!(
            parse(
                r#"{"session_id":"s","hook_event_name":"Notification","message":"Claude needs your permission to use Bash","tool_name":"Bash"}"#
            )
            .kind,
            HookEventKind::PermissionPrompt {
                message: "Claude needs your permission to use Bash".into(),
                tool: Some("Bash".into()),
            }
        );
        // No permission wording anywhere ⇒ idle, never a "blocked" card.
        assert_eq!(
            parse(
                r#"{"session_id":"s","hook_event_name":"Notification","message":"Claude is waiting for your input"}"#
            )
            .kind,
            HookEventKind::Idle {
                message: "Claude is waiting for your input".into(),
            }
        );
        // A type-ish field is honored even when the copy changes.
        assert_eq!(
            parse(
                r#"{"hook_event_name":"Notification","type":"tool_permission","message":"Approve?"}"#
            )
            .kind,
            HookEventKind::PermissionPrompt {
                message: "Approve?".into(),
                tool: None,
            }
        );
        // claude ≥2.1.233 drops `tool_name` — the message names the tool
        // when anything does (EXP-512).
        assert_eq!(
            parse(
                r#"{"session_id":"s","hook_event_name":"Notification","message":"Claude needs your permission to use Bash","notification_type":"permission_prompt"}"#
            )
            .kind,
            HookEventKind::PermissionPrompt {
                message: "Claude needs your permission to use Bash".into(),
                tool: Some("Bash".into()),
            }
        );
        // …and the generic / paused variants carry no tool at all.
        for message in ["Claude needs your permission", "Session paused"] {
            assert_eq!(
                parse(&format!(
                    r#"{{"session_id":"s","hook_event_name":"Notification","message":"{message}","notification_type":"permission_prompt"}}"#
                ))
                .kind,
                HookEventKind::PermissionPrompt {
                    message: message.into(),
                    tool: None,
                }
            );
        }
    }

    #[test]
    fn parses_turn_and_session_edges() {
        assert_eq!(
            parse(r#"{"session_id":"s","hook_event_name":"Stop","stop_hook_active":false}"#).kind,
            HookEventKind::Stop
        );
        assert_eq!(
            parse(r#"{"session_id":"s","hook_event_name":"SessionEnd","reason":"exit"}"#).kind,
            HookEventKind::SessionEnd {
                reason: Some("exit".into()),
            }
        );
    }

    #[test]
    fn junk_and_unhandled_payloads_are_dropped_not_panics() {
        assert_eq!(parse_hook_event(b"not json"), None);
        assert_eq!(parse_hook_event(b"[]"), None);
        assert_eq!(parse_hook_event(b"{}"), None);
        // A PreToolUse for a tool we do not specialize.
        assert_eq!(
            parse_hook_event(
                br#"{"hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":"ls"}}"#
            ),
            None
        );
        // An AskUserQuestion with nothing parseable in it.
        assert_eq!(
            parse_hook_event(
                br#"{"hook_event_name":"PreToolUse","tool_name":"AskUserQuestion","tool_input":{"questions":[{}]}}"#
            ),
            None
        );
        assert_eq!(
            parse_hook_event(br#"{"hook_event_name":"PreCompact"}"#),
            None
        );
    }

    #[test]
    fn event_name_is_inferred_when_absent() {
        assert!(matches!(
            parse(r#"{"tool_name":"ExitPlanMode","tool_input":{"plan":"p"}}"#).kind,
            HookEventKind::PlanProposed { .. }
        ));
        assert!(matches!(
            parse(r#"{"message":"Claude is waiting for your input"}"#).kind,
            HookEventKind::Idle { .. }
        ));
    }

    /// EXP-443: SessionStart is pure pin fuel — the kind carries only the
    /// `source`, the context carries the id the pin needs.
    #[test]
    fn parses_session_start_with_full_context() {
        let event = parse(
            r#"{"hook_event_name":"SessionStart","source":"resume",
                "session_id":"sess-1","transcript_path":"/tmp/t/sess-1.jsonl","cwd":"/repo"}"#,
        );
        assert_eq!(
            event.kind,
            HookEventKind::SessionStarted {
                source: Some("resume".to_string())
            }
        );
        assert_eq!(event.context.session_id.as_deref(), Some("sess-1"));
        assert_eq!(event.context.cwd.as_deref(), Some("/repo"));
    }

    #[test]
    fn settings_json_registers_every_hook_with_the_env_expanded_command() {
        let value: Value = serde_json::from_str(&hook_settings_json()).unwrap();
        let hooks = &value["hooks"];
        assert_eq!(hooks["PreToolUse"][0]["matcher"], PRE_TOOL_USE_MATCHER);
        for event in [
            "SubagentStart",
            "SubagentStop",
            "Notification",
            "Stop",
            "SessionStart",
        ] {
            assert!(
                hooks[event][0].get("matcher").is_none(),
                "{event} must not be matcher-scoped"
            );
        }
        for event in [
            "PreToolUse",
            "SubagentStart",
            "SubagentStop",
            "Notification",
            "Stop",
            "SessionStart",
        ] {
            let command = &hooks[event][0]["hooks"][0];
            assert_eq!(command["type"], "command");
            let command = command["command"].as_str().unwrap();
            assert!(command.starts_with("curl -s -m 3 -X POST"), "{command}");
            // REV-51: the token must never be argv-visible — the bearer
            // header rides the 0600 config file behind `-K`.
            assert!(command.contains("-K \"$EXP_HOOK_CONFIG\""), "{command}");
            assert!(!command.contains("Bearer"), "{command}");
            assert!(
                command.contains("http://127.0.0.1:$EXP_HOOK_PORT/hook"),
                "{command}"
            );
            assert!(command.contains("--data-binary @-"), "{command}");
        }
    }

    // ── The loopback server ────────────────────────────────────────────────

    fn post(port: u16, token: &str, body: &str) -> String {
        let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        write!(
            stream,
            "POST /hook HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        stream.flush().unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    }

    #[test]
    fn server_accepts_authenticated_hooks_and_publishes_events() {
        let server = HookServer::start().unwrap();
        let response = post(
            server.port(),
            server.token(),
            r#"{"hook_event_name":"Stop","session_id":"s"}"#,
        );
        assert!(response.starts_with("HTTP/1.1 204"), "{response}");
        let event = server
            .events()
            .recv_timeout(Duration::from_secs(2))
            .expect("event");
        assert_eq!(event.kind, HookEventKind::Stop);
        assert_eq!(event.context.session_id.as_deref(), Some("s"));
    }

    #[test]
    fn server_rejects_a_wrong_token_and_publishes_nothing() {
        let server = HookServer::start().unwrap();
        let response = post(
            server.port(),
            "not-the-token",
            r#"{"hook_event_name":"Stop"}"#,
        );
        assert!(response.starts_with("HTTP/1.1 401"), "{response}");
        assert!(server
            .events()
            .recv_timeout(Duration::from_millis(200))
            .is_err());
    }

    #[test]
    fn server_ignores_junk_bodies_and_wrong_routes() {
        let server = HookServer::start().unwrap();
        // Junk still gets a 204 — hook exit codes are claude's problem and a
        // non-zero one would spam its UI.
        let response = post(server.port(), server.token(), "not json");
        assert!(response.starts_with("HTTP/1.1 204"), "{response}");

        let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, server.port())).unwrap();
        write!(stream, "GET / HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
        stream.flush().unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 404"), "{response}");

        assert!(server
            .events()
            .recv_timeout(Duration::from_millis(200))
            .is_err());
    }

    #[test]
    fn shutdown_closes_the_event_stream_and_the_port() {
        let mut server = HookServer::start().unwrap();
        let port = server.port();
        server.shutdown();
        server.shutdown(); // idempotent
        drop(server);
        // The listener is gone with the accept thread; a connect either fails
        // or finds nothing serving.
        if let Ok(mut stream) = TcpStream::connect((Ipv4Addr::LOCALHOST, port)) {
            stream
                .set_read_timeout(Some(Duration::from_millis(200)))
                .unwrap();
            let mut response = String::new();
            let _ = stream.read_to_string(&mut response);
            assert!(response.is_empty(), "{response}");
        }
    }
}
