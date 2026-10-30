//! Shared pieces of the live-coding activity stream (masterplan §P7 +
//! EXP-78, steering v2 = EXP-249).
//!
//! EXP-1426: the pure half (the [`Redactor`], the relay caps,
//! [`worktree_diff`], [`SessionAgent`], the normalisers and truncation
//! helpers) lives in `exponential_agent_runtime::wire::activity` and is
//! re-exported here, so `steer::activity::*` paths keep resolving. What is
//! left is what needs the publisher, `api` or `coding`:
//!
//! * [`DiffSnapshots`] — the debounced changed-only `diff` snapshot;
//! * the publisher↔engine seams: [`AnswerLink`] (a steerer's semantic
//!   answer), [`CommandLink`] (a slash command) and [`ConfigLink`] (a live
//!   model/mode switch);
//! * [`TurnSignal`], [`CaptionSignal`] and the forwarders
//!   ([`NeedsInputForwarder`], [`StatusForwarder`], [`BlockedForwarder`],
//!   [`AgentTitleForwarder`]).

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::frames::ActivityEvent;
use crate::publisher::ActivitySender;

pub use exponential_agent_runtime::wire::activity::*;

/// EXP-355: cooldown between re-attempts of a `needs_input` forward whose
/// write failed — fire-and-forget on flips used to stick the synced badge on
/// its last value until the NEXT flip (which may never come this session).
pub(crate) const NEEDS_INPUT_RETRY: Duration = Duration::from_secs(5);

// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
// The publisher ↔ engine answer seam (EXP-249)
// ---------------------------------------------------------------------------

/// A steerer's semantic answer, relayed verbatim by the publisher: the
/// question's own id plus the option `key`s of THAT question. The emitter maps
/// them onto whatever the TUI is showing right now — clients never guess
/// keystrokes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteAnswer {
    pub question_id: String,
    pub ask_id: Option<String>,
    pub keys: Vec<String>,
    /// EXP-513: the typed reply for a `freeText` option — selected with
    /// `keys`, typed into the TUI's inline editor, submitted with Enter.
    pub text: Option<String>,
}

/// The one-way seam between the publisher task (tokio) and the ACP engine:
/// a steerer's semantic answer goes down the channel and the engine maps it
/// onto the pending ACP request. Clients never guess keystrokes.
pub struct AnswerLink {
    tx: flume::Sender<RemoteAnswer>,
}

impl AnswerLink {
    /// The link plus the engine's receiving end.
    pub fn new() -> (Arc<Self>, flume::Receiver<RemoteAnswer>) {
        let (tx, rx) = flume::unbounded();
        (Arc::new(Self { tx }), rx)
    }

    /// Publisher side: hand one answer to the engine (fire-and-forget — a
    /// dead engine just means the card stays unanswered).
    pub fn submit(&self, answer: RemoteAnswer) {
        let _ = self.tx.send(answer);
    }
}

// ---------------------------------------------------------------------------
// The publisher ↔ engine command seam (EXP-724)
// ---------------------------------------------------------------------------

/// The publisher ↔ emitter seam for remote slash commands, the sibling of
/// [`AnswerLink`] (EXP-724).
///
/// The publisher RECOGNISES a command (its first token is a catalog `/name`
/// for the session's agent, [`crate::commands::parse_command`]) but must not
/// execute one: typing needs the grid and the composer's turn state, both of
/// which the emitter owns. So a whole recognised composer message crosses
/// here and the emitter drains it, exactly like an answer.
///
/// One `Arc` is shared by both sides (the receiver lives inside), so the
/// wiring builds it once and hands the same handle to
/// [`crate::publisher::PublisherHooks`] and [`Steering`].
pub struct CommandLink {
    tx: flume::Sender<crate::commands::ParsedCommand>,
    rx: flume::Receiver<crate::commands::ParsedCommand>,
    /// Emitter side: is the agent between turns? codex's mid-task command
    /// handling is unverified, so it holds a command until this reads true
    /// (claude queues input mid-turn and needs no gate).
    composer_idle: AtomicBool,
}

impl CommandLink {
    pub fn new() -> Arc<Self> {
        let (tx, rx) = flume::unbounded();
        Arc::new(Self {
            tx,
            rx,
            composer_idle: AtomicBool::new(false),
        })
    }

    /// Publisher side: hand one recognised command to the emitter
    /// (fire-and-forget — a dead emitter just means it never runs).
    pub fn submit(&self, command: crate::commands::ParsedCommand) {
        let _ = self.tx.send(command);
    }

    /// Emitter side: the next queued command, if any.
    pub fn try_recv(&self) -> Option<crate::commands::ParsedCommand> {
        self.rx.try_recv().ok()
    }

    /// Emitter side: publish the between-turns bit.
    pub fn set_composer_idle(&self, idle: bool) {
        self.composer_idle.store(idle, Ordering::Relaxed);
    }

    pub fn composer_idle(&self) -> bool {
        self.composer_idle.load(Ordering::Relaxed)
    }
}

// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
// The publisher ↔ ENGINE live-config seam (EXP-746)
// ---------------------------------------------------------------------------

/// EXP-746: one live-config change from a steerer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigChange {
    /// `set_config` — the option id and the value in force from now on. A
    /// BLANK value is the "CLI default / omit the flag" choice.
    Option { id: String, value: String },
    /// `set_mode` — one of the ids the publisher advertised in
    /// `config_state.modes`.
    Mode { id: String },
}

