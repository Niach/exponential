//! EXP-1292 — the session pickup: the database is the bus. A new session is
//! a `queued` `sessions` row naming its device, a person's message a
//! `messages` row with `consumed_at` NULL. Both hosts (the CLI daemon and
//! the desktop) read their shapes and ask [`plan`] what to do; the host then
//! executes each [`PickupAction`] (claim + launch, deliver, answer, kill).
//!
//! gpui-free. [`plan`] is pure; the entry points ([`launch_queued`],
//! [`deliver_messages`] / [`resume_session`], [`execute`]) block on the
//! network and git, so hosts call them off their UI thread. [`ClaimLock`] (a
//! per-session lockfile under the data dir) keeps two processes of ONE
//! machine sharing a data dir — the daemon and a foreground `exponential
//! code` — from both running a row; `sessions.claim` is idempotent per
//! device, so the server cannot tell them apart.

use std::collections::{HashMap, HashSet};

use api::session_device;
use api::ApiError;

use crate::launcher::{PrepareRequest, Prepared, ResumeRun, SessionLaunchRequest};
use crate::run_registry::{RunKind, RunRecord};
use domain::contract::{
    SESSION_KIND_AGENT, SESSION_STATUS_ENDED, SESSION_STATUS_IDLE, SESSION_STATUS_QUEUED,
    SESSION_STATUS_RUNNING,
};
use domain::rows::{MessageRow, SessionRow};

/// What the host knows about one session it runs RIGHT NOW (an engine is
/// attached in this process).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LiveView {
    /// Person message ids already handed to the engine (delivered or held in
    /// its queue) and not yet confirmed consumed on the server.
    pub handed: HashSet<String>,
    /// Open question/plan cards this session posted: `messages.id` → the
    /// engine's question id (the card also carries it as `questionId`).
    pub open_cards: HashMap<String, String>,
    /// EXP-849: the row's `account` (a profile id, `""` = none) this engine
    /// was launched for. A row whose account moves past it is a person's
    /// SWITCH ([`PickupAction::Switch`]); `None` = unknown, never switched.
    pub row_account: Option<String>,
    /// EXP-849: the login (profile id) the engine ACTUALLY runs on — a start
    /// pick or a rotation may differ from [`Self::row_account`]; the device
    /// reports it (`sessions.report{account}`), and a row that comes back
    /// naming it moves nothing. `None` = unknown.
    pub running_account: Option<String>,
}

/// One pass's input.
#[derive(Debug, Clone, Copy)]
pub struct PickupInput<'a> {
    /// THIS host's steer device id.
    pub device_id: &'a str,
    /// Every visible `sessions` row.
    pub sessions: &'a [SessionRow],
    /// Every visible `messages` row.
    pub messages: &'a [MessageRow],
    /// Sessions with an engine attached in this process.
    pub live: &'a HashMap<String, LiveView>,
    /// Sessions this host is claiming/launching/resuming right now (no
    /// second action for them until the attempt settles).
    pub in_flight: &'a HashSet<String>,
}

/// What the host must do. Ordered: kills first, then launches/resumes, then
/// deliveries and answers (each in `created_at` order).
#[derive(Debug, Clone, PartialEq)]
pub enum PickupAction {
    /// A `queued` row naming this device with no engine here: take the
    /// [`ClaimLock`], `sessions.claim`, `sessions.brief`, prepare, launch.
    Launch { session: SessionRow },
    /// A `running`/`idle` row on this device with NO engine here (the host
    /// restarted, or an agent's process exited idle) that has pending person
    /// messages: re-spawn the SAME row's agent CLI from its run record
    /// (native resume) with `message` as the first prompt; consume it once
    /// sent. Later pending messages are delivered once it is live.
    Resume { session: SessionRow, message: MessageRow },
    /// Pending person messages of a LIVE session not yet handed to its
    /// engine, oldest first: `EngineSession::deliver(id, body)` each.
    Deliver { session_id: String, messages: Vec<MessageRow> },
    /// A message handed to the engine but still unconsumed whose row is gone
    /// (`sessions.unqueue` deleted it): drop it from the engine's queue.
    Revoke { session_id: String, message_id: String },
    /// A question/plan card of a live session answered in the card
    /// (`answered_at` set): feed the answer/decision to the engine.
    Answer { session_id: String, message_id: String, question_id: String, card: serde_json::Value },
    /// A live session whose row is `ended` or gone from the shape: kill its
    /// engine with [`KILL_OUTCOME`] (replaces the kill watch / kill poll).
    Kill { session_id: String },
    /// EXP-849: a live session whose row's `account` moved past the one its
    /// engine runs for (a person's switch, `sessions.send{sessionId,
    /// account}`): [`switch_session`] suspends it between turns and resumes
    /// the SAME row on `account`, the oldest pending message (if any) as
    /// the seed turn. Its deliveries wait for the switch.
    Switch { session: SessionRow, account: String, messages: Vec<MessageRow> },
}

/// Decide one pass. Pure.
///
/// - **Kill**: a live session whose row is `ended` or gone from the shape
///   (hosts run a pass only once their shapes are up to date, so "gone" is
///   a deletion, never a cold store).
/// - **Launch**: a `queued` row naming this device with no engine here.
/// - **Resume**: a `running`/`idle` row naming this device with no engine
///   here and a pending person message (the oldest rides along).
/// - **Revoke**: a handed message whose row is gone (unqueued).
/// - **Deliver**: a live session's pending person messages not yet handed.
/// - **Answer**: an open card of a live session whose row is answered.
/// - **Switch**: a live session whose row's account moved past its engine's
///   (instead of its deliveries) — at once when the row names the login the
///   engine already runs on (the device's own report: honored, nothing
///   moves), else only with a turn to carry it: a pending message, or the
///   usage wall it continues past (an idle account-only pick waits for the
///   next message rather than seed a turn of its own).
///
/// Rows naming another device and sessions [`PickupInput::in_flight`] get
/// nothing; a killed session gets nothing else in the same pass.
pub fn plan(input: PickupInput<'_>) -> Vec<PickupAction> {
    let rows: HashMap<&str, &SessionRow> =
        input.sessions.iter().map(|row| (row.id.as_str(), row)).collect();
    let messages: HashMap<&str, &MessageRow> =
        input.messages.iter().map(|row| (row.id.as_str(), row)).collect();
    // EXP-1329: pending INPUT — a person's message or another session's (a
    // child's report), the latter worded as its turn ([`input_text`]).
    let inputs: Vec<MessageRow> = input
        .messages
        .iter()
        .filter(|m| is_pending_input(m))
        .map(|m| MessageRow { body: Some(input_text(m, input.sessions)), ..m.clone() })
        .collect();
    let mut pending: HashMap<&str, Vec<&MessageRow>> = HashMap::new();
    for message in &inputs {
        pending.entry(message.session_id.as_str()).or_default().push(message);
    }
    for queue in pending.values_mut() {
        queue.sort_by(|a, b| message_order(a, b));
    }

    let mut live_ids: Vec<&str> = input
        .live
        .keys()
        .map(String::as_str)
        .filter(|id| !input.in_flight.contains(*id))
        .collect();
    live_ids.sort_unstable();

    // 1. Kills.
    let mut actions = Vec::new();
    let mut killed = HashSet::new();
    for &session_id in &live_ids {
        let gone = rows
            .get(session_id)
            .map_or(true, |row| row.status_is(SESSION_STATUS_ENDED));
        if gone {
            killed.insert(session_id);
            actions.push(PickupAction::Kill { session_id: session_id.to_string() });
        }
    }

    // 2. Launches and resumes, oldest row first.
    let mut starts: Vec<(&SessionRow, PickupAction)> = Vec::new();
    for row in input.sessions {
        if !row.on_device(input.device_id)
            || input.live.contains_key(&row.id)
            || input.in_flight.contains(&row.id)
        {
            continue;
        }
        if row.status_is(SESSION_STATUS_QUEUED) {
            starts.push((row, PickupAction::Launch { session: row.clone() }));
        } else if row.status_is(SESSION_STATUS_RUNNING) || row.status_is(SESSION_STATUS_IDLE) {
            if let Some(first) = pending.get(row.id.as_str()).and_then(|queue| queue.first()) {
                starts.push((
                    row,
                    PickupAction::Resume { session: row.clone(), message: (*first).clone() },
                ));
            }
        }
    }
    starts.sort_by(|(a, _), (b, _)| (&a.created_at, &a.id).cmp(&(&b.created_at, &b.id)));
    actions.extend(starts.into_iter().map(|(_, action)| action));

    // 3. Switches, revokes, deliveries, answers of the surviving live
    // sessions.
    let mut switches = Vec::new();
    let mut revokes = Vec::new();
    let mut deliveries: Vec<(&MessageRow, PickupAction)> = Vec::new();
    let mut answers: Vec<(&MessageRow, PickupAction)> = Vec::new();
    for &session_id in live_ids.iter().filter(|id| !killed.contains(*id)) {
        let view = &input.live[session_id];
        let mut handed: Vec<&String> = view.handed.iter().collect();
        handed.sort_unstable();
        for message_id in handed {
            if !messages.contains_key(message_id.as_str()) {
                revokes.push(PickupAction::Revoke {
                    session_id: session_id.to_string(),
                    message_id: message_id.clone(),
                });
            }
        }
        let fresh: Vec<&MessageRow> = pending
            .get(session_id)
            .map(|queue| queue.iter().copied().filter(|m| !view.handed.contains(&m.id)).collect())
            .unwrap_or_default();
        let switch = rows.get(session_id).and_then(|row| {
            switch_target(row, view, !fresh.is_empty()).map(|account| (*row, account))
        });
        if let Some((row, account)) = switch {
            switches.push(PickupAction::Switch {
                session: row.clone(),
                account,
                messages: fresh.into_iter().cloned().collect(),
            });
        } else if let Some(first) = fresh.first().copied() {
            deliveries.push((
                first,
                PickupAction::Deliver {
                    session_id: session_id.to_string(),
                    messages: fresh.into_iter().cloned().collect(),
                },
            ));
        }
        for (message_id, question_id) in &view.open_cards {
            let Some(row) = messages.get(message_id.as_str()).copied() else { continue };
            let (Some(_), Some(card)) = (&row.answered_at, &row.card) else { continue };
            answers.push((
                row,
                PickupAction::Answer {
                    session_id: session_id.to_string(),
                    message_id: message_id.clone(),
                    question_id: question_id.clone(),
                    card: card.clone(),
                },
            ));
        }
    }
    deliveries.sort_by(|(a, _), (b, _)| message_order(a, b));
    answers.sort_by(|(a, _), (b, _)| message_order(a, b));
    actions.extend(switches);
    actions.extend(revokes);
    actions.extend(deliveries.into_iter().map(|(_, action)| action));
    actions.extend(answers.into_iter().map(|(_, action)| action));
    actions
}

/// EXP-849: the profile a live session must move to — its row names an
/// account (non-empty) that is not the one its engine was launched for.
/// The login the engine already runs on is planned at once (the switch
/// finds it [`Switched::AlreadyOn`] and honors it: the device's own
/// `sessions.report{account}` read back, never a move); a real move waits
/// for a turn to carry it (`has_messages`, or a walled row to continue).
fn switch_target(row: &SessionRow, view: &LiveView, has_messages: bool) -> Option<String> {
    let wanted = crate::agent_profiles::profile_id(row.account.as_deref());
    let honored = view.row_account.as_deref()?;
    if wanted.is_empty() || wanted == honored {
        return None;
    }
    if view.running_account.as_deref() == Some(wanted.as_str()) {
        return Some(wanted);
    }
    (has_messages || row_walled(row)).then_some(wanted)
}

/// EXP-804: the row carries a usage wall (`blocked` set, not JSON null).
fn row_walled(row: &SessionRow) -> bool {
    row.blocked.as_ref().is_some_and(|wall| !wall.is_null())
}

/// EXP-1316: the `messages.id` of the open card the engine posted for
/// `question_id` (open cards = `messages.id` → question id); the smallest id
/// when (never in practice) two carry it.
pub fn card_message_for(open_cards: &HashMap<String, String>, question_id: &str) -> Option<String> {
    open_cards
        .iter()
        .filter(|(_, question)| question.as_str() == question_id)
        .map(|(message_id, _)| message_id)
        .min()
        .cloned()
}

/// EXP-1316: write a relay-resolved card back (`sessions.resolveCard`) on
/// its own thread — idempotent server-side, never blocks the engine; a
/// failure is a log line (the card merely stays open elsewhere).
pub fn write_back_card(trpc: std::sync::Arc<api::TrpcClient>, input: api::sessions::AnswerInput) {
    let spawned = std::thread::Builder::new().name("resolve-card".to_string()).spawn(move || {
        if let Err(err) = api::sessions::resolve_card(&trpc, &input) {
            log::warn!("pickup: resolve card {} failed: {err}", input.message_id);
        }
    });
    if let Err(err) = spawned {
        log::warn!("pickup: resolve card thread: {err}");
    }
}

/// EXP-1329: a message the device feeds its session as a turn and then
/// consumes (the server's `pendingInput`): a person's, or one another
/// SESSION posted into this one (`author_session_id`: a child's `report`
/// card, a child asking its parent) — never the session's own reply.
pub fn is_pending_input(message: &MessageRow) -> bool {
    if !message.consumed_at.as_deref().is_none_or(str::is_empty) {
        return false;
    }
    message.is_from_user()
        || message
            .author_session_id
            .as_deref()
            .is_some_and(|from| !from.is_empty() && from != message.session_id)
}

/// EXP-1329: the text a pending input reaches the agent as. A person's
/// message is its body; a child's `report` card reads `Report from <child>
/// (session <id>): <summary>` (the summary, else the body), any other
/// session input `Message from <child> (session <id>): <body>` — the id is
/// what `exponential_sessions_get` reads more from. `<child>` = its title
/// off `sessions`, else `Session`.
pub fn input_text(message: &MessageRow, sessions: &[SessionRow]) -> String {
    let body = message.body_text().trim();
    let Some(from) = message.author_session_id.as_deref().filter(|_| !message.is_from_user()) else {
        return message.body.clone().unwrap_or_default();
    };
    let card = message.card.as_ref();
    let from = card
        .and_then(|card| card.get("sessionId"))
        .and_then(serde_json::Value::as_str)
        .filter(|id| !id.is_empty())
        .unwrap_or(from);
    let name = sessions
        .iter()
        .find(|row| row.id == from)
        .and_then(|row| row.title.as_deref())
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .unwrap_or("Session");
    if message.card_kind() == Some("report") {
        let summary = card
            .and_then(|card| card.get("summary"))
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|summary| !summary.is_empty())
            .unwrap_or(body);
        return format!("Report from {name} (session {from}): {summary}");
    }
    format!("Message from {name} (session {from}): {body}")
}

fn message_order(a: &MessageRow, b: &MessageRow) -> std::cmp::Ordering {
    (&a.created_at, &a.id).cmp(&(&b.created_at, &b.id))
}

/// A per-session lockfile under `{data_dir}/session-claims/` held for the
/// lifetime of the session's engine in this process (inside its
/// [`LiveRegistry`] entry). A `proper-lockfile` mkdir lock
/// ([`crate::lockfile`]) with a heartbeat, so a crashed holder's claim goes
/// stale on its own; taking it never waits.
pub struct ClaimLock {
    guard: crate::lockfile::LockGuard,
}

impl std::fmt::Debug for ClaimLock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClaimLock").field("path", &self.guard.path()).finish()
    }
}

/// The claims directory under the data dir.
pub const CLAIMS_DIR: &str = "session-claims";

/// A live claim's heartbeat period and the age past which a dead holder's
/// claim is reclaimed.
const CLAIM_LOCK_OPTIONS: crate::lockfile::LockOptions = crate::lockfile::LockOptions {
    stale: std::time::Duration::from_secs(30),
    update: std::time::Duration::from_secs(5),
    attempts: 1,
    backoff: std::time::Duration::ZERO,
    jitter: std::time::Duration::ZERO,
};

impl ClaimLock {
    /// `{data_dir}/session-claims/<id>.lock`; `None` for an id that is not
    /// a plain path segment (server ids are untrusted here; real ones are
    /// UUIDs).
    pub fn path(data_dir: &std::path::Path, session_id: &str) -> Option<std::path::PathBuf> {
        let plain = !session_id.is_empty()
            && session_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        plain.then(|| data_dir.join(CLAIMS_DIR).join(format!("{session_id}.lock")))
    }

