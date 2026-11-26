//! #NEW-148 (@exp/emcp hub-client.json): asks and trace steps that come from
//! OUTSIDE the agent's own connection, into a LIVE run's session. The
//! device's MCP hub is the client of the team's MCP servers; when one of them
//! asks the person something (an MRTR `elicitation/create`, a task's
//! `input_required`) the hub raises it here as ONE ACP elicitation of the
//! run, so it reaches the same card an agent's own ask does, and each
//! third-party task it drives is ONE trace step (a tool-call card) the hub
//! moves along.
//!
//! The registry holds a WEAK handle per live run, keyed by the Exponential
//! session id ([`SessionCtx::session_id`]); a run registers when its
//! connection starts and leaves when it ends. Nothing here keeps a run
//! alive, and a session with no live run answers `None` (the hub cancels).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock, Weak};

use agent_client_protocol::v1::schema::{
    ContentBlock, CreateElicitationRequest, ElicitationFormMode, ElicitationMode,
    ElicitationSchema, ElicitationSessionScope, ElicitationUrlMode, SessionId,
    SessionNotification, SessionUpdate, TextContent, ToolCall, ToolCallContent, ToolCallId,
    ToolCallStatus, ToolCallUpdate, ToolCallUpdateFields, ToolKind,
};
use serde_json::{json, Value};

use super::host::{AskOutcome, SessionCtx};
use super::mapper::MapOut;

/// The `_meta` key of an ACP elicitation naming the THIRD-PARTY MCP server
/// that asked (the name Exponential mounted it under): its presence makes the
/// ask the elicitation card (#NEW-146, elicitation-card.json `serverMetaKey`)
/// instead of the question stepper. The same key the codex adapter stamps.
pub const MCP_SERVER_META: &str = "exponential.at/mcpServer";

struct Live {
    ctx: Weak<SessionCtx>,
    /// The trace ids already opened (a second step is an update).
    traces: HashSet<String>,
}

fn registry() -> &'static Mutex<HashMap<String, Live>> {
    static LIVE: OnceLock<Mutex<HashMap<String, Live>>> = OnceLock::new();
    LIVE.get_or_init(Mutex::default)
}

/// The run's connection started: asks for its session reach it.
pub(crate) fn register(ctx: &Arc<SessionCtx>) {
    if let Ok(mut live) = registry().lock() {
        live.insert(ctx.session_id.clone(), Live { ctx: Arc::downgrade(ctx), traces: HashSet::new() });
    }
}

/// The run's connection ended; a newer run of the same session keeps its own
/// entry.
pub(crate) fn unregister(ctx: &Arc<SessionCtx>) {
    if let Ok(mut live) = registry().lock() {
        let same = live
            .get(&ctx.session_id)
            .is_some_and(|entry| entry.ctx.upgrade().is_none_or(|held| Arc::ptr_eq(&held, ctx)));
        if same {
            live.remove(&ctx.session_id);
        }
    }
}

fn live_ctx(session_id: &str) -> Option<Arc<SessionCtx>> {
    registry().lock().ok()?.get(session_id)?.ctx.upgrade()
}

/// Whether `session_id` has a live run on this device.
pub fn is_live(session_id: &str) -> bool {
    live_ctx(session_id).is_some()
}

fn acp_session(ctx: &SessionCtx) -> SessionId {
    SessionId::new(ctx.ids().acp.unwrap_or_else(|| ctx.session_id.clone()))
}

/// An MCP `elicitation/create` request's params (`mode`, `message`, and
/// `requestedSchema` or `url` + `elicitationId`) as THE ACP elicitation of
/// the run, `server` named in its `_meta`. `None` = nothing the card can
/// render honestly (an unknown mode, a schema ACP cannot read, a url mode
/// without its url): the caller declines.
pub fn mcp_elicitation_request(session: SessionId, server: &str, ask_id: &str, params: &Value) -> Option<CreateElicitationRequest> {
    let text = |key: &str| params.get(key).and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty());
    let message = params.get("message").and_then(Value::as_str).unwrap_or_default().to_string();
    let scope = ElicitationSessionScope::new(session);
    // MCP: a request without `mode` is a form.
    let mode: ElicitationMode = match text("mode").unwrap_or("form") {
        "form" => {
            let schema: ElicitationSchema = serde_json::from_value(params.get("requestedSchema")?.clone()).ok()?;
            ElicitationFormMode::new(scope, schema).into()
        }
        "url" => ElicitationUrlMode::new(scope, text("elicitationId")?.to_string(), text("url")?.to_string()).into(),
        _ => return None,
    };
    let mut meta = serde_json::Map::new();
    meta.insert(MCP_SERVER_META.to_string(), Value::String(server.to_string()));
    meta.insert("askId".to_string(), Value::String(ask_id.to_string()));
    Some(CreateElicitationRequest::new(mode, message).meta(meta))
}

