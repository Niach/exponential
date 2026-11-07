//! Local agent runs (#EDE-58): a Local desktop's sessions run on this
//! machine with no server. Three pieces, each the shared one:
//!
//! - **eagent-hub-core in-process** is the orchestration authority: every
//!   session transition goes through `reduce` (`packages/eagent/fixtures`),
//!   its effects land in the store (the posts into a session's conversation,
//!   the parent's reports, the wake-ups) and its state mirrors onto the
//!   `sessions` rows (status, busy, needs_input, ended). The executor's
//!   `sessions.send` writes the person's rows (local-parity.json); the
//!   runner tells the hub (`send.start` / `send.message`), then CLAIMS the
//!   queued ones for this device (`device.claim`) and starts the run the
//!   `brief` effect names.
//! - **eagent's `AcpAgent`** spawns the person's own `claude` / `codex`
//!   (#VAPP-264); its events become the session's agent messages (a turn's
//!   prose, a question card), its turn end a `device.reply` + `device.report`.
//! - **an in-process MCP hub** (`emcp::Server`, loopback + a per-run bearer)
//!   serves the noun subset over this store: `exponential_issues`,
//!   `exponential_comments`, `exponential_boards`, `exponential_statuses`,
//!   `exponential_labels`, `exponential_sessions_send`, `exponential_show`.
//!
//! No steer, no presence, no relay: the host drains [`LocalRuns::changes`]
//! to redraw.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use api::{CallKind, Transport};
use eagent::provider::{AcpAgent, Brief, EventSink, HubGrant, HubLevel};
use eagent::wire::{ActivityEvent, TurnState};
use eagent_hub_core::{reduce, Effect, HubEvent, HubState, Status};
use ecore::SessionId;
use emcp::server::{Caller, Server, ServerInfo, ToolProvider};
use serde_json::{json, Map, Value};

use crate::desktops::LocalDesktop;
use crate::time;

/// One run the runner controls (the ACP session, or a test double).
pub trait Run: Send {
    fn send(&self, text: &str);
    fn answer(&self, question_id: &str, keys: Vec<String>, text: Option<String>);
    fn stop(&self);
    fn is_done(&self) -> bool;
}

/// What starts a run: eagent's ACP agent, or a test double.
pub trait Runner: Send + Sync {
    fn id(&self) -> &str;
    fn start(&self, brief: Brief, grant: HubGrant, sink: Arc<dyn EventSink>) -> Result<Box<dyn Run>, String>;
}

/// The person's own CLI over ACP ([`AcpAgent`]).
pub struct AcpRunner {
    agent: AcpAgent,
}

impl AcpRunner {
    pub fn new(agent: eagent::agent::CodingAgent) -> Self {
        Self { agent: AcpAgent::new(agent) }
    }

    /// Is the CLI installed and signed in here?
    pub fn available(&self) -> bool {
        self.agent.local().check().ok && self.agent.local().signed_in() != Some(false)
    }
}

struct AcpRun(eagent::local::LocalRun);

impl Run for AcpRun {
    fn send(&self, text: &str) {
        self.0.send(text);
    }
    fn answer(&self, question_id: &str, keys: Vec<String>, text: Option<String>) {
        self.0.answer(question_id, keys, text);
    }
    fn stop(&self) {
        let _ = self.0.stop();
    }
    fn is_done(&self) -> bool {
        self.0.is_done()
    }
}

impl Runner for AcpRunner {
    fn id(&self) -> &str {
        self.agent.local().agent().id()
    }

    fn start(&self, brief: Brief, grant: HubGrant, sink: Arc<dyn EventSink>) -> Result<Box<dyn Run>, String> {
        let request = self.agent.request(brief, &grant, sink).map_err(|f| f.message)?;
        let run = self.agent.local().start(request).map_err(|e| e.to_string())?;
        Ok(Box::new(AcpRun(run)))
    }
}

/// A sink that tags every event with its session for the runner's loop.
struct Sink {
    session_id: String,
    tx: flume::Sender<(String, ActivityEvent)>,
}

impl EventSink for Sink {
    fn send(&self, event: ActivityEvent) {
        let _ = self.tx.send((self.session_id.clone(), event));
    }
}

/// What the host hears: something about a session changed in the store.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Changed {
    pub session_id: String,
}

struct Live {
    run: Box<dyn Run>,
    /// The turn's prose so far (the reply the hub posts at the turn's end).
    turn_text: String,
    /// The agent message row the turn's prose lands in.
    turn_row: Option<String>,
}