/// The publisher ↔ ENGINE seam for live config, sibling of [`AnswerLink`] and
/// [`CommandLink`] (EXP-746).
///
/// The publisher only ROUTES: it has no ACP session and cannot know whether a
/// model even exists. The engine drains this, calls `session/set_config_option`
/// or `session/set_mode`, and re-emits `config_state` — that re-emit is the
/// ONLY confirmation the wire has, which is why nothing here acks and nothing
/// blocks.
///
/// One `Arc` is shared by both sides (the receiver lives inside), so the
/// wiring builds it once and hands the same handle to
/// [`crate::publisher::PublisherHooks`] and the engine.
pub struct ConfigLink {
    tx: flume::Sender<ConfigChange>,
    rx: flume::Receiver<ConfigChange>,
}

impl ConfigLink {
    pub fn new() -> Arc<Self> {
        let (tx, rx) = flume::unbounded();
        Arc::new(Self { tx, rx })
    }

    /// Publisher side: hand one change to the engine (fire-and-forget — a
    /// dead engine just means it never applies).
    pub fn submit(&self, change: ConfigChange) {
        let _ = self.tx.send(change);
    }

    /// Engine side: the next queued change, if any.
    pub fn try_recv(&self) -> Option<ConfigChange> {
        self.rx.try_recv().ok()
    }

    /// Engine side: the receiver for its own `select!` arm.
    pub fn receiver(&self) -> flume::Receiver<ConfigChange> {
        self.rx.clone()
    }
}

/// EXP-637 — the "is the agent between turns?" signal, shared by the emitter
/// (which flips it) and the graceful-stop path (which waits on it).
///
/// When the agent declares its run over via `exponential_sessions_end`, the
/// server ends the row while the CLI is still mid-turn — writing its final
/// message, flushing its transcript. Killing it right then truncates exactly
/// the output the close-out is about, so the host waits for the next idle
/// edge (bounded by a timeout) before tearing anything down.
///
/// Deliberately tiny and lock-free on the read path: the emitter thread
/// touches it on every hook/event tick.
#[derive(Debug, Default)]
pub struct TurnSignal {
    idle: AtomicBool,
    waiters: Mutex<Vec<flume::Sender<()>>>,
}

impl TurnSignal {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_idle(&self) -> bool {
        self.idle.load(Ordering::Relaxed)
    }

    /// Record the agent's turn state. A false→true edge wakes every waiter;
    /// repeated `true`s are cheap no-ops.
    pub fn set_idle(&self, idle: bool) {
        let was = self.idle.swap(idle, Ordering::Relaxed);
        if idle && !was {
            self.wake();
        }
    }

    /// A receiver that fires once the agent is between turns — IMMEDIATELY
    /// when it already is, otherwise on the next false→true edge.
    pub fn subscribe(&self) -> flume::Receiver<()> {
        let (tx, rx) = flume::bounded(1);
        if self.is_idle() {
            let _ = tx.try_send(());
            return rx;
        }
        match self.waiters.lock() {
            Ok(mut waiters) => waiters.push(tx),
            Err(poisoned) => poisoned.into_inner().push(tx),
        }
        // Re-check after registering: the edge may have fired in between.
        if self.is_idle() {
            self.wake();
        }
        rx
    }

    fn wake(&self) {
        let waiters = match self.waiters.lock() {
            Ok(mut waiters) => std::mem::take(&mut *waiters),
            Err(poisoned) => std::mem::take(&mut *poisoned.into_inner()),
        };
        for waiter in waiters {
            let _ = waiter.try_send(());
        }
    }
}

/// EXP-850 §8: the in-process workflow CAPTION of a run hosted right here —
/// the desktop's own session rows read it for a run they host. EXP-1292: it
/// is LOCAL only now; the synced `agent_caption` column and its forwarder
/// are gone (a `sessions` row carries no caption).
///
/// A plain mutexed slot rather than an atomic: it holds a string and nothing
/// reads it on the hot event path.
#[derive(Debug, Default)]
pub struct CaptionSignal {
    caption: Mutex<Option<String>>,
}

impl CaptionSignal {
    pub fn new() -> Self {
        Self::default()
    }