/// A host callback run on a session's Stop (the person's, a client's): the
/// device hub cancels the session's third-party tasks (`tasks/cancel`).
pub type StopHook = Arc<dyn Fn(&str) + Send + Sync>;

fn stop_hook() -> &'static Mutex<Option<StopHook>> {
    static HOOK: OnceLock<Mutex<Option<StopHook>>> = OnceLock::new();
    HOOK.get_or_init(Mutex::default)
}

/// Install the process's [`StopHook`] (the last one wins).
pub fn set_stop_hook(hook: StopHook) {
    if let Ok(mut slot) = stop_hook().lock() {
        *slot = Some(hook);
    }
}

/// The session was stopped: run the hook OFF the caller's thread (it does
/// network I/O).
pub(crate) fn stopped(session_id: &str) {
    let Some(hook) = stop_hook().lock().ok().and_then(|slot| slot.clone()) else { return };
    let session_id = session_id.to_string();
    let _ = std::thread::Builder::new().name("mcp-task-stop".into()).spawn(move || hook(&session_id));
}

/// What [`elicit`] got.
#[derive(Clone, Debug, PartialEq)]
pub enum Elicited {
    /// MCP's `ElicitResult` (accept + content, decline, cancel). A Stop, a
    /// kill or the run ending answer `cancel`.
    Answered(Value),
    /// No live run of the session on this device (the caller cancels).
    NoSession,
    /// Nobody answered within the wait: the card was withdrawn.
    TimedOut,
}

/// Ask the person in the live run of `session_id` what the MCP server
/// `server` wants to know (`params` = the `elicitation/create` params) and
/// WAIT for the answer, at most `wait` (then the card is withdrawn like a
/// cancel's). Blocking: never call it on the run's own runtime.
pub fn elicit(session_id: &str, server: &str, params: &Value, wait: std::time::Duration) -> Elicited {
    let Some(ctx) = live_ctx(session_id) else { return Elicited::NoSession };
    let ask_id = format!("mcp-{}", uuid::Uuid::new_v4().simple());
    let Some(request) = mcp_elicitation_request(acp_session(&ctx), server, &ask_id, params) else {
        log::debug!("engine: {server}'s elicitation declined (unrenderable)");
        return Elicited::Answered(json!({ "action": "decline" }));
    };
    let mut out = MapOut::default();
    let key = ctx.with_mapper(|mapper| mapper.on_elicitation(&ask_id, &request, &mut out));
    ctx.dispatch(out);
    let answers = ctx.asks.park(&key);
    // The context is not held while waiting: a run that ends drops its
    // parked senders, which answers `cancel` below.
    drop(ctx);
    match answers.recv_timeout(wait) {
        Ok(AskOutcome::Elicitation(fields)) => Elicited::Answered(match fields {
            Value::Object(map) if !map.is_empty() => json!({ "action": "accept", "content": map }),
            _ => json!({ "action": "accept" }),
        }),
        Err(flume::RecvTimeoutError::Timeout) => {
            if let Some(ctx) = live_ctx(session_id) {
                let mut out = MapOut::default();
                let withdrawn = ctx.with_mapper(|mapper| mapper.withdraw_ask(&ask_id, &mut out));
                ctx.dispatch(out);
                if let Some(key) = withdrawn {
                    ctx.asks.resolve(&key, AskOutcome::Cancelled);
                }
            }
            Elicited::TimedOut
        }
        _ => Elicited::Answered(json!({ "action": "cancel" })),
    }
}

/// A trace step's state (hub-client.json `tasks.traceStatus`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceStatus {
    Running,
    Pending,
    Done,
    Error,
    Skipped,
}

impl TraceStatus {
    /// `running|pending|done|error|skipped`; anything else is running.
    pub fn parse(status: &str) -> Self {
        match status {
            "pending" => Self::Pending,
            "done" => Self::Done,
            "error" => Self::Error,
            "skipped" => Self::Skipped,
            _ => Self::Running,
        }
    }