pub struct LocalRuns {
    desktop: Arc<LocalDesktop>,
    runner: Arc<dyn Runner>,
    cwd: PathBuf,
    server: Server,
    hub: Mutex<HubState>,
    /// The hub's message ids (`m<seq>`) → the store's rows.
    ids: Mutex<HashMap<String, String>>,
    live: Mutex<HashMap<String, Live>>,
    /// The store row a step echoes as a `post` effect into that same session
    /// (the executor's send, the turn's prose already written): mapped,
    /// never written twice. `(store id, session)`.
    echo: Mutex<Option<(String, String)>>,
    events: (flume::Sender<(String, ActivityEvent)>, flume::Receiver<(String, ActivityEvent)>),
    changes: (flume::Sender<Changed>, flume::Receiver<Changed>),
    seq: AtomicU64,
}

/// The `sessions` row columns the hub's state mirrors onto.
fn mirror_columns(session: &eagent_hub_core::HubSession) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("status".into(), json!(session.status));
    m.insert("busy".into(), json!(session.busy));
    m.insert("needs_input".into(), json!(session.needs_input));
    m.insert("blocked".into(), session.blocked.clone().map(|b| serde_json::to_value(b).unwrap_or(Value::Null)).unwrap_or(Value::Null));
    m.insert("started_at".into(), session.started_at.map(time::format_ms).map(Value::from).unwrap_or(Value::Null));
    m.insert("ended_at".into(), session.ended_at.map(time::format_ms).map(Value::from).unwrap_or(Value::Null));
    m.insert("last_turn_at".into(), session.last_turn_at.map(time::format_ms).map(Value::from).unwrap_or(Value::Null));
    m
}

impl LocalRuns {
    /// Starts the runner for `desktop`: the MCP hub, the hub state from the
    /// store's live sessions, the worker thread.
    pub fn start(desktop: Arc<LocalDesktop>, runner: Arc<dyn Runner>, cwd: PathBuf) -> Result<Arc<Self>, String> {
        let tools = Arc::new(LocalTools { desktop: desktop.clone() });
        let info = ServerInfo { name: "exponential".into(), version: env!("CARGO_PKG_VERSION").into(), instructions: Some("The Local desktop: its issues, comments, boards, statuses and labels; sessions_send and show.".into()) };
        let server = Server::serve(info, tools)?;
        let runs = Arc::new(Self {
            desktop,
            runner,
            cwd,
            server,
            hub: Mutex::new(HubState::default()),
            ids: Mutex::new(HashMap::new()),
            live: Mutex::new(HashMap::new()),
            echo: Mutex::new(None),
            events: flume::unbounded(),
            changes: flume::unbounded(),
            seq: AtomicU64::new(1),
        });
        runs.hydrate()?;
        let deltas = runs.desktop.store.subscribe();
        let worker = runs.clone();
        std::thread::Builder::new()
            .name("elocal-runs".into())
            .spawn(move || worker.work(deltas))
            .map_err(|e| e.to_string())?;
        runs.reconcile();
        Ok(runs)
    }

    /// The host's change stream (one per session that changed).
    pub fn changes(&self) -> flume::Receiver<Changed> {
        self.changes.1.clone()
    }

    /// The hub's endpoint (`http://127.0.0.1:<port>/mcp`).
    pub fn hub_url(&self) -> &str {
        self.server.url()
    }

    pub fn is_running(&self, session_id: &str) -> bool {
        self.live.lock().expect("live").contains_key(session_id)
    }

    fn work(self: Arc<Self>, deltas: flume::Receiver<crate::store::LocalDelta>) {
        loop {
            let mut touched = false;
            while let Ok(delta) = deltas.try_recv() {
                if matches!(delta.table, "sessions" | "messages") {
                    touched = true;
                }
            }
            while let Ok((session_id, event)) = self.events.1.try_recv() {
                self.on_event(&session_id, event);
            }
            if touched {
                self.reconcile();
            }
            self.sweep_exits();
            match deltas.recv_timeout(Duration::from_millis(500)) {
                Ok(delta) => {
                    if matches!(delta.table, "sessions" | "messages") {
                        self.reconcile();
                    }
                }
                Err(flume::RecvTimeoutError::Disconnected) => return,
                Err(flume::RecvTimeoutError::Timeout) => {}
            }
        }
    }

    fn now() -> i64 {
        jiff::Timestamp::now().as_millisecond()
    }

    fn envelope(&self, kind: &str, data: Value) -> HubEvent {
        let n = self.seq.fetch_add(1, Ordering::Relaxed);
        HubEvent::new(&format!("e{n}"), kind, Self::now(), "elocal", None, data)
    }