    /// The caption right now — `None` while no workflow runs.
    pub fn get(&self) -> Option<String> {
        match self.caption.lock() {
            Ok(caption) => caption.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    /// Record the caption (`None` clears it). Returns whether it CHANGED,
    /// which is what the forwarder's "only on change" rule keys on.
    pub fn set(&self, caption: Option<String>) -> bool {
        let mut slot = match self.caption.lock() {
            Ok(slot) => slot,
            Err(poisoned) => poisoned.into_inner(),
        };
        if *slot == caption {
            return false;
        }
        *slot = caption;
        true
    }
}

/// EXP-905: how long a FAILED title write waits before it is retried. A title
/// moves a handful of times per run, so a transport hiccup must not turn the
/// 1 s ticker into a request per second.
pub const AGENT_TITLE_RETRY_INTERVAL: Duration = Duration::from_secs(10);

/// EXP-905: the in-process agent title of a run hosted here (what the mapper
/// last learned), shared with the forwarder and any in-process reader.
pub type AgentTitleSignal = CaptionSignal;

pub type AgentTitleHook = Arc<dyn Fn(&str) -> bool + Send + Sync>;

/// EXP-905: the agent-title forwarder (EXP-1292: the hook writes
/// `sessions.report{title}`). Writes only on CHANGE, only ever a
/// title (never a clear: the name outlives the run, so there is no teardown
/// write either), and retries a failed write after
/// [`AGENT_TITLE_RETRY_INTERVAL`] — or at once when the title moves again.
pub struct AgentTitleForwarder {
    forwarded: Option<String>,
    failed: Option<(String, Instant)>,
}

impl AgentTitleForwarder {
    pub fn new() -> Self {
        Self {
            forwarded: None,
            failed: None,
        }
    }

    pub fn tick(&mut self, title: Option<&str>, hook: &Option<AgentTitleHook>) {
        self.tick_at(Instant::now(), title, hook)
    }

    /// [`Self::tick`] with the clock injected.
    pub fn tick_at(&mut self, now: Instant, title: Option<&str>, hook: &Option<AgentTitleHook>) {
        let Some(title) = title else { return };
        if self.forwarded.as_deref() == Some(title) {
            return;
        }
        if let Some((failed, at)) = &self.failed {
            if failed == title && now < *at + AGENT_TITLE_RETRY_INTERVAL {
                return;
            }
        }
        let landed = match hook {
            Some(hook) => hook(title),
            None => true,
        };
        if landed {
            self.forwarded = Some(title.to_string());
            self.failed = None;
        } else {
            self.failed = Some((title.to_string(), now));
        }
    }
}

impl Default for AgentTitleForwarder {
    fn default() -> Self {
        Self::new()
    }
}

/// The debounced changed-only worktree diff snapshot — step 8 of every
/// emitter, extracted verbatim so the codex emitter shares it (EXP-383).
pub struct DiffSnapshots {
    last: String,
    last_at: Option<Instant>,
}

impl DiffSnapshots {
    pub fn new() -> Self {
        Self {
            last: String::new(),
            last_at: None,
        }
    }

    /// EXP-746: the whole rule WITHOUT a sink — the ACP engine drives its own
    /// loop and routes the event to the publisher AND to the local feed, so
    /// it cannot hand a bare [`ActivitySender`] over. `None` means "nothing
    /// to publish this tick" (not due, git failed, or the diff is unchanged).
    /// [`Self::tick`] is this plus the send.
    pub fn next_diff(
        &mut self,
        worktree: &Path,
        base_ref: Option<&str>,
        redactor: &Redactor,
    ) -> Option<ActivityEvent> {
        let due = self.last_at.is_none_or(|at| at.elapsed() >= DIFF_INTERVAL);
        if !due {
            return None;
        }
        self.last_at = Some(Instant::now());
        // A git failure (index lock, rebase in flight) is not an empty diff:
        // keep the last answer and try again next tick.
        let diff = worktree_diff(worktree, base_ref)?;
        if diff == self.last {
            return None;
        }
        let had_diff = !self.last.is_empty();
        self.last = diff.clone();
        // EXP-688: a diff that goes EMPTY publishes an explicit empty frame
        // (the wire allows `""`, and every client treats it as "no diff").
        // Sending nothing left viewers looking at a stale patch forever.
        (!diff.is_empty() || had_diff)
            .then(|| ActivityEvent::diff(truncate(&redactor.redact(&diff), DIFF_MAX)))
    }

    pub fn tick(
        &mut self,
        worktree: &Path,
        base_ref: Option<&str>,
        sender: &ActivitySender,
        redactor: &Redactor,
    ) {
        if let Some(event) = self.next_diff(worktree, base_ref, redactor) {
            sender.send(event);
        }
    }
}

/// The EXP-214 synced needs-input flag, tracked as the last CONFIRMED server
/// value (`None` = the last write failed and wants a retry). The session row
/// is born with the flag off. Forwarded on flips; an unconfirmed write
/// re-attempts every [`NEEDS_INPUT_RETRY`] (EXP-355). Extracted verbatim from
/// the claude emitter so the codex emitter shares it (EXP-383).
pub struct NeedsInputForwarder {
    forwarded: Option<bool>,
    retry_at: Option<Instant>,
}

pub type NeedsInputHook = Arc<dyn Fn(bool) -> bool + Send + Sync>;

/// EXP-848: the synced `agent_busy` flag rides the SAME mechanism — one
/// device-written bool, forwarded on flips, retried when a write does not
/// land, cleared at teardown. Aliases rather than a second copy: there is
/// exactly one rule, and two names for it would be two places to fix.
pub type AgentBusyForwarder = NeedsInputForwarder;
pub type AgentBusyHook = NeedsInputHook;

impl NeedsInputForwarder {
    pub fn new() -> Self {
        Self {
            forwarded: Some(false),
            retry_at: None,
        }
    }

    pub fn tick(&mut self, pending: bool, hook: &Option<NeedsInputHook>) {
        if self.forwarded != Some(pending) && self.retry_at.is_none_or(|at| Instant::now() >= at) {
            let landed = match hook {
                Some(hook) => hook(pending),
                None => true,
            };
            self.forwarded = landed.then_some(pending);
            self.retry_at = (!landed).then(|| Instant::now() + NEEDS_INPUT_RETRY);
        }
    }

    /// Teardown tidiness: never leave the synced attention flag stuck on a
    /// session whose emitter is gone (the terminal-exit `end` supersedes).
    pub fn clear_on_teardown(&mut self, hook: &Option<NeedsInputHook>) {
        if self.forwarded != Some(false) {
            if let Some(hook) = hook {
                hook(false);
            }
        }
    }
}

/// EXP-1292: the synced `sessions.status` a live engine owns — `running`
/// while a turn is open, `idle` between turns (the row then waits for the
/// next `messages` row). Driven off the [`TurnSignal`] edge and written with
/// `sessions.report{status}`; the terminal `ended` (threads) / `idle`
/// (agents) at engine exit is the lifecycle's own write, never this one's.
///
/// The [`NeedsInputForwarder`] rule: the last CONFIRMED server value (`None`
/// = unknown, or the last write failed and wants a retry), written on
/// flips only, an unconfirmed write re-attempted every [`NEEDS_INPUT_RETRY`].
pub struct StatusForwarder {
    forwarded: Option<api::session_device::ReportStatus>,
    retry_at: Option<Instant>,
}

/// Returns whether the write landed.
pub type StatusHook = Arc<dyn Fn(api::session_device::ReportStatus) -> bool + Send + Sync>;

impl StatusForwarder {
    /// Starts UNKNOWN: the first tick writes whatever the turn edge says.
    /// A claimed row reads `running`, but a natively RESUMED row is still
    /// `idle` server-side (a resume claims nothing), so the forwarder never
    /// assumes either.
    pub fn new() -> Self {
        Self {
            forwarded: None,
            retry_at: None,
        }
    }

    /// The status a turn edge maps to.
    pub fn status_for(idle: bool) -> api::session_device::ReportStatus {
        if idle {
            api::session_device::ReportStatus::Idle
        } else {
            api::session_device::ReportStatus::Running
        }
    }

    /// One tick: `idle` = the [`TurnSignal`] says the agent is between turns.
    pub fn tick(&mut self, idle: bool, hook: &Option<StatusHook>) {
        self.tick_at(Instant::now(), idle, hook)
    }

    /// [`Self::tick`] with the clock injected.
    pub fn tick_at(&mut self, now: Instant, idle: bool, hook: &Option<StatusHook>) {
        let wanted = Self::status_for(idle);
        if self.forwarded == Some(wanted) || self.retry_at.is_some_and(|at| now < at) {
            return;
        }
        let landed = match hook {
            Some(hook) => hook(wanted),
            None => true,
        };
        self.forwarded = landed.then_some(wanted);
        self.retry_at = (!landed).then(|| now + NEEDS_INPUT_RETRY);
    }

    /// The last status the server confirmed (`None` = unknown / unconfirmed).
    pub fn forwarded(&self) -> Option<api::session_device::ReportStatus> {
        self.forwarded
    }
}

impl Default for StatusForwarder {
    fn default() -> Self {
        Self::new()
    }
}

/// EXP-804: the agent's usage wall as the synced row records it
/// (EXP-1292: `sessions.blocked`, written with `sessions.report`). The device
/// is the only thing that can see it (the agent reports the wall on its own
/// stream), and a walled run keeps status `running` with a moving
/// `updated_at`, so without this write a rate-limited run is indistinguishable
/// from a healthy one — the 2026-09-09 incident.
///
/// `resets_at` is an ISO string here even though every producer speaks a
/// different unit: claude's own frame carries unix SECONDS, the relay wire
/// unix MILLIS, and the column an ISO stamp matching `DeviceUsageWindow`.
/// [`BlockedForwarder`] is the one place that conversion happens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionBlocked {
    /// Contract `codingSessionBlocked.kinds` — `rate_limit` today.
    pub kind: String,
    pub agent: String,
    /// Contract `codingSessionBlocked.windows`, when the agent named one.
    pub window: String,
    pub resets_at: Option<String>,
    pub since: String,
}

impl SessionBlocked {
    /// EXP-1292: the `sessions.report{blocked}` wire value. `handled` is the
    /// EXP-1005 verdict (the device rotates or waits the wall out itself, so
    /// the server sends no rate-limit notification); `None` = it notifies.
    pub fn report_input(&self, handled: Option<bool>) -> api::session_device::BlockedInput {
        api::session_device::BlockedInput {
            kind: self.kind.clone(),
            agent: self.agent.clone(),
            window: self.window.clone(),
            resets_at: self.resets_at.clone(),
            since: self.since.clone(),
            handled,
        }
    }
}

/// EXP-831 (release-review follow-up): whether a wall's own reset stamp has
/// passed, by the SAME rule every client's banner uses
/// ([`crate::frames::rate_limit_expired`], mirrored x4). The banner hides
/// itself once `resetsAt` is more than a minute old, but the synced row was
/// forwarded verbatim for as long as the agent stayed idle, so a session that
/// showed no banner still read "Rate limited" in every list and on both
/// phones. The ticker drops an expired wall instead of forwarding it.
///
/// A wall with no reset stamp never expires by the clock: only the agent's
/// next allowed call can clear that one. An unparseable stamp is treated the
/// same way, never as "expired".
pub fn blocked_wall_expired(wall: &SessionBlocked, now_ms: i64) -> bool {
    let Some(resets_at) = wall.resets_at.as_deref() else {
        return false;
    };
    match coding::agent_accounts::unix_millis_from_iso(resets_at) {
        Some(at) => crate::frames::rate_limit_expired(Some(at), now_ms),
        None => false,
    }
}

/// The wall clock the expiry rule above compares against (unix ms).
pub fn now_unix_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_millis() as i64)
        .unwrap_or_default()
}

