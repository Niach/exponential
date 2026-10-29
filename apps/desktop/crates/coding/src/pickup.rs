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
use crate::run_registry::RunRecord;
use domain::contract::{
    SESSION_STATUS_ENDED, SESSION_STATUS_IDLE, SESSION_STATUS_QUEUED,
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
    /// EXP-1334: the row's launch options this engine honors (launched
    /// with, or applied live since). A row whose options move past them is
    /// a person's `sessions.configure` ([`options_delta`]); `None` = unknown
    /// (a wake rebuilt from the run record, a foreground run), never
    /// reconfigured.
    pub options: Option<LaunchedOptions>,
    /// EXP-1334: the model the engine runs right now
    /// ([`LiveEngine::model`]); `None` = unknown.
    pub agent_model: Option<String>,
}

/// EXP-1334: a live run's launch options, in the row's terms (the row's
/// pickers as they were when the engine launched — or since applied live —
/// not what the launcher layered over them), so a change is detected ONCE.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LaunchedOptions {
    pub plan_mode: bool,
    /// Trimmed; `None` = the agent's default.
    pub effort: Option<String>,
    pub ultracode: bool,
    pub computer_use: bool,
    /// The team MCP pick the launch resolved (sorted ids,
    /// `PreparedLaunch::mcp_pick`); `None` = unknown, never compared. The
    /// pick is SERVER-ONLY (no synced column), so it is re-read between
    /// turns (`sessions.mcpPicks`) whenever the row moved.
    pub mcp: Option<Vec<String>>,
    /// The row's `updated_at` the MCP pick was last verified at (the
    /// launch's row, then every between-turn check).
    pub mcp_checked_at: Option<String>,
    /// EXP-1280: when this device delivered a plan APPROVAL (leaving plan
    /// mode silently, [`PickupAction::DeliverAnswer`]). The server flips the
    /// row's `plan_mode` in the same write, but the session row may land a
    /// pass after the card: until a row echoes `false`, a row still showing
    /// plan mode is stale, never a person's switch back. The echo clears it
    /// ([`LiveRegistry::settle_options`]); [`REPORT_ECHO_WAIT`] bounds it.
    pub plan_approved: Option<std::time::Instant>,
    /// The row's model (trimmed; `None` = the device's default), switched
    /// LIVE when the row moves (`/model`), written back when the agent's
    /// own moves (a person's `/model` in a terminal or the IDE).
    pub model: Option<String>,
    /// A live model switch this device sent and the engine has not echoed
    /// yet: the engine's (old) model is no move of the agent's own.
    pub model_switching: bool,
    /// The agent's own model move this device reported
    /// (`sessions.report{model}`) and when: until a row ECHOES it, every
    /// row still showing the other value is stale (the pre-report row, or
    /// one of the device's own earlier reports landing after it), never a
    /// person's change. The echo clears it
    /// ([`LiveRegistry::settle_options`]); [`REPORT_ECHO_WAIT`] bounds an
    /// echo that never comes. (EXP-1280: plan mode has no agent-side moves
    /// any more — it is Exponential's flag.)
    pub model_reported: Option<(String, std::time::Instant)>,
    /// The agent's model vocabulary (`CodingAgent::model_values`): only a
    /// model in it is written back. Empty = unknown, any.
    pub models: &'static [&'static str],
}

impl LaunchedOptions {
    /// The options a run launched for `row` honors (`mcp` = the pick the
    /// launch resolved).
    pub fn of(row: &SessionRow, mcp: Option<Vec<String>>) -> Self {
        Self {
            plan_mode: row.plan_mode,
            effort: row_effort(row),
            ultracode: row.ultracode,
            computer_use: row.computer_use,
            mcp,
            mcp_checked_at: row.updated_at.clone(),
            plan_approved: None,
            model: row_model(row),
            model_switching: false,
            model_reported: None,
            models: &[],
        }
    }

    /// EXP-1280: does `row` still show plan mode from BEFORE the approval
    /// this device delivered ([`Self::plan_approved`])? Such a row is stale.
    fn plan_stale(&self, row: &SessionRow) -> bool {
        let approved = self.plan_approved.map(|at| (false, at));
        report_unechoed(approved.as_ref(), &row.plan_mode)
    }

    /// Does `row` still show the model from BEFORE the device reported the
    /// agent's own move (the report not echoed yet)? Such a row is stale.
    fn model_stale(&self, row: &SessionRow) -> bool {
        match (&self.model_reported, row_model(row)) {
            (Some(_), Some(model)) => report_unechoed(self.model_reported.as_ref(), &model),
            _ => false,
        }
    }

    /// A row carrying the reported value IS the echo: the stale marks it
    /// clears are gone for good (a later row showing the other value is a
    /// person's pick).
    fn settle_reports(&mut self, row: &SessionRow) {
        if !row.plan_mode {
            self.plan_approved = None;
        }
        if self.model_reported.as_ref().is_some_and(|(model, _)| Some(model) == row_model(row).as_ref()) {
            self.model_reported = None;
        }
    }

    /// The vocabulary of the agent the run is.
    pub fn for_agent(self, agent: crate::agent::CodingAgent) -> Self {
        Self { models: agent.model_values(), ..self }
    }

    fn knows_model(&self, model: &str) -> bool {
        self.models.is_empty() || self.models.contains(&model)
    }
}

/// How long a reported value keeps the rows still showing the old one
/// stale when no echo arrives (a sync hiccup): past it the rows speak for
/// themselves again.
const REPORT_ECHO_WAIT: std::time::Duration = std::time::Duration::from_secs(30);

/// `value` is the OLD one: a report of the other value went out less than
/// [`REPORT_ECHO_WAIT`] ago and no row has echoed it yet.
fn report_unechoed<T: PartialEq>(reported: Option<&(T, std::time::Instant)>, value: &T) -> bool {
    reported.is_some_and(|(reported, at)| reported != value && at.elapsed() < REPORT_ECHO_WAIT)
}

fn row_model(row: &SessionRow) -> Option<String> {
    row.model.as_deref().map(str::trim).filter(|model| !model.is_empty()).map(str::to_string)
}

fn row_effort(row: &SessionRow) -> Option<String> {
    row.effort.as_deref().map(str::trim).filter(|effort| !effort.is_empty()).map(str::to_string)
}

/// EXP-1334: what a row's options ask of its live engine.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OptionsDelta {
    /// Plan mode moved (a person's `sessions.configure`): switched LIVE —
    /// at once, even mid-turn (EXP-1280: Exponential's flag + one steered
    /// line to the agent).
    pub plan_mode: Option<bool>,
    /// The model moved (`sessions.configure{model}`): switched LIVE (the
    /// `model` config option, `/model`) at once. A cleared row model never
    /// moves it.
    pub model: Option<String>,
    /// Effort, ultracode or computer use moved: launch flags, applied by a
    /// between-turn restart that resumes the SAME run.
    pub restart: bool,
    /// The row moved since the MCP pick was last verified: re-read it
    /// between turns (a changed pick restarts too).
    pub mcp_due: bool,
}

/// EXP-1334: the row's options against the ones its engine honors.
pub fn options_delta(row: &SessionRow, launched: &LaunchedOptions) -> OptionsDelta {
    let model_stale = launched.model_stale(row);
    let stale = launched.plan_stale(row);
    OptionsDelta {
        plan_mode: (row.plan_mode != launched.plan_mode && !stale).then_some(row.plan_mode),
        model: row_model(row).filter(|model| !model_stale && Some(model) != launched.model.as_ref()),
        restart: row_effort(row) != launched.effort
            || row.ultracode != launched.ultracode
            || row.computer_use != launched.computer_use,
        mcp_due: launched.mcp.is_some() && row.updated_at != launched.mcp_checked_at,
    }
}

/// EXP-1334: whether a live session's row asks for a reconfigure NOW (a
/// [`PickupAction::Switch`] with an empty `account`): a plan-mode move at
/// once; a restart, or an MCP re-check of an idle row, only with a turn to
/// carry it (`has_messages`) — options apply at the NEXT turn, never
/// interrupting one. EXP-1280: `plan_card` = an approved plan card of this
/// session is being delivered this pass ([`PickupAction::DeliverAnswer`]),
/// or one of its own plan cards is still open — unanswered, or answered
/// and not handed yet ([`is_open_own_plan_card`]): the server's own
/// `plan_mode=false` flip is (or is about to be) that approval, which
/// leaves plan mode silently — never a person's switch. The session row
/// and the card reach the device through separate shapes, so the flipped
/// row may land a pass before the answered card: an off-flip waits for the
/// card rather than steer "Plan mode is off: go ahead." as a turn of its own
/// (the agent would build without the answers, and the approval arriving
/// next would find a turn running).
fn reconfigure_due(row: &SessionRow, view: &LiveView, has_messages: bool, plan_card: bool) -> bool {
    let Some(launched) = &view.options else {
        return false;
    };
    let delta = options_delta(row, launched);
    delta.plan_mode.is_some_and(|on| on || !plan_card)
        || delta.model.is_some()
        || agent_model_move(view).is_some()
        || (has_messages && (delta.restart || (delta.mcp_due && !row.busy)))
}

/// EXP-1334: the model the agent moved to ON ITS OWN (a person's `/model`
/// typed into the run) — one in its vocabulary past the honored one, a live
/// switch of this device's not pending.
fn agent_model_move(view: &LiveView) -> Option<String> {
    let launched = view.options.as_ref()?;
    if launched.model_switching {
        return None;
    }
    view.agent_model
        .clone()
        .filter(|model| launched.knows_model(model) && Some(model) != launched.model.as_ref())
}

/// EXP-1334: report the agent's own model move in a `sessions.report
/// {model}` and honor it, the reported value remembered
/// ([`LaunchedOptions::model_reported`]) so every row still showing the old
/// one (the pre-report row AND the device's own earlier reports landing
/// after it) never reads as a person's change (a resume then launches on
/// the row's model). Best-effort: an unlanded report is retried at the next
/// pass. EXP-1280: plan mode has no agent-side moves (Exponential's flag).
fn sync_agent_state(row: &SessionRow, engine: &dyn LiveEngine, deps: &PickupDeps) {
    let session_id = row.id.as_str();
    let Some(launched) = deps.live.options(session_id) else { return };
    let view = LiveView { options: Some(launched), agent_model: engine.model(), ..LiveView::default() };
    let Some(model) = agent_model_move(&view) else { return };
    let report = session_device::SessionReport { model: Some(model.clone()), ..session_device::SessionReport::default() };
    if let Err(err) = session_device::report(&deps.coding.trpc, session_id, &report) {
        log::warn!("pickup: session {session_id}'s own model not reported: {err}");
        return;
    }
    log::info!("pickup: session {session_id} model {model} (the agent's own)");
    // EXP-1281: the run record follows too, so a resume with no row at hand
    // (a wake, a switch racing the report's echo) keeps the person's model.
    record_model(deps, session_id, &model);
    let now = std::time::Instant::now();
    deps.live.update_options(session_id, |options| {
        options.model = Some(model.clone());
        options.model_reported = Some((model, now));
    });
}

/// EXP-1281: stamp `model` on the persisted run record (the resume's
/// fallback when the row says none).
fn record_model(deps: &PickupDeps, session_id: &str, model: &str) {
    crate::run_registry::update(&deps.coding.data_dir, session_id, |record| {
        let moved = record.model != model;
        record.model = model.to_string();
        moved
    });
}

/// EXP-1281: the model a live engine honors right now — a person's row pick
/// being switched in (`picked`), else the agent's own move (a `/model` typed
/// into the run, reported or not), else the one it launched with / last
/// switched to. `None` = unknown (a wake with no row at hand).
fn honored_model(engine: &dyn LiveEngine, picked: Option<String>, launched: Option<LaunchedOptions>) -> Option<String> {
    let launched = launched?;
    picked.or_else(|| {
        let view = LiveView { options: Some(launched.clone()), agent_model: engine.model(), ..LiveView::default() };
        agent_model_move(&view).or(launched.model)
    })
}

/// One pass's input.
#[derive(Debug, Clone, Copy)]
pub struct PickupInput<'a> {
    /// THIS host's steer device id.
    pub device_id: &'a str,
    /// Every visible `sessions` row.
    pub sessions: &'a [SessionRow],
    /// Every visible `messages` row, KEYED through its conversation (NEW-8
    /// `domain::message_conversation::key_messages_by_session`: `session_id`
    /// = the session its conversation belongs to, never the row's own).
    pub messages: &'a [MessageRow],
    /// Sessions with an engine attached in this process.
    pub live: &'a HashMap<String, LiveView>,
    /// Sessions this host is claiming/launching/resuming right now (no
    /// second action for them until the attempt settles).
    pub in_flight: &'a HashSet<String>,
    /// EXP-1341: persistent roots NOT to revive this pass
    /// ([`LiveRegistry::revive_held`]: already revived or run in this
    /// process, backing off after a failed attempt, or given up).
    /// EXP-1374: the same set holds an interrupted run's
    /// [`PickupAction::Continue`] (once per absence).
    pub revive_held: &'a HashSet<String>,
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
    /// (native resume) with the first of `messages` as the seed prompt
    /// (consumed once sent); the rest are handed once it is live. FEED-129:
    /// EVERY pending message rides along (oldest first) — a wake refused
    /// for an older set is retried the moment a NEW message arrives, which
    /// the oldest alone could never show ([`LiveRegistry::refused_before`]).
    Resume { session: SessionRow, messages: Vec<MessageRow> },
    /// EXP-1341: a PERSISTENT root (`domain::session_kind`, an agent) on
    /// this device reading `running`/`idle` with NO engine here and nothing
    /// pending (its device came back, its process is gone): [`revive_session`]
    /// resumes it from its run record with no prompt (it idles, waiting), or
    /// with no record launches it from its brief. Once per absence
    /// ([`LiveRegistry::revive_held`]).
    Revive { session: SessionRow },
    /// EXP-1374: a NON-persistent `running`/`idle` row on this device with
    /// no engine here and nothing pending whose run was INTERRUPTED mid-turn
    /// (`blocked.kind` `device_offline` | `restarted`, `blocked.resume`):
    /// [`continue_interrupted`] resumes the SAME row from its run record
    /// with [`continue_prompt`] as a device-made seed turn (nothing to
    /// consume). Once per absence ([`LiveRegistry::revive_held`]).
    Continue { session: SessionRow },
    /// Pending person messages of a LIVE session not yet handed to its
    /// engine, oldest first: `EngineSession::deliver(id, body)` each.
    Deliver { session_id: String, messages: Vec<MessageRow> },
    /// A message handed to the engine but still unconsumed whose row is gone
    /// (`sessions.unqueue` deleted it): drop it from the engine's queue.
    Revoke { session_id: String, message_id: String },
    /// A question/plan card of a live session answered in the card
    /// (`answered_at` set): feed the answer/decision to the engine.
    Answer { session_id: String, message_id: String, question_id: String, card: serde_json::Value },
    /// EXP-1280: a TOOL-POSTED card of a live session answered in the card
    /// ([`is_answered_tool_card`]: `exponential_plan_propose` /
    /// `exponential_sessions_ask`, no open ask behind it): the answer goes
    /// in as the agent's next turn, `text` =
    /// [`domain::plan_card::answer_text`]. `approved` (a plan approval)
    /// leaves plan mode SILENTLY first; `fresh` (an approval in a fresh
    /// context) takes the turn in a fresh conversation. The row is consumed
    /// like any delivery ([`LiveEngine::deliver_answer`]).
    DeliverAnswer { session_id: String, message_id: String, text: String, approved: bool, fresh: bool },
    /// A live session whose row is `ended` or gone from the shape: kill its
    /// engine with [`KILL_OUTCOME`] (replaces the kill watch / kill poll).
    Kill { session_id: String },
    /// EXP-849: a live session whose row's `account` moved past the one its
    /// engine runs for (a person's switch, `sessions.send{sessionId,
    /// account}`): [`switch_session`] suspends it between turns and resumes
    /// the SAME row on `account`, the oldest pending message (if any) as
    /// the seed turn. Its deliveries wait for the switch.
    ///
    /// EXP-1334: an EMPTY `account` = a RECONFIGURE (the row's options moved
    /// past its engine's, [`options_delta`]): the login stays, plan mode
    /// switches live, the rest restarts the run between turns.
    Switch { session: SessionRow, account: String, messages: Vec<MessageRow> },
    /// EXP-1252: a live engine here whose row names ANOTHER device (an
    /// agent moved in Settings): stop it with [`RELEASED_OUTCOME`] — it
    /// writes nothing to the row (one writer). NEW-1: a session's Move to
    /// device ENDS it here (a Kill) and continues it there server-side.
    Release { session_id: String },
}

/// EXP-1252: [`crate::pickup::PickupAction::Release`]'s outcome
/// (`engine::RELEASED_OUTCOME`, mirrored here: `coding` cannot depend on
/// `engine`).
pub const RELEASED_OUTCOME: &str = "released";

impl PickupAction {
    /// The session the action is about.
    pub fn session_id(&self) -> &str {
        match self {
            Self::Launch { session }
            | Self::Resume { session, .. }
            | Self::Revive { session }
            | Self::Continue { session }
            | Self::Switch { session, .. } => &session.id,
            Self::Deliver { session_id, .. }
            | Self::Revoke { session_id, .. }
            | Self::Answer { session_id, .. }
            | Self::DeliverAnswer { session_id, .. }
            | Self::Kill { session_id }
            | Self::Release { session_id } => session_id,
        }
    }
}