    /// The hub's state from the store's live sessions (and the device).
    fn hydrate(&self) -> Result<(), String> {
        let device_id = self.desktop.executor.device_id();
        let sessions: Vec<Value> = self
            .desktop
            .store
            .rows("sessions")?
            .into_iter()
            .filter(|s| s.get("status").and_then(Value::as_str) != Some("ended"))
            .map(|s| {
                json!({
                    "id": s["id"], "parentId": s.get("parent_id").cloned().unwrap_or(Value::Null), "rootId": s.get("root_id").cloned().unwrap_or(s["id"].clone()),
                    "depth": s.get("depth").and_then(Value::as_i64).unwrap_or(0), "teamId": s.get("team_id").cloned().unwrap_or(Value::Null),
                    "userId": s.get("user_id").cloned().unwrap_or(Value::Null), "deviceId": s.get("device_id").cloned().unwrap_or(json!(device_id)),
                    "status": s.get("status").cloned().unwrap_or(json!("queued")), "busy": s.get("busy").and_then(Value::as_bool).unwrap_or(false),
                    "needsInput": s.get("needs_input").and_then(Value::as_bool).unwrap_or(false), "shared": false,
                    "agentId": s.get("agent_id").cloned().unwrap_or(Value::Null),
                    "createdAt": s.get("created_at").and_then(Value::as_str).and_then(domain::instant::parse_instant).unwrap_or(0),
                })
            })
            .collect();
        let given = json!({"sessions": sessions, "devices": [{"id": device_id, "lastSeenAt": Self::now()}]});
        let state = eagent_hub_core::hydrate(&given)?;
        *self.hub.lock().expect("hub") = state;
        Ok(())
    }

    /// One event through the hub, its effects into the store, its sessions mirrored.
    fn step(&self, kind: &str, data: Value) -> Option<eagent_hub_core::Outcome> {
        let event = self.envelope(kind, data);
        let outcome = {
            let state = self.hub.lock().expect("hub");
            reduce(&state, &event)
        };
        if let Some(failure) = &outcome.failure {
            log::info!("hub {kind}: {} ({})", failure.message, failure.code);
            return Some(outcome);
        }
        *self.hub.lock().expect("hub") = outcome.state.clone();
        for effect in &outcome.effects {
            self.apply(effect, kind);
        }
        self.mirror();
        Some(outcome)
    }

    /// The hub's sessions onto the store's rows (what changed).
    fn mirror(&self) {
        let sessions: Vec<eagent_hub_core::HubSession> = self.hub.lock().expect("hub").sessions.values().cloned().collect();
        for session in sessions {
            let Ok(Some(mut row)) = self.desktop.store.row("sessions", &session.id) else { continue };
            let columns = mirror_columns(&session);
            if columns.iter().all(|(k, v)| row.get(k) == Some(v)) {
                continue;
            }
            for (k, v) in columns {
                row.insert(k, v);
            }
            row.insert("updated_at".into(), json!(time::now()));
            if self.desktop.store.write("sessions", row).is_ok() {
                let _ = self.changes.0.send(Changed { session_id: session.id.clone() });
            }
        }
    }

    fn conversation_of(&self, session_id: &str) -> Option<String> {
        let found = self.desktop.store.rows_where("conversations", "session_id", session_id).ok()?.into_iter().next();
        if let Some(c) = found {
            return c.get("id").and_then(Value::as_str).map(str::to_string);
        }
        let team_id = self.desktop.executor.team_id().to_string();
        let id = uuid::Uuid::new_v4().to_string();
        let now = time::now();
        let row = json!({"id": id, "team_id": team_id, "kind": "session", "session_id": session_id, "title": null, "digest": null, "created_at": now, "updated_at": now});
        self.desktop.store.write("conversations", row.as_object().cloned()?).ok()?;
        Some(id)
    }

    /// A message row into `session`'s conversation; its store id.
    fn post(&self, session: &str, author_kind: &str, author_session_id: Option<&str>, body: &str, card: Option<Value>) -> Option<String> {
        let conversation = self.conversation_of(session)?;
        let id = uuid::Uuid::new_v4().to_string();
        let now = time::now();
        let author_id = (author_kind == "user").then(|| self.desktop.executor.identity().user_id);
        let row = json!({
            "id": id, "conversation_id": conversation, "root_id": session, "team_id": self.desktop.executor.team_id(),
            "author_kind": author_kind, "author_id": author_id, "author_session_id": author_session_id, "body": body, "card": card,
            "attachment_ids": [], "issue_brief": null, "answered_at": null, "consumed_at": null, "created_at": now, "updated_at": now,
        });
        self.desktop.store.write("messages", row.as_object().cloned()?).ok()?;
        let _ = self.changes.0.send(Changed { session_id: session.to_string() });
        Some(id)
    }

