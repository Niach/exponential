//! EXP-1292 — the session pickup: the database is the bus. A new session is
//! a `queued` `sessions` row naming its device, a person's message a
//! `messages` row with `consumed_at` NULL. Both hosts (the CLI daemon and
//! the desktop) read their shapes and ask [`plan`] what to do; the host then
//! executes each [`PickupAction`] (claim + launch, deliver, answer, kill).
//!
//! Pure and gpui-free: no I/O except [`ClaimLock`] (a per-session lockfile
//! under the data dir, so two processes of ONE machine sharing a data dir —
//! the daemon and a foreground `exponential code` — never both launch a
//! row; `sessions.claim` is idempotent per device, so the server cannot
//! tell them apart).

use std::collections::{HashMap, HashSet};

use crate::session_rows::{MessageRow, SessionRow};

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
    /// engine (replaces the kill watch / kill poll).
    Kill { session_id: String },
}

/// Decide one pass. Pure.
pub fn plan(input: PickupInput<'_>) -> Vec<PickupAction> {
    let _ = input;
    Vec::new()
}

/// A per-session lockfile under `{data_dir}/session-claims/` held for the
/// lifetime of the session's engine in this process.
pub struct ClaimLock {
    _private: (),
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
    fn kill(&self, outcome: &'static str);
    fn is_done(&self) -> bool;
}

/// Spawn the engine for a prepared launch (the host's `engine::start` +
/// registration). Called by [`launch_queued`] / [`deliver_messages`].
pub type LaunchFn = std::sync::Arc<
    dyn Fn(crate::PreparedLaunch) -> Result<std::sync::Arc<dyn LiveEngine>, String> + Send + Sync,
>;

/// The live sessions of this process (shared by the host and the pickup).
#[derive(Default)]
pub struct LiveRegistry {
    _private: (),
}

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
    /// Not ours / already live / another process holds the claim.
    Skipped { reason: &'static str },
    /// The launcher refused (doctor, session limit, …): the row was reported.
    Disabled { reason: String },
}

/// Claim a `queued` row naming this device (ClaimLock → `sessions.claim` →
/// `sessions.brief` → `prepare(PrepareRequest::Session)` → `deps.launch`).
pub fn launch_queued(row: &SessionRow, deps: &PickupDeps) -> Result<LaunchQueued, String> {
    let _ = (row, deps);
    Err(String::from("not implemented"))
}

/// Feed a session's pending person messages (`consumed_at` NULL, not yet
/// handed) into its live engine, oldest first; a session with NO engine here
/// (idle after a host restart, an agent whose process exited) is resumed
/// natively from its run record with the first message as the prompt. The
/// engine's consume worker calls `sessions.consume` as each one is sent.
/// Returns the delivered message ids.
pub fn deliver_messages(
    session_id: &str,
    rows: &[MessageRow],
    deps: &PickupDeps,
) -> Result<Vec<String>, String> {
    let _ = (session_id, rows, deps);
    Ok(Vec::new())
}