/// The EXP-804 synced usage wall, tracked exactly like [`NeedsInputForwarder`]
/// tracks its flag: the last CONFIRMED server value, `None` on a write that
/// failed and wants a retry. Forwarded on CHANGES only, so a run that sits
/// behind a wall for hours writes once, not every tick.
pub struct BlockedForwarder {
    forwarded: Option<Option<SessionBlocked>>,
    retry_at: Option<Instant>,
}

/// Returns whether the write landed. `None` = clear the wall.
pub type BlockedHook = Arc<dyn Fn(Option<&SessionBlocked>) -> bool + Send + Sync>;

impl BlockedForwarder {
    pub fn new() -> Self {
        Self {
            // A session row is born unblocked, so the initial state is
            // CONFIRMED — nothing to write until the agent hits a wall.
            forwarded: Some(None),
            retry_at: None,
        }
    }

    pub fn tick(&mut self, pending: Option<&SessionBlocked>, hook: &Option<BlockedHook>) {
        let changed = self.forwarded.as_ref() != Some(&pending.cloned());
        if changed && self.retry_at.is_none_or(|at| Instant::now() >= at) {
            let landed = match hook {
                Some(hook) => hook(pending),
                None => true,
            };
            self.forwarded = landed.then(|| pending.cloned());
            self.retry_at = (!landed).then(|| Instant::now() + NEEDS_INPUT_RETRY);
        }
    }