/// Decide one pass. Pure.
///
/// - **Kill**: a live session whose row is `ended` or gone from the shape
///   (hosts run a pass only once their shapes are up to date, so "gone" is
///   a deletion, never a cold store).
/// - **Launch**: a `queued` row naming this device with no engine here.
/// - **Resume**: a `running`/`idle` row naming this device with no engine
///   here and a pending person message (every pending message rides along,
///   oldest first).
/// - **Revive** (EXP-1341): the same without a pending message, for a
///   PERSISTENT root only (an agent never ends: its device revives it when
///   it comes back), unless [`PickupInput::revive_held`] names it.
/// - **Continue** (EXP-1374): the same for a NON-persistent row its device
///   left mid-turn (`blocked` = an interruption with `resume: true`,
///   [`SessionRow::continue_interruption`]), unless
///   [`PickupInput::revive_held`] names it. A pending input wins (Resume:
///   its report clears the flag).
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
    // EXP-1280: answered tool-posted cards, worded as the agent's next turn
    // (`domain::plan_card::answer_text`), oldest first per session.
    let answered_cards: Vec<MessageRow> = input
        .messages
        .iter()
        .filter(|m| is_answered_tool_card(m))
        .map(|m| MessageRow { body: Some(answered_card_text(m)), ..m.clone() })
        .collect();
    let mut answered: HashMap<&str, Vec<&MessageRow>> = HashMap::new();
    for card in &answered_cards {
        answered.entry(card.session_id.as_str()).or_default().push(card);
    }
    for queue in answered.values_mut() {
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
        // EXP-1252: a row naming another device is RELEASED — nothing here
        // may write it any more, not even its end.
        if rows.get(session_id).is_some_and(|row| !row.on_device(input.device_id)) {
            killed.insert(session_id);
            actions.push(PickupAction::Release { session_id: session_id.to_string() });
            continue;
        }
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
            // EXP-1280: an answered tool card wakes its session too (its
            // engine went away while the card waited). FEED-129: the whole
            // pending set rides the Resume, inputs first, then cards.
            let mut queue: Vec<MessageRow> = pending
                .get(row.id.as_str())
                .map(|queue| queue.iter().map(|m| (*m).clone()).collect())
                .unwrap_or_default();
            queue.extend(answered.get(row.id.as_str()).into_iter().flatten().map(|m| (*m).clone()));
            if !queue.is_empty() {
                starts.push((row, PickupAction::Resume { session: row.clone(), messages: queue }));
            } else if row.is_root() && row.is_persistent() && !input.revive_held.contains(&row.id) {
                starts.push((row, PickupAction::Revive { session: row.clone() }));
            } else if !row.is_persistent()
                && row.continue_interruption().is_some()
                && !input.revive_held.contains(&row.id)
            {
                starts.push((row, PickupAction::Continue { session: row.clone() }));
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
        // EXP-1280: the session's answered tool cards not handed yet.
        let cards: Vec<&MessageRow> = answered
            .get(session_id)
            .map(|queue| {
                queue
                    .iter()
                    .copied()
                    .filter(|m| !view.handed.contains(&m.id) && !view.open_cards.contains_key(&m.id))
                    .collect()
            })
            .unwrap_or_default();
        // EXP-1280: an approval delivered this pass, or an own plan card
        // still open (unanswered, or answered and not handed): the row's
        // off-flip is the server's, never a person's — and may land first.
        let plan_card = cards.iter().any(|m| m.card.as_ref().is_some_and(card_approved))
            || input
                .messages
                .iter()
                .any(|m| m.session_id == session_id && is_open_own_plan_card(m) && !view.handed.contains(&m.id));
        // #EXP-1383: an agent's leading `/new` is no turn a switch could
        // carry ([`switch_session`] keeps it pending for [`deliver`]'s
        // restart): planning one for it would loop on `AlreadyOn` forever.
        let carries_turn = rows.get(session_id).map_or(!fresh.is_empty(), |row| {
            fresh.first().is_some_and(|m| !(row.is_persistent() && is_new_conversation(m)))
        });
        let switch =
            rows.get(session_id).and_then(|row| switch_target(row, view, carries_turn).map(|account| (*row, account)));
        // EXP-1334: else the row's options moved past the engine's.
        let switch = switch.or_else(|| {
            rows.get(session_id)
                .filter(|row| reconfigure_due(row, view, carries_turn, plan_card))
                .map(|row| (*row, String::new()))
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
            // FEED-140: a plan the agent replaced answers nothing.
            if card_superseded(card) {
                continue;
            }
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
        for row in cards {
            let card = row.card.as_ref();
            answers.push((
                row,
                PickupAction::DeliverAnswer {
                    session_id: session_id.to_string(),
                    message_id: row.id.clone(),
                    text: row.body.clone().unwrap_or_default(),
                    approved: card.is_some_and(card_approved),
                    fresh: card.is_some_and(card_fresh),
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

/// EXP-1280: a card the session posted ITSELF through an Exponential tool
/// (`exponential_plan_propose` / `exponential_sessions_ask`: kind `plan` |
/// `question`, authored by the session, no `questionId` — no open ask
/// behind it) that a person answered (`answered_at`) and the device has not
/// consumed yet. Its answer is the agent's next turn
/// ([`PickupAction::DeliverAnswer`], [`answered_card_text`]).
pub fn is_answered_tool_card(message: &MessageRow) -> bool {
    let Some(card) = message.card.as_ref() else { return false };
    matches!(message.card_kind(), Some("plan" | "question"))
        && !card_superseded(card)
        && message.answered_at.as_deref().is_some_and(|at| !at.is_empty())
        && message.consumed_at.as_deref().is_none_or(str::is_empty)
        && message.author_session_id.as_deref() == Some(message.session_id.as_str())
        && card
            .get("questionId")
            .and_then(serde_json::Value::as_str)
            .is_none_or(str::is_empty)
}

/// EXP-1280: the turn an answered tool card reaches the agent as
/// (`fixtures/plan-card.json` `answerText`).
pub fn answered_card_text(message: &MessageRow) -> String {
    message.card.as_ref().map(domain::plan_card::answer_text).unwrap_or_default()
}

/// EXP-1280: a plan card answered `approved`.
fn card_approved(card: &serde_json::Value) -> bool {
    card.get("kind").and_then(serde_json::Value::as_str) == Some("plan")
        && card.get("decision").and_then(serde_json::Value::as_str) == Some("approved")
}

/// FEED-140: a plan card the SERVER closed because the agent posted a newer
/// plan (`fixtures/ask-dock.json` decision `superseded`). Nobody answered
/// it: it is never delivered — not as an approval, not as an answer, never
/// as a wake — and it holds no plan open.
fn card_superseded(card: &serde_json::Value) -> bool {
    card.get("kind").and_then(serde_json::Value::as_str) == Some("plan")
        && card.get("decision").and_then(serde_json::Value::as_str) == Some("superseded")
}

/// EXP-1280: a plan card approved "in a fresh context" (`card.fresh`).
fn card_fresh(card: &serde_json::Value) -> bool {
    card_approved(card) && card.get("fresh").and_then(serde_json::Value::as_bool) == Some(true)
}

/// EXP-1280: a plan card the session posted ITSELF (`exponential_plan_propose`,
/// the [`is_answered_tool_card`] shape) that the device has not consumed —
/// unanswered, or answered and not yet delivered. While one is open the
/// server's `plan_mode=false` flip belongs to its approval
/// ([`reconfigure_due`]).
fn is_open_own_plan_card(message: &MessageRow) -> bool {
    let Some(card) = message.card.as_ref() else {
        return false;
    };
    message.card_kind() == Some("plan")
        && !card_superseded(card)
        && message.consumed_at.as_deref().is_none_or(str::is_empty)
        && message.author_session_id.as_deref() == Some(message.session_id.as_str())
        && card.get("questionId").and_then(serde_json::Value::as_str).is_none_or(str::is_empty)
}

/// EXP-1329: the text a pending input reaches the agent as. A person's
/// message is its body; a child's `report` card reads `Report from <child>
/// (session <id>, ended): <summary>` (the summary, else the body), any other
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
        return format!("Report from {name} (session {from}, ended): {summary}");
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
    /// EXP-1252: TRULY idle — between turns, no backgrounded subagent still
    /// running, no open question/plan card. A move waits for it.
    fn quiescent(&self) -> bool {
        self.is_idle() && self.open_cards().is_empty()
    }
    /// EXP-1292: stop the PROCESS but not the session (the row idles, the
    /// run's workspace — a scratch dir included — is kept) for an immediate
    /// re-spawn of the same row on another account.
    fn suspend(&self);
    /// EXP-1280: an answered tool-posted card ([`PickupAction::DeliverAnswer`])
    /// as the agent's next turn: `approved` leaves plan mode SILENTLY first
    /// (no steered line), `fresh` takes the turn in a fresh context (claude
    /// `/clear` first). Consumed like [`Self::deliver`]. The default (a host
    /// with no plan flag) delivers it as a message.
    fn deliver_answer(&self, message_id: String, text: String, _approved: bool, _fresh: bool) {
        self.deliver(message_id, text);
    }
    /// EXP-1334: switch plan mode LIVE (a person's `sessions.configure`).
    /// EXP-1280: Exponential's flag (never the CLI's): a real move steers
    /// the agent ONE line.
    fn set_plan_mode(&self, _on: bool) {}
    /// EXP-1334: whether the run is in plan mode right now (its last
    /// `config_state` mode); `None` = unknown.
    fn plan_mode(&self) -> Option<bool> {
        None
    }
    /// EXP-1334: switch the model LIVE (the `model` config option, what
    /// `/model <alias>` does) — a person's `sessions.configure{model}`.
    fn set_model(&self, _model: &str) {}
    /// EXP-1334: the model the run is on right now (its `config_state`
    /// `model` value); `None` = unknown.
    fn model(&self) -> Option<String> {
        None
    }
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
    /// EXP-1341: persistent roots and their revive state in THIS process
    /// ([`LiveRegistry::revive_held`]).
    revives: std::sync::Mutex<HashMap<String, ReviveState>>,
    /// #EXP-1383: agents whose `/new` restart is in flight on its own
    /// thread — one restart per `/new`, never a second for the same one.
    restarting: std::sync::Mutex<HashSet<String>>,
    /// S-4: since when each live engine has been quiescent with nothing
    /// handed ([`LiveRegistry::release_idle`]).
    quiet_since: std::sync::Mutex<HashMap<String, std::time::Instant>>,
}

/// S-4: how long a quiescent live session keeps its agent process
/// (`codingSession.idleReleaseMinutes`).
pub const IDLE_RELEASE: std::time::Duration =
    std::time::Duration::from_millis(domain::contract::CODING_SESSION_IDLE_RELEASE_MS as u64);

/// EXP-1341: one persistent root's revive state in this process. An engine
/// that ran here (launched, resumed or revived) settles it for good: once
/// per absence = once per host process, so an agent whose process exits
/// idle later waits for its next message instead of looping.
#[derive(Debug, Clone, Copy)]
struct ReviveState {
    attempts: u32,
    not_before: std::time::Instant,
    settled: bool,
}

/// EXP-1341: the wait after the n-th failed revive (30 s, 1, 2, 4 min).
fn revive_backoff(attempts: u32) -> std::time::Duration {
    std::time::Duration::from_secs(30u64 << attempts.saturating_sub(1).min(4))
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
    /// [`LiveView::options`].
    options: Option<LaunchedOptions>,
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
                options: None,
            },
        );
        self.refused
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(session_id);
        self.settle_revive(session_id);
    }

    /// EXP-1341: the persistent roots [`plan`] must not revive right now —
    /// one an engine ran for in this process, one backing off after a
    /// failed attempt, or one given up after [`MAX_LAUNCH_ATTEMPTS`].
    pub fn revive_held(&self) -> HashSet<String> {
        let now = std::time::Instant::now();
        locked(&self.revives)
            .iter()
            .filter(|(_, state)| state.settled || state.attempts >= MAX_LAUNCH_ATTEMPTS || now < state.not_before)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Count a revive attempt (the backoff starts now); returns the count.
    fn note_revive(&self, session_id: &str) -> u32 {
        let mut revives = locked(&self.revives);
        let state = revives.entry(session_id.to_string()).or_insert(ReviveState {
            attempts: 0,
            not_before: std::time::Instant::now(),
            settled: false,
        });
        state.attempts += 1;
        state.not_before = std::time::Instant::now() + revive_backoff(state.attempts);
        state.attempts
    }

    /// Never revive `session_id` again in this process.
    fn settle_revive(&self, session_id: &str) {
        locked(&self.revives)
            .entry(session_id.to_string())
            .and_modify(|state| state.settled = true)
            .or_insert(ReviveState {
                attempts: 0,
                not_before: std::time::Instant::now(),
                settled: true,
            });
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

    /// S-4: suspend every engine that sat quiescent ([`LiveEngine::quiescent`],
    /// nothing handed, no `/new` restart in flight) for `after` — the
    /// process goes, the row idles, nothing ends and the workspace stays;
    /// the next message resumes the SAME row ([`PickupAction::Resume`]).
    /// Each host calls it on its own clock; returns the released ids.
    pub fn release_idle(&self, now: std::time::Instant, after: std::time::Duration) -> Vec<String> {
        let restarting = locked(&self.restarting).clone();
        let entries = self.lock();
        let mut quiet_since = locked(&self.quiet_since);
        quiet_since.retain(|id, _| entries.contains_key(id));
        let mut released = Vec::new();
        for (id, entry) in entries.iter() {
            let quiet = !entry.engine.is_done()
                && entry.handed.is_empty()
                && !restarting.contains(id)
                && entry.engine.quiescent();
            if !quiet {
                quiet_since.remove(id);
                continue;
            }
            let since = *quiet_since.entry(id.clone()).or_insert(now);
            if now.saturating_duration_since(since) >= after {
                quiet_since.remove(id);
                entry.engine.suspend();
                released.push(id.clone());
            }
        }
        released.sort_unstable();
        released
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
            .iter_mut()
            .filter(|(_, entry)| !entry.engine.is_done())
            .map(|(id, entry)| {
                let agent_model = entry.engine.model();
                // EXP-1334: a live switch is through once the engine says it.
                if let Some(options) = entry.options.as_mut() {
                    if agent_model.is_some() && agent_model == options.model {
                        options.model_switching = false;
                    }
                }
                let mut open_cards = entry.engine.open_cards();
                open_cards.retain(|message_id, _| !entry.answered.contains(message_id));
                (
                    id.clone(),
                    LiveView {
                        handed: entry.handed.clone(),
                        open_cards,
                        row_account: entry.row_account.clone(),
                        running_account: entry.running_account.clone(),
                        options: entry.options.clone(),
                        agent_model,
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

    /// EXP-1334: the options the session's engine honors
    /// ([`LiveView::options`]).
    pub fn set_options(&self, session_id: &str, options: Option<LaunchedOptions>) {
        if let Some(entry) = self.lock().get_mut(session_id) {
            entry.options = options;
        }
    }

    /// [`LiveView::row_account`] of a session's engine.
    fn row_account(&self, session_id: &str) -> Option<String> {
        self.lock().get(session_id).and_then(|entry| entry.row_account.clone())
    }

    /// EXP-1334: [`LiveView::options`] of a session's engine.
    pub fn options(&self, session_id: &str) -> Option<LaunchedOptions> {
        self.lock().get(session_id).and_then(|entry| entry.options.clone())
    }

    /// EXP-1334: edit the honored options in place (a live plan switch, a
    /// verified MCP pick); nothing when unknown.
    fn update_options(&self, session_id: &str, edit: impl FnOnce(&mut LaunchedOptions)) {
        if let Some(options) = self.lock().get_mut(session_id).and_then(|entry| entry.options.as_mut()) {
            edit(options);
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

    /// EXP-1334: a row that echoes the device's own model report
    /// ([`sync_agent_state`]) clears its stale mark
    /// ([`LaunchedOptions::model_reported`]): from then on a row showing the
    /// other value is a person's pick. Hosts call it with the pass's rows
    /// before [`Self::live_views`].
    pub fn settle_options(&self, sessions: &[SessionRow]) {
        let mut entries = self.lock();
        for row in sessions {
            if let Some(options) = entries.get_mut(row.id.as_str()).and_then(|entry| entry.options.as_mut()) {
                options.settle_reports(row);
            }
        }
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
        fresh: Option<bool>,
        feedback: Option<String>,
    ) -> Option<api::sessions::AnswerInput> {
        let mut entries = self.lock();
        let entry = entries.get_mut(session_id)?;
        let message_id = card_message_for(&entry.engine.open_cards(), question_id)?;
        if !entry.answered.insert(message_id.clone()) {
            return None;
        }
        Some(api::sessions::AnswerInput { message_id, answer, decision, fresh, feedback, ..Default::default() })
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
    brief_issues(brief).into_iter().map(|(identifier, _)| identifier).collect()
}

/// EXP-1341: [`brief_issue_identifiers`] with each line's title
/// (`- <IDENTIFIER> <title>` → `(identifier, title)`, the title trimmed):
/// what the launcher's label names a session by (`domain::session_title`).
pub fn brief_issues(brief: &str) -> Vec<(String, String)> {
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
        .filter_map(|line| {
            let line = line.trim();
            let (identifier, title) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
            is_issue_identifier(identifier).then(|| (identifier.to_string(), title.trim().to_string()))
        })
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
#[derive(Clone)]
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
    // EXP-1374 (FEED-135): the last attempt's failure is said as the
    // session's reply and the row idles — the next message re-launches it
    // in full (no run record was written, so the wake is a launch).
    let gave_up = |reason: String| {
        if attempt >= MAX_LAUNCH_ATTEMPTS {
            deps.live.settle(&row.id);
            fail_start(&row.id, &reason, &[], deps);
        }
        reason
    };
    let mut waits = CLAIM_RETRY.iter();
    loop {
        match session_device::claim(&deps.coding.trpc, &row.id, Some(&deps.device_id), None) {
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
            return Err(gave_up(err.to_string()));
        }
        Err(err) => {
            deps.live.settle(&row.id);
            let reason = format!("Couldn't start this session: {err}");
            refuse(row.is_persistent(), &row.id, &reason, false, deps);
            return Err(reason);
        }
    };
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
    let prepared = crate::prepare(&PrepareRequest::Session(req), &deps.coding).map(|prepared| match prepared {
        Prepared::Ready(prepared) => Ok(prepared),
        Prepared::Disabled(reason) => Err(reason.message()),
    });
    match prepared {
        Ok(Ok(prepared)) => {
            deps.live.settle(&row.id);
            let honored = Some(crate::agent_profiles::profile_id(row.account.as_deref()));
            spawn(&row.id, row.is_persistent(), prepared, claim, false, honored, Some(row), deps)?;
            Ok(LaunchQueued::Launched { session_id: row.id.clone() })
        }
        Ok(Err(reason)) => {
            deps.live.settle(&row.id);
            refuse(row.is_persistent(), &row.id, &reason, false, deps);
            Ok(LaunchQueued::Disabled { reason })
        }
        Err(err) if prepare_transient(&err) => Err(gave_up(err.to_string())),
        // EXP-1374 (FEED-135): a failed prepare (worktree, checkout, a parent
        // branch not pushed) never ends the thread: replied, idled, and the
        // next message re-launches it.
        Err(err) => {
            deps.live.settle(&row.id);
            let reason = err.to_string();
            fail_start(&row.id, &reason, &[], deps);
            Err(start_failure(&reason))
        }
    }
}

/// EXP-1374 (FEED-135): the reply a failed start posts (it reaches the
/// parent through the bus).
pub fn start_failure(error: &str) -> String {
    format!("Could not start: {error}")
}

/// EXP-1374 (FEED-135): a start that failed AFTER the claim (worktree
/// creation, checkout, a parent branch not pushed, `.git/config` lock
/// contention) never strands the row: the error is posted as the session's
/// reply, the pending messages the attempt carried (`tried`) are consumed
/// (the parent hears the failure and sends again), and the row is reported
/// idle. No run record exists, so the NEXT message re-launches in full
/// (prepare again: worktree + brief) through [`resume_session`]'s
/// no-record leg.
fn fail_start(session_id: &str, error: &str, tried: &[String], deps: &PickupDeps) {
    let reason = start_failure(error);
    log::warn!("pickup: session {session_id} failed to start: {error}");
    if let Err(err) = session_device::reply(&deps.coding.trpc, session_id, &reason, None) {
        log::warn!("pickup: start failure of session {session_id} not replied: {err}");
    }
    let tried: Vec<String> =
        tried.iter().filter(|id| locked(&deps.live.consuming).insert((*id).clone())).cloned().collect();
    if !tried.is_empty() {
        if let Err(err) = session_device::consume(&deps.coding.trpc, session_id, &tried) {
            log::warn!("pickup: tried messages of session {session_id} not consumed yet: {err}");
            consume_off_thread(deps, session_id, tried, || {});
        }
    }
    if let Err(err) = crate::idle_session_on(&deps.coding.trpc, session_id, Some(&deps.device_id)) {
        log::warn!("pickup: failed start of session {session_id} not reported: {err}");
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
/// the session: [`WakeError::Contended`], nothing is delivered), synthesizes
/// the row's pickers from the run record (use [`resume_session`] when the
/// row is at hand) and fetches a fresh `sessions.brief` for the
/// system-append layers. A refused resume is replied into the conversation,
/// the row reported `idle`, and not retried until a NEW message arrives.
pub fn deliver_messages(
    session_id: &str,
    rows: &[MessageRow],
    deps: &PickupDeps,
) -> Result<Vec<String>, WakeError> {
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
) -> Result<Vec<String>, WakeError> {
    deliver(&row.id, Some(row), rows, deps)
}

/// Why a wake ([`deliver_messages`] / [`resume_session`]) handed nothing.
///
/// `Contended` is the one a host must RETRY on a timer, never park on the
/// row: the holder may be a sibling process of this install on its way out
/// (the app that just self-updated, a daemon being replaced) whose claim
/// goes stale seconds later with no row change to re-open the plan — the
/// N-10 release thread sat idle with a pending message for that reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WakeError {
    /// Another process of this machine holds the session's [`ClaimLock`].
    Contended,
    /// The wake failed (network, git, the launcher); the message says how.
    Other(String),
}

impl std::fmt::Display for WakeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Contended => write!(f, "the session is held by another process on this machine"),
            Self::Other(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for WakeError {}

impl From<String> for WakeError {
    fn from(message: String) -> Self {
        Self::Other(message)
    }
}

/// #EXP-1383: the contract's `/new` steer command (`steerCommands`; the
/// engine's `adapters::NEW_CONVERSATION_COMMAND`, mirrored: `coding` cannot
/// depend on `engine`).
pub const NEW_CONVERSATION_COMMAND: &str = "/new";

/// #EXP-1383: a person's `/new` (trimmed, exactly — the transcript's
/// `dividers.new` rule).
pub fn is_new_conversation(message: &MessageRow) -> bool {
    message.is_from_user()
        && message.body.as_deref().is_some_and(|body| body.trim() == NEW_CONVERSATION_COMMAND)
}

/// #EXP-1383: the session's run here is an agent's conversation ROOT (its
/// record's kind; a child of an agent's tree is a thread run and keeps the
/// in-process `/new`).
fn agent_root_run(deps: &PickupDeps, session_id: &str) -> bool {
    crate::run_registry::get(&deps.coding.data_dir, session_id)
        .is_some_and(|record| record.kind == crate::run_registry::RunKind::Agent)
}

/// #EXP-1383: restart an agent's live, idle engine for its `/new`, off the
/// caller's thread (a delivery to a live engine runs on the desktop's UI
/// thread): the PROCESS stops (suspended: the row idles, never ends, the
/// workspace stays), then the wake path ([`deliver`] with no engine) sees
/// the `/new` first and launches the same row fresh. A pass racing it is
/// benign: whoever takes the [`ClaimLock`] runs the same fresh launch, the
/// other finds it contended.
fn restart_agent_off_thread(
    session_id: &str,
    engine: std::sync::Arc<dyn LiveEngine>,
    rows: Vec<MessageRow>,
    deps: &PickupDeps,
) {
    if !locked(&deps.live.restarting).insert(session_id.to_string()) {
        return;
    }
    let session_id = session_id.to_string();
    let deps = deps.clone();
    let thread_deps = deps.clone();
    let thread_session = session_id.clone();
    let spawned = std::thread::Builder::new().name("exp-pickup-restart".to_string()).spawn(move || {
        let (deps, session_id) = (thread_deps, thread_session);
        let _done = RestartDone { live: &deps.live, session_id: &session_id };
        engine.suspend();
        if !engine.wait_done(SWITCH_STOP_TIMEOUT) {
            log::warn!("pickup: session {session_id} did not stop for its /new; the next pass tries again");
            return;
        }
        // The old entry (and its claim lock) goes; the fresh run takes its own.
        if deps.live.get(&session_id).is_some_and(|live| live.is_done()) {
            deps.live.remove(&session_id);
        }
        match deliver(&session_id, None, &rows, &deps) {
            Ok(_) => log::info!("pickup: session {session_id} restarted for /new"),
            Err(err) => log::warn!("pickup: restarting session {session_id} for /new: {err}"),
        }
    });
    if let Err(err) = spawned {
        locked(&deps.live.restarting).remove(&session_id);
        log::warn!("pickup: restart thread failed to spawn: {err}");
    }
}

/// Clears a session's in-flight restart mark on every way out.
struct RestartDone<'a> {
    live: &'a LiveRegistry,
    session_id: &'a str,
}

impl Drop for RestartDone<'_> {
    fn drop(&mut self) {
        locked(&self.live.restarting).remove(self.session_id);
    }
}

fn deliver(
    session_id: &str,
    row: Option<&SessionRow>,
    rows: &[MessageRow],
    deps: &PickupDeps,
) -> Result<Vec<String>, WakeError> {
    // EXP-1280: an answered tool card rides a Resume too (its body is the
    // answer text, [`plan`]).
    let mut pending: Vec<&MessageRow> = rows
        .iter()
        .filter(|m| m.session_id == session_id && (is_pending_input(m) || is_answered_tool_card(m)))
        .collect();
    pending.sort_by(|a, b| message_order(a, b));
    deps.live.settle_handed(rows);

    if let Some(engine) = deps.live.get(session_id) {
        if !engine.is_done() {
            let handed = deps.live.handed(session_id);
            pending.retain(|m| !handed.contains(&m.id));
            deps.live.take_owed(&mut pending, &[]);
            // #EXP-1383: an agent's conversation takes `/new` as a RESTART
            // (a fresh launch re-fetches the brief, so changed instructions
            // apply): what came before it is handed as ever, the `/new` and
            // everything behind it wait for the engine to be between turns
            // with nothing still queued, then the restart runs off this
            // (possibly UI) thread.
            if let Some(at) = pending.iter().position(|m| is_new_conversation(m)) {
                if agent_root_run(deps, session_id) {
                    let delivered = hand(&*engine, session_id, &pending[..at], deps);
                    if at == 0 && handed.is_empty() && engine.is_idle() {
                        restart_agent_off_thread(session_id, engine, rows.to_vec(), deps);
                    }
                    return Ok(delivered);
                }
            }
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
        return Err(WakeError::Contended);
    };
    let record = crate::run_registry::get(&deps.coding.data_dir, session_id);
    // EXP-849: only a row AT HAND can ask for a switch (one rebuilt from the
    // record names the record's own login).
    let row_account = row.map(|row| row.account.clone());
    // EXP-1334: likewise only a row at hand names the options a run honors.
    let at_hand = row.is_some();
    let row = match (row, &record) {
        (Some(row), _) => row.clone(),
        (None, Some(record)) => row_from_record(record, &deps.device_id),
        (None, None) => {
            return Err(format!("session {session_id} has no engine and no run record on this device").into())
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
    // #EXP-1383: a `/new` waking an agent's conversation (or the restart
    // the live path began) is a FRESH launch of the same row: a new CLI
    // conversation on the brief just fetched (its system append carries the
    // current instructions), seeded with NOTHING — the agent's brief would
    // replay the conversation's first message — and the `/new` row itself
    // consumed here, never sent (its divider is the message row).
    let restart = row.is_persistent() && is_new_conversation(first);
    let mut brief = brief;
    if restart {
        log::info!("pickup: session {session_id}: /new restarts the agent's conversation");
        brief.brief.clear();
    }
    // EXP-1280: a plan approved "in a fresh context" waking the session
    // starts a NEW conversation (what the live path's `/clear` does) — the
    // stored card says `fresh: true`, and so is the run it wakes.
    let fresh = restart || (is_answered_tool_card(first) && first.card.as_ref().is_some_and(card_fresh));
    let mut resume = record.map(|record| ResumeRun {
        record,
        prompt: if restart { String::new() } else { first.body.clone().unwrap_or_default() },
        message_id: (!restart).then(|| first.id.clone()),
        reconfigure: None,
        fresh,
    });
    let seeded = resume.is_some() && !restart;
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
    let persistent = row.is_persistent();
    // EXP-1374 (FEED-135): no run record = this wake is a full (re)launch;
    // its failure is a failed START — replied, the tried messages consumed,
    // the row idled, and the next message tries the launch again.
    let launching = req.resume.is_none();
    let refused = |reason: String| {
        refuse(persistent, session_id, &reason, true, deps);
        deps.live.note_refused(session_id, &pending);
        reason
    };
    match crate::prepare(&PrepareRequest::Session(req), &deps.coding) {
        Ok(Prepared::Ready(prepared)) => {
            let engine =
                spawn(session_id, persistent, prepared, claim, true, honored, at_hand.then_some(&row), deps)
                    .inspect_err(|_| deps.live.note_refused(session_id, &pending))?;
            let mut delivered = covered;
            let mut rest = pending.clone();
            if seeded {
                // The seed turn IS the first message; the engine consumes it
                // via `PreparedLaunch::seed_message_id`.
                deps.live.mark_handed(session_id, std::slice::from_ref(&first.id));
                delivered.push(first.id.clone());
                rest.remove(0);
            } else if restart {
                // The `/new` did its work: consumed by the pickup itself.
                if locked(&deps.live.consuming).insert(first.id.clone()) {
                    consume_off_thread(deps, session_id, vec![first.id.clone()], || {});
                }
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
        Err(err) if launching => {
            let tried: Vec<String> = pending.iter().map(|m| m.id.clone()).collect();
            fail_start(session_id, &err.to_string(), &tried, deps);
            deps.live.note_refused(session_id, &pending);
            Err(WakeError::Other(start_failure(&err.to_string())))
        }
        Err(err) => Err(refused(format!("Couldn't resume this session: {err}")).into()),
    }
}

/// EXP-1341: revive a PERSISTENT root whose device came back with no
/// engine for it ([`PickupAction::Revive`]): resumed natively from its run
/// record with NO prompt (it waits idle for its next message), or with no
/// record left launched afresh from its brief (an agent's empty brief says
/// to wait). Once per absence: a failure backs off
/// ([`LiveRegistry::revive_held`]) up to [`MAX_LAUNCH_ATTEMPTS`], then the
/// row idles and stays down until a message wakes it; a refusal idles it at
/// once (a persistent session never ends).
pub fn revive_session(row: &SessionRow, deps: &PickupDeps) -> Result<(), String> {
    let session_id = row.id.as_str();
    if deps.live.contains_live(session_id) {
        deps.live.settle_revive(session_id);
        return Ok(());
    }
    let Some(claim) = ClaimLock::try_acquire(&deps.coding.data_dir, session_id)? else {
        // Another process of this machine runs it.
        deps.live.settle_revive(session_id);
        return Ok(());
    };
    let attempt = deps.live.note_revive(session_id);
    let give_up = |reason: String| {
        log::warn!("pickup: reviving session {session_id} failed: {reason}");
        if attempt >= MAX_LAUNCH_ATTEMPTS {
            deps.live.settle_revive(session_id);
            if let Err(err) = crate::idle_session_on(&deps.coding.trpc, session_id, Some(&deps.device_id)) {
                log::warn!("pickup: unrevived session {session_id} not reported: {err}");
            }
        }
        reason
    };
    let brief = session_device::brief(&deps.coding.trpc, session_id)
        .map_err(|err| give_up(format!("brief of session {session_id}: {err}")))?;
    let record = crate::run_registry::get(&deps.coding.data_dir, session_id);
    let mut identifiers = brief_issue_identifiers(&brief.brief);
    if identifiers.is_empty() {
        identifiers = (deps.issue_identifiers)(row.root_id_or_self());
    }
    let req = SessionLaunchRequest {
        session: row.clone(),
        brief,
        issue_identifiers: identifiers,
        resume: record.map(|record| ResumeRun {
            record,
            prompt: String::new(),
            message_id: None,
            reconfigure: None,
            fresh: false,
        }),
        device_label: deps.device_label.clone(),
        options: deps.options.clone(),
    };
    match crate::prepare(&PrepareRequest::Session(req), &deps.coding) {
        Ok(Prepared::Ready(prepared)) => {
            let honored = Some(crate::agent_profiles::profile_id(row.account.as_deref()));
            // A failed spawn is replied and the row idled (`refuse`).
            let spawned = spawn(session_id, true, prepared, claim, true, honored, Some(row), deps);
            deps.live.settle_revive(session_id);
            spawned.map(|_| log::info!("pickup: revived session {session_id}"))
        }
        Ok(Prepared::Disabled(reason)) => {
            deps.live.settle_revive(session_id);
            let reason = reason.message();
            log::warn!("pickup: session {session_id} not revived: {reason}");
            if let Err(err) = crate::idle_session_on(&deps.coding.trpc, session_id, Some(&deps.device_id)) {
                log::warn!("pickup: unrevived session {session_id} not reported: {err}");
            }
            Err(reason)
        }
        Err(err) => Err(give_up(format!("Couldn't revive this session: {err}"))),
    }
}

/// NEW-5: what a host's startup re-claim does with one row of its device
/// that an earlier process left behind with no engine here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReclaimStep {
    /// A pending input: [`plan`]'s Resume replays it.
    Replay,
    /// Mid-turn (`busy`): [`continue_after_restart`].
    Continue,
    /// `running` between turns: report it idle.
    Idle,
    /// Already idle: nothing to say (a persistent root is [`plan`]'s Revive).
    Waiting,
    /// EXP-1374 (FEED-135): claimed by the process that left it, never
    /// spawned (no run record here: the claim consumed its first message,
    /// the worktree or the spawn never came): [`settle_unstarted`] says so
    /// and idles it, so the sender learns it never ran.
    Unstarted,
}

/// FEED-153: the `recorded` input of [`reclaim_step`] — this machine holds
/// the session's run record AND that run got as far as an agent session
/// (`session/new` answered: the engine wrote its ACP / native id). A record
/// `prepare` wrote for a run whose process died before the agent ever had
/// a session (the host restarted or the spawn died mid-start) names nothing
/// to continue: the claim consumed its first message and no turn ran, so it
/// is [`ReclaimStep::Unstarted`] — said and idled — never a silent idle.
pub fn reclaim_recorded(data_dir: &std::path::Path, session_id: &str) -> bool {
    crate::run_registry::get(data_dir, session_id).is_some_and(|record| run_started(&record))
}

/// FEED-153: the record names an agent session (see [`reclaim_recorded`]).
fn run_started(record: &RunRecord) -> bool {
    let set = |id: &Option<String>| id.as_deref().is_some_and(|id| !id.trim().is_empty());
    set(&record.acp_session_id) || set(&record.agent_native_session_id)
}

/// [`ReclaimStep`] for `row` (`pending` = an input waits for it; `recorded`
/// = this machine holds its run record and that run had an agent session,
/// [`reclaim_recorded`]). Pure.
pub fn reclaim_step(row: &SessionRow, pending: bool, recorded: bool) -> ReclaimStep {
    if pending {
        ReclaimStep::Replay
    } else if !recorded && (row.busy || row.status_is(SESSION_STATUS_RUNNING)) {
        ReclaimStep::Unstarted
    } else if row.busy {
        ReclaimStep::Continue
    } else if row.status_is(SESSION_STATUS_RUNNING) {
        ReclaimStep::Idle
    } else {
        ReclaimStep::Waiting
    }
}

/// EXP-1374: the reply a [`ReclaimStep::Unstarted`] session posts.
pub const UNSTARTED_REPLY: &str = "Could not start: the runner restarted before it began. Send it a message to start it again.";

/// [`ReclaimStep::Unstarted`]: replied ([`UNSTARTED_REPLY`], waking its
/// listeners like any refusal), then idle. Prefer [`settle_unstarted_on`]:
/// without the device id the idle skips the server's stale-host guard.
pub fn settle_unstarted(trpc: &api::trpc::TrpcClient, session_id: &str) {
    settle_unstarted_on(trpc, session_id, None);
}

/// [`settle_unstarted`] stamped with THIS host's device id (EXP-1252), so
/// a stale copy of a session that moved to another device idles nothing.
pub fn settle_unstarted_on(trpc: &api::trpc::TrpcClient, session_id: &str, device_id: Option<&str>) {
    log::warn!("pickup: session {session_id} was claimed but never started here");
    if let Err(err) = session_device::reply(trpc, session_id, UNSTARTED_REPLY, None) {
        log::warn!("pickup: unstarted session {session_id} not replied: {err}");
    }
    if let Err(err) = crate::idle_session_on(trpc, session_id, device_id) {
        log::warn!("pickup: unstarted session {session_id} not reported: {err}");
    }
}

/// NEW-5: the continuation turn a session gets when its host restarted
/// while a turn was in flight ([`continue_after_restart`]).
pub const RESTART_CONTINUATION: &str = "The runner restarted while your last turn was in flight, so that turn was cut off. Continue where you left off.";

/// NEW-5: re-claim a session a previous process of this host left MID-TURN
/// (`busy`, no engine here, nothing pending): resumed natively from its run
/// record — the [`resume_session`] machinery — with
/// [`RESTART_CONTINUATION`] as a device-made seed turn (nothing to consume).
/// `Ok(false)` = nothing was spawned and nothing settled (no run record
/// here, or another process holds it): the caller idles the row. A refused
/// prepare or spawn is replied and the row idled ([`refuse`]).
pub fn continue_after_restart(row: &SessionRow, deps: &PickupDeps) -> Result<bool, String> {
    let session_id = row.id.as_str();
    if deps.live.contains_live(session_id) {
        return Ok(true);
    }
    let Some(record) = crate::run_registry::get(&deps.coding.data_dir, session_id) else {
        return Ok(false);
    };
    let Some(claim) = ClaimLock::try_acquire(&deps.coding.data_dir, session_id)? else {
        return Ok(false);
    };
    let brief = session_device::brief(&deps.coding.trpc, session_id)
        .map_err(|err| format!("brief of session {session_id}: {err}"))?;
    spawn_continuation(row, record, claim, brief, RESTART_CONTINUATION.to_string(), deps).map(|()| true)
}

/// EXP-1374: the seed turn of a [`PickupAction::Continue`] — `kind` is the
/// row's `blocked.kind` (`device_offline` | `restarted`).
pub fn continue_prompt(kind: &str) -> String {
    let reason = if kind == "device_offline" { "device offline" } else { "runner restarted" };
    format!("Your run was interrupted ({reason}) and continues now. Pick up exactly where you left off.")
}

/// EXP-1374: [`PickupAction::Continue`] — resume an interrupted mid-turn
/// run from its run record (same worktree/branch, native CLI resume) with
/// [`continue_prompt`] as the seed turn. Once per absence: an engine that
/// comes up settles it ([`LiveRegistry::insert_for`]); a transient failure
/// (the brief) backs off like a revive; no run record here leaves the row
/// alone (the server's `resumeMinutes` rule hands it to a person); a
/// refused prepare or spawn is replied and the row idled ([`refuse`]).
pub fn continue_interrupted(row: &SessionRow, deps: &PickupDeps) -> Result<(), String> {
    let session_id = row.id.as_str();
    if deps.live.contains_live(session_id) {
        deps.live.settle_revive(session_id);
        return Ok(());
    }
    let Some(kind) = row.continue_interruption() else {
        return Ok(());
    };
    let Some(record) = crate::run_registry::get(&deps.coding.data_dir, session_id) else {
        deps.live.settle_revive(session_id);
        log::info!("pickup: interrupted session {session_id} has no run record here — not continued");
        return Ok(());
    };
    let Some(claim) = ClaimLock::try_acquire(&deps.coding.data_dir, session_id)? else {
        // Another process of this machine runs it.
        deps.live.settle_revive(session_id);
        return Ok(());
    };
    let attempt = deps.live.note_revive(session_id);
    let brief = match session_device::brief(&deps.coding.trpc, session_id) {
        Ok(brief) => brief,
        Err(err) => {
            if attempt >= MAX_LAUNCH_ATTEMPTS {
                deps.live.settle_revive(session_id);
            }
            return Err(format!("brief of session {session_id}: {err}"));
        }
    };
    // Settled before the spawn: a refusal is replied and idled, never
    // retried; a spawned engine settles it anyway.
    deps.live.settle_revive(session_id);
    spawn_continuation(row, record, claim, brief, continue_prompt(kind), deps)
        .map(|()| log::info!("pickup: continued interrupted session {session_id} ({kind})"))
}

/// NEW-5 / EXP-1374: resume `row` from `record` with `prompt` as a
/// device-made seed turn (nothing to consume). A refused prepare or spawn
/// is replied and the row idled ([`refuse`]).
fn spawn_continuation(
    row: &SessionRow,
    record: RunRecord,
    claim: ClaimLock,
    brief: session_device::SessionBrief,
    prompt: String,
    deps: &PickupDeps,
) -> Result<(), String> {
    let session_id = row.id.as_str();
    let mut identifiers = (deps.issue_identifiers)(row.root_id_or_self());
    if identifiers.is_empty() {
        identifiers = brief_issue_identifiers(&brief.brief);
    }
    if identifiers.is_empty() {
        identifiers = record.issues.clone();
    }
    let persistent = row.is_persistent();
    let req = SessionLaunchRequest {
        session: row.clone(),
        brief,
        issue_identifiers: identifiers,
        resume: Some(ResumeRun {
            record,
            prompt,
            message_id: None,
            reconfigure: None,
            fresh: false,
        }),
        device_label: deps.device_label.clone(),
        options: deps.options.clone(),
    };
    match crate::prepare(&PrepareRequest::Session(req), &deps.coding) {
        Ok(Prepared::Ready(prepared)) => {
            let honored = Some(crate::agent_profiles::profile_id(row.account.as_deref()));
            // A failed spawn is replied and the row idled (`refuse`).
            spawn(session_id, persistent, prepared, claim, true, honored, Some(row), deps)?;
            Ok(())
        }
        Ok(Prepared::Disabled(reason)) => {
            let reason = reason.message();
            refuse(persistent, session_id, &reason, true, deps);
            Err(reason)
        }
        Err(err) => {
            let reason = format!("Couldn't resume this session: {err}");
            refuse(persistent, session_id, &reason, true, deps);
            Err(reason)
        }
    }
}

/// Run one [`plan`] action against this host. Launch/Resume/Deliver block
/// (network + git) — call off the UI thread; Revoke/Answer/Kill only touch
/// the live engine.
pub fn execute(action: &PickupAction, deps: &PickupDeps) -> Result<(), String> {
    match action {
        PickupAction::Launch { session } => launch_queued(session, deps).map(|_| ()),
        PickupAction::Resume { session, messages } => resume_session(session, messages, deps)
            .map(|_| ())
            .map_err(|err| err.to_string()),
        PickupAction::Revive { session } => revive_session(session, deps),
        PickupAction::Continue { session } => continue_interrupted(session, deps),
        PickupAction::Deliver { session_id, messages } => deliver_messages(session_id, messages, deps)
            .map(|_| ())
            .map_err(|err| err.to_string()),
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
        PickupAction::DeliverAnswer { session_id, message_id, text, approved, fresh } => {
            let Some(engine) = deps.live.get(session_id).filter(|engine| !engine.is_done()) else {
                return Ok(());
            };
            if *approved {
                // The row's echo (`plan_mode=false`, the server's flip on
                // approval) is then no person's switch: no steered line.
                deps.live.update_options(session_id, |options| {
                    options.plan_mode = false;
                    options.plan_approved = Some(std::time::Instant::now());
                });
            }
            // Delivered once: the consumed stamp settles it
            // ([`LiveRegistry::settle_handed`]).
            deps.live.mark_handed(session_id, std::slice::from_ref(message_id));
            log::info!("pickup: session {session_id} answered card {message_id} (approved {approved}, fresh {fresh})");
            engine.deliver_answer(message_id.clone(), text.clone(), *approved, *fresh);
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
        PickupAction::Release { session_id } => {
            release(session_id, deps);
            Ok(())
        }
    }
}

/// EXP-1252: stop a live engine whose row names another device, writing
/// nothing to the row ([`RELEASED_OUTCOME`]).
pub fn release(session_id: &str, deps: &PickupDeps) {
    if let Some(engine) = deps.live.remove(session_id) {
        log::info!("pickup: session {session_id} runs on another device now: releasing its engine");
        engine.kill(RELEASED_OUTCOME);
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
/// [`wake_account`]). Best-effort.
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
    // #EXP-1383: an agent's `/new` is a restart of its own ([`deliver`]):
    // it and everything behind it stay pending for the next pass.
    if row.is_persistent() {
        if let Some(at) = pending.iter().position(|m| is_new_conversation(m)) {
            pending.truncate(at);
        }
    }
    // EXP-1334: an empty target is a RECONFIGURE — the login stays, and the
    // row's account pick (if it moved) waits for its own switch.
    let reconfiguring = target.is_empty();
    // Nothing moves: the row's pick is honored (live entry AND record, so
    // the two agree) and its messages go to the engine as they are.
    let stay = |record: &mut RunRecord, pending: &[&MessageRow]| {
        if !reconfiguring {
            deps.live.set_row_account(session_id, honored.clone());
            honor_row_account(deps, session_id, record, row.account.as_deref());
        }
        hand(&*engine, session_id, pending, deps);
    };
    // EXP-1334: the row's options past the ones the engine honors. Plan mode
    // switches live at once (even mid-turn); the rest needs a restart, which
    // only a turn may carry (a pending message or the caller's prompt).
    sync_agent_state(row, &*engine, deps);
    let delta = deps.live.options(session_id).map(|launched| options_delta(row, &launched));
    let delta = delta.unwrap_or_default();
    if let Some(on) = delta.plan_mode {
        log::info!("pickup: session {session_id} plan mode -> {on} (live)");
        engine.set_plan_mode(on);
        deps.live.update_options(session_id, |options| options.plan_mode = on);
    }
    if let Some(model) = delta.model.clone() {
        log::info!("pickup: session {session_id} model -> {model} (live)");
        engine.set_model(&model);
        deps.live.update_options(session_id, |options| {
            options.model = Some(model);
            options.model_switching = true;
            options.model_reported = None;
        });
    }
    let has_turn = prompt.is_some() || !pending.is_empty();
    let mut restart = has_turn && delta.restart;
    if has_turn && !restart && delta.mcp_due && engine.is_idle() {
        restart = mcp_pick_moved(row, deps);
    }
    let mut account_moves = !reconfiguring && record.account().as_deref() != Some(target.as_str());
    if !account_moves && !restart {
        stay(&mut record, &pending);
        return Ok(Switched::AlreadyOn);
    }
    if account_moves {
        if let Some(reason) = switch_refusal(&record, &target, deps) {
            log::info!("pickup: account switch of {session_id} refused: {reason}");
            if let Err(err) = session_device::reply(&deps.coding.trpc, session_id, &reason, None) {
                log::warn!("pickup: switch refusal of {session_id} not replied: {err}");
            }
            // The row says the login the run keeps.
            report_running_account(deps, session_id, Some(target.as_str()), record.account().as_deref());
            if !restart {
                stay(&mut record, &pending);
                return Ok(Switched::Refused(reason));
            }
            // The options still move, on the login the run keeps.
            deps.live.set_row_account(session_id, honored.clone());
            honor_row_account(deps, session_id, &mut record, row.account.as_deref());
            account_moves = false;
        }
    }
    if !engine.is_idle() {
        return Err(SwitchError::Busy);
    }
    // EXP-1334: a restart carries the plan mode the run is in right now
    // (else the row's pick), the row's pickers authoritative.
    // EXP-1280: plan mode is Exponential's flag — the honored one (the
    // row's, or an approval not echoed yet), never an agent-side move.
    let honored_plan = deps.live.options(session_id).map(|options| options.plan_mode);
    let reconfigure = (restart || delta.plan_mode.is_some()).then(|| crate::launcher::ResumeReconfigure {
        plan_mode: delta.plan_mode.or(honored_plan).unwrap_or(row.plan_mode),
    });
    // The account the new engine honors: the row's on an account move, else
    // what the old one honored (a reconfigure never honors an unmoved pick).
    let row_account = if account_moves || !reconfiguring {
        honored.clone()
    } else {
        deps.live.row_account(session_id)
    };
    // EXP-1281: the model the run honors right now (a person's `/model`
    // whose echo has not landed on `row` yet included) is the one the new
    // engine launches on — never the stale row's, never the launch's.
    let prior = deps.live.options(session_id);
    let model = honored_model(&*engine, delta.model.clone(), prior.clone());
    let mut launch_row = row.clone();
    // Cleared on every way out but a landed move (that run's turn edges
    // take over); the wall the row carried comes back with it.
    let mut switching = SwitchingShown { trpc: account_moves.then_some(&*deps.coding.trpc), session_id, blocked: row_wall(row) };
    if let Some(model) = &model {
        launch_row.model = Some(model.clone());
        record_model(deps, session_id, model);
    }
    if account_moves {
        log::info!("pickup: account switch of {session_id} -> {target}: suspending the run");
        // EXP-1281: every client shows the move as it happens (the card's
        // spinner), a rotation included; the resumed run's first turn edge
        // (or a refusal) clears it.
        report_switching(deps, session_id);
    } else {
        log::info!("pickup: reconfiguring session {session_id}: suspending the run");
    }
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
        refuse(row.is_persistent(), session_id, reason, true, deps);
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
        reconfigure,
        fresh: false,
    };
    let mut options = deps.options.clone();
    // EXP-849: a named account on a resume IS the switch; `None` keeps the
    // recorded login.
    options.account = account_moves.then_some(target);
    let req = SessionLaunchRequest {
        session: launch_row.clone(),
        brief,
        issue_identifiers: identifiers,
        resume: Some(resume),
        device_label: deps.device_label.clone(),
        options,
    };
    let persistent = row.is_persistent();
    match crate::prepare(&PrepareRequest::Session(req), &deps.coding) {
        Ok(Prepared::Ready(prepared)) => {
            let engine = spawn(session_id, persistent, prepared, claim, true, row_account, Some(&launch_row), deps)
                .inspect_err(|_| deps.live.note_refused(session_id, &pending))
                .map_err(failed)?;
            if let Some(model) = model.filter(|model| Some(model) != row_model(row).as_ref()) {
                keep_model(deps, session_id, model, prior.and_then(|prior| prior.model_reported));
            }
            deps.live.mark_handed(session_id, &handed.into_iter().collect::<Vec<_>>());
            let mut rest = pending;
            if let Some(seed) = seed {
                deps.live.mark_handed(session_id, std::slice::from_ref(&seed.id));
                rest.retain(|m| m.id != seed.id);
            }
            hand(&*engine, session_id, &rest, deps);
            // The landed run's turn edges take over the switching state.
            switching.trpc = None;
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

/// EXP-1281: a shown [`report_switching`] still owed its clear — and the
/// usage wall it lifted off the row, owed back on a switch that led
/// nowhere.
struct SwitchingShown<'a> {
    trpc: Option<&'a api::trpc::TrpcClient>,
    session_id: &'a str,
    blocked: Option<session_device::BlockedInput>,
}

impl Drop for SwitchingShown<'_> {
    fn drop(&mut self) {
        if let Some(trpc) = self.trpc.take() {
            report_switch_settled(trpc, self.session_id, self.blocked.take());
        }
    }
}

/// EXP-804: the usage wall `row` carries (`sessions.blocked`, the server's
/// `codingSessionBlockedSchema`), in `sessions.report` terms; `None` = no
/// wall. Marked `handled`: the server already ran its notification verdict
/// on this very wall when the device first reported it, so putting it back
/// is never a NEW wall (no second push).
fn row_wall(row: &SessionRow) -> Option<session_device::BlockedInput> {
    let wall = row.blocked.as_ref().filter(|wall| wall.is_object())?;
    let text = |key: &str| wall.get(key).and_then(serde_json::Value::as_str).unwrap_or_default().to_string();
    Some(session_device::BlockedInput {
        kind: text("kind"),
        agent: text("agent"),
        window: text("window"),
        resets_at: wall.get("resetsAt").and_then(serde_json::Value::as_str).map(str::to_string),
        since: text("since"),
        handled: Some(true),
    })
}

/// EXP-1281: report a session as busy (an account move in flight, the wall
/// lifted: the cards spin). NEW-8: no caption (the server never stores
/// one). Best-effort.
pub fn report_switching(deps: &PickupDeps, session_id: &str) {
    // The wall stays with the login the run leaves.
    let report = session_device::SessionReport {
        busy: Some(true),
        blocked: Some(None),
        ..session_device::SessionReport::default()
    };
    if let Err(err) = session_device::report(&deps.coding.trpc, session_id, &report) {
        log::warn!("pickup: switching state of {session_id} not reported: {err}");
    }
}

/// EXP-1281: clear a [`report_switching`] that led nowhere (no target, a
/// failed switch): not busy — and the wall the switching state
/// lifted (`blocked`, as the row carried it) put back, else a walled run
/// that stays on its login shows no "Usage limit" and its engine's next
/// `blocked` report reads as a brand-new wall (another push). Best-effort.
pub fn report_switch_settled(trpc: &api::trpc::TrpcClient, session_id: &str, blocked: Option<session_device::BlockedInput>) {
    let report = session_device::SessionReport {
        busy: Some(false),
        blocked: blocked.map(Some),
        ..session_device::SessionReport::default()
    };
    if let Err(err) = session_device::report(trpc, session_id, &report) {
        log::warn!("pickup: settled switch of {session_id} not reported: {err}");
    }
}

/// EXP-1281: a switched run launched on `model` while the row still says
/// another (the agent's `/model` not echoed yet): the row is told (unless an
/// earlier report of it is still in flight), and until it echoes, the stale
/// row never switches the run back.
fn keep_model(deps: &PickupDeps, session_id: &str, model: String, reported: Option<(String, std::time::Instant)>) {
    let reported = match reported.filter(|(value, _)| *value == model) {
        Some(reported) => reported,
        None => {
            let report = session_device::SessionReport {
                model: Some(model.clone()),
                ..session_device::SessionReport::default()
            };
            if let Err(err) = session_device::report(&deps.coding.trpc, session_id, &report) {
                log::warn!("pickup: switched session {session_id}'s model not reported: {err}");
            }
            (model, std::time::Instant::now())
        }
    };
    deps.live.update_options(session_id, |options| options.model_reported = Some(reported));
}

/// EXP-1334: re-read the session's server-only MCP pick between turns and
/// compare it with the one its run launched with. Verified (or unreadable:
/// a blip never restarts a run) = stamped at the row's `updated_at`, so the
/// same row is not re-read; `true` = the pick moved (restart).
fn mcp_pick_moved(row: &SessionRow, deps: &PickupDeps) -> bool {
    let session_id = row.id.as_str();
    let launched = deps.live.options(session_id).and_then(|options| options.mcp);
    match session_device::mcp_picks(&deps.coding.trpc, session_id) {
        Ok(pick) if launched.as_ref() != Some(&pick) => {
            log::info!("pickup: session {session_id}'s MCP pick moved: restarting between turns");
            true
        }
        result => {
            if let Err(err) = result {
                log::warn!("pickup: session {session_id}'s MCP pick not read: {err}");
            }
            deps.live.update_options(session_id, |options| {
                options.mcp_checked_at = row.updated_at.clone();
            });
            false
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
    persistent: bool,
    prepared: crate::PreparedLaunch,
    claim: ClaimLock,
    waking: bool,
    row_account: Option<String>,
    launched_for: Option<&SessionRow>,
    deps: &PickupDeps,
) -> Result<std::sync::Arc<dyn LiveEngine>, String> {
    // EXP-1334: the row's options this engine honors from now on (unknown
    // for a wake with no row at hand).
    let options = launched_for
        .map(|row| LaunchedOptions::of(row, prepared.mcp_pick.clone()).for_agent(prepared.agent));
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
            deps.live.set_options(session_id, options);
            report_running_account(deps, session_id, row_account.as_deref(), running.as_deref());
            Ok(engine)
        }
        Err(err) => {
            let reason = format!("Couldn't start this session: {err}");
            refuse(persistent, session_id, &reason, waking, deps);
            Err(reason)
        }
    }
}

/// Explain a refusal in the conversation (an agent message), then settle
/// the row: a refused LAUNCH ends a thread (a PERSISTENT session idles —
/// it never ends, `domain::session_kind`); a refused WAKE leaves the row
/// `idle`, so a later message can try again.
fn refuse(persistent: bool, session_id: &str, reason: &str, waking: bool, deps: &PickupDeps) {
    log::warn!("pickup: session {session_id} refused: {reason}");
    if let Err(err) = session_device::reply(&deps.coding.trpc, session_id, reason, None) {
        log::warn!("pickup: refusal of session {session_id} not replied: {err}");
    }
    let settled = if persistent || waking {
        crate::idle_session_on(&deps.coding.trpc, session_id, Some(&deps.device_id))
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
        // NEW-2: persistence keys on `agent_id`; the record keeps the kind
        // only, so a recorded agent run stands in its own id (nothing on
        // the device resolves the definition from it).
        agent_id: record.kind.is_persistent().then(|| record.session_id.clone()),
        device_id: Some(device_id.to_string()),
        cli: Some(record.agent.id().to_string()),
        account: record.account(),
        model: some(&record.model),
        effort: some(&record.effort),
        ultracode: record.ultracode,
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
    use crate::run_registry::RunKind;
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
            agent_id: Some("ag-triage".to_string()),
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
        revive_held: HashSet<String>,
    }

    impl Pass {
        fn new(sessions: Vec<SessionRow>, messages: Vec<MessageRow>) -> Self {
            Self {
                sessions,
                messages,
                live: HashMap::new(),
                in_flight: HashSet::new(),
                revive_held: HashSet::new(),
            }
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
                revive_held: &self.revive_held,
            })
        }
    }

    fn handed(ids: &[&str]) -> LiveView {
        LiveView { handed: ids.iter().map(|id| id.to_string()).collect(), ..LiveView::default() }
    }

    // ---- plan ----

    /// EXP-1341: a persistent root (an agent) on this device, running or
    /// idle with no engine and nothing pending, is revived; a thread, a
    /// child, another device's row, a queued/ended row, a live or in-flight
    /// one and one the registry holds are not.
    #[test]
    fn plan_revives_a_persistent_root_once_its_device_is_back() {
        let running = agent("a1", SESSION_STATUS_RUNNING);
        let idle = agent("a2", SESSION_STATUS_IDLE);
        let ended = agent("a3", SESSION_STATUS_ENDED);
        let mut child = agent("a4", SESSION_STATUS_IDLE);
        child.parent_id = Some("a1".to_string());
        child.root_id = Some("a1".to_string());
        let mut elsewhere = agent("a5", SESSION_STATUS_IDLE);
        elsewhere.device_id = Some("dev-2".to_string());
        let thread = session("t1", SESSION_STATUS_IDLE, DEV, "2026-10-01T00:00:00Z");
        let held = agent("a6", SESSION_STATUS_IDLE);
        let live = agent("a7", SESSION_STATUS_RUNNING);
        let flying = agent("a8", SESSION_STATUS_IDLE);
        let mut pass = Pass::new(
            vec![running, idle, ended, child, elsewhere, thread, held, live, flying],
            Vec::new(),
        )
        .live("a7", LiveView::default());
        pass.revive_held.insert("a6".to_string());
        pass.in_flight.insert("a8".to_string());
        let revived: Vec<String> = pass
            .run()
            .into_iter()
            .filter_map(|action| match action {
                PickupAction::Revive { session } => Some(session.id),
                _ => None,
            })
            .collect();
        assert_eq!(revived, vec!["a1", "a2"]);
    }

    /// EXP-1374: an interrupted row (`blocked` = `device_offline` |
    /// `restarted` with `resume: true`).
    fn interrupted(mut row: SessionRow, kind: &str, resume: Option<bool>) -> SessionRow {
        row.blocked = Some(match resume {
            Some(resume) => serde_json::json!({ "kind": kind, "resume": resume }),
            None => serde_json::json!({ "kind": kind }),
        });
        row
    }

    fn continued(pass: &Pass) -> Vec<String> {
        pass.run()
            .into_iter()
            .filter_map(|action| match action {
                PickupAction::Continue { session } => Some(session.id),
                _ => None,
            })
            .collect()
    }

    /// EXP-1374: a non-persistent row its device left mid-turn continues
    /// by itself; not without the resume flag, not for another kind, not
    /// for a persistent root (it keeps Revive), not live/in flight/held.
    #[test]
    fn plan_continues_an_interrupted_mid_turn_run() {
        let at = "2026-10-01T00:00:00Z";
        let offline = interrupted(session("t1", SESSION_STATUS_IDLE, DEV, at), "device_offline", Some(true));
        let restarted = interrupted(session("t2", SESSION_STATUS_RUNNING, DEV, at), "restarted", Some(true));
        let no_resume = interrupted(session("t3", SESSION_STATUS_IDLE, DEV, at), "restarted", Some(false));
        let absent_resume = interrupted(session("t4", SESSION_STATUS_IDLE, DEV, at), "device_offline", None);
        let stuck = interrupted(session("t5", SESSION_STATUS_IDLE, DEV, at), "stuck", Some(true));
        let elsewhere = interrupted(session("t6", SESSION_STATUS_IDLE, "dev-2", at), "restarted", Some(true));
        let ended = interrupted(session("t7", SESSION_STATUS_ENDED, DEV, at), "restarted", Some(true));
        let held = interrupted(session("t8", SESSION_STATUS_IDLE, DEV, at), "restarted", Some(true));
        let live = interrupted(session("t9", SESSION_STATUS_RUNNING, DEV, at), "restarted", Some(true));
        let persistent = interrupted(agent("a1", SESSION_STATUS_IDLE), "restarted", Some(true));
        let mut pass = Pass::new(
            vec![offline, restarted, no_resume, absent_resume, stuck, elsewhere, ended, held, live, persistent],
            Vec::new(),
        )
        .live("t9", LiveView::default());
        pass.revive_held.insert("t8".to_string());
        assert_eq!(continued(&pass), vec!["t1", "t2"]);
        // The persistent root keeps its Revive.
        assert!(pass
            .run()
            .iter()
            .any(|action| matches!(action, PickupAction::Revive { session } if session.id == "a1")));
    }

    /// EXP-1374: once per absence — the registry's hold (an engine that ran
    /// here, or a settled attempt) keeps the next pass from continuing it
    /// again.
    #[test]
    fn plan_continues_once_per_absence() {
        let row = interrupted(
            session("t1", SESSION_STATUS_IDLE, DEV, "2026-10-01T00:00:00Z"),
            "restarted",
            Some(true),
        );
        let mut pass = Pass::new(vec![row], Vec::new());
        assert_eq!(continued(&pass), vec!["t1"]);
        let live = LiveRegistry::default();
        live.settle_revive("t1");
        pass.revive_held = live.revive_held();
        assert!(continued(&pass).is_empty());
    }

    /// EXP-1374: a pending input wins — the row Resumes on it (its report
    /// clears the flag), never a Continue as well.
    #[test]
    fn plan_resumes_an_interrupted_run_with_pending_input() {
        let row = interrupted(
            session("t1", SESSION_STATUS_IDLE, DEV, "2026-10-01T00:00:00Z"),
            "device_offline",
            Some(true),
        );
        let actions = Pass::new(vec![row], vec![message("m1", "t1", "2026-10-02T00:00:00Z")]).run();
        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0], PickupAction::Resume { session, .. } if session.id == "t1"));
    }

    #[test]
    fn continue_prompt_names_the_reason() {
        assert_eq!(
            continue_prompt("device_offline"),
            "Your run was interrupted (device offline) and continues now. Pick up exactly where you left off."
        );
        assert!(continue_prompt("restarted").contains("(runner restarted)"));
    }

    /// NEW-5: the startup re-claim never leaves a dead turn silent — a
    /// mid-turn row continues, a pending input replays, a `running` row
    /// between turns goes idle.
    #[test]
    fn reclaim_steps_cover_every_left_behind_row() {
        let row = |status: &str, busy: bool| SessionRow {
            id: "s".to_string(),
            status: Some(status.to_string()),
            busy,
            ..SessionRow::default()
        };
        assert_eq!(reclaim_step(&row("running", true), false, true), ReclaimStep::Continue);
        assert_eq!(reclaim_step(&row("idle", true), false, true), ReclaimStep::Continue);
        assert_eq!(reclaim_step(&row("running", true), true, true), ReclaimStep::Replay);
        assert_eq!(reclaim_step(&row("idle", false), true, true), ReclaimStep::Replay);
        assert_eq!(reclaim_step(&row("running", false), false, true), ReclaimStep::Idle);
        assert_eq!(reclaim_step(&row("idle", false), false, true), ReclaimStep::Waiting);
    }

    /// EXP-1374 (FEED-135): a row claimed but never spawned here (no run
    /// record) says so instead of going idle in silence; a pending input
    /// still replays (a fresh launch from its brief), an idle one waits.
    #[test]
    fn a_claimed_row_that_never_started_is_unstarted() {
        let row = |status: &str, busy: bool| SessionRow {
            id: "s".to_string(),
            status: Some(status.to_string()),
            busy,
            ..SessionRow::default()
        };
        assert_eq!(reclaim_step(&row("running", false), false, false), ReclaimStep::Unstarted);
        assert_eq!(reclaim_step(&row("running", true), false, false), ReclaimStep::Unstarted);
        assert_eq!(reclaim_step(&row("idle", true), false, false), ReclaimStep::Unstarted);
        assert_eq!(reclaim_step(&row("running", false), true, false), ReclaimStep::Replay);
        assert_eq!(reclaim_step(&row("idle", false), false, false), ReclaimStep::Waiting);
    }

    /// FEED-153: a record `prepare` wrote for a run that never got an agent
    /// session (the host died mid-start, after the claim consumed the
    /// brief) counts as no record — the row is Unstarted (said, idled),
    /// never a silent `running → idle`; one that had a session is a record.
    #[test]
    fn a_record_without_an_agent_session_reclaims_as_unstarted() {
        let dir = temp_dir("pickup-reclaim-unstarted");
        assert!(!reclaim_recorded(&dir.0, "s1"), "no record");
        crate::run_registry::record(&dir.0, record(&dir.0, "s1"));
        assert!(!reclaim_recorded(&dir.0, "s1"), "prepared, never a session");
        let running = SessionRow { id: "s1".to_string(), status: Some("running".to_string()), ..SessionRow::default() };
        assert_eq!(reclaim_step(&running, false, reclaim_recorded(&dir.0, "s1")), ReclaimStep::Unstarted);
        crate::run_registry::record(
            &dir.0,
            RunRecord { acp_session_id: Some("acp-1".to_string()), ..record(&dir.0, "s1") },
        );
        assert!(reclaim_recorded(&dir.0, "s1"));
        assert_eq!(reclaim_step(&running, false, reclaim_recorded(&dir.0, "s1")), ReclaimStep::Idle);
        crate::run_registry::record(
            &dir.0,
            RunRecord { agent_native_session_id: Some("native-1".to_string()), ..record(&dir.0, "s2") },
        );
        assert!(reclaim_recorded(&dir.0, "s2"));
    }

    /// EXP-1341: a pending message wakes a persistent root through Resume
    /// (the message is its turn), never a second Revive.
    #[test]
    fn plan_resumes_rather_than_revives_with_a_pending_message() {
        let actions = Pass::new(
            vec![agent("a1", SESSION_STATUS_IDLE)],
            vec![message("m1", "a1", "2026-10-02T00:00:00Z")],
        )
        .run();
        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0], PickupAction::Resume { session, .. } if session.id == "a1"));
    }

    /// EXP-1341: once per absence — an engine that ran here settles the
    /// root for good; a failed attempt backs off, and gives up after
    /// MAX_LAUNCH_ATTEMPTS.
    #[test]
    fn the_registry_holds_a_revived_root() {
        let live = LiveRegistry::default();
        assert!(live.revive_held().is_empty());
        assert_eq!(live.note_revive("a1"), 1);
        assert!(live.revive_held().contains("a1"), "backing off");
        live.settle_revive("a2");
        assert!(live.revive_held().contains("a2"));
        // The backoff grows and is capped.
        assert_eq!(revive_backoff(1), std::time::Duration::from_secs(30));
        assert_eq!(revive_backoff(2), std::time::Duration::from_secs(60));
        assert_eq!(revive_backoff(9), std::time::Duration::from_secs(480));
        // Past the backoff a failed root is offered again, until it gave up.
        locked(&live.revives).get_mut("a1").unwrap().not_before = std::time::Instant::now();
        assert!(!live.revive_held().contains("a1"));
        locked(&live.revives).get_mut("a1").unwrap().attempts = MAX_LAUNCH_ATTEMPTS;
        assert!(live.revive_held().contains("a1"));
    }

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
            "Report from Fix the cart (session c1, ended): Opened PR #12."
        );
        // An unsynced (or untitled) child is a Session; no summary = the body.
        let mut bare = report("r2", "s1", "c9", "");
        bare.body = Some("Ended without a summary.".to_string());
        assert_eq!(
            input_text(&bare, &sessions),
            "Report from Session (session c9, ended): Ended without a summary."
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
            body: Some("Report from Fix the cart (session c1, ended): Opened PR #12.".to_string()),
            ..rows[0].clone()
        };
        assert_eq!(
            Pass::new(vec![parent.clone(), child.clone()], rows.clone()).run(),
            vec![PickupAction::Resume { session: parent.clone(), messages: vec![expected.clone()] }]
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
                    messages: vec![message("m4", "a1", "2026-10-02T00:00:00Z")],
                },
                // FEED-129: every pending message, oldest first.
                PickupAction::Resume {
                    session: thread,
                    messages: vec![
                        message("m1", "s1", "2026-10-02T00:00:01Z"),
                        message("m2", "s1", "2026-10-02T00:00:02Z"),
                    ],
                },
                PickupAction::Resume {
                    session: restarted,
                    messages: vec![message("m3", "s2", "2026-10-02T00:00:00Z")],
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
            .resolve_card_input("s1", "q-1", None, Some("rejected".into()), None, Some("Smaller".into()))
            .expect("the card");
        assert_eq!(
            input,
            api::sessions::AnswerInput {
                message_id: "c1".into(),
                answer: None,
                decision: Some("rejected".into()),
                fresh: None,
                feedback: Some("Smaller".into()),
                ..Default::default()
            }
        );
        assert!(live.resolve_card_input("s1", "q-1", Some("a".into()), None, None, None).is_none(), "once");
        assert!(live.resolve_card_input("s2", "q-2", Some("a".into()), None, None, None).is_none());
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
                options: None,
                agent_model: None,
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
                    messages: vec![message("m-i1", "i1", "2026-10-02T00:00:00Z")],
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
        /// EXP-1334: live plan-mode switches, in order.
        plan_switches: Mutex<Vec<bool>>,
        /// EXP-1334: the run's current plan mode (`None` = unknown).
        mode: Mutex<Option<bool>>,
        /// EXP-1334: live model switches, in order.
        model_switches: Mutex<Vec<String>>,
        /// EXP-1334: the run's current model (`None` = unknown).
        model: Mutex<Option<String>>,
        /// EXP-1280: answered tool cards delivered, in order:
        /// (message id, text, approved, fresh).
        answered: Mutex<Vec<(String, String, bool, bool)>>,
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
        fn set_plan_mode(&self, on: bool) {
            self.plan_switches.lock().unwrap().push(on);
            *self.mode.lock().unwrap() = Some(on);
        }
        fn deliver_answer(&self, message_id: String, text: String, approved: bool, fresh: bool) {
            if approved {
                *self.mode.lock().unwrap() = Some(false);
            }
            self.answered.lock().unwrap().push((message_id, text, approved, fresh));
        }
        fn plan_mode(&self) -> Option<bool> {
            *self.mode.lock().unwrap()
        }
        fn set_model(&self, model: &str) {
            self.model_switches.lock().unwrap().push(model.to_string());
            *self.model.lock().unwrap() = Some(model.to_string());
        }
        fn model(&self) -> Option<String> {
            self.model.lock().unwrap().clone()
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
            options: None,
            agent_model: None,
        });
        assert!(registry.contains_live("s1"));
        assert!(!registry.contains_live("s2"));
        assert_eq!(registry.prune(), vec!["s2".to_string()]);
        assert_eq!(registry.session_ids(), vec!["s1".to_string()]);
        // The pruned entry's claim went with it.
        assert!(ClaimLock::try_acquire(&dir.0, "s2").unwrap().is_some());
    }

    /// S-4: a quiescent engine is suspended once it sat idle past the
    /// timeout (a turn, a handed message or an open card resets the clock);
    /// the row then resumes through the plan's `Resume`, never revived.
    #[test]
    fn release_idle_suspends_quiet_engines_after_the_timeout() {
        let registry = LiveRegistry::default();
        let quiet = Arc::new(FakeEngine::default());
        let busy = Arc::new(FakeEngine::default());
        let carded = Arc::new(FakeEngine::default());
        busy.working.store(true, Ordering::SeqCst);
        carded.cards.lock().unwrap().insert("c1".to_string(), "q-1".to_string());
        registry.insert("quiet", quiet.clone(), None);
        registry.insert("busy", busy.clone(), None);
        registry.insert("carded", carded.clone(), None);
        let t0 = std::time::Instant::now();
        let after = std::time::Duration::from_secs(60);
        assert!(registry.release_idle(t0, after).is_empty());
        assert!(registry.release_idle(t0 + std::time::Duration::from_secs(59), after).is_empty());
        // A handed message restarts the quiet stretch.
        registry.mark_handed("quiet", &["m1".to_string()]);
        assert!(registry.release_idle(t0 + std::time::Duration::from_secs(61), after).is_empty());
        registry.forget_handed("quiet", "m1");
        let t1 = t0 + std::time::Duration::from_secs(62);
        assert!(registry.release_idle(t1, after).is_empty());
        assert_eq!(registry.release_idle(t1 + after, after), vec!["quiet".to_string()]);
        assert_eq!(*quiet.killed.lock().unwrap(), Some("suspended"), "the process stops, the session stays");
        assert!(busy.killed.lock().unwrap().is_none());
        assert!(carded.killed.lock().unwrap().is_none());
        assert_eq!(registry.prune(), vec!["quiet".to_string()]);
        // The idle row's next message resumes it; the released root is held
        // from a revive (its engine ran here).
        let row = session("quiet", "idle", DEV, "2026-10-01T00:00:00Z");
        let views = registry.live_views();
        let held = registry.revive_held();
        assert!(held.contains("quiet"));
        let actions = plan(PickupInput {
            device_id: DEV,
            sessions: std::slice::from_ref(&row),
            messages: &[message("m2", "quiet", "2026-10-02T00:00:00Z")],
            live: &views,
            in_flight: &HashSet::new(),
            revive_held: &held,
        });
        let actions: Vec<&PickupAction> = actions.iter().filter(|action| action.session_id() == "quiet").collect();
        assert!(
            matches!(actions.as_slice(), [PickupAction::Resume { messages, .. }] if messages[0].id == "m2"),
            "{actions:?}"
        );
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
        /// EXP-1334: the launch options each spawn ran with.
        launched: Arc<Mutex<Vec<crate::LaunchOptions>>>,
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
        let launched = Arc::new(Mutex::new(Vec::new()));
        let launched_seen = Arc::clone(&launched);
        let launch: LaunchFn = Arc::new(move |prepared: crate::PreparedLaunch| {
            launched_seen.lock().unwrap().push(prepared.acp.options.clone());
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
            launched,
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
        // EXP-1252: the claim names this device (one writer).
        assert!(claim.ends_with(r#"{"sessionId":"sess-1","deviceId":"dev-1"}"#), "{claim}");
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

    /// The host retries a contended wake on its timer (the holder may be the
    /// process this one replaced, its claim stale seconds later), so it must
    /// hear contention apart from "nothing pending" and from a refusal.
    #[test]
    fn a_wake_another_process_holds_is_contended_not_refused() {
        let dir = temp_dir("pickup-wake-contended");
        let (base, requests) = canned_server_recording(vec![]);
        let host = host(&base, &dir.0, |_| {});
        crate::run_registry::record(&dir.0, record(&dir.0, "sess-1"));
        let held = ClaimLock::try_acquire(&dir.0, "sess-1").unwrap().unwrap();
        let row = session("sess-1", "idle", DEV, "t");
        let rows = vec![message("m1", "sess-1", "t")];
        assert_eq!(resume_session(&row, &rows, &host.deps), Err(WakeError::Contended));
        assert_eq!(deliver_messages("sess-1", &rows, &host.deps), Err(WakeError::Contended));
        // No request, no spawn, no reply: nothing was refused.
        assert!(requests.lock().unwrap().is_empty());
        assert!(host.spawned.lock().unwrap().is_empty());
        assert!(!host.deps.live.refused_before("sess-1", &rows.iter().collect::<Vec<_>>()));
        // Nothing pending is still an EMPTY delivery, not contention.
        drop(held);
        assert_eq!(deliver_messages("sess-1", &[], &host.deps), Ok(Vec::new()));
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

    /// EXP-1374 (FEED-135): a failed start posts "Could not start: …" as
    /// the session's reply, consumes the messages it tried and idles the
    /// row (never ends it), so the next message launches it again.
    #[test]
    fn a_failed_start_replies_consumes_the_tried_messages_and_idles() {
        let dir = temp_dir("pickup-failed-start");
        let (base, requests) = canned_server_recording(vec![
            (200, REPLY_OK.to_string()),
            (200, CONSUME_OK.to_string()),
            (200, REPORT_OK.to_string()),
        ]);
        let host = host(&base, &dir.0, |_| {});
        fail_start(
            "sess-1",
            "git worktree add: fatal: invalid reference: origin/exp/session-c0",
            &["m1".to_string(), "m2".to_string()],
            &host.deps,
        );
        assert_eq!(
            procedures(&requests),
            vec!["sessions.reply", "sessions.consume", "sessions.report"]
        );
        let requests = requests.lock().unwrap();
        assert!(
            requests[0].contains("Could not start: git worktree add: fatal: invalid reference"),
            "{}",
            requests[0]
        );
        assert!(requests[1].contains(r#""messageIds":["m1","m2"]"#), "{}", requests[1]);
        assert!(requests[2].contains(r#""status":"idle""#), "{}", requests[2]);
        assert!(!requests[2].contains("ended"), "{}", requests[2]);
        drop(requests);
        // The consumed ids are the pickup's own now: never handed again.
        let pending = vec![message("m1", "sess-1", "t"), message("m3", "sess-1", "t")];
        let mut refs: Vec<&MessageRow> = pending.iter().collect();
        host.deps.live.take_owed(&mut refs, &[]);
        assert_eq!(refs.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), vec!["m3"]);
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
        assert!(requests[3].ends_with(r#"{"sessionId":"sess-1","status":"ended","midTurn":false}"#), "{}", requests[3]);
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
        assert!(requests[3].ends_with(r#"{"sessionId":"agent-1","status":"idle","busy":false,"deviceId":"dev-1"}"#), "{}", requests[3]);
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

    // ---- #EXP-1383: `/new` on an agent's conversation restarts the engine ----

    fn new_command(id: &str, session_id: &str, created_at: &str) -> MessageRow {
        MessageRow { body: Some(" /new ".to_string()), ..message(id, session_id, created_at) }
    }

    /// A live agent root whose row asks for a between-turn restart: its
    /// `/new` alone is no turn a reconfigure could carry (the switch keeps
    /// it pending, so planning one looped on `AlreadyOn`), it is delivered;
    /// a message before it still carries the reconfigure.
    #[test]
    fn an_agent_new_is_delivered_not_planned_as_a_reconfigure() {
        let launched = SessionRow { agent_id: Some("ag-1".to_string()), ..session("sess-1", "idle", DEV, "t") };
        let row = SessionRow { effort: Some("high".to_string()), ..launched.clone() };
        let view = LiveView { options: Some(LaunchedOptions::of(&launched, Some(Vec::new()))), ..LiveView::default() };
        let alone = Pass::new(vec![row.clone()], vec![new_command("n1", "sess-1", "t1")]).live("sess-1", view.clone()).run();
        assert!(matches!(alone.as_slice(), [PickupAction::Deliver { .. }]), "{alone:?}");
        let behind = Pass::new(vec![row], vec![message("m1", "sess-1", "t1"), new_command("n1", "sess-1", "t2")])
            .live("sess-1", view)
            .run();
        assert!(matches!(behind.as_slice(), [PickupAction::Switch { account, .. }] if account.is_empty()), "{behind:?}");
    }

    /// An agent root's run record (kind `agent`) with an old conversation
    /// on it, and its team's agent key already minted.
    fn agent_record(host: &Host, dir: &Path, session_id: &str) {
        let mut record = record(dir, session_id);
        record.kind = RunKind::Agent;
        record.acp_session_id = Some("acp-old".to_string());
        record.agent_native_session_id = Some("claude-old".to_string());
        record.claude_session_id = Some("claude-old".to_string());
        crate::run_registry::record(dir, record);
        host.deps
            .coding
            .token_store
            .set("acct", api::token_store::SecretKind::AgentTeamKey("team-1".to_string()), "expu_team")
            .unwrap();
    }

    fn wait_until(what: &str, done: impl Fn() -> bool) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        while !done() {
            assert!(std::time::Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    #[test]
    fn only_a_persons_exact_new_is_the_command() {
        assert!(is_new_conversation(&new_command("n", "s", "t")));
        assert!(!is_new_conversation(&message("m", "s", "t")));
        let longer = MessageRow { body: Some("/new please".to_string()), ..message("m", "s", "t") };
        assert!(!is_new_conversation(&longer));
        let agents = MessageRow { author_kind: Some("agent".to_string()), ..new_command("n", "s", "t") };
        assert!(!is_new_conversation(&agents));
    }

    /// The agent's live, idle engine is SUSPENDED (the row idles, never
    /// ends), the same row launches fresh off the brief just fetched — no
    /// seed turn, no reopen of the old conversation, its ids gone from the
    /// record — the `/new` row is consumed by the pickup (never sent), and
    /// what came after it goes to the NEW engine.
    #[test]
    fn an_agents_new_restarts_its_idle_engine_fresh() {
        let dir = temp_dir("pickup-new-restart");
        let (base, requests) = canned_server_recording(vec![
            (200, BRIEF_OK.to_string()),
            (200, RESOLVE_NONE.to_string()),
            (200, CONSUME_OK.to_string()),
        ]);
        let host = host(&base, &dir.0, |_| {});
        agent_record(&host, &dir.0, "agent-1");
        let old = Arc::new(FakeEngine::default());
        host.deps.live.insert("agent-1", old.clone(), None);
        let rows = vec![
            new_command("n1", "agent-1", "2026-10-24T10:00:01Z"),
            message("m2", "agent-1", "2026-10-24T10:00:02Z"),
        ];

        assert!(deliver_messages("agent-1", &rows, &host.deps).unwrap().is_empty());
        wait_until("the fresh launch", || !host.spawned.lock().unwrap().is_empty());
        wait_until("the /new consume", || procedures(&requests).len() == 3);
        assert_eq!(*old.killed.lock().unwrap(), Some("suspended"), "the process stops, the session stays");
        assert!(old.delivered_ids().is_empty(), "the old conversation hears nothing");
        let spawned = host.spawned.lock().unwrap().clone();
        assert_eq!(
            spawned,
            vec![Spawned { session_id: "agent-1".to_string(), prompt: None, seed_message_id: None }]
        );
        assert_eq!(
            procedures(&requests),
            vec!["sessions.brief", "mcpServers.resolveForLaunch", "sessions.consume"]
        );
        assert!(requests.lock().unwrap()[2].ends_with(r#"{"sessionId":"agent-1","messageIds":["n1"]}"#));
        assert!(!requests.lock().unwrap().iter().any(|r| r.contains(r#""status":"ended""#)));
        let record = crate::run_registry::get(&dir.0, "agent-1").unwrap();
        assert_eq!((record.acp_session_id, record.agent_native_session_id, record.claude_session_id), (None, None, None));
        wait_until("the message after /new", || host.engine.delivered_ids() == vec!["m2"]);
        assert!(host.deps.live.contains_live("agent-1"));
        // A later pass hands nothing twice (the /new is being consumed).
        assert!(deliver_messages("agent-1", &rows, &host.deps).unwrap().is_empty());
        assert_eq!(host.engine.delivered_ids(), vec!["m2"]);
    }

    /// The restart waits for the agent to be between turns with nothing
    /// queued: what came BEFORE the `/new` is handed as ever, the `/new` and
    /// what follows it stay pending.
    #[test]
    fn an_agents_new_waits_for_the_turn_and_the_queue() {
        let dir = temp_dir("pickup-new-wait");
        let (base, requests) = canned_server_recording(vec![]);
        let host = host(&base, &dir.0, |_| {});
        agent_record(&host, &dir.0, "agent-1");
        host.engine.working.store(true, Ordering::SeqCst);
        host.deps.live.insert("agent-1", host.engine.clone(), None);
        let rows = vec![
            message("m1", "agent-1", "2026-10-24T10:00:01Z"),
            new_command("n2", "agent-1", "2026-10-24T10:00:02Z"),
            message("m3", "agent-1", "2026-10-24T10:00:03Z"),
        ];
        assert_eq!(deliver_messages("agent-1", &rows, &host.deps).unwrap(), vec!["m1"]);
        // Mid-turn: nothing more.
        assert!(deliver_messages("agent-1", &rows, &host.deps).unwrap().is_empty());
        // Between turns but m1 not consumed yet: still waiting.
        host.engine.working.store(false, Ordering::SeqCst);
        assert!(deliver_messages("agent-1", &rows, &host.deps).unwrap().is_empty());
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert_eq!(*host.engine.killed.lock().unwrap(), None);
        assert_eq!(host.engine.delivered_ids(), vec!["m1"]);
        assert!(requests.lock().unwrap().is_empty());
    }

    /// A thread (and a child of an agent's tree: a thread run) keeps the
    /// in-process `/new`: the engine gets the line.
    #[test]
    fn a_threads_new_stays_in_process() {
        let dir = temp_dir("pickup-new-thread");
        let host = host(&canned_server(vec![]), &dir.0, |_| {});
        crate::run_registry::record(&dir.0, record(&dir.0, "sess-1"));
        host.deps.live.insert("sess-1", host.engine.clone(), None);
        let rows = vec![new_command("n1", "sess-1", "t")];
        assert_eq!(deliver_messages("sess-1", &rows, &host.deps).unwrap(), vec!["n1"]);
        assert_eq!(host.engine.delivered_ids(), vec!["n1"]);
        assert_eq!(*host.engine.killed.lock().unwrap(), None);
    }

    /// A `/new` waking an agent with NO engine here starts it fresh too
    /// (never a native reopen with the line as its prompt).
    #[test]
    fn a_new_waking_an_engineless_agent_starts_it_fresh() {
        let dir = temp_dir("pickup-new-wake");
        let (base, requests) = canned_server_recording(vec![
            (200, BRIEF_OK.to_string()),
            (200, RESOLVE_NONE.to_string()),
            (200, CONSUME_OK.to_string()),
        ]);
        let host = host(&base, &dir.0, |_| {});
        agent_record(&host, &dir.0, "agent-1");
        let row = agent("agent-1", "idle");
        let rows = vec![new_command("n1", "agent-1", "t")];
        assert_eq!(resume_session(&row, &rows, &host.deps).unwrap(), vec!["n1"]);
        let spawned = host.spawned.lock().unwrap().clone();
        assert_eq!(spawned[0].prompt, None);
        assert_eq!(spawned[0].seed_message_id, None);
        assert!(host.engine.delivered_ids().is_empty());
        wait_until("the /new consume", || procedures(&requests).len() == 3);
        assert_eq!(crate::run_registry::get(&dir.0, "agent-1").unwrap().acp_session_id, None);
    }

    /// A switch hands an agent nothing from its `/new` on: the restart is
    /// the next pass's.
    #[test]
    fn a_switch_leaves_an_agents_new_pending() {
        let dir = temp_dir("pickup-new-switch");
        let host = host(&canned_server(vec![]), &dir.0, |_| {});
        agent_record(&host, &dir.0, "agent-1");
        host.deps.live.insert_for("agent-1", host.engine.clone(), None, Some(String::new()));
        let row = agent("agent-1", "idle");
        let rows = vec![new_command("n1", "agent-1", "t"), message("m2", "agent-1", "u")];
        // The row's own login: nothing moves, the pending messages are
        // handed — but never the `/new` or what follows it.
        let _ = switch_session(&row, "", None, &rows, &host.deps);
        assert!(host.engine.delivered_ids().is_empty(), "{:?}", host.engine.delivered_ids());
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
        assert!(report.ends_with(r#"{"sessionId":"sess-1","status":"idle","busy":false,"deviceId":"dev-1"}"#), "a wake never ends: {report}");
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
                revive_held: &HashSet::new(),
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
            (200, REPORT_OK.to_string()),
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
            vec!["sessions.report", "sessions.brief", "mcpServers.resolveForLaunch", "sessions.report"]
        );
        // EXP-1281: every client sees the move while it happens (the wall
        // lifted, the cards spinning), and the landed run reports running.
        let shown = requests.lock().unwrap()[0].clone();
        assert!(
            shown.ends_with(r#"{"sessionId":"sess-1","busy":true,"blocked":null}"#),
            "{shown}"
        );
        let running = requests.lock().unwrap()[3].clone();
        assert!(running.ends_with(r#"{"sessionId":"sess-1","status":"running"}"#), "{running}");
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
            (200, REPORT_OK.to_string()),
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
            vec!["sessions.report", "sessions.brief", "mcpServers.resolveForLaunch", "sessions.report", "sessions.consume"]
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
            (200, REPORT_OK.to_string()),
            (200, BRIEF_OK.to_string()),
            (200, REPLY_OK.to_string()),
            (200, REPORT_OK.to_string()),
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
        // EXP-1281: the switching state shown first is cleared last.
        assert_eq!(
            procedures(&requests),
            vec!["sessions.report", "sessions.brief", "sessions.reply", "sessions.report", "sessions.report"]
        );
        let settled = requests.lock().unwrap()[4].clone();
        assert!(settled.ends_with(r#"{"sessionId":"sess-1","busy":false}"#), "{settled}");
        // The wake the next pass plans for the same message stays quiet.
        assert!(resume_session(&row, &pending, &host.deps).unwrap().is_empty());
        assert_eq!(requests.lock().unwrap().len(), 5);
    }

    /// EXP-804: the switching state lifts the row's usage wall (the cards
    /// spin while the run moves off the walled login); a switch that leads
    /// nowhere puts the wall back with its clear — marked `handled`, so the
    /// server counts no new wall — else the run that stays on its login
    /// shows no "Usage limit" and its engine's next `blocked` report reads
    /// as a brand-new wall (another push, every time).
    #[test]
    fn a_refused_switch_puts_the_lifted_wall_back() {
        let dir = temp_dir("pickup-switch-walled");
        let (base, requests) = canned_server_recording(vec![
            (200, REPORT_OK.to_string()),
            (200, BRIEF_OK.to_string()),
            (200, REPLY_OK.to_string()),
            (200, REPORT_OK.to_string()),
            (200, REPORT_OK.to_string()),
        ]);
        let host = host(&base, &dir.0, |coding| coding.acp_available = false);
        let target = crate::agent_profiles::create(&dir.0, CodingAgent::Claude).unwrap();
        let mut on_a = record(&dir.0, "sess-1");
        on_a.set_account(Some(crate::test_support::TEST_PROFILE));
        crate::run_registry::record(&dir.0, on_a);
        host.deps.live.insert_for("sess-1", host.engine.clone(), None, Some(crate::test_support::TEST_PROFILE.to_string()));
        let wall = serde_json::json!({
            "kind": "rate_limit", "agent": "claude", "window": "weekly",
            "resetsAt": "2026-10-03T00:00:00Z", "since": "2026-10-02T00:00:00Z",
        });
        let row = SessionRow { blocked: Some(wall), ..on_account(session("sess-1", "idle", DEV, "t"), &target.id) };
        let pending = vec![message("m1", "sess-1", "2026-10-02T00:00:01Z")];
        assert!(matches!(switch_session(&row, &target.id, None, &pending, &host.deps), Ok(Switched::Refused(_))));
        let shown = requests.lock().unwrap()[0].clone();
        assert!(shown.contains(r#""blocked":null"#), "lifted while it moves: {shown}");
        let settled = requests.lock().unwrap()[4].clone();
        assert!(
            settled.ends_with(
                r#"{"sessionId":"sess-1","busy":false,"blocked":{"kind":"rate_limit","agent":"claude","window":"weekly","resetsAt":"2026-10-03T00:00:00Z","since":"2026-10-02T00:00:00Z","handled":true}}"#
            ),
            "{settled}"
        );
        // An unwalled row (or a cleared wall) has nothing to put back.
        assert_eq!(row_wall(&session("sess-1", "idle", DEV, "t")), None);
        assert_eq!(row_wall(&SessionRow { blocked: Some(serde_json::Value::Null), ..row.clone() }), None);
        let stampless = SessionRow { blocked: Some(serde_json::json!({"kind": "rate_limit"})), ..row };
        let back = row_wall(&stampless).expect("a wall");
        assert_eq!((back.kind.as_str(), back.resets_at, back.handled), ("rate_limit", None, Some(true)));
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
        // EXP-1341: the titles ride along for the launcher's label.
        assert_eq!(
            brief_issues("## Issues\n\n- APP-7 First one\n- APP-8"),
            vec![("APP-7".to_string(), "First one".to_string()), ("APP-8".to_string(), String::new())]
        );
        assert!(brief_issue_identifiers("## Issues\n\n- not-an-id x").is_empty());
    }

    // ---- EXP-1334: options change mid-session ----

    fn launched_for(row: &SessionRow, mcp: Option<Vec<String>>) -> LiveView {
        LiveView { options: Some(LaunchedOptions::of(row, mcp)), ..LiveView::default() }
    }

    /// A live row whose options moved past its engine's plans a RECONFIGURE
    /// (a Switch with no account): plan mode at once (even mid-turn, no
    /// message needed), effort/ultracode/computer use only with a turn to
    /// carry them, the MCP re-check only for an idle row that moved; an
    /// engine with unknown options never.
    #[test]
    fn a_row_whose_options_moved_plans_a_reconfigure() {
        let base = session("s1", "idle", DEV, "t");
        let view = launched_for(&base, Some(Vec::new()));
        let pending = message("m1", "s1", "2026-10-02T00:00:00Z");
        let run = |row: &SessionRow, messages: &[MessageRow], view: &LiveView| {
            Pass::new(vec![row.clone()], messages.to_vec()).live("s1", view.clone()).run()
        };
        let reconfigure = |row: &SessionRow, messages: &[MessageRow]| PickupAction::Switch {
            session: row.clone(),
            account: String::new(),
            messages: messages.to_vec(),
        };
        let deliver = PickupAction::Deliver { session_id: "s1".to_string(), messages: vec![pending.clone()] };
        assert_eq!(run(&base, &[pending.clone()], &view), vec![deliver.clone()]);
        assert!(run(&base, &[], &view).is_empty());

        let planned = SessionRow { plan_mode: true, busy: true, ..base.clone() };
        assert_eq!(run(&planned, &[], &view), vec![reconfigure(&planned, &[])]);

        for moved in [
            SessionRow { effort: Some("high".to_string()), ..base.clone() },
            SessionRow { ultracode: true, ..base.clone() },
            SessionRow { computer_use: true, ..base.clone() },
        ] {
            assert!(run(&moved, &[], &view).is_empty(), "waits for a turn: {moved:?}");
            assert_eq!(run(&moved, &[pending.clone()], &view), vec![reconfigure(&moved, &[pending.clone()])]);
            assert_eq!(run(&moved, &[pending.clone()], &LiveView::default()), vec![deliver.clone()]);
        }
        // A blank effort is the default, like none.
        let blank = SessionRow { effort: Some("  ".to_string()), ..base.clone() };
        assert_eq!(run(&blank, &[pending.clone()], &view), vec![deliver.clone()]);

        // The MCP pick is server-only: a row that moved is re-checked at the
        // next idle turn; never mid-turn, never with an unknown pick.
        let touched = SessionRow { updated_at: Some("2026-10-02T00:00:09Z".to_string()), ..base.clone() };
        assert_eq!(run(&touched, &[pending.clone()], &view), vec![reconfigure(&touched, &[pending.clone()])]);
        assert!(run(&touched, &[], &view).is_empty());
        let busy = SessionRow { busy: true, ..touched.clone() };
        assert_eq!(run(&busy, &[pending.clone()], &view), vec![deliver.clone()]);
        assert_eq!(run(&touched, &[pending.clone()], &launched_for(&base, None)), vec![deliver]);
    }

    /// Plan mode switches LIVE: no restart, no request, the messages go to
    /// the engine as they are, and the row's unmoved account pick is not
    /// honored behind a person's back.
    #[test]
    fn a_plan_mode_change_switches_the_live_engine() {
        let dir = temp_dir("pickup-plan-live");
        let (base, requests) = canned_server_recording(vec![]);
        let host = host(&base, &dir.0, |_| {});
        crate::run_registry::record(&dir.0, record(&dir.0, "sess-1"));
        let launched = session("sess-1", "idle", DEV, "t");
        host.deps.live.insert_for("sess-1", host.engine.clone(), None, Some("aaaa".to_string()));
        host.deps.live.set_options("sess-1", Some(LaunchedOptions::of(&launched, Some(Vec::new()))));
        let row = SessionRow { plan_mode: true, ..on_account(launched.clone(), "bbbb") };
        let pending = vec![message("m1", "sess-1", "2026-10-02T00:00:01Z")];

        assert_eq!(switch_session(&row, "", None, &pending, &host.deps), Ok(Switched::AlreadyOn));
        assert_eq!(*host.engine.plan_switches.lock().unwrap(), vec![true]);
        assert_eq!(host.engine.delivered_ids(), vec!["m1"]);
        assert_eq!(*host.engine.killed.lock().unwrap(), None);
        assert!(procedures(&requests).is_empty());
        let view = &host.deps.live.live_views()["sess-1"];
        assert_eq!(view.row_account.as_deref(), Some("aaaa"));
        // Honored: the next pass plans nothing more for it.
        assert!(options_delta(&row, view.options.as_ref().unwrap()).plan_mode.is_none());
    }

    /// Effort / ultracode / computer use: the idle engine is suspended and
    /// the SAME run resumed with the row's new pickers (the plan mode the
    /// run is in carried over), the pending message its seed turn. A busy
    /// engine is never touched.
    #[test]
    fn a_launch_option_change_restarts_the_run_between_turns() {
        let dir = temp_dir("pickup-reconfigure");
        let (base, requests) = canned_server_recording(vec![
            (200, BRIEF_OK.to_string()),
            (200, RESOLVE_NONE.to_string()),
            (200, REPORT_OK.to_string()),
        ]);
        let host = host(&base, &dir.0, |_| {});
        let mut low = record(&dir.0, "sess-1");
        low.effort = "low".to_string();
        crate::run_registry::record(&dir.0, low);
        let launched = SessionRow {
            effort: Some("low".to_string()),
            plan_mode: true,
            updated_at: Some("u0".to_string()),
            ..session("sess-1", "idle", DEV, "t")
        };
        let old = Arc::new(FakeEngine::default());
        *old.mode.lock().unwrap() = Some(true);
        host.deps.live.insert_for("sess-1", old.clone(), None, Some(String::new()));
        host.deps.live.set_options("sess-1", Some(LaunchedOptions::of(&launched, Some(Vec::new()))));
        let row = SessionRow {
            effort: Some("high".to_string()),
            ultracode: true,
            computer_use: true,
            ..launched.clone()
        };
        let pending = vec![message("m1", "sess-1", "2026-10-02T00:00:01Z")];

        old.working.store(true, Ordering::SeqCst);
        assert_eq!(switch_session(&row, "", None, &pending, &host.deps), Err(SwitchError::Busy));
        assert_eq!(*old.killed.lock().unwrap(), None, "a turn in flight is never interrupted");
        old.working.store(false, Ordering::SeqCst);

        assert_eq!(switch_session(&row, "", None, &pending, &host.deps), Ok(Switched::Moved));
        assert_eq!(*old.killed.lock().unwrap(), Some("suspended"));
        assert_eq!(
            procedures(&requests),
            vec!["sessions.brief", "mcpServers.resolveForLaunch", "sessions.report"]
        );
        assert!(old.plan_switches.lock().unwrap().is_empty(), "plan mode never moved");
        let spawned = host.spawned.lock().unwrap().clone();
        assert_eq!(spawned.len(), 1);
        assert_eq!(spawned[0].session_id, "sess-1", "the SAME row");
        assert_eq!(spawned[0].seed_message_id.as_deref(), Some("m1"));
        let options = host.launched.lock().unwrap()[0].clone();
        assert_eq!(options.effort, "high");
        assert!(options.ultracode && options.plan_mode);
        assert_eq!(options.computer_use, Some(true));
        let record = crate::run_registry::get(&dir.0, "sess-1").expect("record");
        assert_eq!(record.effort, "high");
        assert!(record.ultracode);
        // The new engine honors the row: nothing more to do.
        let honored = host.deps.live.options("sess-1").expect("options");
        assert_eq!(options_delta(&row, &honored), OptionsDelta::default());
    }

    /// A cleared effort restarts on this machine's default, never on the
    /// effort the record last ran.
    #[test]
    fn a_cleared_effort_restarts_on_the_default() {
        let dir = temp_dir("pickup-reconfigure-default");
        let base = canned_server(vec![
            (200, BRIEF_OK.to_string()),
            (200, RESOLVE_NONE.to_string()),
            (200, REPORT_OK.to_string()),
        ]);
        let host = host(&base, &dir.0, |_| {});
        let mut high = record(&dir.0, "sess-1");
        high.effort = "max".to_string();
        crate::run_registry::record(&dir.0, high);
        let launched = SessionRow { effort: Some("max".to_string()), ..session("sess-1", "idle", DEV, "t") };
        host.deps.live.insert_for("sess-1", Arc::new(FakeEngine::default()), None, Some(String::new()));
        host.deps.live.set_options("sess-1", Some(LaunchedOptions::of(&launched, Some(Vec::new()))));
        let row = SessionRow { effort: None, ..launched };
        let pending = vec![message("m1", "sess-1", "2026-10-02T00:00:01Z")];
        assert_eq!(switch_session(&row, "", None, &pending, &host.deps), Ok(Switched::Moved));
        assert_eq!(host.launched.lock().unwrap()[0].effort, host.deps.options.effort);
    }

    /// The MCP pick is re-read between turns once the row moved: the same
    /// pick is stamped verified and the message delivered; a moved pick
    /// restarts the run (the resume resolves the new pick).
    #[test]
    fn a_moved_mcp_pick_restarts_the_run_between_turns() {
        let dir = temp_dir("pickup-reconfigure-mcp");
        let picks = |ids: &str| format!(r#"{{"result":{{"data":{{"mcpServerIds":[{ids}]}}}}}}"#);
        let (base, requests) = canned_server_recording(vec![
            (200, picks(r#""srv-a""#)),
            (200, picks(r#""srv-b","srv-a""#)),
            (200, BRIEF_OK.to_string()),
            (200, RESOLVE_NONE.to_string()),
            (200, REPORT_OK.to_string()),
        ]);
        let host = host(&base, &dir.0, |_| {});
        crate::run_registry::record(&dir.0, record(&dir.0, "sess-1"));
        let launched = session("sess-1", "idle", DEV, "t");
        let old = Arc::new(FakeEngine::default());
        host.deps.live.insert_for("sess-1", old.clone(), None, Some(String::new()));
        host.deps.live.set_options("sess-1", Some(LaunchedOptions::of(&launched, Some(vec!["srv-a".to_string()]))));

        let touched = SessionRow { updated_at: Some("u1".to_string()), ..launched.clone() };
        let first = vec![message("m1", "sess-1", "2026-10-02T00:00:01Z")];
        assert_eq!(switch_session(&touched, "", None, &first, &host.deps), Ok(Switched::AlreadyOn));
        assert_eq!(old.delivered_ids(), vec!["m1"]);
        let verified = host.deps.live.options("sess-1").expect("options");
        assert_eq!(verified.mcp_checked_at.as_deref(), Some("u1"));
        assert!(!options_delta(&touched, &verified).mcp_due);

        let moved = SessionRow { updated_at: Some("u2".to_string()), ..launched };
        let second = vec![message("m2", "sess-1", "2026-10-02T00:00:02Z")];
        assert_eq!(switch_session(&moved, "", None, &second, &host.deps), Ok(Switched::Moved));
        assert_eq!(*old.killed.lock().unwrap(), Some("suspended"));
        assert_eq!(
            procedures(&requests),
            vec![
                "sessions.mcpPicks",
                "sessions.mcpPicks",
                "sessions.brief",
                "mcpServers.resolveForLaunch",
                "sessions.report",
            ]
        );
        assert_eq!(host.spawned.lock().unwrap()[0].seed_message_id.as_deref(), Some("m2"));
    }

    /// EXP-1280: plan mode is Exponential's flag, so the engine's own mode
    /// never moves the row: a run reading Build under a planning row plans
    /// nothing and reports nothing (only a person's pick switches it).
    #[test]
    fn the_engines_plan_mode_never_writes_the_row() {
        let dir = temp_dir("pickup-agent-plan");
        let (base, requests) = canned_server_recording(vec![(200, REPORT_OK.to_string())]);
        let host = host(&base, &dir.0, |_| {});
        crate::run_registry::record(&dir.0, record(&dir.0, "sess-1"));
        let planning = SessionRow {
            plan_mode: true,
            updated_at: Some("u1".to_string()),
            ..session("sess-1", "idle", DEV, "t")
        };
        host.deps.live.insert_for("sess-1", host.engine.clone(), None, Some(String::new()));
        host.deps.live.set_options("sess-1", Some(LaunchedOptions::of(&planning, Some(Vec::new()))));
        let pass = |row: &SessionRow| {
            host.deps.live.settle_options(std::slice::from_ref(row));
            plan(PickupInput {
                device_id: DEV,
                sessions: std::slice::from_ref(row),
                messages: &[],
                live: &host.deps.live.live_views(),
                in_flight: &HashSet::new(),
                revive_held: &HashSet::new(),
            })
        };
        *host.engine.mode.lock().unwrap() = Some(false);
        assert!(pass(&planning).is_empty());
        assert!(procedures(&requests).is_empty(), "no sessions.report");
        // A person's pick still switches it.
        let off = SessionRow { plan_mode: false, updated_at: Some("u2".to_string()), ..planning };
        assert!(matches!(pass(&off).as_slice(), [PickupAction::Switch { account, .. }] if account.is_empty()));
        assert_eq!(switch_session(&off, "", None, &[], &host.deps), Ok(Switched::AlreadyOn));
        assert_eq!(*host.engine.plan_switches.lock().unwrap(), vec![false]);
        assert!(procedures(&requests).is_empty());
    }

    /// EXP-1280: a card the session posted through a tool, answered.
    fn tool_card(id: &str, session_id: &str, card: serde_json::Value) -> MessageRow {
        MessageRow {
            author_kind: Some("agent".to_string()),
            author_session_id: Some(session_id.to_string()),
            body: Some(String::new()),
            card: Some(card),
            answered_at: Some("2026-10-02T00:00:05Z".to_string()),
            ..message(id, session_id, "2026-10-02T00:00:00Z")
        }
    }

    fn approved_plan() -> serde_json::Value {
        serde_json::json!({"kind": "plan", "plan": "1. One", "decision": "approved", "fresh": false})
    }

    /// EXP-1280: an answered tool-posted card of a live session is the
    /// agent's next turn ([`PickupAction::DeliverAnswer`], the fixture's
    /// answer text); an engine card (`questionId`), an unanswered, consumed,
    /// handed or open one, and another session's card are not.
    #[test]
    fn an_answered_tool_card_is_delivered_as_the_next_turn() {
        let fresh = serde_json::json!({"kind": "plan", "plan": "1. One", "decision": "approved", "fresh": true});
        let rejected = serde_json::json!({"kind": "plan", "plan": "1.", "decision": "rejected", "feedback": "Smaller."});
        let question = serde_json::json!({"kind": "question", "question": "Which?", "options": ["a"], "answers": ["a"]});
        let mut engine_card = tool_card("e1", "l1", question.clone());
        engine_card.card.as_mut().unwrap()["questionId"] = serde_json::json!("q-9");
        let mut unanswered = tool_card("u1", "l1", question.clone());
        unanswered.answered_at = None;
        // Another session's card is ITS input (a child asking), never a
        // tool card of this one.
        let mut foreign = tool_card("x1", "l1", question.clone());
        foreign.author_session_id = Some("other".to_string());
        assert!(!is_answered_tool_card(&foreign) && is_pending_input(&foreign));
        let messages = vec![
            tool_card("p1", "l1", approved_plan()),
            tool_card("p2", "l2", fresh),
            tool_card("p3", "l3", rejected),
            tool_card("q1", "l4", question),
            engine_card,
            unanswered,
            consumed(tool_card("c1", "l1", approved_plan())),
            tool_card("h1", "l1", approved_plan()),
        ];
        let sessions: Vec<SessionRow> =
            ["l1", "l2", "l3", "l4"].iter().map(|id| session(id, "idle", DEV, "t")).collect();
        let actions = Pass::new(sessions, messages)
            .live("l1", handed(&["h1"]))
            .live("l2", LiveView::default())
            .live("l3", LiveView::default())
            .live("l4", LiveView::default())
            .run();
        let delivered: Vec<(String, String, bool, bool)> = actions
            .iter()
            .filter_map(|action| match action {
                PickupAction::DeliverAnswer { message_id, text, approved, fresh, .. } => {
                    Some((message_id.clone(), text.clone(), *approved, *fresh))
                }
                _ => None,
            })
            .collect();
        assert_eq!(actions.len(), delivered.len(), "{actions:?}");
        assert_eq!(
            delivered,
            vec![
                ("p1".to_string(), "Plan approved. Go ahead.".to_string(), true, false),
                ("p2".to_string(), "Plan approved. Implement the following plan:\n\n1. One".to_string(), true, true),
                ("p3".to_string(), "Plan not approved. Keep planning.\n\nFeedback: Smaller.".to_string(), false, false),
                ("q1".to_string(), "Answer to \"Which?\": a".to_string(), false, false),
            ]
        );
    }

    /// EXP-1280: the server flips `plan_mode` off on approval; when that row
    /// lands in the same pass as the approved card, the flip is the
    /// approval (delivered silently), never a person's switch. Executing it
    /// honors `plan_mode = false`, marks the card handed (delivered once)
    /// and hands the engine the answer.
    #[test]
    fn an_approval_leaves_plan_mode_silently() {
        let dir = temp_dir("pickup-approval");
        let (base, _requests) = canned_server_recording(vec![]);
        let host = host(&base, &dir.0, |_| {});
        let planning = SessionRow { plan_mode: true, ..session("sess-1", "idle", DEV, "t") };
        host.deps.live.insert_for("sess-1", host.engine.clone(), None, Some(String::new()));
        host.deps.live.set_options("sess-1", Some(LaunchedOptions::of(&planning, Some(Vec::new()))));
        let flipped = SessionRow { plan_mode: false, ..planning.clone() };
        let card = tool_card("p1", "sess-1", approved_plan());
        let pass = |row: &SessionRow, messages: &[MessageRow]| {
            plan(PickupInput {
                device_id: DEV,
                sessions: std::slice::from_ref(row),
                messages,
                live: &host.deps.live.live_views(),
                in_flight: &HashSet::new(),
                revive_held: &HashSet::new(),
            })
        };
        let actions = pass(&flipped, std::slice::from_ref(&card));
        assert!(matches!(actions.as_slice(), [PickupAction::DeliverAnswer { approved: true, .. }]), "{actions:?}");
        execute(&actions[0], &host.deps).unwrap();
        assert_eq!(
            *host.engine.answered.lock().unwrap(),
            vec![("p1".to_string(), "Plan approved. Go ahead.".to_string(), true, false)]
        );
        assert!(host.engine.plan_switches.lock().unwrap().is_empty(), "no person's switch, no steered line");
        assert!(!host.deps.live.options("sess-1").unwrap().plan_mode);
        // Delivered once; the flipped row is no switch either.
        assert!(pass(&flipped, std::slice::from_ref(&card)).is_empty());
        // The card landed BEFORE the session row: the stale planning row is
        // no person's switch back until a row echoes the flip.
        let stale = pass(&planning, std::slice::from_ref(&card));
        assert!(stale.is_empty(), "stale until echoed: {stale:?}");
        host.deps.live.settle_options(std::slice::from_ref(&flipped));
        assert!(host.deps.live.options("sess-1").unwrap().plan_approved.is_none());
        let picked = SessionRow { plan_mode: true, ..flipped };
        assert!(matches!(
            pass(&picked, std::slice::from_ref(&card)).as_slice(),
            [PickupAction::Switch { account, .. }] if account.is_empty()
        ));
    }

    /// EXP-1280: the session row and the card reach the device through
    /// separate shapes. A row already flipped `plan_mode=false` while the
    /// session's own plan card is still unanswered (or answered and not
    /// handed yet) is the server's approval on its way, never a person's
    /// switch: no Switch (no "Plan mode is off: go ahead." turn of its own),
    /// and the answered card arriving next is delivered as the approval. A
    /// question card, a consumed card or an engine's card holds nothing; an
    /// ON-flip is a person's even with a plan card open.
    #[test]
    fn a_plan_mode_off_flip_waits_for_the_plan_card_it_belongs_to() {
        let planning = SessionRow { plan_mode: true, ..session("sess-1", "idle", DEV, "t") };
        let flipped = SessionRow { plan_mode: false, ..planning.clone() };
        let view = LiveView { options: Some(LaunchedOptions::of(&planning, Some(Vec::new()))), ..LiveView::default() };
        let mut open = tool_card("p1", "sess-1", serde_json::json!({"kind": "plan", "plan": "1. One"}));
        open.answered_at = None;
        // The flipped row lands first: held, nothing steered.
        let first = Pass::new(vec![flipped.clone()], vec![open.clone()]).live("sess-1", view.clone()).run();
        assert!(first.is_empty(), "held for the card: {first:?}");
        // The card lands answered: the approval, delivered silently.
        let next =
            Pass::new(vec![flipped.clone()], vec![tool_card("p1", "sess-1", approved_plan())]).live("sess-1", view.clone()).run();
        assert!(matches!(next.as_slice(), [PickupAction::DeliverAnswer { approved: true, .. }]), "{next:?}");
        // A rejection not handed yet holds it too: the row is no person's
        // word until the card is consumed.
        let rejected = tool_card(
            "p1",
            "sess-1",
            serde_json::json!({"kind": "plan", "plan": "1.", "decision": "rejected", "feedback": "Smaller."}),
        );
        let held = Pass::new(vec![flipped.clone()], vec![rejected]).live("sess-1", view.clone()).run();
        assert!(matches!(held.as_slice(), [PickupAction::DeliverAnswer { approved: false, .. }]), "{held:?}");
        // No open plan card of its own: a person's switch, as before.
        let mut question =
            tool_card("q1", "sess-1", serde_json::json!({"kind": "question", "question": "Which?", "options": ["a"]}));
        question.answered_at = None;
        let mut engines = open.clone();
        engines.id = "e1".to_string();
        engines.card.as_mut().unwrap()["questionId"] = serde_json::json!("q-9");
        let switched =
            Pass::new(vec![flipped.clone()], vec![question, consumed(open.clone()), engines]).live("sess-1", view.clone()).run();
        assert!(matches!(switched.as_slice(), [PickupAction::Switch { account, .. }] if account.is_empty()), "{switched:?}");
        // An ON-flip goes through with the card open.
        let mut off = view.clone();
        off.options.as_mut().unwrap().plan_mode = false;
        let on = Pass::new(vec![planning.clone()], vec![open]).live("sess-1", off).run();
        assert!(matches!(on.as_slice(), [PickupAction::Switch { account, .. }] if account.is_empty()), "{on:?}");
    }

    /// EXP-1280: an answered tool card whose session has no engine here (the
    /// host restarted while the card waited) wakes it: a Resume seeded with
    /// the answer text.
    #[test]
    fn an_answered_tool_card_resumes_a_session_without_an_engine() {
        let actions = Pass::new(
            vec![session("s1", "idle", DEV, "t")],
            vec![tool_card("p1", "s1", approved_plan())],
        )
        .run();
        match actions.as_slice() {
            [PickupAction::Resume { session, messages }] => {
                assert_eq!(session.id, "s1");
                assert_eq!(messages.len(), 1);
                assert_eq!(messages[0].id, "p1");
                assert_eq!(messages[0].body.as_deref(), Some("Plan approved. Go ahead."));
            }
            other => panic!("{other:?}"),
        }
    }

    fn superseded_plan() -> serde_json::Value {
        serde_json::json!({"kind": "plan", "plan": "1. Old", "decision": "superseded"})
    }

    /// FEED-140: a plan card the server resolved `superseded` (the agent
    /// posted a newer plan) is never the agent's next turn — not on a live
    /// engine, not as a wake — and holds no plan open; the newer approved
    /// card beside it still delivers.
    #[test]
    fn a_superseded_plan_card_is_never_delivered() {
        let superseded = tool_card("p-old", "l1", superseded_plan());
        assert!(!is_answered_tool_card(&superseded));
        assert!(!is_open_own_plan_card(&superseded));
        assert!(!card_approved(&superseded_plan()) && !card_fresh(&superseded_plan()));

        let live = Pass::new(
            vec![session("l1", "idle", DEV, "t")],
            vec![superseded.clone(), tool_card("p-new", "l1", approved_plan())],
        )
        .live("l1", LiveView::default())
        .run();
        let delivered: Vec<&str> = live
            .iter()
            .filter_map(|action| match action {
                PickupAction::DeliverAnswer { message_id, .. } => Some(message_id.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(delivered, vec!["p-new"], "{live:?}");

        let alone = Pass::new(vec![session("l1", "idle", DEV, "t")], vec![superseded.clone()])
            .live("l1", LiveView::default())
            .run();
        assert!(alone.is_empty(), "{alone:?}");

        // No engine here: a superseded card wakes nothing.
        let asleep = Pass::new(vec![session("s1", "idle", DEV, "t")], vec![tool_card("p-old", "s1", superseded_plan())]).run();
        assert!(!asleep.iter().any(|action| matches!(action, PickupAction::Resume { .. })), "{asleep:?}");
    }

    /// FEED-140: an engine-posted plan card (`questionId`, an open ask)
    /// resolved `superseded` is no answer to that ask.
    #[test]
    fn a_superseded_open_card_answers_no_ask() {
        let mut row = tool_card("c1", "s1", superseded_plan());
        row.card.as_mut().unwrap()["questionId"] = serde_json::json!("q-1");
        let view = LiveView {
            open_cards: HashMap::from([("c1".to_string(), "q-1".to_string())]),
            ..LiveView::default()
        };
        let actions = Pass::new(vec![session("s1", "running", DEV, "t")], vec![row]).live("s1", view).run();
        assert!(actions.is_empty(), "{actions:?}");
    }

    /// FEED-129: a Resume carries EVERY pending message (inputs oldest
    /// first, then answered cards), so a wake the host refused for an older
    /// set (`refused_before` memo) is tried again on the next message — the
    /// oldest alone matched the memo forever, and the session never resumed.
    #[test]
    fn a_resume_carries_every_pending_message_so_a_new_one_retries_a_refused_wake() {
        let rows = vec![
            message("m2", "s1", "2026-10-02T00:00:02Z"),
            tool_card("p1", "s1", approved_plan()),
            message("m1", "s1", "2026-10-02T00:00:01Z"),
        ];
        let actions = Pass::new(vec![session("s1", "idle", DEV, "t")], rows.clone()).run();
        let ids: Vec<&str> = match actions.as_slice() {
            [PickupAction::Resume { session, messages }] if session.id == "s1" => {
                messages.iter().map(|m| m.id.as_str()).collect()
            }
            other => panic!("{other:?}"),
        };
        assert_eq!(ids, vec!["m1", "m2", "p1"]);

        // The memo the host keeps: refused for {m1}, a wake for {m1, m2} is
        // not "refused before" — the new message gets its attempt.
        let live = LiveRegistry::default();
        live.note_refused("s1", &[&rows[2]]);
        assert!(live.refused_before("s1", &[&rows[2]]));
        assert!(!live.refused_before("s1", &[&rows[2], &rows[0]]));
    }

    /// A reported value keeps the OLD value stale only until it is echoed
    /// or [`REPORT_ECHO_WAIT`] passed; the reported value itself and an
    /// absent report are never stale.
    #[test]
    fn a_report_keeps_the_old_value_stale_until_echoed_or_timed_out() {
        let now = std::time::Instant::now();
        assert!(report_unechoed(Some(&(false, now)), &true));
        assert!(!report_unechoed(Some(&(false, now)), &false));
        assert!(!report_unechoed::<bool>(None, &true));
        assert!(report_unechoed(Some(&("haiku".to_string(), now)), &"opus".to_string()));
        if let Some(long_ago) = now.checked_sub(REPORT_ECHO_WAIT + std::time::Duration::from_secs(1)) {
            assert!(!report_unechoed(Some(&(false, long_ago)), &true), "past the wait the row speaks");
        }
        let mut options = LaunchedOptions::of(&session("s1", "idle", DEV, "t"), None);
        options.model_reported = Some(("haiku".to_string(), now));
        let stale = SessionRow { model: Some("opus".to_string()), ..session("s1", "idle", DEV, "t") };
        assert_eq!(options_delta(&stale, &options), OptionsDelta::default());
        options.settle_reports(&stale);
        assert!(options.model_reported.is_some(), "no echo yet");
        let echo = SessionRow { model: Some("haiku".to_string()), ..session("s1", "idle", DEV, "t") };
        options.settle_reports(&echo);
        assert!(options.model_reported.is_none());
    }

    /// The model follows either path: a person's `sessions.configure
    /// {model}` switches the live run at once (no restart), and the agent's
    /// own `/model` is written back onto the row (so a resume launches on
    /// it) — the stale pre-report row moving nothing, a model outside the
    /// agent's vocabulary never written.
    #[test]
    fn the_model_follows_the_row_and_the_agent() {
        let dir = temp_dir("pickup-model");
        let (base, requests) = canned_server_recording(vec![(200, REPORT_OK.to_string())]);
        let host = host(&base, &dir.0, |_| {});
        crate::run_registry::record(&dir.0, record(&dir.0, "sess-1"));
        let launched = SessionRow {
            model: Some("opus".to_string()),
            updated_at: Some("u1".to_string()),
            ..session("sess-1", "idle", DEV, "t")
        };
        host.deps.live.insert_for("sess-1", host.engine.clone(), None, Some(String::new()));
        host.deps.live.set_options(
            "sess-1",
            Some(LaunchedOptions::of(&launched, Some(Vec::new())).for_agent(CodingAgent::Claude)),
        );
        *host.engine.model.lock().unwrap() = Some("opus".to_string());
        let pass = |row: &SessionRow| {
            host.deps.live.settle_options(std::slice::from_ref(row));
            plan(PickupInput {
                device_id: DEV,
                sessions: std::slice::from_ref(row),
                messages: &[],
                live: &host.deps.live.live_views(),
                in_flight: &HashSet::new(),
                revive_held: &HashSet::new(),
            })
        };
        assert!(pass(&launched).is_empty());

        // A person picks sonnet: switched live, no restart, no request.
        let picked = SessionRow { model: Some("sonnet".to_string()), updated_at: Some("u2".to_string()), ..launched.clone() };
        assert!(matches!(pass(&picked).as_slice(), [PickupAction::Switch { account, .. }] if account.is_empty()));
        assert_eq!(switch_session(&picked, "", None, &[], &host.deps), Ok(Switched::AlreadyOn));
        assert_eq!(*host.engine.model_switches.lock().unwrap(), vec!["sonnet".to_string()]);
        assert!(host.spawned.lock().unwrap().is_empty());
        assert!(procedures(&requests).is_empty());
        assert!(pass(&picked).is_empty());

        // The agent's own `/model haiku`: written back, honored.
        *host.engine.model.lock().unwrap() = Some("haiku".to_string());
        assert!(matches!(pass(&picked).as_slice(), [PickupAction::Switch { .. }]));
        assert_eq!(switch_session(&picked, "", None, &[], &host.deps), Ok(Switched::AlreadyOn));
        assert_eq!(procedures(&requests), vec!["sessions.report"]);
        let report = requests.lock().unwrap()[0].clone();
        assert!(report.ends_with(r#"{"sessionId":"sess-1","model":"haiku"}"#), "{report}");
        assert_eq!(host.engine.model_switches.lock().unwrap().len(), 1, "never switched back");
        assert!(pass(&picked).is_empty(), "the stale row moves nothing");
        let echoed = SessionRow { model: Some("haiku".to_string()), updated_at: Some("u3".to_string()), ..picked };
        assert!(pass(&echoed).is_empty());

        // A model id outside the vocabulary is never written back.
        *host.engine.model.lock().unwrap() = Some("claude-opus-4-1-20250805".to_string());
        assert!(pass(&echoed).is_empty());
    }

    /// EXP-1281: a person's `/model` typed into a walled run, then an account
    /// switch before the row echoes it — the resumed run keeps the person's
    /// model (never the stale row's, never the launch's), and so does the
    /// run record a later wake falls back to.
    #[test]
    fn an_account_switch_keeps_the_model_the_run_is_on() {
        let dir = temp_dir("pickup-switch-model");
        let (base, requests) = canned_server_recording(vec![
            (200, REPORT_OK.to_string()),
            (200, REPORT_OK.to_string()),
            (200, BRIEF_OK.to_string()),
            (200, RESOLVE_NONE.to_string()),
            (200, REPORT_OK.to_string()),
        ]);
        let host = host(&base, &dir.0, |_| {});
        let target = crate::agent_profiles::create(&dir.0, CodingAgent::Claude).unwrap();
        let mut on_a = record(&dir.0, "sess-1");
        on_a.set_account(Some(crate::test_support::TEST_PROFILE));
        on_a.model = "sonnet".to_string();
        crate::run_registry::record(&dir.0, on_a);
        let launched = SessionRow {
            model: Some("sonnet".to_string()),
            updated_at: Some("u1".to_string()),
            ..on_account(session("sess-1", "idle", DEV, "t"), crate::test_support::TEST_PROFILE)
        };
        let old = Arc::new(FakeEngine::default());
        host.deps.live.insert_for(
            "sess-1",
            old.clone(),
            None,
            Some(crate::test_support::TEST_PROFILE.to_string()),
        );
        host.deps.live.set_options(
            "sess-1",
            Some(LaunchedOptions::of(&launched, Some(Vec::new())).for_agent(CodingAgent::Claude)),
        );
        // The person's `/model opus`, not on the row yet.
        *old.model.lock().unwrap() = Some("opus".to_string());
        let switched = SessionRow { updated_at: Some("u2".to_string()), ..on_account(launched, &target.id) };

        assert_eq!(switch_session(&switched, &target.id, Some("moved"), &[], &host.deps), Ok(Switched::Moved));
        let launched = host.launched.lock().unwrap().clone();
        assert_eq!(launched.last().map(|options| options.model.as_str()), Some("opus"));
        assert_eq!(crate::run_registry::get(&dir.0, "sess-1").expect("record").model, "opus");
        let first = requests.lock().unwrap()[0].clone();
        assert!(first.ends_with(r#"{"sessionId":"sess-1","model":"opus"}"#), "{first}");
        // The stale row (still sonnet) never switches the new run back.
        let options = host.deps.live.options("sess-1").expect("options");
        assert_eq!(options.model.as_deref(), Some("opus"));
        assert_eq!(options_delta(&switched, &options).model, None);
    }

    /// EXP-1401: a rate-limit model wall the server clears by moving the
    /// row's model (`blocked` → null, `model` → the new pick) carries the
    /// messages queued behind it: a live engine gets a reconfigure Switch
    /// with every pending message (the switch moves the model live and
    /// hands them, oldest first); a row with no engine here gets a Resume
    /// with all of them.
    #[test]
    fn a_cleared_model_wall_hands_the_queued_messages() {
        let walled = SessionRow {
            model: Some("opus".to_string()),
            updated_at: Some("u1".to_string()),
            blocked: Some(serde_json::json!({"kind": "rate_limit", "model": "opus"})),
            ..session("sess-1", SESSION_STATUS_IDLE, DEV, "2026-10-01T00:00:00Z")
        };
        let cleared = SessionRow {
            model: Some("sonnet".to_string()),
            updated_at: Some("u2".to_string()),
            blocked: Some(serde_json::Value::Null),
            ..walled.clone()
        };
        let queued = vec![
            message("m2", "sess-1", "2026-10-02T00:00:02Z"),
            message("m1", "sess-1", "2026-10-02T00:00:01Z"),
        ];
        let ids = |messages: &[MessageRow]| messages.iter().map(|m| m.id.clone()).collect::<Vec<_>>();
        let view = LiveView {
            row_account: Some(String::new()),
            options: Some(LaunchedOptions::of(&walled, Some(Vec::new())).for_agent(CodingAgent::Claude)),
            agent_model: Some("opus".to_string()),
            ..LiveView::default()
        };

        // Live engine: one reconfigure carrying both messages, no delivery
        // racing it.
        let actions = Pass::new(vec![cleared.clone()], queued.clone()).live("sess-1", view.clone()).run();
        match actions.as_slice() {
            [PickupAction::Switch { session, account, messages }] => {
                assert_eq!(session.id, "sess-1");
                assert!(account.is_empty(), "a reconfigure, not an account move");
                assert_eq!(ids(messages), vec!["m1", "m2"]);
            }
            other => panic!("expected one Switch, got {other:?}"),
        }

        // No engine here (the walled run exited): a Resume with both.
        let actions = Pass::new(vec![cleared.clone()], queued.clone()).run();
        match actions.as_slice() {
            [PickupAction::Resume { session, messages }] => {
                assert_eq!(session.id, "sess-1");
                assert_eq!(ids(messages), vec!["m1", "m2"]);
            }
            other => panic!("expected one Resume, got {other:?}"),
        }

        // The switch itself: the model moves live, both messages are handed
        // in order, nothing relaunches.
        let dir = temp_dir("pickup-model-wall");
        let host = host("http://127.0.0.1:9", &dir.0, |_| {});
        crate::run_registry::record(&dir.0, record(&dir.0, "sess-1"));
        host.deps.live.insert_for("sess-1", host.engine.clone(), None, Some(String::new()));
        host.deps.live.set_options("sess-1", view.options.clone());
        *host.engine.model.lock().unwrap() = Some("opus".to_string());
        let Some(PickupAction::Switch { messages, .. }) = Pass::new(vec![cleared.clone()], queued.clone())
            .live("sess-1", host.deps.live.live_views().remove("sess-1").unwrap())
            .run()
            .into_iter()
            .next()
        else {
            panic!("expected a Switch");
        };
        assert_eq!(switch_session(&cleared, "", None, &messages, &host.deps), Ok(Switched::AlreadyOn));
        assert_eq!(*host.engine.model_switches.lock().unwrap(), vec!["sonnet".to_string()]);
        assert_eq!(host.engine.delivered_ids(), vec!["m1", "m2"]);
        assert!(host.spawned.lock().unwrap().is_empty());
        // Handed: the next pass plans nothing more for them.
        let after = Pass::new(vec![cleared], queued)
            .live("sess-1", host.deps.live.live_views().remove("sess-1").unwrap())
            .run();
        assert!(after.is_empty(), "{after:?}");
    }

    // ---- EXP-1252: a row naming another device releases its engine ----

    #[test]
    fn a_live_engine_whose_row_names_another_device_is_released_never_killed() {
        let pass = Pass::new(
            vec![
                session("s1", "queued", "dev-2", "2026-10-01T00:00:00Z"),
                session("s2", "ended", "dev-2", "2026-10-01T00:00:00Z"),
            ],
            vec![message("m1", "s1", "2026-10-02T00:00:01Z")],
        )
        .live("s1", LiveView::default())
        .live("s2", LiveView::default());
        assert_eq!(
            pass.run(),
            vec![
                PickupAction::Release { session_id: "s1".to_string() },
                PickupAction::Release { session_id: "s2".to_string() },
            ]
        );
    }

    #[test]
    fn release_kills_the_engine_with_the_released_outcome() {
        let dir = temp_dir("pickup-release");
        let host = host("http://127.0.0.1:9", &dir.0, |_| {});
        let engine = Arc::new(FakeEngine::default());
        host.deps.live.insert("s1", engine.clone(), None);
        execute(&PickupAction::Release { session_id: "s1".to_string() }, &host.deps).unwrap();
        assert_eq!(*engine.killed.lock().unwrap(), Some(RELEASED_OUTCOME));
        assert!(host.deps.live.get("s1").is_none());
    }
}