    /// Take the claim without waiting: `Ok(None)` = another process holds
    /// it (it runs the session; skip).
    pub fn try_acquire(data_dir: &std::path::Path, session_id: &str) -> Result<Option<Self>, String> {
        let path = Self::path(data_dir, session_id)
            .ok_or_else(|| format!("session id {session_id:?} is not a plain path segment"))?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| format!("create {}: {err}", parent.display()))?;
        }
        match crate::lockfile::acquire(&path, CLAIM_LOCK_OPTIONS) {
            Ok(guard) => Ok(Some(Self { guard })),
            Err(crate::lockfile::LockError::Contended) => Ok(None),
            Err(err) => Err(format!("claim lock {}: {err}", path.display())),
        }
    }

    /// The lock was taken over (it went stale under a stalled host).
    pub fn is_compromised(&self) -> bool {
        self.guard.is_compromised()
    }
}

// ── The host-facing entry points (pinned with #EXP-1294's desktop pickup) ──
// `coding` depends on neither `engine` nor `steer` (§3.1), so the running
// engine sits behind [`LiveEngine`] (implemented for `engine::EngineSession`
// in the engine crate) and the spawn behind [`LaunchFn`].

/// A session engine running in this process.
pub trait LiveEngine: Send + Sync {
    /// Hand a person's `messages` row to the agent (idle → a new turn,
    /// mid-turn → queued/sent like a steer). The engine reports it consumed.
    fn deliver(&self, message_id: String, text: String);
    /// Drop a handed-but-unconsumed message (its row was unqueued).
    fn unqueue(&self, message_id: &str);
    /// Feed an answered question/plan card to the engine's open question.
    fn answer_card(&self, question_id: &str, card: &serde_json::Value);
    /// Open question/plan cards: `messages.id` → engine question id.
    fn open_cards(&self) -> HashMap<String, String>;
    /// End the run. The engine lets a turn in flight finish first (bounded,
    /// EXP-637): an agent that ended its own session over MCP is still
    /// writing the close-out reply.
    fn kill(&self, outcome: &'static str);
    fn is_done(&self) -> bool;
    /// EXP-849: between turns (nothing in flight) — a switch waits for it.
    fn is_idle(&self) -> bool;
    /// EXP-1292: stop the PROCESS but not the session (the row idles, the
    /// run's workspace — a scratch dir included — is kept) for an immediate
    /// re-spawn of the same row on another account.
    fn suspend(&self);
    /// Wait up to `timeout` for the run to end; `true` = it did.
    fn wait_done(&self, timeout: std::time::Duration) -> bool {
        let deadline = std::time::Instant::now() + timeout;
        while !self.is_done() {
            if std::time::Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        true
    }
}

/// Spawn the engine for a prepared launch (the host's `engine::start` +
/// registration). Called by [`launch_queued`] / [`deliver_messages`].
pub type LaunchFn = std::sync::Arc<
    dyn Fn(crate::PreparedLaunch) -> Result<std::sync::Arc<dyn LiveEngine>, String> + Send + Sync,
>;

/// The live sessions of this process (shared by the host and the pickup):
/// session id → its engine, the [`ClaimLock`] it runs under, the person
/// messages handed to it and the cards already answered into it.
#[derive(Default)]
pub struct LiveRegistry {
    entries: std::sync::Mutex<HashMap<String, LiveEntry>>,
    /// Sessions whose (re)launch was refused → the pending message ids at
    /// the time, so a refused wake is not retried (and re-explained) every
    /// pass; a NEW message tries again.
    refused: std::sync::Mutex<HashMap<String, HashSet<String>>>,
    /// Rows this host claimed (or may have — a claim whose answer was lost)
    /// without an engine coming up yet, with the attempts so far: retried
    /// through [`LiveRegistry::retry_launches`] even once the row reads
    /// `running`, so a transient failure never strands it.
    unsettled: std::sync::Mutex<HashMap<String, (SessionRow, u32)>>,
    /// Rows whose claim the server refused for a reason the person must hear
    /// (a session limit) → the row's `updated_at` then: explained once, not
    /// re-claimed until the row changes.
    claim_refused: std::sync::Mutex<HashMap<String, Option<String>>>,
    /// Person message ids the pickup consumes itself (ones the session's
    /// previous engine handed over but could not report) — never delivered.
    consuming: std::sync::Mutex<HashSet<String>>,
}

fn locked<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

struct LiveEntry {
    engine: std::sync::Arc<dyn LiveEngine>,
    _claim: Option<ClaimLock>,
    handed: HashSet<String>,
    answered: HashSet<String>,
    /// [`LiveView::row_account`].
    row_account: Option<String>,
    /// [`LiveView::running_account`].
    running_account: Option<String>,
}

impl LiveRegistry {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, LiveEntry>> {
        self.entries.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Register a session's engine (replacing — and releasing — any earlier
    /// entry for it). `claim` is held until the entry goes.
    pub fn insert(
        &self,
        session_id: &str,
        engine: std::sync::Arc<dyn LiveEngine>,
        claim: Option<ClaimLock>,
    ) {
        self.insert_for(session_id, engine, claim, None);
    }

    /// [`Self::insert`] naming the row account the engine was launched for
    /// ([`LiveView::row_account`]).
    pub fn insert_for(
        &self,
        session_id: &str,
        engine: std::sync::Arc<dyn LiveEngine>,
        claim: Option<ClaimLock>,
        row_account: Option<String>,
    ) {
        self.lock().insert(
            session_id.to_string(),
            LiveEntry {
                engine,
                _claim: claim,
                handed: HashSet::new(),
                answered: HashSet::new(),
                row_account,
                running_account: None,
            },
        );
        self.refused
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(session_id);
    }

    pub fn get(&self, session_id: &str) -> Option<std::sync::Arc<dyn LiveEngine>> {
        self.lock().get(session_id).map(|entry| std::sync::Arc::clone(&entry.engine))
    }

    /// A RUNNING engine for the session (a done one does not count).
    pub fn contains_live(&self, session_id: &str) -> bool {
        self.lock().get(session_id).is_some_and(|entry| !entry.engine.is_done())
    }

    /// Drop the entry (releasing its claim) and hand back its engine.
    pub fn remove(&self, session_id: &str) -> Option<std::sync::Arc<dyn LiveEngine>> {
        self.lock().remove(session_id).map(|entry| entry.engine)
    }

    /// Drop every entry whose engine is done; returns their ids.
    pub fn prune(&self) -> Vec<String> {
        let mut entries = self.lock();
        let mut done: Vec<String> = entries
            .iter()
            .filter(|(_, entry)| entry.engine.is_done())
            .map(|(id, _)| id.clone())
            .collect();
        done.sort_unstable();
        for id in &done {
            entries.remove(id);
        }
        done
    }

    pub fn session_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.lock().keys().cloned().collect();
        ids.sort_unstable();
        ids
    }

    /// [`plan`]'s view of the running engines (done ones excluded — the
    /// pass treats them as gone): handed ids and the open, not yet answered
    /// cards.
    pub fn live_views(&self) -> HashMap<String, LiveView> {
        self.lock()
            .iter()
            .filter(|(_, entry)| !entry.engine.is_done())
            .map(|(id, entry)| {
                let mut open_cards = entry.engine.open_cards();
                open_cards.retain(|message_id, _| !entry.answered.contains(message_id));
                (
                    id.clone(),
                    LiveView {
                        handed: entry.handed.clone(),
                        open_cards,
                        row_account: entry.row_account.clone(),
                        running_account: entry.running_account.clone(),
                    },
                )
            })
            .collect()
    }

    /// EXP-849: the row account the session's engine now honors (a switch
    /// that found it already there, or one it refused).
    pub fn set_row_account(&self, session_id: &str, row_account: Option<String>) {
        if let Some(entry) = self.lock().get_mut(session_id) {
            entry.row_account = row_account;
        }
    }

    /// EXP-849: the login the session's engine actually runs on
    /// ([`LiveView::running_account`]).
    pub fn set_running_account(&self, session_id: &str, account: Option<String>) {
        if let Some(entry) = self.lock().get_mut(session_id) {
            entry.running_account = account;
        }
    }

    /// Record message ids as handed to the session's engine.
    pub fn mark_handed(&self, session_id: &str, message_ids: &[String]) {
        if let Some(entry) = self.lock().get_mut(session_id) {
            entry.handed.extend(message_ids.iter().cloned());
        }
    }

    /// Forget a handed id (revoked).
    pub fn forget_handed(&self, session_id: &str, message_id: &str) {
        if let Some(entry) = self.lock().get_mut(session_id) {
            entry.handed.remove(message_id);
        }
    }

    /// Drop handed ids whose rows are consumed — they need no more
    /// bookkeeping (a gone row stays handed until its [`PickupAction::Revoke`]).
    pub fn settle_handed(&self, messages: &[MessageRow]) {
        let consumed: HashSet<&str> = messages
            .iter()
            .filter(|m| m.consumed_at.is_some())
            .map(|m| m.id.as_str())
            .collect();
        if consumed.is_empty() {
            return;
        }
        for entry in self.lock().values_mut() {
            entry.handed.retain(|id| !consumed.contains(id.as_str()));
        }
        locked(&self.consuming).retain(|id| !consumed.contains(id.as_str()));
    }

    /// EXP-1292: Launch actions for the rows this host claimed (or may have
    /// claimed) without an engine coming up — a transient brief/prepare
    /// failure, or a claim whose answer was lost — that now read `running`
    /// (or still `queued`) on this device. [`plan`] only launches `queued`
    /// rows, so hosts append these to its actions; rows ended, gone or moved
    /// elsewhere are forgotten.
    pub fn retry_launches(
        &self,
        device_id: &str,
        sessions: &[SessionRow],
        in_flight: &HashSet<String>,
    ) -> Vec<PickupAction> {
        let rows: HashMap<&str, &SessionRow> =
            sessions.iter().map(|row| (row.id.as_str(), row)).collect();
        let mut unsettled = locked(&self.unsettled);
        unsettled.retain(|id, _| {
            rows.get(id.as_str()).is_some_and(|row| {
                row.on_device(device_id)
                    && (row.status_is(SESSION_STATUS_QUEUED) || row.status_is(SESSION_STATUS_RUNNING))
            })
        });
        let mut ids: Vec<&String> = unsettled
            .keys()
            .filter(|id| !in_flight.contains(*id))
            .filter(|id| rows[id.as_str()].status_is(SESSION_STATUS_RUNNING))
            .collect();
        ids.sort_unstable();
        let live = self.lock();
        ids.into_iter()
            .filter(|id| live.get(*id).is_none_or(|entry| entry.engine.is_done()))
            .map(|id| PickupAction::Launch { session: rows[id.as_str()].clone() })
            .collect()
    }

    /// Bump a claimed row's failed attempts; returns the count.
    fn note_unsettled(&self, row: &SessionRow) -> u32 {
        let mut unsettled = locked(&self.unsettled);
        let entry = unsettled.entry(row.id.clone()).or_insert_with(|| (row.clone(), 0));
        entry.1 += 1;
        entry.1
    }

    fn is_unsettled(&self, session_id: &str) -> bool {
        locked(&self.unsettled).contains_key(session_id)
    }

    fn settle(&self, session_id: &str) {
        locked(&self.unsettled).remove(session_id);
    }

    /// EXP-1316: a relay answer (the session view, not the card) resolved
    /// `question_id` of `session_id`: the `sessions.resolveCard` input for
    /// the card this engine posted for it ([`card_message_for`] over its
    /// open cards), the card marked answered so a later pass never feeds the
    /// stamped row back. `None` = not a session of this registry, or no card
    /// carries that question (nothing to close). Read at once, before the
    /// engine resolves the question out of its open cards.
    pub fn resolve_card_input(
        &self,
        session_id: &str,
        question_id: &str,
        answer: Option<String>,
        decision: Option<String>,
        feedback: Option<String>,
    ) -> Option<api::sessions::AnswerInput> {
        let mut entries = self.lock();
        let entry = entries.get_mut(session_id)?;
        let message_id = card_message_for(&entry.engine.open_cards(), question_id)?;
        if !entry.answered.insert(message_id.clone()) {
            return None;
        }
        Some(api::sessions::AnswerInput { message_id, answer, decision, feedback })
    }

    fn mark_answered(&self, session_id: &str, message_id: &str) {
        if let Some(entry) = self.lock().get_mut(session_id) {
            entry.answered.insert(message_id.to_string());
        }
    }

    fn handed(&self, session_id: &str) -> HashSet<String> {
        self.lock().get(session_id).map(|entry| entry.handed.clone()).unwrap_or_default()
    }

    fn refused_before(&self, session_id: &str, pending: &[&MessageRow]) -> bool {
        let refused = self.refused.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        refused
            .get(session_id)
            .is_some_and(|ids| pending.iter().all(|m| ids.contains(&m.id)))
    }

    fn note_refused(&self, session_id: &str, pending: &[&MessageRow]) {
        self.refused
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(session_id.to_string(), pending.iter().map(|m| m.id.clone()).collect());
    }

    /// Take the ids the pickup consumes itself out of `pending`: `owed`
    /// (handed by a previous engine that ended before its consume landed),
    /// plus any already being consumed. Returns the ids newly marked
    /// [`LiveRegistry::consuming`]. EXP-1292: the start's own message needs
    /// no such care — `sessions.claim` consumes it (the brief's Request
    /// carries it).
    fn take_owed(&self, pending: &mut Vec<&MessageRow>, owed: &[String]) -> Vec<String> {
        let mut covered: Vec<String> =
            pending.iter().filter(|m| owed.contains(&m.id)).map(|m| m.id.clone()).collect();
        let mut consuming = locked(&self.consuming);
        pending.retain(|m| !covered.contains(&m.id) && !consuming.contains(&m.id));
        covered.retain(|id| consuming.insert(id.clone()));
        covered
    }
}

/// The `## Issues` section's identifiers (`- <IDENTIFIER> <title>` lines,
/// link order) — the root's linked issues as the server saw them at the
/// claim. The pickup's `issue_identifiers` read off the host's shapes can
/// lag the `sessions` row (`session_issues` / `issues` are other shapes);
/// the brief never does. The LAST heading wins: sections run Request ·
/// Action · Issues · Parent, so a person's text cannot shadow it.
pub fn brief_issue_identifiers(brief: &str) -> Vec<String> {
    const HEADING: &str = "## Issues\n\n";
    let section = if brief.starts_with(HEADING) {
        Some(&brief[HEADING.len()..])
    } else {
        brief
            .rfind(&format!("\n{HEADING}"))
            .map(|at| &brief[at + 1 + HEADING.len()..])
    };
    let Some(section) = section else {
        return Vec::new();
    };
    section
        .lines()
        .map_while(|line| line.strip_prefix("- "))
        .filter_map(|line| line.split_whitespace().next())
        .filter(|identifier| is_issue_identifier(identifier))
        .map(str::to_string)
        .collect()
}

/// `[A-Z0-9]+-\d+` (an issue identifier: `EXP-42`).
fn is_issue_identifier(text: &str) -> bool {
    let Some((key, number)) = text.rsplit_once('-') else {
        return false;
    };
    !key.is_empty()
        && key.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        && !number.is_empty()
        && number.chars().all(|c| c.is_ascii_digit())
}

/// Consume ids off-thread with a short retry ladder; `then` runs once they
/// landed (best-effort: a consume that never lands leaves the row pending,
/// and the next pass consumes it again).
fn consume_off_thread(
    deps: &PickupDeps,
    session_id: &str,
    ids: Vec<String>,
    then: impl FnOnce() + Send + 'static,
) {
    if ids.is_empty() {
        return;
    }
    let trpc = std::sync::Arc::clone(&deps.coding.trpc);
    let session_id = session_id.to_string();
    let spawned = std::thread::Builder::new().name("exp-pickup-consume".to_string()).spawn(move || {
        for wait in CONSUME_RETRY {
            match session_device::consume(&trpc, &session_id, &ids) {
                Ok(()) => return then(),
                Err(err) if transient(&err) => {
                    log::debug!("pickup: consume of {ids:?} failed, retrying: {err}");
                    std::thread::sleep(wait);
                }
                Err(err) => {
                    log::warn!("pickup: consume of {ids:?} refused: {err}");
                    return then();
                }
            }
        }
        log::warn!("pickup: consume of {ids:?} for session {session_id} still failing");
    });
    if let Err(err) = spawned {
        log::warn!("pickup: consume thread failed to spawn: {err}");
    }
}

const CONSUME_RETRY: [std::time::Duration; 4] = [
    std::time::Duration::from_secs(1),
    std::time::Duration::from_secs(5),
    std::time::Duration::from_secs(15),
    std::time::Duration::from_secs(30),
];

/// A failure worth another attempt: the network, a server error, a timeout
/// or a rate limit (`engine::outbox::retryable`'s rule).
fn transient(err: &ApiError) -> bool {
    match err {
        ApiError::Transport { .. } => true,
        ApiError::Http { status, .. } => *status >= 500 || *status == 408 || *status == 429,
        _ => false,
    }
}

/// A prepare failure worth another attempt: git/IO (a clone or fetch over a
/// flaky network) and transient API errors; an API refusal is final.
fn prepare_transient(err: &crate::CodingError) -> bool {
    match err {
        crate::CodingError::Api(err) => transient(err),
        _ => true,
    }
}

/// How many times a claimed row is tried before the pickup gives up on it
/// (replies why and idles it — the next message tries afresh).
pub const MAX_LAUNCH_ATTEMPTS: u32 = 5;

/// The claim's own retries (it is idempotent per device): a lost answer
/// must not leave a claimed row with nothing behind it.
const CLAIM_RETRY: [std::time::Duration; 2] =
    [std::time::Duration::from_secs(1), std::time::Duration::from_secs(3)];

/// Everything the pickup needs from its host.
pub struct PickupDeps {
    pub coding: std::sync::Arc<crate::CodingDeps>,
    /// THIS host's steer device id.
    pub device_id: String,
    pub device_label: String,
    /// The host's launch defaults (layered under the row's pickers).
    pub options: crate::LaunchOptions,
    pub live: std::sync::Arc<LiveRegistry>,
    pub launch: LaunchFn,
    /// The root's linked issue identifiers in link order (session_issues ⋈
    /// issues off the host's store); empty when unknown.
    pub issue_identifiers: std::sync::Arc<dyn Fn(&str) -> Vec<String> + Send + Sync>,
}

/// What [`launch_queued`] did.
#[derive(Debug)]
pub enum LaunchQueued {
    /// Claimed, prepared and spawned.
    Launched { session_id: String },
    /// Not ours / already live / another process holds the claim / the
    /// server refused the claim.
    Skipped { reason: &'static str },
    /// The launcher refused (doctor, ACP, token, …): the row was reported
    /// (`ended`; an agent `idle`) and the reason replied into its
    /// conversation.
    Disabled { reason: String },
}

/// Claim a `queued` row naming this device (ClaimLock → `sessions.claim` →
/// `sessions.brief` → `prepare(PrepareRequest::Session)` → `deps.launch`).
/// Blocking (network + git); hosts run it off their UI thread and keep the
/// row in their pass's `in_flight` set until it returns.
///
/// EXP-1292 failure rules, so a row never sits `running` with nothing
/// behind it and a blip never ends a thread:
/// - the claim is retried (idempotent per device); a refusal the person
///   must hear (anything but "not yours any more": 404/409) is replied
///   once and not re-claimed until the row changes;
/// - after the claim, a TRANSIENT brief/prepare failure leaves the row
///   unsettled — [`LiveRegistry::retry_launches`] relaunches it even though
///   it reads `running` — up to [`MAX_LAUNCH_ATTEMPTS`], then the reason is
///   replied and the row idles (the next message tries afresh);
/// - a genuine refusal (a disabled prepare, a 4xx brief, a failed spawn) is
///   replied, then a thread is reported `ended`, an agent `idle`.
pub fn launch_queued(row: &SessionRow, deps: &PickupDeps) -> Result<LaunchQueued, String> {
    let retrying = deps.live.is_unsettled(&row.id)
        && (row.status_is(SESSION_STATUS_QUEUED) || row.status_is(SESSION_STATUS_RUNNING));
    if !row.status_is(SESSION_STATUS_QUEUED) && !retrying {
        return Ok(LaunchQueued::Skipped { reason: "not queued" });
    }
    if !row.on_device(&deps.device_id) {
        return Ok(LaunchQueued::Skipped { reason: "another device" });
    }
    if deps.live.contains_live(&row.id) {
        return Ok(LaunchQueued::Skipped { reason: "already live" });
    }
    if locked(&deps.live.claim_refused).get(&row.id).is_some_and(|at| *at == row.updated_at) {
        return Ok(LaunchQueued::Skipped { reason: "claim refused" });
    }
    let Some(claim) = ClaimLock::try_acquire(&deps.coding.data_dir, &row.id)? else {
        return Ok(LaunchQueued::Skipped { reason: "claimed by another process" });
    };
    // From the first claim attempt on, the row may be ours on the server.
    let attempt = deps.live.note_unsettled(row);
    let gave_up = |reason: String| {
        if attempt >= MAX_LAUNCH_ATTEMPTS {
            deps.live.settle(&row.id);
            refuse(row.is_agent(), &row.id, &reason, true, deps);
        }
        reason
    };
    let mut waits = CLAIM_RETRY.iter();
    loop {
        match session_device::claim(&deps.coding.trpc, &row.id, None) {
            Ok(()) => break,
            Err(ApiError::Http { status, message }) if claim_refused(status) => {
                deps.live.settle(&row.id);
                log::info!("pickup: session {} not claimed ({status}): {message}", row.id);
                if !claim_lost(status) {
                    locked(&deps.live.claim_refused).insert(row.id.clone(), row.updated_at.clone());
                    let reason = format!("Couldn't start this session: {message}");
                    if let Err(err) = session_device::reply(&deps.coding.trpc, &row.id, &reason, None) {
                        log::warn!("pickup: claim refusal of {} not replied: {err}", row.id);
                    }
                }
                return Ok(LaunchQueued::Skipped { reason: "claim refused" });
            }
            Err(err) => match waits.next() {
                Some(wait) if transient(&err) => std::thread::sleep(*wait),
                _ => return Err(format!("claim session {}: {err}", row.id)),
            },
        }
    }
    let brief = match session_device::brief(&deps.coding.trpc, &row.id) {
        Ok(brief) => brief,
        Err(err) if transient(&err) => {
            return Err(gave_up(format!("Couldn't start this session: {err}")));
        }
        Err(err) => {
            deps.live.settle(&row.id);
            let reason = format!("Couldn't start this session: {err}");
            refuse(row.is_agent(), &row.id, &reason, false, deps);
            return Err(reason);
        }
    };
    // EXP-1330: a `/btw` aside forks its parent's run (`crate::aside`): no
    // worktree, no brief turn — the question is the first prompt.
    let prepared = if row.aside {
        crate::aside::prepare(row, &brief, &deps.options, &deps.device_label, &deps.coding).map(
            |prepared| match prepared {
                crate::aside::AsidePrepared::Ready(prepared) => Ok(*prepared),
                crate::aside::AsidePrepared::Refused(reason) => Err(reason),
            },
        )
    } else {
        // The brief names the root's issues as the server saw them at the
        // claim; the host's shapes may not have them yet.
        let mut issue_identifiers = brief_issue_identifiers(&brief.brief);
        if issue_identifiers.is_empty() {
            issue_identifiers = (deps.issue_identifiers)(row.root_id_or_self());
        }
        let req = SessionLaunchRequest {
            session: row.clone(),
            brief,
            issue_identifiers,
            resume: None,
            device_label: deps.device_label.clone(),
            options: deps.options.clone(),
        };
        crate::prepare(&PrepareRequest::Session(req), &deps.coding).map(|prepared| match prepared {
            Prepared::Ready(prepared) => Ok(prepared),
            Prepared::Disabled(reason) => Err(reason.message()),
        })
    };
    match prepared {
        Ok(Ok(prepared)) => {
            deps.live.settle(&row.id);
            let honored = Some(crate::agent_profiles::profile_id(row.account.as_deref()));
            spawn(&row.id, row.is_agent(), prepared, claim, false, honored, deps)?;
            Ok(LaunchQueued::Launched { session_id: row.id.clone() })
        }
        Ok(Err(reason)) => {
            deps.live.settle(&row.id);
            refuse(row.is_agent(), &row.id, &reason, false, deps);
            Ok(LaunchQueued::Disabled { reason })
        }
        Err(err) if prepare_transient(&err) => {
            Err(gave_up(format!("Couldn't start this session: {err}")))
        }
        Err(err) => {
            deps.live.settle(&row.id);
            let reason = format!("Couldn't start this session: {err}");
            refuse(row.is_agent(), &row.id, &reason, false, deps);
            Err(reason)
        }
    }
}

/// Feed a session's pending person messages (`consumed_at` NULL, not yet
/// handed) into its live engine, oldest first; a session with NO engine here
/// (idle after a host restart, an agent whose process exited) is resumed
/// natively from its run record with the first message as the prompt. The
/// engine's consume worker calls `sessions.consume` as each one is sent.
/// Returns the delivered message ids.
///
/// `rows` may hold any messages; only this session's pending person rows
/// count. A resume takes the [`ClaimLock`] (another process holding it runs
/// the session: nothing is delivered), synthesizes the row's pickers from
/// the run record (use [`resume_session`] when the row is at hand) and
/// fetches a fresh `sessions.brief` for the system-append layers. A refused
/// resume is replied into the conversation, the row reported `idle`, and
/// not retried until a NEW message arrives.
pub fn deliver_messages(
    session_id: &str,
    rows: &[MessageRow],
    deps: &PickupDeps,
) -> Result<Vec<String>, String> {
    deliver(session_id, None, rows, deps)
}

/// [`deliver_messages`] for a [`PickupAction::Resume`]: the row is known, so
/// its CURRENT pickers (model, effort, computer use) drive the resume, and a
/// session with no run record left here (a purged record) starts afresh
/// from its brief, the messages delivered after it.
pub fn resume_session(
    row: &SessionRow,
    rows: &[MessageRow],
    deps: &PickupDeps,
) -> Result<Vec<String>, String> {
    deliver(&row.id, Some(row), rows, deps)
}

fn deliver(
    session_id: &str,
    row: Option<&SessionRow>,
    rows: &[MessageRow],
    deps: &PickupDeps,
) -> Result<Vec<String>, String> {
    let mut pending: Vec<&MessageRow> = rows
        .iter()
        .filter(|m| m.session_id == session_id && is_pending_input(m))
        .collect();
    pending.sort_by(|a, b| message_order(a, b));
    deps.live.settle_handed(rows);

    if let Some(engine) = deps.live.get(session_id) {
        if !engine.is_done() {
            let handed = deps.live.handed(session_id);
            pending.retain(|m| !handed.contains(&m.id));
            deps.live.take_owed(&mut pending, &[]);
            return Ok(hand(&*engine, session_id, &pending, deps));
        }
        // Its process exited (an agent idles out, a thread ended): wake it.
        deps.live.remove(session_id);
    }
    // EXP-1292: what the session's previous engine handed over but could
    // not report consumed before it ended (`engine::outbox`) is consumed —
    // never delivered again.
    let owed = crate::run_registry::get(&deps.coding.data_dir, session_id)
        .map(|record| record.pending_consumes())
        .unwrap_or_default();
    let covered = deps.live.take_owed(&mut pending, &owed);
    settle_owed(deps, session_id, covered.clone());
    let Some(first) = pending.first().copied() else {
        return Ok(covered);
    };
    if deps.live.refused_before(session_id, &pending) {
        return Ok(Vec::new());
    }
    let Some(claim) = ClaimLock::try_acquire(&deps.coding.data_dir, session_id)? else {
        log::debug!("pickup: session {session_id} runs in another process");
        return Ok(Vec::new());
    };
    let record = crate::run_registry::get(&deps.coding.data_dir, session_id);
    // EXP-849: only a row AT HAND can ask for a switch (one rebuilt from the
    // record names the record's own login).
    let row_account = row.map(|row| row.account.clone());
    let row = match (row, &record) {
        (Some(row), _) => row.clone(),
        (None, Some(record)) => row_from_record(record, &deps.device_id),
        (None, None) => {
            return Err(format!(
                "session {session_id} has no engine and no run record on this device"
            ))
        }
    };
    // A transient failure here leaves everything as it was: the next pass
    // tries again.
    let brief = session_device::brief(&deps.coding.trpc, session_id)
        .map_err(|err| format!("brief of session {session_id}: {err}"))?;
    let mut identifiers = (deps.issue_identifiers)(row.root_id_or_self());
    if identifiers.is_empty() {
        identifiers = brief_issue_identifiers(&brief.brief);
    }
    if identifiers.is_empty() {
        identifiers = record.as_ref().map(|r| r.issues.clone()).unwrap_or_default();
    }
    let mut resume = record.map(|record| ResumeRun {
        record,
        prompt: first.body.clone().unwrap_or_default(),
        message_id: Some(first.id.clone()),
    });
    let seeded = resume.is_some();
    let mut options = deps.options.clone();
    // On a resume a named account is a SWITCH (EXP-849); an ordinary wake
    // keeps the recorded login — unless the row's account moved past the
    // one the run last honored (a person switched while no engine ran).
    let honored = row_account_to_honor(row_account.as_ref(), resume.as_ref().map(|r| &r.record));
    if let (Some(resume), Some(account)) = (&mut resume, &row_account) {
        options.account = match wake_account(account.as_deref(), &resume.record, deps) {
            WakeAccount::Recorded => None,
            WakeAccount::Switch(target) => Some(target),
            // A switch this machine cannot make never bricks the session: it
            // wakes on its recorded login, the reason is said ONCE — the
            // record honors the row's pick from now on (persisted at once,
            // whatever the prepare below does).
            WakeAccount::Refused(reason) => {
                log::info!("pickup: account switch of {session_id} on wake refused: {reason}");
                if let Err(err) = session_device::reply(&deps.coding.trpc, session_id, &reason, None) {
                    log::warn!("pickup: switch refusal of {session_id} not replied: {err}");
                }
                honor_row_account(deps, session_id, &mut resume.record, account.as_deref());
                None
            }
        };
    }
    let req = SessionLaunchRequest {
        session: row.clone(),
        brief,
        issue_identifiers: identifiers,
        resume,
        device_label: deps.device_label.clone(),
        options,
    };
    let is_agent = row.is_agent();
    let refused = |reason: String| {
        refuse(is_agent, session_id, &reason, true, deps);
        deps.live.note_refused(session_id, &pending);
        reason
    };
    match crate::prepare(&PrepareRequest::Session(req), &deps.coding) {
        Ok(Prepared::Ready(prepared)) => {
            let engine = spawn(session_id, is_agent, prepared, claim, true, honored, deps)
                .inspect_err(|_| deps.live.note_refused(session_id, &pending))?;
            let mut delivered = covered;
            let mut rest = pending.clone();
            if seeded {
                // The seed turn IS the first message; the engine consumes it
                // via `PreparedLaunch::seed_message_id`.
                deps.live.mark_handed(session_id, std::slice::from_ref(&first.id));
                delivered.push(first.id.clone());
                rest.remove(0);
            }
            delivered.extend(hand(&*engine, session_id, &rest, deps));
            Ok(delivered)
        }
        Ok(Prepared::Disabled(reason)) => {
            refused(reason.message());
            Ok(Vec::new())
        }
        Err(err) => Err(refused(format!("Couldn't resume this session: {err}"))),
    }
}

/// Run one [`plan`] action against this host. Launch/Resume/Deliver block
/// (network + git) — call off the UI thread; Revoke/Answer/Kill only touch
/// the live engine.
pub fn execute(action: &PickupAction, deps: &PickupDeps) -> Result<(), String> {
    match action {
        PickupAction::Launch { session } => launch_queued(session, deps).map(|_| ()),
        PickupAction::Resume { session, message } => {
            resume_session(session, std::slice::from_ref(message), deps).map(|_| ())
        }
        PickupAction::Deliver { session_id, messages } => {
            deliver_messages(session_id, messages, deps).map(|_| ())
        }
        PickupAction::Revoke { session_id, message_id } => {
            if let Some(engine) = deps.live.get(session_id) {
                engine.unqueue(message_id);
            }
            deps.live.forget_handed(session_id, message_id);
            Ok(())
        }
        PickupAction::Answer { session_id, message_id, question_id, card } => {
            if let Some(engine) = deps.live.get(session_id) {
                engine.answer_card(question_id, card);
            }
            deps.live.mark_answered(session_id, message_id);
            Ok(())
        }
        PickupAction::Kill { session_id } => {
            if let Some(engine) = deps.live.remove(session_id) {
                engine.kill(KILL_OUTCOME);
            }
            Ok(())
        }
        PickupAction::Switch { session, account, messages } => {
            switch_session(session, account, None, messages, deps).map(|_| ()).map_err(|err| err.to_string())
        }
    }
}

/// EXP-849: the row account a (re)launched engine honors: the row's when
/// the row is at hand, else what its record last honored.
fn row_account_to_honor(
    row_account: Option<&Option<String>>,
    record: Option<&RunRecord>,
) -> Option<String> {
    match (row_account, record) {
        (Some(account), _) => Some(crate::agent_profiles::profile_id(account.as_deref())),
        (None, Some(record)) => Some(record.honored_row_account()),
        (None, None) => None,
    }
}

/// EXP-849: the account a WAKE of `record` runs on: `Some(target)` when the
/// row names a login the run never honored (a person's switch made while no
/// engine ran here), else `None` = the recorded login.
fn resume_switch(row_account: Option<&str>, record: &RunRecord) -> Option<String> {
    let wanted = crate::agent_profiles::profile_id(row_account);
    (!wanted.is_empty() && wanted != record.honored_row_account()).then_some(wanted)
}

/// EXP-849: what a wake does with the row's `account`.
#[derive(Debug, Clone, PartialEq, Eq)]
enum WakeAccount {
    /// The recorded login (nothing to switch, or the row names the login
    /// the run already runs on — the device's own report read back).
    Recorded,
    /// Resume on this login.
    Switch(String),
    /// A switch this machine cannot make ([`switch_refusal`]): wake on the
    /// recorded login and say why.
    Refused(String),
}

fn wake_account(row_account: Option<&str>, record: &RunRecord, deps: &PickupDeps) -> WakeAccount {
    let Some(target) = resume_switch(row_account, record) else {
        return WakeAccount::Recorded;
    };
    if record.account().as_deref() == Some(target.as_str()) {
        return WakeAccount::Recorded;
    }
    match switch_refusal(record, &target, deps) {
        Some(reason) => WakeAccount::Refused(reason),
        None => WakeAccount::Switch(target),
    }
}

/// EXP-849: stamp `row_account` as honored on `record` AND the persisted run
/// record, so a refused or no-op switch is never planned (or replied) again.
fn honor_row_account(deps: &PickupDeps, session_id: &str, record: &mut RunRecord, row_account: Option<&str>) {
    record.set_honored_row_account(row_account);
    crate::run_registry::update(&deps.coding.data_dir, session_id, |persisted| {
        persisted.set_honored_row_account(row_account);
        true
    });
}

/// EXP-849: tell the server the login the run ACTUALLY runs on when it is
/// not the one the row named (`honored`): a start pick or a rotation moved
/// it. The row then says it, so a person's later pick of the old login is
/// a real move; reading the report back moves nothing ([`switch_target`],
/// [`wake_account`]). Best-effort; an older server drops the key.
fn report_running_account(deps: &PickupDeps, session_id: &str, honored: Option<&str>, running: Option<&str>) {
    let (Some(honored), Some(running)) = (honored, running) else { return };
    if honored.is_empty() || running.is_empty() || honored == running {
        return;
    }
    let report = session_device::SessionReport {
        account: Some(running.to_string()),
        ..session_device::SessionReport::default()
    };
    if let Err(err) = session_device::report(&deps.coding.trpc, session_id, &report) {
        log::warn!("pickup: session {session_id}'s login {running} not reported: {err}");
    }
}

/// EXP-1292: consume `covered` (messages a previous engine handed over but
/// could not report consumed) off-thread; the record forgets them once the
/// consume landed.
fn settle_owed(deps: &PickupDeps, session_id: &str, covered: Vec<String>) {
    if covered.is_empty() {
        return;
    }
    let data_dir = deps.coding.data_dir.clone();
    let settled_session = session_id.to_string();
    let settled = covered.clone();
    consume_off_thread(deps, session_id, covered, move || {
        crate::run_registry::update(&data_dir, &settled_session, |record| {
            let left: Vec<String> = record
                .pending_consumes()
                .into_iter()
                .filter(|id| !settled.contains(id))
                .collect();
            record.set_pending_consumes(&left);
            true
        });
    });
}

/// Why [`switch_session`] did not move the session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitchError {
    /// A turn is in flight: nothing moved, try again between turns.
    Busy,
    /// The switch failed (no engine or record here, the run did not stop in
    /// time, a launch error).
    Failed(String),
}

impl std::fmt::Display for SwitchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // The ×4 sentence (`SessionAccountSwitch.REASON_BUSY`).
            Self::Busy => f.write_str("The agent is working — switching waits for the turn to finish."),
            Self::Failed(reason) => f.write_str(reason),
        }
    }
}

