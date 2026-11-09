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
//!
//! #EDE-65: ONE runner per desktop on the machine. The IDE and eDE may both
//! open a desktop; only the process holding the desktop's `runs.lock` (an
//! exclusive lock for the runner's life, so a crash frees it) claims and
//! runs sessions, the other stands by and takes over when the lock frees.
//! The holder also wakes on the other process's writes (the store file
//! moved), so a message sent from either app runs.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use api::{CallKind, Transport};
use eagent::provider::{AcpAgent, Brief, EventSink, HubGrant, HubLevel};
use eagent::wire::{ActivityEvent, TurnState};
use eagent_hub_core::{reduce, Effect, HubEvent, HubState, Status};
use ecore::SessionId;
use emcp::server::{Caller, Server, ServerInfo, ToolProvider};
use serde_json::{json, Map, Value};

use crate::desktops::LocalDesktop;
use crate::time;

/// The CLIs this machine has (signed in), in the default's order: Claude, Codex.
pub fn installed_runners() -> Vec<Arc<dyn Runner>> {
    [eagent::agent::CodingAgent::Claude, eagent::agent::CodingAgent::Codex].into_iter().map(AcpRunner::new).filter(AcpRunner::available).map(|r| Arc::new(r) as Arc<dyn Runner>).collect()
}

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

/// #EXP-1727: the working copy a session runs in: its root's linked issue's
/// board's linked folder, as a git worktree of its own under the desktop
/// (`worktrees/<root>`, branch `exp/<first 8 of the root>`), made once and
/// reused by every session of the tree. `None` = no linked folder (the run
/// takes the desktop's default directory).
pub fn worktree_for(desktop: &LocalDesktop, session_id: &str) -> Option<(PathBuf, crate::LocalRepository, String)> {
    let session = desktop.store.row("sessions", session_id).ok().flatten()?;
    let root = session.get("root_id").and_then(Value::as_str).filter(|r| !r.is_empty()).unwrap_or(session_id).to_string();
    let issue = [root.as_str(), session_id].iter().find_map(|id| {
        desktop.store.rows_where("session_issues", "session_id", id).ok()?.into_iter().find_map(|l| l.get("issue_id").and_then(Value::as_str).map(str::to_string))
    })?;
    let board = desktop.store.row("issues", &issue).ok().flatten()?.get("board_id").and_then(Value::as_str)?.to_string();
    let repo = desktop.executor.board_repository(&board)?;
    let short: String = root.chars().take(8).collect();
    let branch = format!("exp/{short}");
    let dir = desktop.dir.join("worktrees").join(&root);
    if !dir.exists() {
        let _ = std::fs::create_dir_all(dir.parent()?);
        let made = std::process::Command::new("git")
            .arg("-C")
            .arg(&repo.path)
            .args(["worktree", "add", "-b", &branch])
            .arg(&dir)
            .arg(&repo.default_branch)
            .output();
        match made {
            Ok(out) if out.status.success() => {}
            Ok(out) => {
                log::warn!("worktree for {root}: {}", String::from_utf8_lossy(&out.stderr).trim());
                return Some((repo.path.clone(), repo, branch));
            }
            Err(e) => {
                log::warn!("worktree for {root}: {e}");
                return Some((repo.path.clone(), repo, branch));
            }
        }
    }
    Some((dir, repo, branch))
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
    /// A turn started and has not ended: the engine seeds `ended` before the
    /// first prompt (EXP-848), which is no turn end to report.
    in_turn: bool,
}

pub struct LocalRuns {
    desktop: Arc<LocalDesktop>,
    /// The CLIs this machine can run, the default first (#EDE-65 J5: a
    /// session runs the one its `cli` names, else its agent's).
    runners: Vec<Arc<dyn Runner>>,
    cwd: PathBuf,
    /// What the brief says about the host's tools (`HostTools::note`).
    host_note: Option<String>,
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
    /// The desktop's `runs.lock` while this runner is THE runner (#EDE-65).
    lock: Mutex<Option<std::fs::File>>,
    /// After [`LocalRuns::stop_all`]: the lock is gone and never taken again.
    stopped: std::sync::atomic::AtomicBool,
    /// The store files' (modified, length) when last looked at.
    seen: Mutex<Option<Vec<(SystemTime, u64)>>>,
}