    /// Teardown tidiness, the [`NeedsInputForwarder::clear_on_teardown`] rule:
    /// never leave a walled flag on a session whose emitter is gone. The
    /// terminal `end` supersedes it, but an ended row that still reads
    /// "Rate limited" would be a lie every client renders.
    pub fn clear_on_teardown(&mut self, hook: &Option<BlockedHook>) {
        if self.forwarded != Some(None) {
            if let Some(hook) = hook {
                hook(None);
            }
        }
    }
}

impl Default for BlockedForwarder {
    fn default() -> Self {
        Self::new()
    }
}

/// EXP-804: every producer of a reset stamp speaks a different unit — the
/// relay wire unix MILLIS, claude's own frame unix SECONDS, the column an ISO
/// string. Normalise ONCE, here, on top of the ISO helper the usage collector
/// already uses so the two never format differently. A non-positive stamp is
/// "no reset named", never 1970.
pub fn iso_from_unix_millis(millis: i64) -> Option<String> {
    if millis <= 0 {
        return None;
    }
    coding::agent_accounts::iso_from_unix_secs(millis / 1000)
}

#[cfg(test)]
mod blocked_tests {
    use super::*;

    fn wall(resets_at: Option<&str>) -> SessionBlocked {
        SessionBlocked {
            kind: BLOCKED_KIND_RATE_LIMIT.to_string(),
            agent: "claude".to_string(),
            window: BLOCKED_WINDOW_SESSION.to_string(),
            resets_at: resets_at.map(str::to_string),
            since: "2026-09-09T11:30:00.000Z".to_string(),
        }
    }

    #[test]
    fn a_wall_expires_by_its_own_reset_stamp_like_every_banner() {
        // The ×4 banner rule (`rate_limit_expired`) with the same grace, so
        // the synced row and the session view never disagree: a run walled
        // until 12:00 still reads blocked at 12:00:30 and stops at 12:01:30.
        let resets_at = "2026-09-09T12:00:00.000Z";
        let at = coding::agent_accounts::unix_millis_from_iso(resets_at).expect("parses");
        assert!(!blocked_wall_expired(&wall(Some(resets_at)), at - 1));
        assert!(!blocked_wall_expired(
            &wall(Some(resets_at)),
            at + crate::frames::RATE_LIMIT_EXPIRY_GRACE_MS
        ));
        assert!(blocked_wall_expired(
            &wall(Some(resets_at)),
            at + crate::frames::RATE_LIMIT_EXPIRY_GRACE_MS + 1
        ));
    }

    #[test]
    fn a_wall_without_a_usable_reset_stamp_never_expires_by_the_clock() {
        // Only the agent's next allowed call clears these two; guessing an
        // expiry would hide a wall that is still up.
        let now = coding::agent_accounts::unix_millis_from_iso("2030-01-01T00:00:00.000Z")
            .expect("parses");
        assert!(!blocked_wall_expired(&wall(None), now));
        assert!(!blocked_wall_expired(&wall(Some("soon")), now));
    }

    #[test]
    fn the_blocked_vocabulary_is_the_contract_vocabulary() {
        // Named constants over slice indices, but the contract still owns the
        // values — a renamed kind or window must break here, not in the wild.
        assert!(domain::contract::CODING_SESSION_BLOCKED_KINDS.contains(&BLOCKED_KIND_RATE_LIMIT));
        for window in [BLOCKED_WINDOW_SESSION, BLOCKED_WINDOW_WEEKLY, BLOCKED_WINDOW_MODEL] {
            assert!(
                domain::contract::CODING_SESSION_BLOCKED_WINDOWS.contains(&window),
                "{window}"
            );
        }
    }

    #[test]
    fn session_agent_ids_are_the_contract_agents() {
        for agent in [SessionAgent::Claude, SessionAgent::Codex] {
            assert!(domain::contract::CODING_AGENT_VALUES.contains(&agent.id()));
        }
        // EXP-746: deliberately un-nameable by the contract.
        assert!(!domain::contract::CODING_AGENT_VALUES.contains(&SessionAgent::External.id()));
    }