impl std::error::Error for SwitchError {}

/// What [`switch_session`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Switched {
    /// Suspended and resumed on the target login.
    Moved,
    /// The run already runs on the target login: nothing to do.
    AlreadyOn,
    /// The machine cannot honor it (codex, a login not set up here): the
    /// reason was replied into the conversation and the session keeps its
    /// login.
    Refused(String),
}

/// How long a suspended engine gets to stop before the switch gives up.
const SWITCH_STOP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// EXP-849/EXP-1005/EXP-1292 — THE account switch, for a person's
/// (`sessions.send{sessionId, account}` → [`PickupAction::Switch`]) and the
/// rotation's (`prompt` = its "moved this session" line): suspend the live,
/// IDLE engine of `row` (the row idles, its workspace — a scratch dir too —
/// is kept), then re-spawn the SAME row's agent natively from its run record
/// under `target`. The seed turn is `prompt`, else the oldest of `messages`'
/// pending person rows (consumed by the engine), else the launcher's
/// continue prompt; the rest are handed to the new engine.
pub fn switch_session(
    row: &SessionRow,
    target: &str,
    prompt: Option<&str>,
    messages: &[MessageRow],
    deps: &PickupDeps,
) -> Result<Switched, SwitchError> {
    let session_id = row.id.as_str();
    let target = crate::agent_profiles::profile_id(Some(target));
    let honored = Some(crate::agent_profiles::profile_id(row.account.as_deref()));
    let failed = |reason: String| SwitchError::Failed(reason);
    let engine = deps
        .live
        .get(session_id)
        .filter(|engine| !engine.is_done())
        .ok_or_else(|| failed(format!("session {session_id} is not running here any more")))?;
    let mut record = crate::run_registry::get(&deps.coding.data_dir, session_id)
        .ok_or_else(|| failed(format!("no local run record for session {session_id}")))?;
    let handed = deps.live.handed(session_id);
    let mut pending: Vec<&MessageRow> = messages
        .iter()
        .filter(|m| m.session_id == session_id && is_pending_input(m) && !handed.contains(&m.id))
        .collect();
    pending.sort_by(|a, b| message_order(a, b));
    // Nothing moves: the row's pick is honored (live entry AND record, so
    // the two agree) and its messages go to the engine as they are.
    let stay = |record: &mut RunRecord, pending: &[&MessageRow]| {
        deps.live.set_row_account(session_id, honored.clone());
        honor_row_account(deps, session_id, record, row.account.as_deref());
        hand(&*engine, session_id, pending, deps);
    };
    if target.is_empty() || record.account().as_deref() == Some(target.as_str()) {
        stay(&mut record, &pending);
        return Ok(Switched::AlreadyOn);
    }
    if let Some(reason) = switch_refusal(&record, &target, deps) {
        log::info!("pickup: account switch of {session_id} refused: {reason}");
        if let Err(err) = session_device::reply(&deps.coding.trpc, session_id, &reason, None) {
            log::warn!("pickup: switch refusal of {session_id} not replied: {err}");
        }
        stay(&mut record, &pending);
        // The row says the login the run keeps.
        report_running_account(deps, session_id, Some(target.as_str()), record.account().as_deref());
        return Ok(Switched::Refused(reason));
    }
    if !engine.is_idle() {
        return Err(SwitchError::Busy);
    }
    log::info!("pickup: account switch of {session_id} -> {target}: suspending the run");
    engine.suspend();
    if !engine.wait_done(SWITCH_STOP_TIMEOUT) {
        return Err(failed(format!(
            "account switch for {session_id} refused: the session did not stop in time"
        )));
    }
    // The old entry (and its claim lock) goes; the resume takes a fresh one.
    // What the old engine was handed stays handed — delivered once.
    deps.live.remove(session_id);
    // EXP-1292: the record as the suspended engine LEFT it (its exit may
    // have parked consumes it could not report): those messages are consumed
    // here, never delivered to the new engine again.
    if let Some(left) = crate::run_registry::get(&deps.coding.data_dir, session_id) {
        record = left;
    }
    let owed = record.pending_consumes();
    let mut covered = deps.live.take_owed(&mut pending, &owed);
    {
        let mut consuming = locked(&deps.live.consuming);
        let mut handed_owed: Vec<&String> =
            handed.iter().filter(|id| owed.contains(id) && consuming.insert((*id).clone())).collect();
        handed_owed.sort_unstable();
        covered.extend(handed_owed.into_iter().cloned());
    }
    // Consumed once this switch is through (or failed): never racing the
    // resume's own requests, never stranding an id marked consuming.
    let _settle = SettleOwed { deps, session_id, ids: covered };
    let refused = |reason: &str| {
        refuse(row.is_agent(), session_id, reason, true, deps);
        // Explained once: the same pending messages never re-post it.
        deps.live.note_refused(session_id, &pending);
    };
    let claim = ClaimLock::try_acquire(&deps.coding.data_dir, session_id)
        .map_err(failed)?
        .ok_or_else(|| failed(format!("session {session_id} is held by another process")))?;
    let brief = session_device::brief(&deps.coding.trpc, session_id)
        .map_err(|err| failed(format!("brief of session {session_id}: {err}")))?;
    let mut identifiers = (deps.issue_identifiers)(row.root_id_or_self());
    if identifiers.is_empty() {
        identifiers = brief_issue_identifiers(&brief.brief);
    }
    if identifiers.is_empty() {
        identifiers = record.issues.clone();
    }
    let seed = if prompt.is_some() { None } else { pending.first().copied() };
    let resume = ResumeRun {
        record,
        prompt: prompt.map(str::to_string).or_else(|| seed.and_then(|m| m.body.clone())).unwrap_or_default(),
        message_id: seed.map(|m| m.id.clone()),
    };
    let mut options = deps.options.clone();
    // EXP-849: a named account on a resume IS the switch.
    options.account = Some(target);
    let req = SessionLaunchRequest {
        session: row.clone(),
        brief,
        issue_identifiers: identifiers,
        resume: Some(resume),
        device_label: deps.device_label.clone(),
        options,
    };
    let is_agent = row.is_agent();
    match crate::prepare(&PrepareRequest::Session(req), &deps.coding) {
        Ok(Prepared::Ready(prepared)) => {
            let engine = spawn(session_id, is_agent, prepared, claim, true, honored, deps)
                .inspect_err(|_| deps.live.note_refused(session_id, &pending))
                .map_err(failed)?;
            deps.live.mark_handed(session_id, &handed.into_iter().collect::<Vec<_>>());
            let mut rest = pending;
            if let Some(seed) = seed {
                deps.live.mark_handed(session_id, std::slice::from_ref(&seed.id));
                rest.retain(|m| m.id != seed.id);
            }
            hand(&*engine, session_id, &rest, deps);
            let running = session_device::SessionReport {
                status: Some(session_device::ReportStatus::Running),
                ..session_device::SessionReport::default()
            };
            if let Err(err) = session_device::report(&deps.coding.trpc, session_id, &running) {
                log::warn!("pickup: switched session {session_id} not reported running: {err}");
            }
            Ok(Switched::Moved)
        }
        Ok(Prepared::Disabled(reason)) => {
            let reason = reason.message();
            refused(&reason);
            Ok(Switched::Refused(reason))
        }
        Err(err) => {
            let reason = format!("Couldn't resume this session: {err}");
            refused(&reason);
            Err(failed(reason))
        }
    }
}