/// The desktop's run lock, `None` while another process holds it.
fn try_run_lock(dir: &std::path::Path) -> Option<std::fs::File> {
    let file = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(dir.join("runs.lock")).ok()?;
    match file.try_lock() {
        Ok(()) => Some(file),
        Err(std::fs::TryLockError::WouldBlock) => None,
        Err(std::fs::TryLockError::Error(e)) => {
            log::warn!("[elocal] runs.lock: {e}");
            None
        }
    }
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

/// Tools the HOST adds to every run's hub next to the noun subset
/// (#VAPP-273: Exponential Desktop's shell serves `shell_edit`, its
/// host-only `shell.edit`), with the line the brief tells the agent.
pub struct HostTools {
    pub provider: Arc<dyn ToolProvider>,
    pub note: String,
}

/// The noun subset and the host's tools behind one server: a name the host
/// lists goes to the host, every other one to the store.
struct Merged {
    local: Arc<LocalTools>,
    host: Arc<dyn ToolProvider>,
}

impl Merged {
    fn host_has(&self, name: &str) -> bool {
        self.host.tools().unwrap_or_default().iter().any(|t| t.get("name").and_then(Value::as_str) == Some(name))
    }
}

impl ToolProvider for Merged {
    fn tools(&self) -> Result<Vec<Value>, String> {
        let mut tools = self.local.tools()?;
        tools.extend(self.host.tools()?);
        Ok(tools)
    }

    fn call(&self, caller: &Caller, name: &str, arguments: Value) -> Value {
        if self.host_has(name) {
            self.host.call(caller, name, arguments)
        } else {
            self.local.call(caller, name, arguments)
        }
    }
}

impl LocalRuns {
    /// Starts the runner for `desktop`: the MCP hub, the hub state from the
    /// store's live sessions, the worker thread.
    /// The runner over the person's usable CLIs, `None` without one.
    pub fn start_first_cli(desktop: Arc<LocalDesktop>, cwd: PathBuf) -> Option<Result<Arc<Self>, String>> {
        let runners = installed_runners();
        (!runners.is_empty()).then(|| Self::start_with_runners(desktop, runners, cwd, None))
    }

    pub fn start(desktop: Arc<LocalDesktop>, runner: Arc<dyn Runner>, cwd: PathBuf) -> Result<Arc<Self>, String> {
        Self::start_with(desktop, runner, cwd, None)
    }

    /// [`Self::start`] with the host's tools on the same hub.
    pub fn start_with(desktop: Arc<LocalDesktop>, runner: Arc<dyn Runner>, cwd: PathBuf, host: Option<HostTools>) -> Result<Arc<Self>, String> {
        Self::start_with_runners(desktop, vec![runner], cwd, host)
    }

    /// [`Self::start_with`] over several CLIs (the first = the default).
    pub fn start_with_runners(desktop: Arc<LocalDesktop>, runners: Vec<Arc<dyn Runner>>, cwd: PathBuf, host: Option<HostTools>) -> Result<Arc<Self>, String> {
        if runners.is_empty() {
            return Err("no agent CLI to run".into());
        }
        let local = Arc::new(LocalTools { desktop: desktop.clone() });
        let (provider, note): (Arc<dyn ToolProvider>, Option<String>) = match host {
            Some(host) => (Arc::new(Merged { local, host: host.provider }), Some(host.note)),
            None => (local, None),
        };
        let info = ServerInfo { name: "exponential".into(), version: env!("CARGO_PKG_VERSION").into(), instructions: Some("The Local desktop: its issues, comments, boards, statuses and labels; sessions_send and show.".into()) };
        let server = Server::serve(info, provider)?;
        let runs = Arc::new(Self {
            desktop,
            runners,
            cwd,
            host_note: note,
            server,
            hub: Mutex::new(HubState::default()),
            ids: Mutex::new(HashMap::new()),
            live: Mutex::new(HashMap::new()),
            echo: Mutex::new(None),
            events: flume::unbounded(),
            changes: flume::unbounded(),
            seq: AtomicU64::new(1),
            lock: Mutex::new(None),
            stopped: std::sync::atomic::AtomicBool::new(false),
            seen: Mutex::new(None),
        });
        let deltas = runs.desktop.store.subscribe();
        if runs.take_over() {
            runs.reconcile();
        } else {
            log::info!("[elocal] desktop {}: another app runs its agents, standing by", runs.desktop.entry.id);
        }
        let worker = runs.clone();
        std::thread::Builder::new()
            .name("elocal-runs".into())
            .spawn(move || worker.work(deltas))
            .map_err(|e| e.to_string())?;
        Ok(runs)
    }

    /// Is this THE runner of the desktop (the holder of its `runs.lock`)?
    pub fn is_runner(&self) -> bool {
        self.lock.lock().expect("lock").is_some()
    }

    /// Takes the desktop's run lock when it is free: the hub starts over
    /// from the store (what the other process ran is in it).
    fn take_over(&self) -> bool {
        if self.is_runner() {
            return true;
        }
        if self.stopped.load(Ordering::Relaxed) {
            return false;
        }
        let Some(lock) = try_run_lock(&self.desktop.dir) else { return false };
        *self.lock.lock().expect("lock") = Some(lock);
        // The files as they are now: a later move is the other app's write.
        self.store_moved();
        if let Err(e) = self.hydrate() {
            log::warn!("[elocal] hydrate: {e}");
        }
        true
    }

    /// Did the store's files change since the last look (another process wrote)?
    fn store_moved(&self) -> bool {
        let stamp: Vec<(SystemTime, u64)> = ["local.sqlite", "local.sqlite-wal"]
            .iter()
            .map(|f| std::fs::metadata(self.desktop.dir.join(f)).map(|m| (m.modified().unwrap_or(SystemTime::UNIX_EPOCH), m.len())).unwrap_or((SystemTime::UNIX_EPOCH, 0)))
            .collect();
        let mut seen = self.seen.lock().expect("seen");
        let moved = seen.as_ref().is_some_and(|s| *s != stamp);
        *seen = Some(stamp);
        moved
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

    /// The worker: wakes on a store delta (a session or message changed:
    /// reconcile) or a run's event (into the hub), never on a clock; a quiet
    /// second sweeps the runs whose process went away.
    fn work(self: Arc<Self>, deltas: flume::Receiver<crate::store::LocalDelta>) {
        enum Woke {
            Delta(crate::store::LocalDelta),
            Event(String, ActivityEvent),
            Gone,
        }
        loop {
            let woke = flume::Selector::new()
                .recv(&deltas, |d| d.map(Woke::Delta).unwrap_or(Woke::Gone))
                .recv(&self.events.1, |e| e.map(|(s, ev)| Woke::Event(s, ev)).unwrap_or(Woke::Gone))
                .wait_timeout(Duration::from_secs(1));
            match woke {
                Ok(Woke::Delta(delta)) => {
                    // Coalesce what queued up behind it, then reconcile once.
                    let mut touched = matches!(delta.table, "sessions" | "messages");
                    while let Ok(more) = deltas.try_recv() {
                        touched |= matches!(more.table, "sessions" | "messages");
                    }
                    if touched && self.is_runner() {
                        self.reconcile();
                    }
                }
                Ok(Woke::Event(session_id, event)) => self.on_event(&session_id, event),
                Ok(Woke::Gone) => return,
                // A quiet second: take the lock when it freed, follow the other app's writes.
                Err(_) => {
                    if self.stopped.load(Ordering::Relaxed) {
                        return;
                    }
                    let was = self.is_runner();
                    if self.take_over() && (!was || self.store_moved()) {
                        self.reconcile();
                    }
                }
            }
            self.sweep_exits();
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
        // The person's messages a turn already answered (before the session's
        // last turn ended) are heard: a restart or a takeover never re-sends them.
        let answered: HashMap<String, String> = self
            .desktop
            .store
            .rows("sessions")?
            .into_iter()
            .filter_map(|s| {
                let at = s.get("last_turn_at").and_then(Value::as_str).filter(|_| s.get("busy").and_then(Value::as_bool) != Some(true))?.to_string();
                Some((s.get("id")?.as_str()?.to_string(), at))
            })
            .collect();
        let mut ids = self.ids.lock().expect("ids");
        for m in self.desktop.store.rows("messages")? {
            let (Some(id), Some(root)) = (m.get("id").and_then(Value::as_str), m.get("root_id").and_then(Value::as_str)) else { continue };
            let done = answered.get(root).zip(m.get("created_at").and_then(Value::as_str)).is_some_and(|(turn, at)| domain::instant::parse_instant(at) <= domain::instant::parse_instant(turn));
            if done && m.get("author_kind").and_then(Value::as_str) == Some("user") {
                ids.insert(format!("store:{id}"), id.to_string());
            }
        }
        drop(ids);
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
            let columns = mirror_columns(&session);
            let mut changed = false;
            let _ = self.desktop.store.modify("sessions", &session.id, |row| {
                // The person ended it in the store and the hub has not heard
                // yet (a turn still in flight): the end sticks; `reconcile`
                // tells the hub, which then mirrors `ended` itself.
                if row.get("status").and_then(Value::as_str) == Some("ended") && session.status != Status::Ended {
                    return None;
                }
                if columns.iter().all(|(k, v)| row.get(k) == Some(v)) {
                    return None;
                }
                let mut next = row.clone();
                for (k, v) in &columns {
                    next.insert(k.clone(), v.clone());
                }
                next.insert("updated_at".into(), json!(time::now()));
                changed = true;
                Some(next)
            });
            if changed {
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
            "id": id, "conversation_id": conversation, "session_id": session, "root_id": session, "team_id": self.desktop.executor.team_id(),
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
        let (cwd, worktree_note) = match worktree_for(&self.desktop, session_id) {
            Some((dir, repo, branch)) => (dir, Some(format!("You work in a git worktree of {} on the branch {branch}; commit there, nothing pushes.", repo.name))),
            None => (self.cwd.clone(), None),
        };
        let Ok(sid) = SessionId::new(session_id) else { return };
        let token = self.server.grant(&sid, "local");
        let endpoint = format!("{}", self.server.url());
        let identity = self.desktop.executor.identity();
        let grant = HubGrant::new(endpoint, token).server("exponential", HubLevel::Write).acting_for(Some(self.desktop.executor.team_id().to_string()), Some(identity.user_id));
        let mut system = String::from("You work on a Local desktop of Exponential Desktop: there is no server; the exponential tools reach this desktop's own store.");
        if let Some(note) = &self.host_note {
            system.push(' ');
            system.push_str(note);
        }
        if let Some(note) = &worktree_note {
            system.push(' ');
            system.push_str(note);
        }
        let brief = Brief::new(sid, &localize_attachments(&self.desktop, text)).cwd(cwd).system_append(&system);
        let sink: Arc<dyn EventSink> = Arc::new(Sink { session_id: session_id.to_string(), tx: self.events.0.clone() });
        let (runner, missing) = self.runner_for(session_id);
        if let Some(missing) = missing {
            let _ = self.post(session_id, "system", None, &format!("{missing} is not installed on this machine: {} runs this session.", runner.id()), None);
        }
        match runner.start(brief, grant, sink) {
            Ok(run) => {
                self.live.lock().expect("live").insert(session_id.to_string(), Live { run, turn_text: String::new(), turn_row: None, in_turn: false });
            }
            Err(e) => {
                log::warn!("run {session_id}: {e}");
                self.step("device.report", json!({"sessionId": session_id, "deviceId": self.desktop.executor.device_id(), "status": "idle", "busy": false, "blocked": {"kind": "start_failed"}}));
                let _ = self.post(session_id, "system", None, &format!("The agent could not start: {e}"), None);
            }
        }
    }

    /// The CLI a session runs: the one its row's `cli` names (the composer's
    /// pick), else its agent's, else the default; a named one this machine
    /// lacks comes back as the second value (the default runs instead).
    fn runner_for(&self, session_id: &str) -> (Arc<dyn Runner>, Option<String>) {
        let text = |row: &Map<String, Value>, key: &str| row.get(key).and_then(Value::as_str).filter(|v| !v.is_empty()).map(str::to_string);
        let session = self.desktop.store.row("sessions", session_id).ok().flatten();
        let wanted = session.as_ref().and_then(|s| {
            text(s, "cli").or_else(|| {
                let agent = text(s, "agent_id")?;
                // #EXP-1730: an agent's runner defaults live in `defaults`.
                let row = self.desktop.store.row("agents", &agent).ok().flatten()?;
                row.get("defaults")?.get("cli")?.as_str().filter(|v| !v.is_empty()).map(str::to_string)
            })
        });
        let default = self.runners[0].clone();
        match wanted {
            None => (default, None),
            Some(cli) => match self.runners.iter().find(|r| r.id() == cli) {
                Some(runner) => (runner.clone(), None),
                None => (default, Some(cli)),
            },
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
                        live.run.send(&localize_attachments(&self.desktop, &text));
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
                // A new assistant message (after a tool call) is a new block:
                // a blank line between blocks, as the hub's copies join them
                // (eagent-hub-core `copy`); a message's own chunks join as sent.
                let last = run.turn_row.as_deref().and_then(|r| r.split_once('|')).map(|(_, m)| m);
                if let (Some(new), Some(last)) = (message_id.as_deref(), last) {
                    if !last.is_empty() && new != last && !run.turn_text.trim_end().is_empty() {
                        let trimmed = run.turn_text.trim_end().len();
                        run.turn_text.truncate(trimmed);
                        run.turn_text.push_str("\n\n");
                    }
                }
                run.turn_text.push_str(&text);
                if let (Some(row), Some(new)) = (run.turn_row.as_mut(), message_id.as_deref()) {
                    let id = row.split('|').next().unwrap_or("").to_string();
                    *row = format!("{id}|{new}");
                }
                let same_row = run.turn_row.is_some();
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
                if let Some(run) = self.live.lock().expect("live").get_mut(session_id) {
                    run.in_turn = true;
                }
                let device_id = self.desktop.executor.device_id();
                self.step("device.report", json!({"sessionId": session_id, "deviceId": device_id, "busy": true}));
            }
            ActivityEvent::Turn { state: TurnState::Ended, .. } => {
                let (reply, row) = {
                    let mut live = self.live.lock().expect("live");
                    let Some(run) = live.get_mut(session_id) else { return };
                    if !std::mem::take(&mut run.in_turn) {
                        return;
                    }
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
    /// Stops every run and gives the desktop's run lock up for good (the
    /// other app, or the next runner, takes over).
    pub fn stop_all(&self) {
        self.stopped.store(true, Ordering::Relaxed);
        for (_, live) in self.live.lock().expect("live").drain() {
            live.run.stop();
        }
        self.server.revoke_all();
        *self.lock.lock().expect("lock") = None;
    }
}

/// The noun subset over the desktop's store, as the server's tools are named.
struct LocalTools {
    desktop: Arc<LocalDesktop>,
}

/// The folder a desktop's agents read their messages' attachments from.
pub const AGENT_FILES_DIR: &str = "agent-files";

/// #EDE-65 lane L: a CLI cannot GET `/api/attachments/{id}` (the server's
/// devices download them first, `.exp-steer-images`), so every attachment a
/// message names is copied out of the blob store to
/// `<desktop>/agent-files/<id>/<filename>` and that path replaces its URL.
/// An id the desktop holds no bytes for keeps its URL.
pub fn localize_attachments(desktop: &LocalDesktop, text: &str) -> String {
    const URL: &str = "/api/attachments/";
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(URL) {
        let tail = &rest[at + URL.len()..];
        let len = tail.find(|c: char| !(c.is_ascii_alphanumeric() || c == '-')).unwrap_or(tail.len());
        let id = &tail[..len];
        out.push_str(&rest[..at]);
        match (!id.is_empty()).then(|| agent_file(desktop, id)).flatten() {
            Some(path) => out.push_str(&path.to_string_lossy()),
            None => {
                out.push_str(URL);
                out.push_str(id);
            }
        }
        rest = &tail[len..];
    }
    out.push_str(rest);
    out
}

/// One attachment's bytes on disk under its filename (written once).
fn agent_file(desktop: &LocalDesktop, id: &str) -> Option<PathBuf> {
    let (bytes, _) = desktop.blobs.get(id)?;
    let row = desktop.store.row("attachments", id).ok().flatten();
    let name = row.as_ref().and_then(|r| r.get("filename")).and_then(Value::as_str).filter(|n| !n.is_empty());
    // Only the last path component: a stored name never escapes the folder.
    let name = name.and_then(|n| std::path::Path::new(n).file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| id.to_string());
    let dir = desktop.dir.join(AGENT_FILES_DIR).join(id);
    let path = dir.join(name);
    if !path.is_file() {
        std::fs::create_dir_all(&dir).ok()?;
        std::fs::write(&path, bytes).ok()?;
    }
    Some(path)
}

/// The noun subset as a provider of its own (#VAPP-273: an app's `action`
/// run calls one of these "via" the app).
pub fn local_tools(desktop: Arc<LocalDesktop>) -> Arc<dyn ToolProvider> {
    Arc::new(LocalTools { desktop })
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
                let row = json!({"id": uuid::Uuid::new_v4().to_string(), "conversation_id": conversation, "session_id": session, "root_id": session, "team_id": self.desktop.executor.team_id(), "author_kind": "agent", "author_id": null, "author_session_id": session, "body": text, "card": {"kind": "show", "title": title}, "attachment_ids": [], "issue_brief": null, "answered_at": null, "consumed_at": null, "created_at": now, "updated_at": now});
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
            if text.contains("[seed-only]") {
                // The engine's seed before any prompt: no turn ran.
                self.sink.send(ActivityEvent::Turn { state: TurnState::Ended, started_at: None, tokens: None, at: None, writing: None });
                return;
            }
            if text.contains("[two-blocks]") {
                self.sink.send(ActivityEvent::Turn { state: TurnState::Ended, started_at: None, tokens: None, at: None, writing: None });
                self.sink.send(ActivityEvent::Turn { state: TurnState::Started, started_at: None, tokens: None, at: None, writing: None });
                self.sink.send(ActivityEvent::Narration { text: "I'll open the image.".into(), before_question_id: None, message_id: Some("m1".into()), subagent_id: None, at: None });
                self.sink.send(ActivityEvent::Narration { text: "It is red.".into(), before_question_id: None, message_id: Some("m2".into()), subagent_id: None, at: None });
                self.sink.send(ActivityEvent::Narration { text: " Text: 42.".into(), before_question_id: None, message_id: Some("m2".into()), subagent_id: None, at: None });
                self.sink.send(ActivityEvent::Turn { state: TurnState::Ended, started_at: None, tokens: None, at: None, writing: None });
                return;
            }
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

    /// Waits on the runner's change stream until `check` holds: every
    /// store write of the runner wakes it, so a loaded machine only delays,
    /// never fails; the bound is generous on purpose.
    fn wait_for(changes: &flume::Receiver<Changed>, mut check: impl FnMut() -> bool) {
        let deadline = std::time::Instant::now() + Duration::from_secs(120);
        loop {
            if check() {
                return;
            }
            assert!(std::time::Instant::now() < deadline, "the condition never held within 120 s");
            let _ = changes.recv_timeout(Duration::from_secs(5));
        }
    }

    struct HostEdit;

    impl ToolProvider for HostEdit {
        fn tools(&self) -> Result<Vec<Value>, String> {
            Ok(vec![tool("shell_edit", "Write a shell document.", json!({"name": {"type": "string"}}), &["name"])])
        }
        fn call(&self, caller: &Caller, name: &str, arguments: Value) -> Value {
            content(json!({"tool": name, "by": caller.session_id.as_str(), "name": arguments["name"]}))
        }
    }

    #[test]
    fn the_hosts_tools_sit_next_to_the_nouns_on_one_hub() {
        let dir = tempfile::tempdir().unwrap();
        let desktops = crate::Desktops::open(dir.path()).unwrap();
        let desktop = Arc::new(desktops.open_desktop(desktops.current().unwrap()).unwrap());
        let merged = Merged { local: Arc::new(LocalTools { desktop }), host: Arc::new(HostEdit) };
        let names: Vec<String> = merged.tools().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_string()).collect();
        assert!(names.contains(&"exponential_issues".to_string()) && names.last() == Some(&"shell_edit".to_string()));
        let caller = Caller { session_id: SessionId::new("run-1").unwrap(), label: "test".into() };
        let out = merged.call(&caller, "shell_edit", json!({"name": "taskbar"}));
        assert_eq!(out["structuredContent"]["tool"], json!("shell_edit"));
        assert_eq!(out["structuredContent"]["by"], json!("run-1"));
        let out = merged.call(&caller, "exponential_boards", json!({"op": "list"}));
        assert!(out["structuredContent"].is_array());
    }

    /// #EDE-65 lane L: a sent image reaches the CLI as a file on disk under
    /// its name, in place of its URL; an id without bytes keeps its URL.
    #[test]
    fn a_message_s_attachments_reach_the_agent_as_files() {
        let dir = tempfile::tempdir().unwrap();
        let desktops = crate::Desktops::open(dir.path()).unwrap();
        let desktop = desktops.open_desktop(desktops.current().unwrap()).unwrap();
        let row = desktop.executor.attachment_create("shot.png", "image/png", b"PNG!", None).unwrap();
        let id = row["id"].as_str().unwrap().to_string();
        let text = format!("look\n\n![shot.png](/api/attachments/{id})\n[x](/api/attachments/missing-1)");
        let local = localize_attachments(&desktop, &text);
        let path = desktop.dir.join(AGENT_FILES_DIR).join(&id).join("shot.png");
        assert_eq!(local, format!("look\n\n![shot.png]({})\n[x](/api/attachments/missing-1)", path.display()));
        assert_eq!(std::fs::read(&path).unwrap(), b"PNG!");
        assert_eq!(localize_attachments(&desktop, "no files"), "no files");
    }

    /// #EDE-65: a turn with two assistant messages (prose, a tool call,
    /// more prose) lands as two blocks with a blank line between them, and
    /// a message's own chunks join as they came; the session goes idle.
    #[test]
    fn a_turns_assistant_messages_keep_their_block_break() {
        let dir = tempfile::tempdir().unwrap();
        let desktops = crate::Desktops::open(dir.path()).unwrap();
        let desktop = Arc::new(desktops.open_desktop(desktops.current().unwrap()).unwrap());
        let runner = Arc::new(FakeRunner { prompts: Arc::new(Mutex::new(Vec::new())), grants: Arc::new(Mutex::new(Vec::new())) });
        let runs = LocalRuns::start(desktop.clone(), runner, dir.path().to_path_buf()).unwrap();
        let changes = runs.changes();
        let mut send = api::sessions::SendInput::default();
        send.team_id = Some(desktop.executor.team_id().to_string());
        send.text = Some("[two-blocks] look".into());
        let session = api::sessions::send(&desktop.client(), &send).unwrap().session_id;
        wait_for(&changes, || desktop.store.row("sessions", &session).unwrap().unwrap()["status"] == json!("idle"));
        let bodies: Vec<String> = desktop.store.rows_where("messages", "root_id", &session).unwrap().into_iter().filter(|m| m["author_kind"] == "agent").filter_map(|m| m["body"].as_str().map(str::to_string)).collect();
        assert_eq!(bodies, ["I'll open the image.\n\nIt is red. Text: 42."]);
        runs.stop_all();
    }

    /// #EDE-65: the engine seeds a turn `ended` before the first prompt; that
    /// is no turn end: the session stays running until a turn really ends.
    #[test]
    fn the_engines_seeded_turn_end_is_no_turn_end() {
        let dir = tempfile::tempdir().unwrap();
        let desktops = crate::Desktops::open(dir.path()).unwrap();
        let desktop = Arc::new(desktops.open_desktop(desktops.current().unwrap()).unwrap());
        let runner = Arc::new(FakeRunner { prompts: Arc::new(Mutex::new(Vec::new())), grants: Arc::new(Mutex::new(Vec::new())) });
        let runs = LocalRuns::start(desktop.clone(), runner, dir.path().to_path_buf()).unwrap();
        let changes = runs.changes();
        let mut send = api::sessions::SendInput::default();
        send.team_id = Some(desktop.executor.team_id().to_string());
        send.text = Some("[seed-only] look".into());
        let session = api::sessions::send(&desktop.client(), &send).unwrap().session_id;
        let status = |d: &LocalDesktop| d.store.row("sessions", &session).unwrap().unwrap()["status"].as_str().unwrap_or("").to_string();
        wait_for(&changes, || runs.is_running(&session));
        // Every event the run sent is handled by now (one worker, in order).
        while changes.recv_timeout(Duration::from_millis(500)).is_ok() {}
        assert_eq!(status(&desktop), "running");
        runs.stop_all();
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
        wait_for(&changes, || desktop.store.row("sessions", &session).unwrap().unwrap()["status"] == json!("idle"));
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

        // A follow-up goes to the live run as its next prompt.
        api::sessions::send(&client, &api::sessions::SendInput::message(&session, "And the logout", vec![])).unwrap();
        wait_for(&changes, || prompts.lock().unwrap().len() == 2);
        wait_for(&changes, || desktop.store.rows_where("messages", "root_id", &session).unwrap().iter().filter(|m| m["author_kind"] == json!("agent")).count() == 2);

        // Ending it in the store stops the run and the hub agrees.
        api::sessions::end(&client, &session).unwrap();
        wait_for(&changes, || !runs.is_running(&session));
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

    /// #EDE-65: the IDE and eDE on one desktop (two opens of its folder, as
    /// two processes would): one runs the agents, the other stands by; a
    /// message sent from the standby's side runs on the runner, and when the
    /// runner stops the standby takes over.
    #[test]
    fn one_runner_per_desktop_and_the_other_takes_over() {
        let dir = tempfile::tempdir().unwrap();
        let desktops = crate::Desktops::open(dir.path()).unwrap();
        let id = desktops.current().unwrap().to_string();
        let (ide, ede) = (Arc::new(desktops.open_desktop(&id).unwrap()), Arc::new(desktops.open_desktop(&id).unwrap()));
        let (ide_prompts, ede_prompts) = (Arc::new(Mutex::new(Vec::new())), Arc::new(Mutex::new(Vec::new())));
        let fake = |prompts: &Arc<Mutex<Vec<String>>>| Arc::new(FakeRunner { prompts: prompts.clone(), grants: Arc::new(Mutex::new(Vec::new())) });
        let ide_runs = LocalRuns::start(ide.clone(), fake(&ide_prompts), dir.path().to_path_buf()).unwrap();
        let ede_runs = LocalRuns::start(ede.clone(), fake(&ede_prompts), dir.path().to_path_buf()).unwrap();
        assert!(ide_runs.is_runner());
        assert!(!ede_runs.is_runner());

        // Sent through eDE's store: the IDE's runner hears the file move.
        let mut send = api::sessions::SendInput::default();
        send.team_id = Some(id.clone());
        send.text = Some("From the shell".into());
        let session = api::sessions::send(&ede.client(), &send).unwrap().session_id;
        wait_for(&ide_runs.changes(), || ide.store.row("sessions", &session).unwrap().unwrap()["status"] == json!("idle"));
        assert_eq!(ide_prompts.lock().unwrap().as_slice(), ["From the shell"]);
        assert!(ede_prompts.lock().unwrap().is_empty());

        // The IDE leaves the desktop: eDE takes over, never re-running what was answered.
        ide_runs.stop_all();
        let changes = ede_runs.changes();
        wait_for(&changes, || ede_runs.is_runner());
        send.text = Some("Second chat".into());
        let second = api::sessions::send(&ede.client(), &send).unwrap().session_id;
        wait_for(&changes, || ede.store.row("sessions", &second).unwrap().unwrap()["status"] == json!("idle"));
        assert_eq!(ede_prompts.lock().unwrap().as_slice(), ["Second chat"]);
        ede_runs.stop_all();
    }

    /// A fake CLI under a real id ("claude", "codex").
    struct NamedFake(&'static str, FakeRunner);

    impl Runner for NamedFake {
        fn id(&self) -> &str {
            self.0
        }
        fn start(&self, brief: Brief, grant: HubGrant, sink: Arc<dyn EventSink>) -> Result<Box<dyn Run>, String> {
            self.1.start(brief, grant, sink)
        }
    }

    /// #EDE-65 J5: a session runs the CLI the chat's picker chose (its
    /// `cli`), else its agent's, else the default; one this machine lacks
    /// runs on the default with a note.
    #[test]
    fn a_session_runs_the_cli_it_names() {
        let dir = tempfile::tempdir().unwrap();
        let desktops = crate::Desktops::open(dir.path()).unwrap();
        let desktop = Arc::new(desktops.open_desktop(desktops.current().unwrap()).unwrap());
        let (claude, codex) = (Arc::new(Mutex::new(Vec::new())), Arc::new(Mutex::new(Vec::new())));
        let fake = |id: &'static str, prompts: &Arc<Mutex<Vec<String>>>| Arc::new(NamedFake(id, FakeRunner { prompts: prompts.clone(), grants: Arc::new(Mutex::new(Vec::new())) })) as Arc<dyn Runner>;
        let runs = LocalRuns::start_with_runners(desktop.clone(), vec![fake("claude", &claude), fake("codex", &codex)], dir.path().to_path_buf(), None).unwrap();
        let changes = runs.changes();
        let client = desktop.client();
        let send = |text: &str, cli: Option<&str>| {
            let mut input = api::sessions::SendInput::default();
            input.team_id = Some(desktop.executor.team_id().to_string());
            input.text = Some(text.into());
            input.cli = cli.map(str::to_string);
            api::sessions::send(&client, &input).unwrap().session_id
        };
        let idle = |session: &str| desktop.store.row("sessions", session).unwrap().unwrap()["status"] == json!("idle");

        let picked = send("On codex", Some("codex"));
        wait_for(&changes, || idle(&picked));
        assert_eq!(codex.lock().unwrap().as_slice(), ["On codex"]);

        let plain = send("On the default", None);
        wait_for(&changes, || idle(&plain));
        assert_eq!(claude.lock().unwrap().as_slice(), ["On the default"]);

        // An agent's CLI when the session names none.
        let agent = api::agents::ensure_assistant(&client, desktop.executor.team_id()).unwrap().expect("the assistant");
        desktop.store.modify("agents", &agent.id, |row| {
            let mut row = row.clone();
            row.insert("cli".into(), json!("codex"));
            Some(row)
        }).unwrap();
        let mut input = api::sessions::SendInput::default();
        input.team_id = Some(desktop.executor.team_id().to_string());
        input.agent_id = Some(agent.id.clone());
        input.text = Some("For the assistant".into());
        let for_agent = api::sessions::send(&client, &input).unwrap().session_id;
        wait_for(&changes, || idle(&for_agent));
        assert_eq!(codex.lock().unwrap().last().map(String::as_str), Some("For the assistant"));

        let missing = send("On cursor", Some("cursor"));
        wait_for(&changes, || idle(&missing));
        assert_eq!(claude.lock().unwrap().last().map(String::as_str), Some("On cursor"));
        let notes: Vec<Value> = desktop.store.rows_where("messages", "root_id", &missing).unwrap().into_iter().filter(|m| m["author_kind"] == json!("system")).map(|m| m["body"].clone()).collect();
        assert_eq!(notes, [json!("cursor is not installed on this machine: claude runs this session.")]);
        runs.stop_all();
    }
}