    fn apply(&self, effect: &Effect, kind: &str) {
        match effect {
            Effect::Post { message_id, to, author_kind, author_session_id, body, card, .. } => {
                if self.ids.lock().expect("ids").contains_key(message_id) {
                    return;
                }
                {
                    // The echo of a row already in the store: mapped, not written.
                    let mut echo = self.echo.lock().expect("echo");
                    if echo.as_ref().is_some_and(|(_, session)| session == to) {
                        let (store_id, _) = echo.take().expect("echo");
                        self.ids.lock().expect("ids").insert(message_id.clone(), store_id);
                        return;
                    }
                }
                let author = serde_json::to_value(author_kind).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_else(|| "system".into());
                let card = card.as_ref().and_then(|c| serde_json::to_value(c).ok());
                if let Some(id) = self.post(to, &author, author_session_id.as_deref(), body, card) {
                    self.ids.lock().expect("ids").insert(message_id.clone(), id);
                }
            }
            Effect::Edit { message_id, body, .. } => {
                let Some(id) = self.ids.lock().expect("ids").get(message_id).cloned() else { return };
                if let Ok(Some(mut row)) = self.desktop.store.row("messages", &id) {
                    row.insert("body".into(), json!(body));
                    row.insert("updated_at".into(), json!(time::now()));
                    let _ = self.desktop.store.write("messages", row);
                }
            }
            Effect::Brief { session_id, message_id, .. } => {
                let text = message_id
                    .as_ref()
                    .and_then(|m| self.ids.lock().expect("ids").get(m).cloned())
                    .and_then(|id| self.desktop.store.row("messages", &id).ok().flatten())
                    .and_then(|m| m.get("body").and_then(Value::as_str).map(str::to_string))
                    .unwrap_or_default();
                self.launch(session_id, &text);
            }
            Effect::Nudge { .. } => {}
            Effect::Kill { session_id } => {
                if let Some(live) = self.live.lock().expect("live").remove(session_id) {
                    live.run.stop();
                }
            }
        }
        log::debug!("hub {kind}: {effect:?}");
    }

    /// Starts the CLI for `session_id` with `text` as its first prompt.
    fn launch(&self, session_id: &str, text: &str) {
        let Ok(sid) = SessionId::new(session_id) else { return };
        let token = self.server.grant(&sid, "local");
        let endpoint = format!("{}", self.server.url());
        let identity = self.desktop.executor.identity();
        let grant = HubGrant::new(endpoint, token).server("exponential", HubLevel::Write).acting_for(Some(self.desktop.executor.team_id().to_string()), Some(identity.user_id));
        let brief = Brief::new(sid, text).cwd(self.cwd.clone()).system_append("You work on a Local desktop of Exponential Desktop: there is no server; the exponential tools reach this desktop's own store.");
        let sink: Arc<dyn EventSink> = Arc::new(Sink { session_id: session_id.to_string(), tx: self.events.0.clone() });
        match self.runner.start(brief, grant, sink) {
            Ok(run) => {
                self.live.lock().expect("live").insert(session_id.to_string(), Live { run, turn_text: String::new(), turn_row: None });
            }
            Err(e) => {
                log::warn!("run {session_id}: {e}");
                self.step("device.report", json!({"sessionId": session_id, "deviceId": self.desktop.executor.device_id(), "status": "idle", "busy": false, "blocked": {"kind": "start_failed"}}));
                let _ = self.post(session_id, "system", None, &format!("The agent could not start: {e}"), None);
            }
        }
    }