    #[test]
    fn a_reset_stamp_normalises_from_wire_millis() {
        // 2026-09-09T00:59:00Z. The wire carries MILLIS and the column takes
        // an ISO stamp; getting the unit wrong here reads as a reset in 1970
        // or in the year 57000, and the badge would say either with a
        // straight face.
        assert_eq!(
            iso_from_unix_millis(1_788_915_540_000).as_deref(),
            Some("2026-09-09T00:59:00.000Z")
        );
        // Non-positive is "no reset named", never 1970.
        assert_eq!(iso_from_unix_millis(0), None);
        assert_eq!(iso_from_unix_millis(-5), None);
    }

    #[test]
    fn forwards_only_changes_and_retries_a_failed_write() {
        let seen = Arc::new(Mutex::new(Vec::<Option<SessionBlocked>>::new()));
        let ok: Option<BlockedHook> = {
            let seen = Arc::clone(&seen);
            Some(Arc::new(move |wall: Option<&SessionBlocked>| {
                seen.lock().unwrap().push(wall.cloned());
                true
            }))
        };
        let mut forwarder = BlockedForwarder::new();
        // A row is born unblocked: nothing to say until a wall lands.
        forwarder.tick(None, &ok);
        assert!(seen.lock().unwrap().is_empty());
        // The wall lands, then the run sits behind it for many ticks — ONE
        // write, or a walled run would rewrite the row every poll interval.
        let hit = wall(Some("2026-09-09T14:00:00.000Z"));
        forwarder.tick(Some(&hit), &ok);
        forwarder.tick(Some(&hit), &ok);
        forwarder.tick(Some(&hit), &ok);
        assert_eq!(seen.lock().unwrap().len(), 1);
        // Clearing is a change like any other.
        forwarder.tick(None, &ok);
        assert_eq!(seen.lock().unwrap().len(), 2);
        assert_eq!(seen.lock().unwrap()[1], None);
    }

    #[test]
    fn an_unlanded_write_is_never_confirmed() {
        let calls = Arc::new(Mutex::new(0usize));
        let failing: Option<BlockedHook> = {
            let calls = Arc::clone(&calls);
            Some(Arc::new(move |_: Option<&SessionBlocked>| {
                *calls.lock().unwrap() += 1;
                false
            }))
        };
        let mut forwarder = BlockedForwarder::new();
        let hit = wall(None);
        forwarder.tick(Some(&hit), &failing);
        assert_eq!(*calls.lock().unwrap(), 1);
        // Behind the retry backoff nothing is re-sent...
        forwarder.tick(Some(&hit), &failing);
        assert_eq!(*calls.lock().unwrap(), 1);
        // ...but the value is NOT recorded as forwarded, so once the backoff
        // passes the wall is written again rather than silently lost.
        std::thread::sleep(NEEDS_INPUT_RETRY + Duration::from_millis(50));
        forwarder.tick(Some(&hit), &failing);
        assert_eq!(*calls.lock().unwrap(), 2);
    }