/// [`settle_owed`] on drop.
struct SettleOwed<'a> {
    deps: &'a PickupDeps,
    session_id: &'a str,
    ids: Vec<String>,
}

impl Drop for SettleOwed<'_> {
    fn drop(&mut self) {
        settle_owed(self.deps, self.session_id, std::mem::take(&mut self.ids));
    }
}

/// EXP-849 — a switch the machine cannot honor, decided BEFORE the run is
/// touched: codex (its conversation lives in the login's own rollout
/// store) and a target login that is not set up here.
fn switch_refusal(record: &RunRecord, target: &str, deps: &PickupDeps) -> Option<String> {
    if record.agent != crate::agent::CodingAgent::Claude {
        // The ×4 sentence (`SessionAccountSwitch.REASON_AGENT`).
        return Some("Only claude can switch accounts during a session.".to_string());
    }
    crate::agent_profiles::account_dir(&deps.coding.data_dir, Some(record.agent), Some(target))
        .is_none()
        .then(|| format!("That account ({target}) is not set up on this machine."))
}

/// EXP-849: why a foreground `code`/`run` session (one engine, this
/// terminal's process) does not take a person's account pick right away.
pub const REASON_FOREGROUND_SWITCH: &str =
    "This session runs in a terminal; its account switches after that run ends.";

/// The outcome a [`PickupAction::Kill`] ends a run with. EXP-1292: the row
/// carries no `ended_by` any more, so an ended row reads as "stopped from
/// outside the process" and keeps FEED-63's WIP save of leftover work
/// (`engine::stopped_by_person`; a clean tree saves nothing).
pub const KILL_OUTCOME: &str = "killed";

/// A claim the server REFUSED (another device took it, the row ended, the
/// session limit) — skip the row; anything else is worth a retry.
fn claim_refused(status: u16) -> bool {
    (400..500).contains(&status) && status != 408 && status != 429
}

/// A refused claim that only means "not yours any more" (the row is gone,
/// or another device took it): skipped silently. Any other refusal (a
/// session limit, a forbidden device) is explained in the conversation.
fn claim_lost(status: u16) -> bool {
    status == 404 || status == 409
}

fn hand(
    engine: &dyn LiveEngine,
    session_id: &str,
    messages: &[&MessageRow],
    deps: &PickupDeps,
) -> Vec<String> {
    let ids: Vec<String> = messages.iter().map(|m| m.id.clone()).collect();
    for message in messages {
        engine.deliver(message.id.clone(), message.body.clone().unwrap_or_default());
    }
    deps.live.mark_handed(session_id, &ids);
    ids
}

/// `deps.launch` + the registry entry (holding `claim`); a failed spawn is
/// refused like a disabled prepare.
#[allow(clippy::too_many_arguments)]
fn spawn(
    session_id: &str,
    is_agent: bool,
    prepared: crate::PreparedLaunch,
    claim: ClaimLock,
    waking: bool,
    row_account: Option<String>,
    deps: &PickupDeps,
) -> Result<std::sync::Arc<dyn LiveEngine>, String> {
    match (deps.launch)(prepared) {
        Ok(engine) => {
            // EXP-849: the login the run landed on (the prepare recorded it).
            let running = crate::run_registry::get(&deps.coding.data_dir, session_id)
                .and_then(|record| record.account());
            deps.live.insert_for(
                session_id,
                std::sync::Arc::clone(&engine),
                Some(claim),
                row_account.clone(),
            );
            deps.live.set_running_account(session_id, running.clone());
            report_running_account(deps, session_id, row_account.as_deref(), running.as_deref());
            Ok(engine)
        }
        Err(err) => {
            let reason = format!("Couldn't start this session: {err}");
            refuse(is_agent, session_id, &reason, waking, deps);
            Err(reason)
        }
    }
}

/// Explain a refusal in the conversation (an agent message), then settle
/// the row: a refused LAUNCH ends a thread (an agent idles — agents never
/// end); a refused WAKE leaves the row `idle`, so a later message can try
/// again.
fn refuse(is_agent: bool, session_id: &str, reason: &str, waking: bool, deps: &PickupDeps) {
    log::warn!("pickup: session {session_id} refused: {reason}");
    if let Err(err) = session_device::reply(&deps.coding.trpc, session_id, reason, None) {
        log::warn!("pickup: refusal of session {session_id} not replied: {err}");
    }
    let settled = if is_agent || waking {
        crate::idle_session(&deps.coding.trpc, session_id)
    } else {
        crate::end_session(&deps.coding.trpc, session_id)
    };
    if let Err(err) = settled {
        log::warn!("pickup: refused session {session_id} not reported: {err}");
    }
}