    fn acp(self) -> ToolCallStatus {
        match self {
            Self::Running => ToolCallStatus::InProgress,
            Self::Pending => ToolCallStatus::Pending,
            Self::Done => ToolCallStatus::Completed,
            Self::Error | Self::Skipped => ToolCallStatus::Failed,
        }
    }
}

/// Open or move the trace step `id` of the live run of `session_id`: the
/// first call opens a tool-call card titled `title`, every later one updates
/// its title and status; `text` (a terminal step's result) becomes its
/// content. Returns whether a live run took it.
pub fn trace(session_id: &str, id: &str, title: &str, status: TraceStatus, text: Option<&str>) -> bool {
    let (ctx, opened) = {
        let Ok(mut live) = registry().lock() else { return false };
        let Some(entry) = live.get_mut(session_id) else { return false };
        let Some(ctx) = entry.ctx.upgrade() else { return false };
        (ctx, !entry.traces.insert(id.to_string()))
    };
    let content: Vec<ToolCallContent> = text
        .filter(|text| !text.trim().is_empty())
        .map(|text| vec![ToolCallContent::from(ContentBlock::Text(TextContent::new(text.to_string())))])
        .unwrap_or_default();
    let update = if opened {
        let mut fields = ToolCallUpdateFields::new().title(title.to_string()).status(status.acp());
        if !content.is_empty() {
            fields = fields.content(content);
        }
        SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(ToolCallId::new(id.to_string()), fields))
    } else {
        SessionUpdate::ToolCall(
            ToolCall::new(ToolCallId::new(id.to_string()), title.to_string())
                .kind(ToolKind::Other)
                .status(status.acp())
                .content(content),
        )
    };
    let notification = SessionNotification::new(acp_session(&ctx), update);
    let mut out = MapOut::default();
    ctx.with_mapper(|mapper| mapper.on_update(&notification, &mut out));
    ctx.dispatch(out);
    if matches!(status, TraceStatus::Done | TraceStatus::Error | TraceStatus::Skipped) {
        if let Ok(mut live) = registry().lock() {
            if let Some(entry) = live.get_mut(session_id) {
                entry.traces.remove(id);
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_mcp_elicitation_maps_to_the_acp_one() {
        use agent_client_protocol::v1::schema::ElicitationPropertySchema;
        let form = json!({
            "mode": "form", "message": "Pick a team",
            "requestedSchema": {
                "type": "object",
                "properties": { "team": { "type": "string", "enum": ["web", "ios"] } },
                "required": ["team"]
            }
        });
        let request = mcp_elicitation_request(SessionId::new("s1"), "linear", "mcp-1", &form).expect("form");
        assert_eq!(request.message, "Pick a team");
        let meta = request.meta.as_ref().unwrap();
        assert_eq!(meta[MCP_SERVER_META], json!("linear"));
        assert_eq!(meta["askId"], json!("mcp-1"));
        let ElicitationMode::Form(mode) = &request.mode else { panic!("{:?}", request.mode) };
        assert!(matches!(mode.requested_schema.properties.get("team"), Some(ElicitationPropertySchema::String(_))));

        // MCP's default mode is form.
        let bare = json!({ "message": "m", "requestedSchema": { "type": "object", "properties": {} } });
        assert!(matches!(mcp_elicitation_request(SessionId::new("s1"), "x", "a", &bare).unwrap().mode, ElicitationMode::Form(_)));

        let url = json!({ "mode": "url", "message": "Sign in", "url": "https://example.com/a", "elicitationId": "e-1" });
        let request = mcp_elicitation_request(SessionId::new("s1"), "github", "mcp-2", &url).expect("url");
        let ElicitationMode::Url(mode) = &request.mode else { panic!("{:?}", request.mode) };
        assert_eq!(mode.url, "https://example.com/a");

        assert!(mcp_elicitation_request(SessionId::new("s1"), "x", "a", &json!({ "mode": "url", "message": "m" })).is_none());
        assert!(mcp_elicitation_request(SessionId::new("s1"), "x", "a", &json!({ "mode": "sampling" })).is_none());
    }

    #[test]
    fn no_live_run_answers_none() {
        assert_eq!(elicit("nobody-here", "x", &json!({}), std::time::Duration::from_millis(1)), Elicited::NoSession);
        assert!(!trace("nobody-here", "t", "x", TraceStatus::Running, None));
        assert!(!is_live("nobody-here"));
        assert_eq!(TraceStatus::parse("done"), TraceStatus::Done);
        assert_eq!(TraceStatus::parse("weird"), TraceStatus::Running);
    }
}