    #[test]
    fn teardown_clears_a_wall_but_says_nothing_when_unblocked() {
        let seen = Arc::new(Mutex::new(Vec::<Option<SessionBlocked>>::new()));
        let ok: Option<BlockedHook> = {
            let seen = Arc::clone(&seen);
            Some(Arc::new(move |wall: Option<&SessionBlocked>| {
                seen.lock().unwrap().push(wall.cloned());
                true
            }))
        };
        // An ended row that still reads "Rate limited" is a lie every client
        // renders, so teardown clears it.
        let mut forwarder = BlockedForwarder::new();
        forwarder.tick(Some(&wall(None)), &ok);
        forwarder.clear_on_teardown(&ok);
        assert_eq!(seen.lock().unwrap().as_slice(), &[Some(wall(None)), None]);
        // An unblocked run writes nothing on the way out.
        let mut clean = BlockedForwarder::new();
        let before = seen.lock().unwrap().len();
        clean.clear_on_teardown(&ok);
        assert_eq!(seen.lock().unwrap().len(), before);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    // ── EXP-688: the published diff is the PR's content ─────────────────────

    struct DiffRepo(PathBuf);

    impl Drop for DiffRepo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    impl DiffRepo {
        /// A repo on `exp/EXP-1`, cut from `main`, with one base commit.
        fn new(tag: &str) -> Self {
            let mut path = std::env::temp_dir();
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            path.push(format!("exp-steer-{tag}-{}-{nanos}", std::process::id()));
            std::fs::create_dir_all(&path).unwrap();
            let repo = Self(path);
            repo.git(&["init", "--quiet", "-b", "main"]);
            repo.git(&["config", "user.email", "t@example.com"]);
            repo.git(&["config", "user.name", "t"]);
            repo.git(&["config", "commit.gpgsign", "false"]);
            repo.write("base.txt", "a\n");
            repo.commit("init");
            repo.git(&["checkout", "--quiet", "-b", "exp/EXP-1"]);
            repo
        }

        fn git(&self, args: &[&str]) {
            let output = std::process::Command::new("git")
                .args(args)
                .current_dir(&self.0)
                .env("GIT_AUTHOR_NAME", "t")
                .env("GIT_AUTHOR_EMAIL", "t@example.com")
                .env("GIT_COMMITTER_NAME", "t")
                .env("GIT_COMMITTER_EMAIL", "t@example.com")
                .output()
                .unwrap();
            assert!(output.status.success(), "git {args:?} failed");
        }

        fn write(&self, rel: &str, content: &str) {
            std::fs::write(self.0.join(rel), content).unwrap();
        }

        fn commit(&self, message: &str) {
            self.git(&["add", "-A"]);
            self.git(&["commit", "--quiet", "-m", message]);
        }
    }

    /// EXP-746: `next_diff` is the same rule with the send taken out, so the
    /// ACP engine can route the event to the publisher AND its local feed.
    /// Mirrors the sink-driven test above answer for answer.
    #[test]
    fn next_diff_returns_what_tick_would_have_published() {
        let repo = DiffRepo::new("diff-next");
        let redactor = Redactor::new(Vec::new());
        let mut diffs = DiffSnapshots::new();

        repo.write("base.txt", "a\nwip\n");
        match diffs.next_diff(&repo.0, None, &redactor) {
            Some(ActivityEvent::Diff { diff, .. }) => assert!(diff.contains("wip"), "{diff}"),
            other => panic!("expected a diff event, got {other:?}"),
        }

        // The same 3s debounce, and it lives in `next_diff`, not in `tick`.
        assert!(
            diffs.next_diff(&repo.0, None, &redactor).is_none(),
            "the 3s debounce still holds"
        );

        // EXP-688: the clear is an explicit empty event, not silence.
        repo.commit("committed");
        diffs.last_at = None;
        match diffs.next_diff(&repo.0, None, &redactor) {
            Some(ActivityEvent::Diff { diff, .. }) => {
                assert_eq!(diff, "", "an empty event clears the viewer")
            }
            other => panic!("expected an empty diff event, got {other:?}"),
        }

        diffs.last_at = None;
        assert!(
            diffs.next_diff(&repo.0, None, &redactor).is_none(),
            "an unchanged empty diff is silent"
        );
    }

    /// EXP-746: the publisher→engine seam is a plain queue — submit, drain,
    /// nothing acks (the re-emitted `config_state` is the confirmation).
    #[test]
    fn the_config_link_queues_changes_for_the_engine() {
        let link = ConfigLink::new();
        assert_eq!(link.try_recv(), None);
        link.submit(ConfigChange::Option {
            id: "model".to_string(),
            value: String::new(),
        });
        link.submit(ConfigChange::Mode {
            id: "plan".to_string(),
        });
        assert_eq!(
            link.try_recv(),
            Some(ConfigChange::Option {
                id: "model".to_string(),
                // A blank value is the "CLI default" choice and must survive
                // the seam intact.
                value: String::new(),
            })
        );
        assert_eq!(
            link.receiver().try_recv().ok(),
            Some(ConfigChange::Mode {
                id: "plan".to_string(),
            })
        );
        assert_eq!(link.try_recv(), None);
    }

    /// EXP-637: the graceful-stop signal. Already-idle subscribers fire
    /// IMMEDIATELY (an agent that called `sessions_end` between turns must
    /// not wait for a timeout), a busy one on the next false→true edge, and
    /// repeated `true`s never re-fire a consumed waiter.
    #[test]
    fn turn_signal_fires_immediately_when_already_idle() {
        let signal = TurnSignal::new();
        assert!(!signal.is_idle());
        signal.set_idle(true);
        assert!(signal.is_idle());
        let rx = signal.subscribe();
        assert!(rx.try_recv().is_ok(), "an idle signal must fire at once");
    }

    #[test]
    fn turn_signal_fires_on_the_next_idle_edge() {
        let signal = TurnSignal::new();
        let rx = signal.subscribe();
        assert!(rx.try_recv().is_err(), "busy: nothing yet");
        // Staying busy changes nothing.
        signal.set_idle(false);
        assert!(rx.try_recv().is_err());
        signal.set_idle(true);
        assert!(rx.try_recv().is_ok());
    }

    #[test]
    fn turn_signal_wakes_every_waiter_once() {
        let signal = TurnSignal::new();
        let first = signal.subscribe();
        let second = signal.subscribe();
        signal.set_idle(true);
        assert!(first.try_recv().is_ok());
        assert!(second.try_recv().is_ok());
        // The edge is consumed: a second `true` re-fires nothing on the old
        // receivers, but a NEW subscriber still gets its immediate hit.
        signal.set_idle(true);
        assert!(first.try_recv().is_err());
        assert!(signal.subscribe().try_recv().is_ok());
    }

    /// A busy→idle→busy cycle re-arms: the second turn's waiter must wait
    /// for the SECOND boundary, not inherit the first.
    #[test]
    fn turn_signal_rearms_across_turns() {
        let signal = TurnSignal::new();
        signal.set_idle(true);
        signal.set_idle(false);
        let rx = signal.subscribe();
        assert!(rx.try_recv().is_err());
        signal.set_idle(true);
        assert!(rx.try_recv().is_ok());
    }
}

#[cfg(test)]
mod exp850_caption_tests {
    use super::*;