/// A session row rebuilt from its run record, for a wake with no row at
/// hand: the recorded agent/model/effort/account/workspace (computer use is
/// not recorded, so it stays off).
fn row_from_record(record: &RunRecord, device_id: &str) -> SessionRow {
    let some = |value: &str| (!value.trim().is_empty()).then(|| value.to_string());
    SessionRow {
        id: record.session_id.clone(),
        team_id: some(&record.team_id),
        kind: Some(
            match record.kind {
                RunKind::Agent => SESSION_KIND_AGENT,
                RunKind::Thread => "thread",
            }
            .to_string(),
        ),
        device_id: Some(device_id.to_string()),
        cli: Some(record.agent.id().to_string()),
        account: record.account(),
        model: some(&record.model),
        effort: some(&record.effort),
        ultracode: record.ultracode,
        dispatch: record.is_dispatch(),
        repository_id: record.repository_id.clone(),
        branch: record.branch.clone(),
        status: Some(SESSION_STATUS_IDLE.to_string()),
        title: record.title.clone(),
        ..SessionRow::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::CodingAgent;
    use crate::test_support::{
        canned_server, canned_server_recording, make_deps, temp_dir, FakeWorktrees, REPORT_OK,
        RESOLVE_NONE,
    };
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    const DEV: &str = "dev-1";

    fn session(id: &str, status: &str, device: &str, created_at: &str) -> SessionRow {
        SessionRow {
            id: id.to_string(),
            team_id: Some("team-1".to_string()),
            kind: Some("thread".to_string()),
            device_id: Some(device.to_string()),
            cli: Some("claude".to_string()),
            status: Some(status.to_string()),
            title: Some("Code review".to_string()),
            created_at: Some(created_at.to_string()),
            ..SessionRow::default()
        }
    }

    fn agent(id: &str, status: &str) -> SessionRow {
        SessionRow {
            kind: Some(SESSION_KIND_AGENT.to_string()),
            name: Some("Triage".to_string()),
            ..session(id, status, DEV, "2026-10-01T00:00:00Z")
        }
    }

    fn message(id: &str, session_id: &str, created_at: &str) -> MessageRow {
        MessageRow {
            id: id.to_string(),
            session_id: session_id.to_string(),
            author_kind: Some("user".to_string()),
            body: Some(format!("body of {id}")),
            created_at: Some(created_at.to_string()),
            ..MessageRow::default()
        }
    }

    fn card(id: &str, session_id: &str, answered: bool) -> MessageRow {
        MessageRow {
            author_kind: Some("agent".to_string()),
            body: Some(String::new()),
            card: Some(serde_json::json!({
                "kind": "question", "question": "Which?", "options": ["a", "b"],
                "questionId": "q-1", "answer": "a",
            })),
            answered_at: answered.then(|| "2026-10-02T00:00:05Z".to_string()),
            ..message(id, session_id, "2026-10-02T00:00:00Z")
        }
    }

    fn consumed(mut row: MessageRow) -> MessageRow {
        row.consumed_at = Some("2026-10-02T00:00:09Z".to_string());
        row
    }

    struct Pass {
        sessions: Vec<SessionRow>,
        messages: Vec<MessageRow>,
        live: HashMap<String, LiveView>,
        in_flight: HashSet<String>,
    }

    impl Pass {
        fn new(sessions: Vec<SessionRow>, messages: Vec<MessageRow>) -> Self {
            Self { sessions, messages, live: HashMap::new(), in_flight: HashSet::new() }
        }

        fn live(mut self, id: &str, view: LiveView) -> Self {
            self.live.insert(id.to_string(), view);
            self
        }

        fn run(&self) -> Vec<PickupAction> {
            plan(PickupInput {
                device_id: DEV,
                sessions: &self.sessions,
                messages: &self.messages,
                live: &self.live,
                in_flight: &self.in_flight,
            })
        }
    }

    fn handed(ids: &[&str]) -> LiveView {
        LiveView { handed: ids.iter().map(|id| id.to_string()).collect(), ..LiveView::default() }
    }

    // ---- plan ----

    /// EXP-1329: a child's end, as the server posts it on the PARENT.
    fn report(id: &str, parent: &str, child: &str, summary: &str) -> MessageRow {
        MessageRow {
            author_kind: Some("agent".to_string()),
            author_session_id: Some(child.to_string()),
            body: Some(summary.to_string()),
            card: Some(serde_json::json!({"kind": "report", "sessionId": child, "summary": summary})),
            ..message(id, parent, "2026-10-02T00:00:05Z")
        }
    }

    #[test]
    fn pending_input_is_a_persons_message_or_another_sessions() {
        assert!(is_pending_input(&message("m1", "s1", "t")));
        assert!(!is_pending_input(&consumed(message("m1", "s1", "t"))));
        assert!(is_pending_input(&report("r1", "s1", "c1", "Done.")));
        assert!(!is_pending_input(&consumed(report("r1", "s1", "c1", "Done."))));
        // The session's own reply and its cards never wait.
        let reply = MessageRow { author_kind: Some("agent".to_string()), ..message("a1", "s1", "t") };
        assert!(!is_pending_input(&reply));
        assert!(!is_pending_input(&card("q1", "s1", false)));
        let own = MessageRow { author_session_id: Some("s1".to_string()), ..reply.clone() };
        assert!(!is_pending_input(&own));
    }

    #[test]
    fn a_report_reads_as_the_childs_summary_with_its_id() {
        let child = SessionRow { title: Some("Fix the cart".to_string()), ..session("c1", "ended", DEV, "t") };
        let sessions = vec![child];
        assert_eq!(
            input_text(&report("r1", "s1", "c1", "Opened PR #12."), &sessions),
            "Report from Fix the cart (session c1): Opened PR #12."
        );
        // An unsynced (or untitled) child is a Session; no summary = the body.
        let mut bare = report("r2", "s1", "c9", "");
        bare.body = Some("Ended without a summary.".to_string());
        assert_eq!(
            input_text(&bare, &sessions),
            "Report from Session (session c9): Ended without a summary."
        );
        let asked = MessageRow {
            card: None,
            body: Some("Which branch?".to_string()),
            ..report("q2", "s1", "c1", "")
        };
        assert_eq!(input_text(&asked, &sessions), "Message from Fix the cart (session c1): Which branch?");
        assert_eq!(input_text(&message("m1", "s1", "t"), &sessions), "body of m1");
    }

    #[test]
    fn a_report_wakes_an_idle_parent_and_reaches_a_live_one_like_a_message() {
        let parent = session("s1", "idle", DEV, "2026-10-01T00:00:00Z");
        let child = SessionRow {
            parent_id: Some("s1".to_string()),
            title: Some("Fix the cart".to_string()),
            ..session("c1", "ended", DEV, "2026-10-01T00:00:01Z")
        };
        let rows = vec![report("r1", "s1", "c1", "Opened PR #12.")];
        let expected = MessageRow {
            body: Some("Report from Fix the cart (session c1): Opened PR #12.".to_string()),
            ..rows[0].clone()
        };
        assert_eq!(
            Pass::new(vec![parent.clone(), child.clone()], rows.clone()).run(),
            vec![PickupAction::Resume { session: parent.clone(), message: expected.clone() }]
        );
        let running = session("s1", "running", DEV, "2026-10-01T00:00:00Z");
        assert_eq!(
            Pass::new(vec![running, child.clone()], rows.clone()).live("s1", LiveView::default()).run(),
            vec![PickupAction::Deliver { session_id: "s1".to_string(), messages: vec![expected] }]
        );
        // Consumed = delivered: nothing more.
        let done = vec![consumed(rows[0].clone())];
        assert_eq!(Pass::new(vec![parent, child], done).run(), vec![]);
    }

    #[test]
    fn a_queued_row_on_this_device_launches_and_nothing_else_does() {
        let mine = session("s1", "queued", DEV, "2026-10-01T00:00:00Z");
        let pass = Pass::new(
            vec![
                mine.clone(),
                session("s2", "queued", "dev-2", "2026-10-01T00:00:00Z"),
                session("s3", "ended", DEV, "2026-10-01T00:00:00Z"),
                session("s4", "running", DEV, "2026-10-01T00:00:00Z"),
                session("s5", "idle", DEV, "2026-10-01T00:00:00Z"),
            ],
            vec![],
        );
        assert_eq!(pass.run(), vec![PickupAction::Launch { session: mine }]);
    }

    #[test]
    fn in_flight_and_live_rows_are_not_launched_again() {
        let mut pass = Pass::new(
            vec![
                session("s1", "queued", DEV, "2026-10-01T00:00:00Z"),
                session("s2", "queued", DEV, "2026-10-01T00:00:00Z"),
            ],
            vec![message("m1", "s1", "2026-10-02T00:00:00Z")],
        )
        .live("s2", LiveView::default());
        pass.in_flight.insert("s1".to_string());
        assert_eq!(pass.run(), vec![]);
    }

    #[test]
    fn a_queued_agent_launches() {
        let row = agent("a1", "queued");
        assert_eq!(
            Pass::new(vec![row.clone()], vec![]).run(),
            vec![PickupAction::Launch { session: row }]
        );
    }

    #[test]
    fn an_engineless_running_or_idle_row_with_a_pending_message_resumes_on_the_oldest() {
        let thread = session("s1", "idle", DEV, "2026-10-01T00:00:00Z");
        let restarted = session("s2", "running", DEV, "2026-10-01T00:00:01Z");
        let sleeping = agent("a1", "idle");
        let pass = Pass::new(
            vec![
                thread.clone(),
                restarted.clone(),
                sleeping.clone(),
                session("s3", "idle", DEV, "2026-10-01T00:00:02Z"),
                session("s4", "ended", DEV, "2026-10-01T00:00:03Z"),
                session("s5", "idle", "dev-2", "2026-10-01T00:00:04Z"),
            ],
            vec![
                message("m2", "s1", "2026-10-02T00:00:02Z"),
                message("m1", "s1", "2026-10-02T00:00:01Z"),
                message("m3", "s2", "2026-10-02T00:00:00Z"),
                message("m4", "a1", "2026-10-02T00:00:00Z"),
                consumed(message("m5", "s3", "2026-10-02T00:00:00Z")),
                message("m6", "s4", "2026-10-02T00:00:00Z"),
                message("m7", "s5", "2026-10-02T00:00:00Z"),
            ],
        );
        assert_eq!(
            pass.run(),
            vec![
                PickupAction::Resume {
                    session: sleeping,
                    message: message("m4", "a1", "2026-10-02T00:00:00Z"),
                },
                PickupAction::Resume {
                    session: thread,
                    message: message("m1", "s1", "2026-10-02T00:00:01Z"),
                },
                PickupAction::Resume {
                    session: restarted,
                    message: message("m3", "s2", "2026-10-02T00:00:00Z"),
                },
            ]
        );
    }

    #[test]
    fn ended_and_gone_live_sessions_are_killed_and_get_nothing_else() {
        let pass = Pass::new(
            vec![session("s1", "ended", DEV, "2026-10-01T00:00:00Z")],
            vec![message("m1", "s1", "2026-10-02T00:00:00Z")],
        )
        .live("s1", handed(&["gone"]))
        .live("s2", LiveView::default());
        assert_eq!(
            pass.run(),
            vec![
                PickupAction::Kill { session_id: "s1".to_string() },
                PickupAction::Kill { session_id: "s2".to_string() },
            ]
        );
    }

    #[test]
    fn a_live_session_gets_its_unhanded_person_messages_oldest_first() {
        let pass = Pass::new(
            vec![session("s1", "running", DEV, "2026-10-01T00:00:00Z")],
            vec![
                message("m3", "s1", "2026-10-02T00:00:03Z"),
                message("m1", "s1", "2026-10-02T00:00:01Z"),
                message("m2", "s1", "2026-10-02T00:00:02Z"),
                consumed(message("m0", "s1", "2026-10-02T00:00:00Z")),
                MessageRow { author_kind: Some("agent".to_string()), ..message("r1", "s1", "2026-10-02T00:00:04Z") },
                card("c1", "s1", false),
                message("x1", "other", "2026-10-02T00:00:00Z"),
            ],
        )
        .live("s1", handed(&["m1"]));
        assert_eq!(
            pass.run(),
            vec![PickupAction::Deliver {
                session_id: "s1".to_string(),
                messages: vec![
                    message("m2", "s1", "2026-10-02T00:00:02Z"),
                    message("m3", "s1", "2026-10-02T00:00:03Z"),
                ],
            }]
        );
    }

    #[test]
    fn a_handed_message_whose_row_is_gone_is_revoked_a_consumed_one_is_not() {
        let pass = Pass::new(
            vec![session("s1", "running", DEV, "2026-10-01T00:00:00Z")],
            vec![consumed(message("m1", "s1", "2026-10-02T00:00:01Z"))],
        )
        .live("s1", handed(&["m1", "m2"]));
        assert_eq!(
            pass.run(),
            vec![PickupAction::Revoke { session_id: "s1".to_string(), message_id: "m2".to_string() }]
        );
    }

    /// EXP-1316: a relay answer finds the card posted for its question,
    /// once; another session's or an unknown question finds none.
    #[test]
    fn a_relay_answer_maps_onto_its_card_once() {
        let cards = HashMap::from([
            ("c1".to_string(), "q-1".to_string()),
            ("c2".to_string(), "q-2".to_string()),
        ]);
        assert_eq!(card_message_for(&cards, "q-2").as_deref(), Some("c2"));
        assert_eq!(card_message_for(&cards, "q-9"), None);

        let engine = Arc::new(FakeEngine::default());
        engine.cards.lock().unwrap().extend(cards);
        let live = LiveRegistry::default();
        live.insert("s1", engine, None);
        let input = live
            .resolve_card_input("s1", "q-1", None, Some("rejected".into()), Some("Smaller".into()))
            .expect("the card");
        assert_eq!(
            input,
            api::sessions::AnswerInput {
                message_id: "c1".into(),
                answer: None,
                decision: Some("rejected".into()),
                feedback: Some("Smaller".into()),
            }
        );
        assert!(live.resolve_card_input("s1", "q-1", Some("a".into()), None, None).is_none(), "once");
        assert!(live.resolve_card_input("s2", "q-2", Some("a".into()), None, None).is_none());
        assert!(!live.live_views()["s1"].open_cards.contains_key("c1"), "never re-fed");
    }

    #[test]
    fn only_answered_open_cards_are_answered() {
        let answered = card("c1", "s1", true);
        let view = LiveView {
            open_cards: HashMap::from([
                ("c1".to_string(), "q-1".to_string()),
                ("c2".to_string(), "q-2".to_string()),
                ("c-gone".to_string(), "q-3".to_string()),
            ]),
            ..LiveView::default()
        };
        let pass = Pass::new(
            vec![session("s1", "running", DEV, "2026-10-01T00:00:00Z")],
            // c2 is still open; c3 is answered but not one of ours.
            vec![answered.clone(), card("c2", "s1", false), card("c3", "s1", true)],
        )
        .live("s1", view);
        assert_eq!(
            pass.run(),
            vec![PickupAction::Answer {
                session_id: "s1".to_string(),
                message_id: "c1".to_string(),
                question_id: "q-1".to_string(),
                card: answered.card.unwrap(),
            }]
        );
    }

    #[test]
    fn a_pass_orders_kills_starts_revokes_deliveries_answers() {
        let late = session("q2", "queued", DEV, "2026-10-01T00:00:09Z");
        let early = session("q1", "queued", DEV, "2026-10-01T00:00:01Z");
        let idle = session("i1", "idle", DEV, "2026-10-01T00:00:05Z");
        let answered = card("c1", "l1", true);
        let pass = Pass::new(
            vec![
                late.clone(),
                session("l1", "running", DEV, "2026-10-01T00:00:00Z"),
                session("l2", "running", DEV, "2026-10-01T00:00:00Z"),
                idle.clone(),
                early.clone(),
            ],
            vec![
                message("m-l2", "l2", "2026-10-02T00:00:01Z"),
                message("m-l1", "l1", "2026-10-02T00:00:02Z"),
                message("m-i1", "i1", "2026-10-02T00:00:00Z"),
                answered.clone(),
            ],
        )
        .live(
            "l1",
            LiveView {
                handed: HashSet::from(["revoked".to_string()]),
                open_cards: HashMap::from([("c1".to_string(), "q-1".to_string())]),
                row_account: None,
                running_account: None,
            },
        )
        .live("l2", LiveView::default())
        .live("dead", LiveView::default());
        assert_eq!(
            pass.run(),
            vec![
                PickupAction::Kill { session_id: "dead".to_string() },
                PickupAction::Launch { session: early },
                PickupAction::Resume {
                    session: idle,
                    message: message("m-i1", "i1", "2026-10-02T00:00:00Z"),
                },
                PickupAction::Launch { session: late },
                PickupAction::Revoke { session_id: "l1".to_string(), message_id: "revoked".to_string() },
                PickupAction::Deliver {
                    session_id: "l2".to_string(),
                    messages: vec![message("m-l2", "l2", "2026-10-02T00:00:01Z")],
                },
                PickupAction::Deliver {
                    session_id: "l1".to_string(),
                    messages: vec![message("m-l1", "l1", "2026-10-02T00:00:02Z")],
                },
                PickupAction::Answer {
                    session_id: "l1".to_string(),
                    message_id: "c1".to_string(),
                    question_id: "q-1".to_string(),
                    card: answered.card.unwrap(),
                },
            ]
        );
    }

    // ---- ClaimLock + LiveRegistry ----

    #[test]
    fn a_claim_is_exclusive_until_released() {
        let dir = temp_dir("pickup-claim");
        let first = ClaimLock::try_acquire(&dir.0, "s-1").unwrap().expect("first claim");
        assert!(dir.0.join(CLAIMS_DIR).join("s-1.lock").is_dir());
        assert!(ClaimLock::try_acquire(&dir.0, "s-1").unwrap().is_none());
        assert!(ClaimLock::try_acquire(&dir.0, "s-2").unwrap().is_some());
        drop(first);
        assert!(ClaimLock::try_acquire(&dir.0, "s-1").unwrap().is_some());
        assert!(ClaimLock::try_acquire(&dir.0, "../escape").is_err());
        assert!(ClaimLock::try_acquire(&dir.0, "").is_err());
    }

    #[derive(Default)]
    struct FakeEngine {
        delivered: Mutex<Vec<(String, String)>>,
        unqueued: Mutex<Vec<String>>,
        answers: Mutex<Vec<(String, serde_json::Value)>>,
        cards: Mutex<HashMap<String, String>>,
        killed: Mutex<Option<&'static str>>,
        done: AtomicBool,
        /// A turn in flight (`is_idle` = false).
        working: AtomicBool,
    }

    impl FakeEngine {
        fn delivered_ids(&self) -> Vec<String> {
            self.delivered.lock().unwrap().iter().map(|(id, _)| id.clone()).collect()
        }
    }

    impl LiveEngine for FakeEngine {
        fn deliver(&self, message_id: String, text: String) {
            self.delivered.lock().unwrap().push((message_id, text));
        }
        fn unqueue(&self, message_id: &str) {
            self.unqueued.lock().unwrap().push(message_id.to_string());
        }
        fn answer_card(&self, question_id: &str, card: &serde_json::Value) {
            self.answers.lock().unwrap().push((question_id.to_string(), card.clone()));
        }
        fn open_cards(&self) -> HashMap<String, String> {
            self.cards.lock().unwrap().clone()
        }
        fn kill(&self, outcome: &'static str) {
            *self.killed.lock().unwrap() = Some(outcome);
            self.done.store(true, Ordering::SeqCst);
        }
        fn is_done(&self) -> bool {
            self.done.load(Ordering::SeqCst)
        }
        fn is_idle(&self) -> bool {
            !self.working.load(Ordering::SeqCst)
        }
        fn suspend(&self) {
            self.kill("suspended");
        }
    }

    #[test]
    fn the_registry_views_running_engines_and_prunes_done_ones() {
        let dir = temp_dir("pickup-registry");
        let registry = LiveRegistry::default();
        let running = Arc::new(FakeEngine::default());
        running.cards.lock().unwrap().insert("c1".to_string(), "q-1".to_string());
        running.cards.lock().unwrap().insert("c2".to_string(), "q-2".to_string());
        let done = Arc::new(FakeEngine::default());
        done.done.store(true, Ordering::SeqCst);
        registry.insert("s1", running.clone(), None);
        let claim = ClaimLock::try_acquire(&dir.0, "s2").unwrap();
        registry.insert("s2", done.clone(), claim);
        registry.mark_handed("s1", &["m1".to_string(), "m2".to_string()]);
        registry.mark_answered("s1", "c1");
        registry.settle_handed(&[consumed(message("m1", "s1", "t"))]);

        let views = registry.live_views();
        assert_eq!(views.keys().collect::<Vec<_>>(), vec!["s1"]);
        assert_eq!(views["s1"], LiveView {
            handed: HashSet::from(["m2".to_string()]),
            open_cards: HashMap::from([("c2".to_string(), "q-2".to_string())]),
            row_account: None,
            running_account: None,
        });
        assert!(registry.contains_live("s1"));
        assert!(!registry.contains_live("s2"));
        assert_eq!(registry.prune(), vec!["s2".to_string()]);
        assert_eq!(registry.session_ids(), vec!["s1".to_string()]);
        // The pruned entry's claim went with it.
        assert!(ClaimLock::try_acquire(&dir.0, "s2").unwrap().is_some());
    }

    // ---- launch_queued / deliver_messages over the wire ----

    const CLAIM_OK: &str = r#"{"result":{"data":{"txid":"1"}}}"#;
    const BRIEF_OK: &str = r#"{"result":{"data":{"brief":"Review the login page.","teamPrompt":null,"agentInstructions":null,"parentBranch":null}}}"#;
    const REPLY_OK: &str = r#"{"result":{"data":{"messageId":"m-reply","txid":"2"}}}"#;

    /// What the fake host's spawn saw.
    #[derive(Debug, Clone, PartialEq)]
    struct Spawned {
        session_id: String,
        prompt: Option<String>,
        seed_message_id: Option<String>,
    }

    struct Host {
        deps: PickupDeps,
        engine: Arc<FakeEngine>,
        spawned: Arc<Mutex<Vec<Spawned>>>,
    }

    fn host(base: &str, data_dir: &Path, tweak: impl FnOnce(&mut crate::CodingDeps)) -> Host {
        let mut coding = make_deps(
            base,
            data_dir,
            Arc::new(FakeWorktrees { worktree: data_dir.join("unused"), seen: Default::default() }),
        );
        // No claude transcripts anywhere: a resume re-seeds.
        coding.claude_projects_root = Some(data_dir.join("empty-projects"));
        tweak(&mut coding);
        let options = crate::LaunchOptions::defaults_for(&coding.settings, CodingAgent::Claude);
        let engine = Arc::new(FakeEngine::default());
        let spawned = Arc::new(Mutex::new(Vec::new()));
        let (launch_engine, launch_seen) = (Arc::clone(&engine), Arc::clone(&spawned));
        let launch: LaunchFn = Arc::new(move |prepared: crate::PreparedLaunch| {
            launch_seen.lock().unwrap().push(Spawned {
                session_id: prepared.session_id.clone(),
                prompt: prepared.acp.prompt.clone(),
                seed_message_id: prepared.seed_message_id.clone(),
            });
            Ok(Arc::clone(&launch_engine) as Arc<dyn LiveEngine>)
        });
        Host {
            deps: PickupDeps {
                coding: Arc::new(coding),
                device_id: DEV.to_string(),
                device_label: "testbox".to_string(),
                options,
                live: Arc::new(LiveRegistry::default()),
                launch,
                issue_identifiers: Arc::new(|_root: &str| Vec::new()),
            },
            engine,
            spawned,
        }
    }

    fn procedures(requests: &Mutex<Vec<String>>) -> Vec<String> {
        requests
            .lock()
            .unwrap()
            .iter()
            .map(|request| {
                let path = request.split_whitespace().nth(1).unwrap_or_default();
                let path = path.split('?').next().unwrap_or_default();
                path.trim_start_matches("/api/trpc/").to_string()
            })
            .collect()
    }

    #[test]
    fn a_queued_repo_less_row_is_claimed_briefed_prepared_and_registered() {
        let dir = temp_dir("pickup-launch");
        let (base, requests) = canned_server_recording(vec![
            (200, CLAIM_OK.to_string()),
            (200, BRIEF_OK.to_string()),
            (200, RESOLVE_NONE.to_string()),
        ]);
        let host = host(&base, &dir.0, |_| {});
        let row = session("sess-1", "queued", DEV, "2026-10-01T00:00:00Z");

        let outcome = launch_queued(&row, &host.deps).unwrap();
        assert!(matches!(outcome, LaunchQueued::Launched { ref session_id } if session_id == "sess-1"));
        assert_eq!(
            procedures(&requests),
            vec!["sessions.claim", "sessions.brief", "mcpServers.resolveForLaunch"]
        );
        let claim = requests.lock().unwrap()[0].clone();
        assert!(claim.ends_with(r#"{"sessionId":"sess-1"}"#), "{claim}");
        assert_eq!(
            *host.spawned.lock().unwrap(),
            vec![Spawned {
                session_id: "sess-1".to_string(),
                prompt: Some("Review the login page.".to_string()),
                seed_message_id: None,
            }]
        );
        assert!(host.deps.live.contains_live("sess-1"));
        // The registry entry holds the claim.
        assert!(ClaimLock::try_acquire(&host.deps.coding.data_dir, "sess-1").unwrap().is_none());
        // A second pass skips it without a request.
        let again = launch_queued(&row, &host.deps).unwrap();
        assert!(matches!(again, LaunchQueued::Skipped { reason: "already live" }));
        assert_eq!(requests.lock().unwrap().len(), 3);
    }

    #[test]
    fn a_row_another_process_or_device_holds_is_skipped_without_a_request() {
        let dir = temp_dir("pickup-skip");
        let (base, requests) = canned_server_recording(vec![]);
        let host = host(&base, &dir.0, |_| {});
        let _held = ClaimLock::try_acquire(&dir.0, "sess-1").unwrap().unwrap();
        let row = session("sess-1", "queued", DEV, "t");
        assert!(matches!(
            launch_queued(&row, &host.deps).unwrap(),
            LaunchQueued::Skipped { reason: "claimed by another process" }
        ));
        let elsewhere = session("sess-2", "queued", "dev-2", "t");
        assert!(matches!(
            launch_queued(&elsewhere, &host.deps).unwrap(),
            LaunchQueued::Skipped { reason: "another device" }
        ));
        let running = session("sess-3", "running", DEV, "t");
        assert!(matches!(
            launch_queued(&running, &host.deps).unwrap(),
            LaunchQueued::Skipped { reason: "not queued" }
        ));
        assert!(requests.lock().unwrap().is_empty());
    }

    #[test]
    fn a_refused_claim_skips_the_row_and_frees_the_lock() {
        let dir = temp_dir("pickup-claim-refused");
        let (base, requests) = canned_server_recording(vec![(
            409,
            r#"{"error":{"message":"Session already claimed","code":-32009,"data":{"code":"CONFLICT","httpStatus":409}}}"#
                .to_string(),
        )]);
        let host = host(&base, &dir.0, |_| {});
        let row = session("sess-1", "queued", DEV, "t");
        assert!(matches!(
            launch_queued(&row, &host.deps).unwrap(),
            LaunchQueued::Skipped { reason: "claim refused" }
        ));
        assert_eq!(procedures(&requests), vec!["sessions.claim"]);
        assert!(host.spawned.lock().unwrap().is_empty());
        assert!(ClaimLock::try_acquire(&dir.0, "sess-1").unwrap().is_some());
    }

    #[test]
    fn a_disabled_launch_replies_the_reason_and_ends_the_thread() {
        let dir = temp_dir("pickup-disabled");
        let (base, requests) = canned_server_recording(vec![
            (200, CLAIM_OK.to_string()),
            (200, BRIEF_OK.to_string()),
            (200, REPLY_OK.to_string()),
            (200, REPORT_OK.to_string()),
        ]);
        let host = host(&base, &dir.0, |coding| coding.acp_available = false);
        let row = session("sess-1", "queued", DEV, "t");

        let LaunchQueued::Disabled { reason } = launch_queued(&row, &host.deps).unwrap() else {
            panic!("expected a refusal");
        };
        assert!(reason.contains("cannot run a session"), "{reason}");
        assert_eq!(
            procedures(&requests),
            vec!["sessions.claim", "sessions.brief", "sessions.reply", "sessions.report"]
        );
        let requests = requests.lock().unwrap();
        assert!(requests[2].contains(&reason), "{}", requests[2]);
        assert!(requests[3].ends_with(r#"{"sessionId":"sess-1","status":"ended"}"#), "{}", requests[3]);
        drop(requests);
        assert!(host.spawned.lock().unwrap().is_empty());
        assert!(!host.deps.live.contains_live("sess-1"));
    }

    #[test]
    fn a_disabled_agent_launch_idles_the_agent() {
        let dir = temp_dir("pickup-disabled-agent");
        let (base, requests) = canned_server_recording(vec![
            (200, CLAIM_OK.to_string()),
            (200, BRIEF_OK.to_string()),
            (200, REPLY_OK.to_string()),
            (200, REPORT_OK.to_string()),
        ]);
        let host = host(&base, &dir.0, |coding| coding.acp_available = false);
        let row = agent("agent-1", "queued");
        assert!(matches!(launch_queued(&row, &host.deps).unwrap(), LaunchQueued::Disabled { .. }));
        let requests = requests.lock().unwrap();
        assert!(requests[3].ends_with(r#"{"sessionId":"agent-1","status":"idle"}"#), "{}", requests[3]);
    }

    #[test]
    fn messages_to_a_live_session_are_handed_once_oldest_first() {
        let dir = temp_dir("pickup-deliver");
        let host = host(&canned_server(vec![]), &dir.0, |_| {});
        host.deps.live.insert("s1", host.engine.clone(), None);
        let rows = vec![
            message("m2", "s1", "2026-10-02T00:00:02Z"),
            message("m1", "s1", "2026-10-02T00:00:01Z"),
            consumed(message("m0", "s1", "2026-10-02T00:00:00Z")),
            message("x1", "s9", "2026-10-02T00:00:00Z"),
        ];
        assert_eq!(deliver_messages("s1", &rows, &host.deps).unwrap(), vec!["m1", "m2"]);
        assert_eq!(
            *host.engine.delivered.lock().unwrap(),
            vec![
                ("m1".to_string(), "body of m1".to_string()),
                ("m2".to_string(), "body of m2".to_string()),
            ]
        );
        assert!(deliver_messages("s1", &rows, &host.deps).unwrap().is_empty());
        assert_eq!(host.engine.delivered_ids(), vec!["m1", "m2"]);
        assert_eq!(host.deps.live.live_views()["s1"].handed.len(), 2);
    }

    fn record(data_dir: &Path, session_id: &str) -> RunRecord {
        let cwd = data_dir.join("scratch").join(session_id);
        std::fs::create_dir_all(&cwd).unwrap();
        RunRecord {
            session_id: session_id.to_string(),
            account_id: "acct".to_string(),
            agent: CodingAgent::Claude,
            kind: RunKind::Thread,
            team_id: "team-1".to_string(),
            issues: vec!["EXP-42".to_string()],
            title: None,
            cwd,
            clone: None,
            repo: None,
            repository_id: None,
            branch: None,
            base_branch: None,
            claude_session_id: None,
            codex_originator: None,
            model: String::new(),
            effort: String::new(),
            ultracode: false,
            transport: Some(crate::launcher::ACP_TRANSPORT.to_string()),
            acp_session_id: None,
            agent_native_session_id: None,
            acp_child_pid: None,
            host_pid: None,
            recorded_at: crate::run_registry::now_secs(),
            extra: Default::default(),
        }
    }

    #[test]
    fn a_message_to_an_engineless_session_resumes_it_from_its_record() {
        let dir = temp_dir("pickup-resume");
        let (base, requests) = canned_server_recording(vec![
            (200, BRIEF_OK.to_string()),
            (200, RESOLVE_NONE.to_string()),
        ]);
        let host = host(&base, &dir.0, |_| {});
        crate::run_registry::record(&dir.0, record(&dir.0, "sess-1"));
        let rows = vec![
            message("m2", "sess-1", "2026-10-02T00:00:02Z"),
            message("m1", "sess-1", "2026-10-02T00:00:01Z"),
        ];

        assert_eq!(deliver_messages("sess-1", &rows, &host.deps).unwrap(), vec!["m1", "m2"]);
        assert_eq!(procedures(&requests), vec!["sessions.brief", "mcpServers.resolveForLaunch"]);
        let spawned = host.spawned.lock().unwrap().clone();
        assert_eq!(spawned.len(), 1);
        assert_eq!(spawned[0].session_id, "sess-1", "the SAME row");
        assert_eq!(spawned[0].seed_message_id.as_deref(), Some("m1"));
        let prompt = spawned[0].prompt.clone().unwrap();
        assert!(prompt.ends_with("body of m1"), "{prompt}");
        // The seed turn IS m1; m2 rides the live engine.
        assert_eq!(host.engine.delivered_ids(), vec!["m2"]);
        assert!(host.deps.live.contains_live("sess-1"));
        assert_eq!(
            host.deps.live.live_views()["sess-1"].handed,
            HashSet::from(["m1".to_string(), "m2".to_string()])
        );
    }

    #[test]
    fn a_done_engine_is_replaced_by_a_resume() {
        let dir = temp_dir("pickup-resume-done");
        let base = canned_server(vec![(200, BRIEF_OK.to_string()), (200, RESOLVE_NONE.to_string())]);
        let host = host(&base, &dir.0, |_| {});
        crate::run_registry::record(&dir.0, record(&dir.0, "sess-1"));
        let exited = Arc::new(FakeEngine::default());
        exited.done.store(true, Ordering::SeqCst);
        host.deps.live.insert("sess-1", exited.clone(), None);
        let rows = vec![message("m1", "sess-1", "t")];
        assert_eq!(deliver_messages("sess-1", &rows, &host.deps).unwrap(), vec!["m1"]);
        assert!(exited.delivered_ids().is_empty());
        assert_eq!(host.spawned.lock().unwrap().len(), 1);
    }

    #[test]
    fn a_known_row_with_no_record_starts_afresh_and_delivers_after_the_brief() {
        let dir = temp_dir("pickup-resume-fresh");
        let base = canned_server(vec![(200, BRIEF_OK.to_string()), (200, RESOLVE_NONE.to_string())]);
        let host = host(&base, &dir.0, |_| {});
        let row = session("sess-1", "idle", DEV, "t");
        let rows = vec![message("m1", "sess-1", "t")];
        assert!(deliver_messages("sess-1", &rows, &host.deps).is_err(), "no row, no record");
        assert_eq!(resume_session(&row, &rows, &host.deps).unwrap(), vec!["m1"]);
        let spawned = host.spawned.lock().unwrap().clone();
        assert_eq!(spawned[0].prompt.as_deref(), Some("Review the login page."));
        assert_eq!(spawned[0].seed_message_id, None);
        assert_eq!(host.engine.delivered_ids(), vec!["m1"]);
    }

    #[test]
    fn a_refused_wake_is_explained_once_and_retried_on_a_new_message() {
        let dir = temp_dir("pickup-resume-refused");
        let (base, requests) = canned_server_recording(vec![
            (200, BRIEF_OK.to_string()),
            (200, REPLY_OK.to_string()),
            (200, REPORT_OK.to_string()),
        ]);
        let host = host(&base, &dir.0, |coding| coding.acp_available = false);
        crate::run_registry::record(&dir.0, record(&dir.0, "sess-1"));
        let rows = vec![message("m1", "sess-1", "t")];
        assert!(deliver_messages("sess-1", &rows, &host.deps).unwrap().is_empty());
        assert!(deliver_messages("sess-1", &rows, &host.deps).unwrap().is_empty());
        assert_eq!(
            procedures(&requests),
            vec!["sessions.brief", "sessions.reply", "sessions.report"]
        );
        let report = requests.lock().unwrap()[2].clone();
        assert!(report.ends_with(r#"{"sessionId":"sess-1","status":"idle"}"#), "a wake never ends: {report}");
        // The claim was released with the refusal.
        assert!(ClaimLock::try_acquire(&dir.0, "sess-1").unwrap().is_some());
        // A NEW message tries again (the canned server is spent, so the
        // brief fails — proof a request went out).
        let more = vec![message("m1", "sess-1", "t"), message("m2", "sess-1", "u")];
        assert!(deliver_messages("sess-1", &more, &host.deps).is_err());
    }

    #[test]
    fn execute_revokes_answers_and_kills_on_the_live_engine() {
        let dir = temp_dir("pickup-execute");
        let host = host(&canned_server(vec![]), &dir.0, |_| {});
        host.engine.cards.lock().unwrap().insert("c1".to_string(), "q-1".to_string());
        let claim = ClaimLock::try_acquire(&dir.0, "s1").unwrap();
        host.deps.live.insert("s1", host.engine.clone(), claim);
        host.deps.live.mark_handed("s1", &["m1".to_string()]);

        execute(
            &PickupAction::Revoke { session_id: "s1".to_string(), message_id: "m1".to_string() },
            &host.deps,
        )
        .unwrap();
        assert_eq!(*host.engine.unqueued.lock().unwrap(), vec!["m1"]);
        assert!(host.deps.live.live_views()["s1"].handed.is_empty());

        let answer = serde_json::json!({"kind": "question", "questionId": "q-1", "answer": "a"});
        execute(
            &PickupAction::Answer {
                session_id: "s1".to_string(),
                message_id: "c1".to_string(),
                question_id: "q-1".to_string(),
                card: answer.clone(),
            },
            &host.deps,
        )
        .unwrap();
        assert_eq!(*host.engine.answers.lock().unwrap(), vec![("q-1".to_string(), answer)]);
        assert!(host.deps.live.live_views()["s1"].open_cards.is_empty(), "answered once");

        execute(&PickupAction::Kill { session_id: "s1".to_string() }, &host.deps).unwrap();
        assert_eq!(*host.engine.killed.lock().unwrap(), Some(KILL_OUTCOME));
        assert!(host.deps.live.session_ids().is_empty());
        assert!(ClaimLock::try_acquire(&dir.0, "s1").unwrap().is_some());
    }

    // ---- EXP-1292 fix pass: start messages, transient failures, refusals ----

    const CONSUME_OK: &str = r#"{"result":{"data":{"txid":"3"}}}"#;
    const BRIEF_REQUEST: &str = r###"{"result":{"data":{"brief":"## Request\n\nMake login faster.\n\n## Issues\n\n- EXP-12 Login is slow","teamPrompt":null,"agentInstructions":null,"parentBranch":null}}}"###;

    fn wait_for(requests: &Mutex<Vec<String>>, procedure: &str) -> bool {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            if procedures(requests).iter().any(|p| p == procedure) {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        false
    }

    /// EXP-1292: `sessions.claim` consumes the start's own message (the
    /// brief's Request carries it), so a fresh launch just delivers what is
    /// still pending — no device-side matching, no consume of its own.
    #[test]
    fn a_fresh_launch_delivers_what_the_claim_left_pending() {
        let dir = temp_dir("pickup-seed");
        let (base, requests) = canned_server_recording(vec![
            (200, CLAIM_OK.to_string()),
            (200, BRIEF_REQUEST.to_string()),
            (200, RESOLVE_NONE.to_string()),
        ]);
        let host = host(&base, &dir.0, |_| {});
        let row = session("sess-1", "queued", DEV, "2026-10-01T00:00:00Z");
        assert!(matches!(launch_queued(&row, &host.deps).unwrap(), LaunchQueued::Launched { .. }));
        // The brief named the issue before the shapes did.
        let record = crate::run_registry::get(&dir.0, "sess-1").expect("record");
        assert_eq!(record.issues, vec!["EXP-12".to_string()]);

        let start = MessageRow {
            body: Some("Make login faster.".to_string()),
            ..consumed(message("m1", "sess-1", "2026-10-02T00:00:01Z"))
        };
        let rows = vec![start, message("m2", "sess-1", "2026-10-02T00:00:02Z")];
        assert_eq!(deliver_messages("sess-1", &rows, &host.deps).unwrap(), vec!["m2"]);
        assert_eq!(host.engine.delivered_ids(), vec!["m2"]);
        assert!(!procedures(&requests).iter().any(|p| p == "sessions.consume"));
        // Handed: the next pass delivers nothing again.
        assert!(deliver_messages("sess-1", &rows, &host.deps).unwrap().is_empty());
    }

    /// A transient brief failure after the claim never ends the thread: the
    /// row stays this host's to retry even though it reads `running`.
    #[test]
    fn a_transient_failure_after_the_claim_is_retried_not_ended() {
        let dir = temp_dir("pickup-transient");
        let (base, requests) = canned_server_recording(vec![
            (200, CLAIM_OK.to_string()),
            (502, r#"{"error":{"message":"bad gateway"}}"#.to_string()),
            (200, CLAIM_OK.to_string()),
            (200, BRIEF_OK.to_string()),
            (200, RESOLVE_NONE.to_string()),
        ]);
        let host = host(&base, &dir.0, |_| {});
        let row = session("sess-1", "queued", DEV, "t");
        assert!(launch_queued(&row, &host.deps).is_err());
        assert_eq!(procedures(&requests), vec!["sessions.claim", "sessions.brief"]);

        let running = session("sess-1", "running", DEV, "t");
        let retries = host.deps.live.retry_launches(DEV, std::slice::from_ref(&running), &HashSet::new());
        assert_eq!(retries, vec![PickupAction::Launch { session: running.clone() }]);
        assert!(matches!(launch_queued(&running, &host.deps).unwrap(), LaunchQueued::Launched { .. }));
        assert!(host
            .deps
            .live
            .retry_launches(DEV, std::slice::from_ref(&running), &HashSet::new())
            .is_empty());
        // An ordinary running row was never this host's to relaunch.
        let other = session("sess-2", "running", DEV, "t");
        assert!(host.deps.live.retry_launches(DEV, &[other], &HashSet::new()).is_empty());
    }

    /// A refusal the person must hear (a session limit) is replied once and
    /// not re-claimed until the row changes.
    #[test]
    fn a_claim_refused_for_a_limit_is_explained_once() {
        let dir = temp_dir("pickup-claim-limit");
        let (base, requests) = canned_server_recording(vec![
            (
                412,
                r#"{"error":{"message":"Session limit reached","code":-32012,"data":{"code":"PRECONDITION_FAILED","httpStatus":412}}}"#
                    .to_string(),
            ),
            (200, REPLY_OK.to_string()),
        ]);
        let host = host(&base, &dir.0, |_| {});
        let row = session("sess-1", "queued", DEV, "t");
        for _ in 0..2 {
            assert!(matches!(
                launch_queued(&row, &host.deps).unwrap(),
                LaunchQueued::Skipped { reason: "claim refused" }
            ));
        }
        assert_eq!(procedures(&requests), vec!["sessions.claim", "sessions.reply"]);
        assert!(requests.lock().unwrap()[1].contains("Session limit reached"));
    }

    /// A message the previous engine handed over but could not report
    /// consumed before it ended is consumed on the next wake, never handed
    /// to the agent twice; the record forgets it once the consume landed.
    #[test]
    fn an_owed_consume_is_settled_not_redelivered() {
        let dir = temp_dir("pickup-owed");
        let (base, requests) = canned_server_recording(vec![
            (200, CONSUME_OK.to_string()),
            (200, BRIEF_OK.to_string()),
            (200, RESOLVE_NONE.to_string()),
        ]);
        let host = host(&base, &dir.0, |_| {});
        let mut owed = record(&dir.0, "sess-1");
        owed.set_pending_consumes(&["m1".to_string()]);
        crate::run_registry::record(&dir.0, owed);

        let only_owed = vec![message("m1", "sess-1", "2026-10-02T00:00:01Z")];
        assert_eq!(deliver_messages("sess-1", &only_owed, &host.deps).unwrap(), vec!["m1"]);
        assert!(host.spawned.lock().unwrap().is_empty(), "nothing to wake for");
        assert!(wait_for(&requests, "sessions.consume"));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while crate::run_registry::get(&dir.0, "sess-1").is_some_and(|r| !r.pending_consumes().is_empty())
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(crate::run_registry::get(&dir.0, "sess-1").unwrap().pending_consumes().is_empty());

        // A newer message wakes the session with ITS text.
        let rows = vec![
            message("m1", "sess-1", "2026-10-02T00:00:01Z"),
            message("m2", "sess-1", "2026-10-02T00:00:02Z"),
        ];
        assert_eq!(deliver_messages("sess-1", &rows, &host.deps).unwrap(), vec!["m2"]);
        assert_eq!(host.spawned.lock().unwrap()[0].seed_message_id.as_deref(), Some("m2"));
    }

    // ---- EXP-849: the account switch ----

    fn on_account(mut row: SessionRow, account: &str) -> SessionRow {
        row.account = Some(account.to_string());
        row
    }

    fn honoring(account: &str) -> LiveView {
        LiveView { row_account: Some(account.to_string()), ..LiveView::default() }
    }

    /// A live session whose row's account moved past the one its engine
    /// honors switches — its pending messages ride the switch instead of a
    /// delivery. The same account, no account and an unknown honor do not.
    #[test]
    fn a_row_whose_account_moved_plans_a_switch_instead_of_deliveries() {
        let moved = on_account(session("s1", "idle", DEV, "2026-10-01T00:00:00Z"), "bbbb");
        let same = on_account(session("s2", "idle", DEV, "2026-10-01T00:00:01Z"), "aaaa");
        let cleared = session("s3", "idle", DEV, "2026-10-01T00:00:02Z");
        let unknown = on_account(session("s4", "idle", DEV, "2026-10-01T00:00:03Z"), "bbbb");
        let pending = message("m1", "s1", "2026-10-02T00:00:00Z");
        let pass = Pass::new(
            vec![moved.clone(), same, cleared, unknown],
            vec![pending.clone(), message("m2", "s2", "2026-10-02T00:00:00Z")],
        )
        .live("s1", honoring("aaaa"))
        .live("s2", honoring("aaaa"))
        .live("s3", honoring("aaaa"))
        .live("s4", LiveView::default());
        assert_eq!(
            pass.run(),
            vec![
                PickupAction::Switch {
                    session: moved,
                    account: "bbbb".to_string(),
                    messages: vec![pending],
                },
                PickupAction::Deliver {
                    session_id: "s2".to_string(),
                    messages: vec![message("m2", "s2", "2026-10-02T00:00:00Z")],
                },
            ]
        );
    }

    /// A fresh launch honors the row's account: the next pass plans
    /// nothing for it, a later move of the row plans the switch.
    #[test]
    fn a_launch_honors_the_rows_account() {
        let dir = temp_dir("pickup-honor");
        let base = canned_server(vec![
            (200, CLAIM_OK.to_string()),
            (200, BRIEF_OK.to_string()),
            (200, RESOLVE_NONE.to_string()),
        ]);
        let host = host(&base, &dir.0, |_| {});
        let row = on_account(session("sess-1", "queued", DEV, "t"), "aaaa");
        assert!(matches!(launch_queued(&row, &host.deps).unwrap(), LaunchQueued::Launched { .. }));
        let views = host.deps.live.live_views();
        assert_eq!(views["sess-1"].row_account.as_deref(), Some("aaaa"));
        let record = crate::run_registry::get(&dir.0, "sess-1").expect("record");
        assert_eq!(record.honored_row_account(), "aaaa");
        // The live entry knows the login the run landed on.
        assert_eq!(views["sess-1"].running_account, record.account());
        let running = on_account(session("sess-1", "running", DEV, "t"), "aaaa");
        let pass = |row: SessionRow, messages: &[MessageRow]| {
            plan(PickupInput {
                device_id: DEV,
                sessions: std::slice::from_ref(&row),
                messages,
                live: &views,
                in_flight: &HashSet::new(),
            })
        };
        assert!(pass(running.clone(), &[]).is_empty());
        let moved = on_account(running, "bbbb");
        // An account-only pick of an idle session waits for a turn to carry
        // it: the next message …
        assert!(pass(moved.clone(), &[]).is_empty());
        assert!(matches!(
            pass(moved.clone(), &[message("m1", "sess-1", "t")]).as_slice(),
            [PickupAction::Switch { account, messages, .. }] if account == "bbbb" && messages.len() == 1
        ));
        // … or the usage wall it continues past.
        let walled = SessionRow { blocked: Some(serde_json::json!({"kind": "rate_limit"})), ..moved.clone() };
        assert!(matches!(
            pass(walled, &[]).as_slice(),
            [PickupAction::Switch { account, messages, .. }] if account == "bbbb" && messages.is_empty()
        ));
        let cleared = SessionRow { blocked: Some(serde_json::Value::Null), ..moved };
        assert!(pass(cleared, &[]).is_empty());
    }

    /// MEDIUM 2: the row coming back naming the login the engine already
    /// runs on (the device's own `sessions.report{account}` after a start
    /// pick or a rotation) is planned at once — the switch finds it
    /// AlreadyOn and only honors it, so a later pick of the old login is
    /// a real move; the stale row before the report lands moves nothing.
    #[test]
    fn the_devices_own_account_report_is_honored_not_moved() {
        let view = LiveView {
            row_account: Some("aaaa".to_string()),
            running_account: Some("bbbb".to_string()),
            ..LiveView::default()
        };
        let stale = on_account(session("s1", "idle", DEV, "t"), "aaaa");
        assert!(Pass::new(vec![stale], vec![]).live("s1", view.clone()).run().is_empty());
        let reported = on_account(session("s1", "idle", DEV, "t"), "bbbb");
        assert!(matches!(
            Pass::new(vec![reported], vec![]).live("s1", view).run().as_slice(),
            [PickupAction::Switch { account, .. }] if account == "bbbb"
        ));
    }

    /// THE switch: an idle engine is suspended, the SAME row resumed on the
    /// target login with the pending message as its seed turn, and the new
    /// engine honors the row's account. A busy engine is refused untouched.
    #[test]
    fn a_switch_suspends_the_idle_engine_and_resumes_on_the_target() {
        let dir = temp_dir("pickup-switch");
        let (base, requests) = canned_server_recording(vec![
            (200, BRIEF_OK.to_string()),
            (200, RESOLVE_NONE.to_string()),
            (200, REPORT_OK.to_string()),
        ]);
        let host = host(&base, &dir.0, |_| {});
        let target = crate::agent_profiles::create(&dir.0, CodingAgent::Claude).unwrap();
        let mut on_a = record(&dir.0, "sess-1");
        on_a.set_account(Some(crate::test_support::TEST_PROFILE));
        crate::run_registry::record(&dir.0, on_a);
        let old = Arc::new(FakeEngine::default());
        host.deps.live.insert_for(
            "sess-1",
            old.clone(),
            None,
            Some(crate::test_support::TEST_PROFILE.to_string()),
        );
        let row = on_account(session("sess-1", "idle", DEV, "t"), &target.id);
        let pending = vec![message("m1", "sess-1", "2026-10-02T00:00:01Z")];

        old.working.store(true, Ordering::SeqCst);
        assert_eq!(
            switch_session(&row, &target.id, None, &pending, &host.deps),
            Err(SwitchError::Busy)
        );
        assert_eq!(*old.killed.lock().unwrap(), None, "a busy engine is never touched");
        old.working.store(false, Ordering::SeqCst);

        assert_eq!(switch_session(&row, &target.id, None, &pending, &host.deps), Ok(Switched::Moved));
        assert_eq!(*old.killed.lock().unwrap(), Some("suspended"));
        assert_eq!(
            procedures(&requests),
            vec!["sessions.brief", "mcpServers.resolveForLaunch", "sessions.report"]
        );
        let spawned = host.spawned.lock().unwrap().clone();
        assert_eq!(spawned.len(), 1);
        assert_eq!(spawned[0].session_id, "sess-1", "the SAME row");
        assert_eq!(spawned[0].seed_message_id.as_deref(), Some("m1"));
        let record = crate::run_registry::get(&dir.0, "sess-1").expect("record");
        assert_eq!(record.account().as_deref(), Some(target.id.as_str()));
        assert_eq!(record.honored_row_account(), target.id);
        assert_eq!(host.deps.live.live_views()["sess-1"].row_account.as_deref(), Some(target.id.as_str()));
        // The scratch dir the run lives in survives the switch.
        assert!(record.cwd.is_dir());
    }

    /// A switch to the login the run is already on just honors the row; one
    /// the machine cannot take (codex, a login not set up here) is explained
    /// once and the session keeps working on its login.
    #[test]
    fn a_switch_it_need_not_or_cannot_make_leaves_the_engine_running() {
        let dir = temp_dir("pickup-switch-noop");
        let (base, requests) = canned_server_recording(vec![
            (200, REPLY_OK.to_string()),
            (200, REPORT_OK.to_string()),
        ]);
        let host = host(&base, &dir.0, |_| {});
        let mut on_a = record(&dir.0, "sess-1");
        on_a.set_account(Some("aaaa"));
        on_a.set_honored_row_account(Some("cccc"));
        crate::run_registry::record(&dir.0, on_a);
        host.deps.live.insert_for("sess-1", host.engine.clone(), None, Some(String::new()));

        // AlreadyOn: honored in the live entry AND the record; the messages
        // that rode the switch go to the engine as they are.
        let same = on_account(session("sess-1", "idle", DEV, "t"), "aaaa");
        let pending = vec![message("m1", "sess-1", "2026-10-02T00:00:01Z")];
        assert_eq!(switch_session(&same, "aaaa", None, &pending, &host.deps), Ok(Switched::AlreadyOn));
        assert_eq!(host.deps.live.live_views()["sess-1"].row_account.as_deref(), Some("aaaa"));
        assert_eq!(crate::run_registry::get(&dir.0, "sess-1").unwrap().honored_row_account(), "aaaa");
        assert_eq!(host.engine.delivered_ids(), vec!["m1"]);

        let missing = on_account(session("sess-1", "idle", DEV, "t"), "nope");
        let Ok(Switched::Refused(reason)) = switch_session(&missing, "nope", None, &pending, &host.deps) else {
            panic!("expected a refusal");
        };
        assert!(reason.contains("not set up on this machine"), "{reason}");
        // Said once, then the row is told the login the run keeps.
        assert_eq!(procedures(&requests), vec!["sessions.reply", "sessions.report"]);
        let report = requests.lock().unwrap()[1].clone();
        assert!(report.ends_with(r#"{"sessionId":"sess-1","account":"aaaa"}"#), "{report}");
        assert_eq!(*host.engine.killed.lock().unwrap(), None);
        assert_eq!(host.deps.live.live_views()["sess-1"].row_account.as_deref(), Some("nope"));
        assert_eq!(crate::run_registry::get(&dir.0, "sess-1").unwrap().honored_row_account(), "nope");
        // m1 was handed already: never twice.
        assert_eq!(host.engine.delivered_ids(), vec!["m1"]);
        assert!(host.spawned.lock().unwrap().is_empty());
    }

    /// BLOCKING 1: a switch the machine cannot make, found on a WAKE (the
    /// row moved while no engine ran — an owner's agent login change, a
    /// login not set up here), never bricks the session: it wakes on its
    /// recorded login, the reason is replied ONCE, the record honors the
    /// pick, and the row is told the login the run keeps.
    #[test]
    fn a_wake_refuses_a_switch_it_cannot_make_once_and_runs_on() {
        let dir = temp_dir("pickup-wake-refused");
        let (base, requests) = canned_server_recording(vec![
            (200, BRIEF_OK.to_string()),
            (200, REPLY_OK.to_string()),
            (200, RESOLVE_NONE.to_string()),
            (200, REPORT_OK.to_string()),
            (200, BRIEF_OK.to_string()),
            (200, RESOLVE_NONE.to_string()),
        ]);
        let host = host(&base, &dir.0, |_| {});
        let mut on_a = record(&dir.0, "sess-1");
        on_a.set_account(Some(crate::test_support::TEST_PROFILE));
        on_a.set_honored_row_account(Some(crate::test_support::TEST_PROFILE));
        crate::run_registry::record(&dir.0, on_a);
        let row = on_account(session("sess-1", "idle", DEV, "t"), "nope");

        let rows = vec![message("m1", "sess-1", "2026-10-02T00:00:01Z")];
        assert_eq!(resume_session(&row, &rows, &host.deps).unwrap(), vec!["m1"]);
        assert_eq!(
            procedures(&requests),
            vec!["sessions.brief", "sessions.reply", "mcpServers.resolveForLaunch", "sessions.report"]
        );
        let recorded = requests.lock().unwrap().clone();
        assert!(recorded[1].contains("not set up on this machine"), "{}", recorded[1]);
        assert!(
            recorded[3].ends_with(&format!(
                r#"{{"sessionId":"sess-1","account":"{}"}}"#,
                crate::test_support::TEST_PROFILE
            )),
            "{}",
            recorded[3]
        );
        let record = crate::run_registry::get(&dir.0, "sess-1").unwrap();
        assert_eq!(record.account().as_deref(), Some(crate::test_support::TEST_PROFILE));
        assert_eq!(record.honored_row_account(), "nope");
        assert_eq!(host.spawned.lock().unwrap()[0].seed_message_id.as_deref(), Some("m1"));

        // The run exits; the next message wakes the row the report moved —
        // no second reply, no second report, nothing to switch.
        host.engine.done.store(true, Ordering::SeqCst);
        let reported = on_account(row, crate::test_support::TEST_PROFILE);
        let more = vec![message("m2", "sess-1", "2026-10-02T00:00:02Z")];
        assert_eq!(resume_session(&reported, &more, &host.deps).unwrap(), vec!["m2"]);
        assert_eq!(procedures(&requests)[4..], ["sessions.brief", "mcpServers.resolveForLaunch"]);
        let record = crate::run_registry::get(&dir.0, "sess-1").unwrap();
        assert_eq!(record.honored_row_account(), crate::test_support::TEST_PROFILE);
        assert_eq!(record.account().as_deref(), Some(crate::test_support::TEST_PROFILE));
    }

    /// The wake's account decision: a codex run, a login not set up here
    /// and the login the run already runs on.
    #[test]
    fn a_wake_switches_only_where_the_machine_can() {
        let dir = temp_dir("pickup-wake-account");
        let host = host(&canned_server(vec![]), &dir.0, |_| {});
        let target = crate::agent_profiles::create(&dir.0, CodingAgent::Claude).unwrap();
        let mut on_a = record(&dir.0, "sess-1");
        on_a.set_account(Some(crate::test_support::TEST_PROFILE));
        on_a.set_honored_row_account(Some("aaaa"));
        assert_eq!(wake_account(Some(&target.id), &on_a, &host.deps), WakeAccount::Switch(target.id.clone()));
        assert_eq!(wake_account(Some("aaaa"), &on_a, &host.deps), WakeAccount::Recorded);
        // The device's own report read back: the login it runs on.
        assert_eq!(
            wake_account(Some(crate::test_support::TEST_PROFILE), &on_a, &host.deps),
            WakeAccount::Recorded
        );
        assert!(matches!(
            wake_account(Some("nope"), &on_a, &host.deps),
            WakeAccount::Refused(reason) if reason.contains("not set up")
        ));
        let mut codex = on_a.clone();
        codex.agent = CodingAgent::Codex;
        assert!(matches!(
            wake_account(Some(&target.id), &codex, &host.deps),
            WakeAccount::Refused(reason) if reason.contains("Only claude")
        ));
    }

    /// MEDIUM 3: what the suspended engine was handed is never delivered to
    /// the new one — handed ids carry over, and the consumes its exit parked
    /// in the record are made by the pickup, not re-sent as turns.
    #[test]
    fn a_switch_never_redelivers_what_the_old_engine_was_handed() {
        let dir = temp_dir("pickup-switch-owed");
        let (base, requests) = canned_server_recording(vec![
            (200, BRIEF_OK.to_string()),
            (200, RESOLVE_NONE.to_string()),
            (200, REPORT_OK.to_string()),
            (200, CONSUME_OK.to_string()),
        ]);
        let host = host(&base, &dir.0, |_| {});
        let target = crate::agent_profiles::create(&dir.0, CodingAgent::Claude).unwrap();
        let mut on_a = record(&dir.0, "sess-1");
        on_a.set_account(Some(crate::test_support::TEST_PROFILE));
        crate::run_registry::record(&dir.0, on_a);
        let old = Arc::new(OwingEngine { inner: FakeEngine::default(), data_dir: dir.0.clone() });
        host.deps.live.insert_for(
            "sess-1",
            old.clone(),
            None,
            Some(crate::test_support::TEST_PROFILE.to_string()),
        );
        host.deps.live.mark_handed("sess-1", &["m1".to_string()]);
        let row = on_account(session("sess-1", "idle", DEV, "t"), &target.id);
        // m1 was handed (its consume parked on exit), m2 slipped into the
        // outbox too, m3 is new.
        let pending = vec![
            message("m2", "sess-1", "2026-10-02T00:00:02Z"),
            message("m3", "sess-1", "2026-10-02T00:00:03Z"),
        ];
        assert_eq!(switch_session(&row, &target.id, None, &pending, &host.deps), Ok(Switched::Moved));
        let spawned = host.spawned.lock().unwrap().clone();
        assert_eq!(spawned[0].seed_message_id.as_deref(), Some("m3"));
        assert!(host.engine.delivered_ids().is_empty(), "m1/m2 never reach the new engine");
        let handed = host.deps.live.live_views()["sess-1"].handed.clone();
        assert!(handed.contains("m1") && handed.contains("m3"), "{handed:?}");
        assert!(wait_for(&requests, "sessions.consume"));
        let consume = requests.lock().unwrap().last().cloned().unwrap();
        assert!(consume.contains("m1") && consume.contains("m2") && !consume.contains("m3"), "{consume}");
        assert_eq!(
            procedures(&requests),
            vec!["sessions.brief", "mcpServers.resolveForLaunch", "sessions.report", "sessions.consume"]
        );
        // A later pass hands nothing twice.
        let rows = vec![message("m1", "sess-1", "a"), pending[0].clone(), pending[1].clone()];
        assert!(deliver_messages("sess-1", &rows, &host.deps).unwrap().is_empty());
    }

    /// An engine whose suspend parks two consumes in the run record (the
    /// outbox it could not flush) — like a real exit.
    struct OwingEngine {
        inner: FakeEngine,
        data_dir: std::path::PathBuf,
    }

    impl LiveEngine for OwingEngine {
        fn deliver(&self, message_id: String, text: String) {
            self.inner.deliver(message_id, text)
        }
        fn unqueue(&self, message_id: &str) {
            self.inner.unqueue(message_id)
        }
        fn answer_card(&self, question_id: &str, card: &serde_json::Value) {
            self.inner.answer_card(question_id, card)
        }
        fn open_cards(&self) -> HashMap<String, String> {
            self.inner.open_cards()
        }
        fn kill(&self, outcome: &'static str) {
            self.inner.kill(outcome)
        }
        fn is_done(&self) -> bool {
            self.inner.is_done()
        }
        fn is_idle(&self) -> bool {
            self.inner.is_idle()
        }
        fn suspend(&self) {
            crate::run_registry::update(&self.data_dir, "sess-1", |record| {
                record.set_pending_consumes(&["m1".to_string(), "m2".to_string()]);
                true
            });
            self.inner.suspend()
        }
    }

    /// LOW (c): a resume that fails AFTER the suspend is explained once —
    /// the same pending messages never re-post it on a later wake.
    #[test]
    fn a_switch_whose_resume_fails_is_explained_once() {
        let dir = temp_dir("pickup-switch-prepare-fails");
        let (base, requests) = canned_server_recording(vec![
            (200, BRIEF_OK.to_string()),
            (200, REPLY_OK.to_string()),
            (200, REPORT_OK.to_string()),
        ]);
        let host = host(&base, &dir.0, |coding| coding.acp_available = false);
        let target = crate::agent_profiles::create(&dir.0, CodingAgent::Claude).unwrap();
        let mut on_a = record(&dir.0, "sess-1");
        on_a.set_account(Some(crate::test_support::TEST_PROFILE));
        crate::run_registry::record(&dir.0, on_a);
        host.deps.live.insert_for(
            "sess-1",
            host.engine.clone(),
            None,
            Some(crate::test_support::TEST_PROFILE.to_string()),
        );
        let row = on_account(session("sess-1", "idle", DEV, "t"), &target.id);
        let pending = vec![message("m1", "sess-1", "2026-10-02T00:00:01Z")];
        assert!(matches!(
            switch_session(&row, &target.id, None, &pending, &host.deps),
            Ok(Switched::Refused(_))
        ));
        assert_eq!(procedures(&requests), vec!["sessions.brief", "sessions.reply", "sessions.report"]);
        // The wake the next pass plans for the same message stays quiet.
        assert!(resume_session(&row, &pending, &host.deps).unwrap().is_empty());
        assert_eq!(requests.lock().unwrap().len(), 3);
    }

    /// A row whose account moved while no engine ran here wakes ON it; a
    /// record that honors the row's account wakes on its recorded login.
    #[test]
    fn a_wake_honors_a_switch_made_while_no_engine_ran() {
        let mut on_a = record(&temp_dir("pickup-wake-switch").0, "sess-1");
        on_a.set_account(Some("aaaa"));
        on_a.set_honored_row_account(Some("aaaa"));
        assert_eq!(resume_switch(Some("bbbb"), &on_a), Some("bbbb".to_string()));
        assert_eq!(resume_switch(Some("aaaa"), &on_a), None);
        assert_eq!(resume_switch(None, &on_a), None);
        // A start pick moved the run off the row's (walled) login: the row
        // still names the honored one, so the wake keeps the recorded login.
        let mut picked = on_a.clone();
        picked.set_account(Some("cccc"));
        assert_eq!(resume_switch(Some("aaaa"), &picked), None);
        // A record from before the key honors the login it ran on.
        let mut legacy = on_a.clone();
        legacy.extra.remove(crate::run_registry::ROW_ACCOUNT_KEY);
        assert_eq!(legacy.honored_row_account(), "aaaa");
    }

    #[test]
    fn the_briefs_issues_section_names_the_identifiers_in_order() {
        assert_eq!(
            brief_issue_identifiers(
                "## Request\n\nKeep the API stable.\n\n## Issues\n\n- EXP-12 Login fails\n- EXP-13 Cookie\n\n## Parent\n\nx"
            ),
            vec!["EXP-12".to_string(), "EXP-13".to_string()]
        );
        assert_eq!(brief_issue_identifiers("## Issues\n\n- APP-7 First"), vec!["APP-7".to_string()]);
        assert!(brief_issue_identifiers("## Request\n\nFix it.").is_empty());
        assert!(brief_issue_identifiers("## Issues\n\n- not-an-id x").is_empty());
    }
}