    /// The store's sessions and messages against the hub: new sends go in,
    /// queued sessions of this device get claimed, ended rows stop their run.
    pub fn reconcile(&self) {
        let device_id = self.desktop.executor.device_id();
        let identity = self.desktop.executor.identity();
        let Ok(sessions) = self.desktop.store.rows("sessions") else { return };
        let Ok(messages) = self.desktop.store.rows("messages") else { return };
        let known: Vec<String> = self.ids.lock().expect("ids").values().cloned().collect();
        let mut rows = sessions;
        rows.sort_by(|a, b| a.get("depth").and_then(Value::as_i64).unwrap_or(0).cmp(&b.get("depth").and_then(Value::as_i64).unwrap_or(0)).then_with(|| a.get("created_at").and_then(Value::as_str).cmp(&b.get("created_at").and_then(Value::as_str))));
        for row in &rows {
            let Some(id) = row.get("id").and_then(Value::as_str) else { continue };
            let status = row.get("status").and_then(Value::as_str).unwrap_or("queued");
            let in_hub = self.hub.lock().expect("hub").sessions.contains_key(id);
            if !in_hub && status == "ended" {
                continue;
            }
            // The person's messages the hub has not heard: the first one starts
            // the session for the hub, the others are sends into it.
            let mut mine: Vec<&Map<String, Value>> = messages.iter().filter(|m| m.get("root_id").and_then(Value::as_str) == Some(id) && m.get("author_kind").and_then(Value::as_str) == Some("user") && !known.contains(&m.get("id").and_then(Value::as_str).unwrap_or("").to_string())).collect();
            mine.sort_by(|a, b| a.get("created_at").and_then(Value::as_str).cmp(&b.get("created_at").and_then(Value::as_str)));
            let mut first = !in_hub;
            for message in mine {
                let (message_id, text) = (message.get("id").and_then(Value::as_str).unwrap_or("").to_string(), message.get("body").and_then(Value::as_str).unwrap_or("").to_string());
                *self.echo.lock().expect("echo") = Some((message_id.clone(), id.to_string()));
                let outcome = if first {
                    first = false;
                    let mut data = json!({"id": id, "userId": identity.user_id, "deviceId": row.get("device_id").cloned().unwrap_or(json!(device_id)), "text": text});
                    if let Some(parent) = row.get("parent_id").and_then(Value::as_str) {
                        data["parentId"] = json!(parent);
                        data["callerId"] = json!(parent);
                    }
                    self.step("send.start", data)
                } else {
                    self.step("send.message", json!({"sessionId": id, "userId": identity.user_id, "text": text}))
                };
                *self.echo.lock().expect("echo") = None;
                if let Some(hub_id) = outcome.and_then(|o| o.result).and_then(|r| match r.message_id { eagent_hub_core::Field::Value(v) => Some(v), _ => None }) {
                    self.ids.lock().expect("ids").insert(hub_id, message_id.clone());
                } else {
                    // Refused or silent: remembered as handled all the same.
                    self.ids.lock().expect("ids").insert(format!("store:{message_id}"), message_id.clone());
                }
                // A message into a live run goes to the CLI as the next prompt.
                if let Some(live) = self.live.lock().expect("live").get(id) {
                    if !text.trim().is_empty() {
                        live.run.send(&text);
                    }
                }
            }
            if first {
                // A session with no person message yet (a child the hub made itself).
                continue;
            }
            // The person ended it in the store: the hub and the run follow.
            if status == "ended" && self.hub.lock().expect("hub").sessions.get(id).is_some_and(|s| s.status != Status::Ended) {
                self.step("session.end", json!({"sessionId": id}));
                if let Some(live) = self.live.lock().expect("live").remove(id) {
                    live.run.stop();
                }
            }
        }
        // Queued on this device and not running here: claim it.
        let queued: Vec<String> = self.hub.lock().expect("hub").sessions.values().filter(|s| s.status == Status::Queued && s.device_id == device_id).map(|s| s.id.clone()).collect();
        for id in queued {
            if self.is_running(&id) {
                continue;
            }
            self.step("device.claim", json!({"sessionId": id, "deviceId": device_id}));
        }
    }

    fn on_event(&self, session_id: &str, event: ActivityEvent) {
        match event {
            ActivityEvent::Narration { text, message_id, subagent_id, .. } => {
                if subagent_id.is_some() {
                    return;
                }
                let mut live = self.live.lock().expect("live");
                let Some(run) = live.get_mut(session_id) else { return };
                run.turn_text.push_str(&text);
                let same_row = message_id.as_deref().is_some_and(|m| run.turn_row.as_deref().is_some_and(|r| r.ends_with(m))) || run.turn_row.is_some();
                if let (true, Some(row_id)) = (same_row, run.turn_row.as_ref().map(|r| r.split('|').next().unwrap_or("").to_string())) {
                    if let Ok(Some(mut row)) = self.desktop.store.row("messages", &row_id) {
                        row.insert("body".into(), json!(run.turn_text));
                        row.insert("updated_at".into(), json!(time::now()));
                        let _ = self.desktop.store.write("messages", row);
                        let _ = self.changes.0.send(Changed { session_id: session_id.to_string() });
                    }
                } else if let Some(id) = self.post(session_id, "agent", None, &text, None) {
                    run.turn_row = Some(format!("{id}|{}", message_id.unwrap_or_default()));
                }
            }
            ActivityEvent::Question { text, options, multi_select, id, .. } => {
                let labels: Vec<String> = options.iter().map(|o| o.label.clone()).collect();
                let card = json!({"kind": "question", "question": text, "options": labels, "multi": multi_select.unwrap_or(false), "questionId": id});
                self.step("device.reply", json!({"sessionId": session_id, "card": card}));
            }
            ActivityEvent::Turn { state: TurnState::Started, .. } => {
                let device_id = self.desktop.executor.device_id();
                self.step("device.report", json!({"sessionId": session_id, "deviceId": device_id, "busy": true}));
            }
            ActivityEvent::Turn { state: TurnState::Ended, .. } => {
                let (reply, row) = {
                    let mut live = self.live.lock().expect("live");
                    let Some(run) = live.get_mut(session_id) else { return };
                    let row = run.turn_row.take().map(|r| r.split('|').next().unwrap_or("").to_string());
                    (std::mem::take(&mut run.turn_text), row)
                };
                if !reply.trim().is_empty() {
                    // The reply row exists (the turn's prose): the hub posts the
                    // bus copies for the listeners; the session's own echo maps.
                    *self.echo.lock().expect("echo") = row.map(|r| (r, session_id.to_string()));
                    self.step("device.reply", json!({"sessionId": session_id, "body": reply}));
                    *self.echo.lock().expect("echo") = None;
                }
                let device_id = self.desktop.executor.device_id();
                self.step("device.report", json!({"sessionId": session_id, "deviceId": device_id, "status": "idle", "busy": false}));
            }
            _ => {}
        }
    }