    /// EXP-850 §8: the in-process slot the desktop reads for a run it hosts.
    #[test]
    fn the_caption_signal_reports_changes() {
        let signal = CaptionSignal::new();
        assert_eq!(signal.get(), None);
        assert!(signal.set(Some("Workflow w \u{b7} starting".to_string())));
        assert!(!signal.set(Some("Workflow w \u{b7} starting".to_string())));
        assert_eq!(signal.get().as_deref(), Some("Workflow w \u{b7} starting"));
        assert!(signal.set(None));
        assert_eq!(signal.get(), None);
    }
}

#[cfg(test)]
mod exp905_agent_title_tests {
    use super::*;
    /// EXP-905: one write per change, never a clear, and a failed write waits
    /// out the retry interval unless the title moves again.
    #[test]
    fn the_title_forwarder_dedupes_never_clears_and_backs_off_a_failure() {
        let writes: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let landed = Arc::new(AtomicBool::new(true));
        let hook: Option<AgentTitleHook> = {
            let writes = Arc::clone(&writes);
            let landed = Arc::clone(&landed);
            Some(Arc::new(move |title: &str| {
                writes.lock().unwrap().push(title.to_string());
                landed.load(Ordering::Relaxed)
            }))
        };
        let mut forwarder = AgentTitleForwarder::new();
        let start = Instant::now();
        forwarder.tick_at(start, None, &hook);
        assert!(writes.lock().unwrap().is_empty());
        forwarder.tick_at(start, Some("one"), &hook);
        forwarder.tick_at(start, Some("one"), &hook);
        // `None` (nothing known) never clears what landed.
        forwarder.tick_at(start, None, &hook);
        assert_eq!(writes.lock().unwrap().clone(), vec!["one".to_string()]);

        landed.store(false, Ordering::Relaxed);
        forwarder.tick_at(start, Some("two"), &hook);
        // Inside the retry interval: silent.
        forwarder.tick_at(start + Duration::from_secs(1), Some("two"), &hook);
        assert_eq!(writes.lock().unwrap().len(), 2);
        // A NEW title goes out at once.
        forwarder.tick_at(start + Duration::from_secs(2), Some("three"), &hook);
        assert_eq!(writes.lock().unwrap().len(), 3);
        // Past the interval the failed one is retried and lands.
        landed.store(true, Ordering::Relaxed);
        let later = start + Duration::from_secs(2) + AGENT_TITLE_RETRY_INTERVAL;
        forwarder.tick_at(later, Some("three"), &hook);
        forwarder.tick_at(later, Some("three"), &hook);
        assert_eq!(
            writes.lock().unwrap().clone(),
            vec!["one", "two", "three", "three"]
        );
    }
}

#[cfg(test)]
mod exp1292_status_tests {
    use super::*;
    use api::session_device::ReportStatus;

    fn recording_hook(
        landed: &Arc<AtomicBool>,
    ) -> (Arc<Mutex<Vec<ReportStatus>>>, Option<StatusHook>) {
        let writes: Arc<Mutex<Vec<ReportStatus>>> = Arc::new(Mutex::new(Vec::new()));
        let hook: StatusHook = {
            let writes = Arc::clone(&writes);
            let landed = Arc::clone(landed);
            Arc::new(move |status| {
                writes.lock().unwrap().push(status);
                landed.load(Ordering::Relaxed)
            })
        };
        (writes, Some(hook))
    }

    /// EXP-1292: the turn edge maps to running/idle, written on flips only;
    /// the first tick always writes (the forwarder starts unknown).
    #[test]
    fn the_status_forwarder_writes_turn_edges_once() {
        let landed = Arc::new(AtomicBool::new(true));
        let (writes, hook) = recording_hook(&landed);
        let mut forwarder = StatusForwarder::new();
        assert_eq!(forwarder.forwarded(), None);
        let now = Instant::now();
        forwarder.tick_at(now, false, &hook);
        forwarder.tick_at(now, false, &hook);
        forwarder.tick_at(now, true, &hook);
        forwarder.tick_at(now, true, &hook);
        forwarder.tick_at(now, false, &hook);
        assert_eq!(
            writes.lock().unwrap().clone(),
            vec![ReportStatus::Running, ReportStatus::Idle, ReportStatus::Running]
        );
        assert_eq!(forwarder.forwarded(), Some(ReportStatus::Running));
    }

    /// A failed write stays unconfirmed and is retried after the backoff,
    /// not on every 1 s tick.
    #[test]
    fn a_failed_status_write_is_retried_after_the_backoff() {
        let landed = Arc::new(AtomicBool::new(false));
        let (writes, hook) = recording_hook(&landed);
        let mut forwarder = StatusForwarder::new();
        let now = Instant::now();
        forwarder.tick_at(now, true, &hook);
        forwarder.tick_at(now + Duration::from_secs(1), true, &hook);
        assert_eq!(writes.lock().unwrap().len(), 1);
        assert_eq!(forwarder.forwarded(), None);
        landed.store(true, Ordering::Relaxed);
        forwarder.tick_at(now + NEEDS_INPUT_RETRY, true, &hook);
        assert_eq!(
            writes.lock().unwrap().clone(),
            vec![ReportStatus::Idle, ReportStatus::Idle]
        );
        assert_eq!(forwarder.forwarded(), Some(ReportStatus::Idle));
    }

    /// The wall rides `sessions.report{blocked}` verbatim, plus the EXP-1005
    /// `handled` verdict.
    #[test]
    fn a_wall_maps_onto_the_report_input() {
        let wall = SessionBlocked {
            kind: BLOCKED_KIND_RATE_LIMIT.to_string(),
            agent: "claude".to_string(),
            window: BLOCKED_WINDOW_SESSION.to_string(),
            resets_at: Some("2030-01-01T00:00:00.000Z".to_string()),
            since: "2029-12-31T23:00:00.000Z".to_string(),
        };
        let input = wall.report_input(Some(true));
        assert_eq!(
            serde_json::to_value(&input).unwrap(),
            serde_json::json!({
                "kind": "rate_limit",
                "agent": "claude",
                "window": "session",
                "resetsAt": "2030-01-01T00:00:00.000Z",
                "since": "2029-12-31T23:00:00.000Z",
                "handled": true,
            })
        );
        assert_eq!(wall.report_input(None).handled, None);
    }
}