    /// Runs whose process went away: ended, reported.
    fn sweep_exits(&self) {
        let gone: Vec<String> = self.live.lock().expect("live").iter().filter(|(_, l)| l.run.is_done()).map(|(id, _)| id.clone()).collect();
        for id in gone {
            self.live.lock().expect("live").remove(&id);
            let device_id = self.desktop.executor.device_id();
            self.step("device.report", json!({"sessionId": id, "deviceId": device_id, "status": "ended"}));
        }
    }

    /// Stops every run (the host is going away).
    pub fn stop_all(&self) {
        for (_, live) in self.live.lock().expect("live").drain() {
            live.run.stop();
        }
        self.server.revoke_all();
    }
}

/// The noun subset over the desktop's store, as the server's tools are named.
struct LocalTools {
    desktop: Arc<LocalDesktop>,
}

fn tool(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({"name": name, "description": description, "inputSchema": {"type": "object", "properties": properties, "required": required}})
}

fn content(value: Value) -> Value {
    json!({"content": [{"type": "text", "text": value.to_string()}], "structuredContent": value})
}

fn refused(text: &str) -> Value {
    json!({"content": [{"type": "text", "text": text}], "isError": true})
}

impl LocalTools {
    fn call_executor(&self, caller: &Caller, path: &str, input: Value) -> Value {
        match self.desktop.executor.call(CallKind::Mutation, path, Some(&input.to_string()), Some(caller.session_id.as_str())) {
            Ok(text) => content(serde_json::from_str(&text).unwrap_or(Value::String(text))),
            Err(e) => refused(&e.user_message()),
        }
    }

    fn rows(&self, table: &str, pick: &[&str]) -> Value {
        let rows = self.desktop.store.rows(table).unwrap_or_default();
        Value::Array(rows.iter().map(|r| Value::Object(pick.iter().filter_map(|k| r.get(*k).map(|v| ((*k).to_string(), v.clone()))).collect())).collect())
    }
}

impl ToolProvider for LocalTools {
    fn tools(&self) -> Result<Vec<Value>, String> {
        Ok(vec![
            tool("exponential_issues", "This desktop's issues. op = list | get{id} | create{title, boardId?, description?, priority?, status?} | update{id, title?, description?, priority?, status?, assigneeId?}", json!({"op": {"type": "string"}, "id": {"type": "string"}, "title": {"type": "string"}, "boardId": {"type": "string"}, "description": {"type": "string"}, "priority": {"type": "string"}, "status": {"type": "string"}, "assigneeId": {"type": "string"}}), &["op"]),
            tool("exponential_comments", "Comments on an issue. op = list{issueId} | create{issueId, body}", json!({"op": {"type": "string"}, "issueId": {"type": "string"}, "body": {"type": "string"}}), &["op"]),
            tool("exponential_boards", "This desktop's boards. op = list", json!({"op": {"type": "string"}}), &["op"]),
            tool("exponential_statuses", "This desktop's statuses. op = list", json!({"op": {"type": "string"}}), &["op"]),
            tool("exponential_labels", "This desktop's labels. op = list", json!({"op": {"type": "string"}}), &["op"]),
            tool("exponential_sessions_send", "A message into a session of this desktop (sessionId), or a new one (text alone).", json!({"sessionId": {"type": "string"}, "text": {"type": "string"}}), &["text"]),
            tool("exponential_show", "A card in your own conversation: a result or status the person sees.", json!({"title": {"type": "string"}, "text": {"type": "string"}}), &["text"]),
        ])
    }

    fn call(&self, caller: &Caller, name: &str, arguments: Value) -> Value {
        let op = arguments.get("op").and_then(Value::as_str).unwrap_or("list");
        let mut input = arguments.as_object().cloned().unwrap_or_default();
        input.remove("op");
        match (name, op) {
            ("exponential_issues", "list") => content(self.rows("issues", &["id", "identifier", "title", "status", "priority", "assignee_id", "board_id"])),
            ("exponential_issues", "get") => {
                let id = arguments.get("id").and_then(Value::as_str).unwrap_or("");
                match self.desktop.executor.call(CallKind::Query, "issues.get", Some(&json!({"id": id}).to_string()), None) {
                    Ok(text) => content(serde_json::from_str(&text).unwrap_or(Value::String(text))),
                    Err(e) => refused(&e.user_message()),
                }
            }
            ("exponential_issues", "create") => {
                if input.get("boardId").and_then(Value::as_str).is_none() {
                    if let Some(board) = self.desktop.store.rows("boards").unwrap_or_default().into_iter().next() {
                        input.insert("boardId".into(), board.get("id").cloned().unwrap_or(Value::Null));
                    }
                }
                self.call_executor(caller, "issues.create", Value::Object(input))
            }
            ("exponential_issues", "update") => self.call_executor(caller, "issues.update", Value::Object(input)),
            ("exponential_comments", "list") => {
                let issue = arguments.get("issueId").and_then(Value::as_str).unwrap_or("");
                let rows = self.desktop.store.rows_where("comments", "issue_id", issue).unwrap_or_default();
                content(Value::Array(rows.iter().map(|r| json!({"id": r["id"], "body": r["body"], "authorId": r.get("author_id").cloned().unwrap_or(Value::Null), "createdAt": r.get("created_at").cloned().unwrap_or(Value::Null)})).collect()))
            }
            ("exponential_comments", "create") => self.call_executor(caller, "comments.create", Value::Object(input)),
            ("exponential_boards", _) => content(self.rows("boards", &["id", "name", "slug", "prefix"])),
            ("exponential_statuses", _) => content(self.rows("issue_statuses", &["id", "category", "name", "builtin_key"])),
            ("exponential_labels", _) => content(self.rows("labels", &["id", "name", "color"])),
            ("exponential_sessions_send", _) => {
                let mut send = Map::new();
                send.insert("text".into(), arguments.get("text").cloned().unwrap_or(Value::Null));
                match arguments.get("sessionId").and_then(Value::as_str) {
                    Some(session) => {
                        send.insert("sessionId".into(), json!(session));
                    }
                    None => {
                        send.insert("teamId".into(), json!(self.desktop.executor.team_id()));
                    }
                }
                self.call_executor(caller, "sessions.send", Value::Object(send))
            }
            ("exponential_show", _) => {
                let text = arguments.get("text").and_then(Value::as_str).unwrap_or("").to_string();
                let title = arguments.get("title").and_then(Value::as_str).map(str::to_string);
                let session = caller.session_id.as_str();
                let conversation = self.desktop.store.rows_where("conversations", "session_id", session).unwrap_or_default().into_iter().next();
                let Some(conversation) = conversation.and_then(|c| c.get("id").and_then(Value::as_str).map(str::to_string)) else { return refused("no conversation for this session") };
                let now = time::now();
                let row = json!({"id": uuid::Uuid::new_v4().to_string(), "conversation_id": conversation, "root_id": session, "team_id": self.desktop.executor.team_id(), "author_kind": "agent", "author_id": null, "author_session_id": session, "body": text, "card": {"kind": "show", "title": title}, "attachment_ids": [], "issue_brief": null, "answered_at": null, "consumed_at": null, "created_at": now, "updated_at": now});
                match self.desktop.store.write("messages", row.as_object().cloned().unwrap_or_default()) {
                    Ok(()) => content(json!({"ok": true})),
                    Err(e) => refused(&e),
                }
            }
            (other, op) => refused(&format!("Unknown tool or op: {other} {op}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    /// A run that answers every prompt with one turn: "Hello" + " there".
    struct FakeRun {
        sink: Arc<dyn EventSink>,
        done: Arc<AtomicBool>,
        prompts: Arc<Mutex<Vec<String>>>,
    }

    impl FakeRun {
        fn turn(&self, text: &str) {
            self.sink.send(ActivityEvent::Turn { state: TurnState::Started, started_at: None, tokens: None, at: None, writing: None });
            self.sink.send(ActivityEvent::Narration { text: format!("Hello {text}"), before_question_id: None, message_id: Some("a1".into()), subagent_id: None, at: None });
            self.sink.send(ActivityEvent::Narration { text: " there".into(), before_question_id: None, message_id: Some("a1".into()), subagent_id: None, at: None });
            self.sink.send(ActivityEvent::Turn { state: TurnState::Ended, started_at: None, tokens: None, at: None, writing: None });
        }
    }

    impl Run for FakeRun {
        fn send(&self, text: &str) {
            self.prompts.lock().unwrap().push(text.to_string());
            self.turn(text);
        }
        fn answer(&self, _question_id: &str, _keys: Vec<String>, _text: Option<String>) {}
        fn stop(&self) {
            self.done.store(true, Ordering::Relaxed);
        }
        fn is_done(&self) -> bool {
            self.done.load(Ordering::Relaxed)
        }
    }

    struct FakeRunner {
        prompts: Arc<Mutex<Vec<String>>>,
        grants: Arc<Mutex<Vec<HubGrant>>>,
    }

    impl Runner for FakeRunner {
        fn id(&self) -> &str {
            "fake"
        }
        fn start(&self, brief: Brief, grant: HubGrant, sink: Arc<dyn EventSink>) -> Result<Box<dyn Run>, String> {
            self.grants.lock().unwrap().push(grant);
            let run = FakeRun { sink, done: Arc::new(AtomicBool::new(false)), prompts: self.prompts.clone() };
            run.send(&brief.prompt);
            Ok(Box::new(run))
        }
    }

    fn wait_for(mut check: impl FnMut() -> bool) {
        for _ in 0..100 {
            if check() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("the condition never held");
    }

    #[test]
    fn a_sent_message_runs_the_agent_and_lands_its_reply() {
        let dir = tempfile::tempdir().unwrap();
        let desktops = crate::Desktops::open(dir.path()).unwrap();
        let desktop = Arc::new(desktops.open_desktop(desktops.current().unwrap()).unwrap());
        let prompts = Arc::new(Mutex::new(Vec::new()));
        let grants = Arc::new(Mutex::new(Vec::new()));
        let runner = Arc::new(FakeRunner { prompts: prompts.clone(), grants: grants.clone() });
        let runs = LocalRuns::start(desktop.clone(), runner, dir.path().to_path_buf()).unwrap();
        let changes = runs.changes();
        let client = desktop.client();

        let mut send = api::sessions::SendInput::default();
        send.team_id = Some(desktop.executor.team_id().to_string());
        send.text = Some("Fix the login".into());
        let out = api::sessions::send(&client, &send).unwrap();
        let session = out.session_id.clone();
        wait_for(|| desktop.store.row("sessions", &session).unwrap().unwrap()["status"] == json!("idle"));
        assert_eq!(prompts.lock().unwrap().as_slice(), ["Fix the login"]);
        let grant = grants.lock().unwrap()[0].clone();
        assert!(grant.endpoint.starts_with("http://127.0.0.1:"));
        assert_eq!(grant.servers.get("exponential"), Some(&HubLevel::Write));
        let messages = desktop.store.rows_where("messages", "root_id", &session).unwrap();
        let agent: Vec<&Map<String, Value>> = messages.iter().filter(|m| m["author_kind"] == json!("agent")).collect();
        assert_eq!(agent.len(), 1, "{messages:?}");
        assert_eq!(agent[0]["body"], json!("Hello Fix the login there"));
        let row = desktop.store.row("sessions", &session).unwrap().unwrap();
        assert_eq!(row["busy"], json!(false));
        assert!(row["started_at"].is_string());
        assert!(changes.try_recv().is_ok());

        // A follow-up goes to the live run as its next prompt.
        api::sessions::send(&client, &api::sessions::SendInput::message(&session, "And the logout", vec![])).unwrap();
        wait_for(|| prompts.lock().unwrap().len() == 2);
        wait_for(|| desktop.store.rows_where("messages", "root_id", &session).unwrap().iter().filter(|m| m["author_kind"] == json!("agent")).count() == 2);

        // Ending it in the store stops the run and the hub agrees.
        api::sessions::end(&client, &session).unwrap();
        wait_for(|| !runs.is_running(&session));
        assert_eq!(runs.hub.lock().unwrap().sessions[&session].status, Status::Ended);
        runs.stop_all();
    }

    #[test]
    fn the_tools_read_and_write_the_store() {
        let dir = tempfile::tempdir().unwrap();
        let desktops = crate::Desktops::open(dir.path()).unwrap();
        let desktop = Arc::new(desktops.open_desktop(desktops.current().unwrap()).unwrap());
        let tools = LocalTools { desktop: desktop.clone() };
        let names: Vec<String> = tools.tools().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_string()).collect();
        assert_eq!(names, ["exponential_issues", "exponential_comments", "exponential_boards", "exponential_statuses", "exponential_labels", "exponential_sessions_send", "exponential_show"]);
        let session = SessionId::new("11111111-1111-4111-8111-111111111111").unwrap();
        let caller = Caller { session_id: session, label: "test".into() };
        let created = tools.call(&caller, "exponential_issues", json!({"op": "create", "title": "From the agent"}));
        assert_eq!(created["isError"], Value::Null);
        let id = created["structuredContent"]["issue"]["id"].as_str().unwrap().to_string();
        assert_eq!(created["structuredContent"]["issue"]["identifier"], json!("GEN-1"));
        let listed = tools.call(&caller, "exponential_issues", json!({"op": "list"}));
        assert_eq!(listed["structuredContent"][0]["id"], json!(id));
        let comment = tools.call(&caller, "exponential_comments", json!({"op": "create", "issueId": id, "body": "noted"}));
        assert_eq!(comment["structuredContent"]["comment"]["body"], json!("noted"));
        assert_eq!(tools.call(&caller, "exponential_statuses", json!({"op": "list"}))["structuredContent"].as_array().unwrap().len(), 6);
        let refused = tools.call(&caller, "exponential_nope", json!({}));
        assert_eq!(refused["isError"], json!(true));
    }
}

